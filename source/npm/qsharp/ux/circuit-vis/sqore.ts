// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { formatInputs } from "./formatters/inputFormatter.js";
import { formatGates } from "./formatters/gateFormatter.js";
import { formatRegisters } from "./formatters/registerFormatter.js";
import { processOperations } from "./process.js";
import {
  ConditionalRender,
  Circuit,
  CircuitGroup,
  ComponentGrid,
  Operation,
  SourceLocation,
  Qubit,
} from "./circuit.js";
import { GateRenderData } from "./gateRenderData.js";
import {
  gateHeight,
  minGateWidth,
  minToolboxHeight,
  svgNS,
} from "./constants.js";
import { createDropzones } from "./draggable.js";
import { enableEvents } from "./events.js";
import { createPanel, enableRunButton } from "./panel.js";
import { getMinMaxRegIdx } from "./utils.js";

/**
 * Contains render data for visualization.
 */
interface ComposedSqore {
  /** Width of visualization. */
  width: number;
  /** Height of visualization. */
  height: number;
  /** SVG elements the make up the visualization. */
  elements: SVGElement[];
}

/**
 * Defines the mapping of unique location to each operation. Used for enabling
 * interactivity.
 */
type GateRegistry = {
  [location: string]: Operation;
};

export type DrawOptions = {
  renderDepth?: number;
  isEditable?: boolean;
  editCallback?: (circuitGroup: CircuitGroup) => void;
  runCallback?: () => void;
  renderLocations?: (l: SourceLocation[]) => { title: string; href: string };
  statePanelInitiallyExpanded?: boolean;
};

/**
 * Entrypoint class for rendering circuit visualizations.
 */
export class Sqore {
  circuit: Circuit;
  gateRegistry: GateRegistry = {};
  renderDepth: number = this.options.renderDepth ?? 0;
  /**
   * Initializes Sqore object.
   *
   * @param circuitGroup Group of circuits to be visualized.
   * @param isEditable Whether the circuit is editable.
   * @param editCallback Callback function to be called when the circuit is edited.
   * @param runCallback Callback function to be called when the circuit is run.
   */
  constructor(
    public circuitGroup: CircuitGroup,
    private options: DrawOptions = {},
  ) {
    if (
      this.circuitGroup == null ||
      this.circuitGroup.circuits == null ||
      this.circuitGroup.circuits.length === 0
    ) {
      throw new Error(
        `No circuit found in file. Please provide a valid circuit.`,
      );
    }
    // For now we only visualize the first circuit in the group
    this.circuit = this.circuitGroup.circuits[0];
  }

  /**
   * Render circuit into `container` at the specified layer depth.
   *
   * @param container HTML element for rendering visualization into.
   */
  draw(container: HTMLElement): void {
    // Inject into container
    if (container == null) throw new Error(`Container not provided.`);

    this.renderCircuit(container);
  }

  /**
   * Render circuit into `container`.
   *
   * @param container HTML element for rendering visualization into.
   * @param circuit Circuit object to be rendered.
   */
  private renderCircuit(container: HTMLElement, circuit?: Circuit): void {
    // Create copy of circuit to prevent mutation
    const _circuit: Circuit =
      circuit ?? JSON.parse(JSON.stringify(this.circuit));

    // Assign unique locations to each operation
    _circuit.componentGrid.forEach((col, colIndex) =>
      col.components.forEach((op, i) =>
        this.fillGateRegistry(op, `${colIndex},${i}`),
      ),
    );

    // Expand operations to the specified render depth
    this.expandOperationsToDepth(_circuit.componentGrid, this.renderDepth);

    // Auto-expand any groups with single children
    this.expandIfSingleOperation(_circuit.componentGrid);

    // Create visualization components
    const composedSqore: ComposedSqore = this.compose(_circuit);
    const svg: SVGElement = this.generateSvg(composedSqore);
    this.setViewBox(svg);
    const previousSvg = container.querySelector("svg.qviz");
    if (previousSvg == null) {
      container.appendChild(svg);
    } else {
      const wrapper = previousSvg.parentElement;
      if (wrapper) {
        wrapper.replaceChild(svg, previousSvg);
      } else {
        container.replaceChild(svg, previousSvg);
      }
    }
    this.addGateClickHandlers(container, _circuit);

    if (this.options.isEditable) {
      createDropzones(container, this);
      createPanel(container, this.options.statePanelInitiallyExpanded === true);
      if (this.options.runCallback != undefined) {
        const callback = this.options.runCallback;
        enableRunButton(container, callback);
      }
      enableEvents(container, this, () => this.renderCircuit(container));
      if (this.options.editCallback != undefined) {
        this.options.editCallback(this.minimizeCircuits(this.circuitGroup));
      }
    }
  }

  private expandOperationsToDepth(
    componentGrid: ComponentGrid,
    targetDepth: number,
    currentDepth: number = 0,
  ) {
    for (const col of componentGrid) {
      for (const op of col.components) {
        if (currentDepth < targetDepth && op.children != null) {
          op.conditionalRender = ConditionalRender.AsGroup;
          op.dataAttributes = op.dataAttributes || {};
          op.dataAttributes["expanded"] = "true";
          this.expandOperationsToDepth(
            op.children,
            targetDepth,
            currentDepth + 1,
          );
        }
      }
    }
  }

  private expandIfSingleOperation(grid: ComponentGrid) {
    if (grid.length == 1 && grid[0].components.length == 1) {
      const onlyComponent = grid[0].components[0];
      if (
        onlyComponent.dataAttributes != null &&
        Object.prototype.hasOwnProperty.call(
          onlyComponent.dataAttributes,
          "location",
        ) &&
        onlyComponent.dataAttributes["expanded"] !== "false"
      ) {
        const location: string = onlyComponent.dataAttributes["location"];
        this.expandOperation(grid, location);
      }
    }
    // Recursively expand if the only child is also a single operation
    for (const col of grid) {
      for (const op of col.components) {
        this.expandIfSingleOperation(op.children || []);
      }
    }
  }

  /**
   * Sets the viewBox attribute of the SVG element to enable zooming and panning.
   *
   * @param svg The SVG element to set the viewBox for.
   */
  private setViewBox(svg: SVGElement) {
    // width and height are the true dimensions generated by qviz
    const width = parseInt(svg.getAttribute("width")!);
    const height = parseInt(svg.getAttribute("height")!);
    svg.setAttribute("viewBox", `0 0 ${width} ${height}`);
  }

  /**
   * Generates the components required for visualization.
   *
   * @param circuit Circuit to be visualized.
   *
   * @returns `ComposedSqore` object containing render data for visualization.
   */
  private compose(circuit: Circuit): ComposedSqore {
    const add = (
      acc: GateRenderData[],
      gate: GateRenderData | GateRenderData[],
    ): void => {
      if (Array.isArray(gate)) {
        gate.forEach((g) => add(acc, g));
      } else {
        acc.push(gate);
        gate.children?.forEach((col) => col.forEach((g) => add(acc, g)));
      }
    };

    const flatten = (renderData: GateRenderData[][]): GateRenderData[] => {
      const result: GateRenderData[] = [];
      renderData.forEach((col) => col.forEach((g) => add(result, g)));
      return result;
    };

    const { qubits, componentGrid } = circuit;

    // Calculate the row heights, which may vary depending on how many
    // expanded group borders need to fit between qubit wires.
    const rowHeights = getRowHeights(qubits, componentGrid);

    // Draw the qubit labels.
    // Also calculate other register render data to be used later in the rendering.
    const { qubitLabels, registers, svgHeight } = formatInputs(
      qubits,
      rowHeights,
      this.options.isEditable ? undefined : this.options.renderLocations,
    );

    // Calculate the render data for the operations.
    const topY = qubits[0] ? registers[qubits[0].id].y : -1;
    const bottomY = qubits[qubits.length - 1]
      ? registers[qubits[qubits.length - 1].id].y
      : -1;
    const { renderDataArray, svgWidth } = processOperations(
      componentGrid,
      topY,
      bottomY,
      registers,
      this.options.isEditable ? undefined : this.options.renderLocations,
    );

    // Draw the operations.
    const formattedGates: SVGElement = formatGates(renderDataArray);

    // Draw the lines that represent qubit and classical wires.
    const formattedRegs: SVGElement = formatRegisters(
      registers,
      flatten(renderDataArray),
      svgWidth,
    );

    const composedSqore: ComposedSqore = {
      width: svgWidth,
      height: svgHeight,
      elements: [qubitLabels, formattedRegs, formattedGates],
    };
    return composedSqore;
  }

  /**
   * Generates visualization of `composedSqore` as an SVG.
   *
   * @param composedSqore ComposedSqore to be visualized.
   *
   * @returns SVG representation of circuit visualization.
   */
  private generateSvg(composedSqore: ComposedSqore): SVGElement {
    const { width, height, elements } = composedSqore;

    const svg: SVGElement = document.createElementNS(svgNS, "svg");
    svg.setAttribute("class", "qviz");
    svg.setAttribute("width", width.toString());
    svg.setAttribute("height", height.toString());
    svg.style.setProperty("max-width", "fit-content");

    // Add styles
    document.documentElement.style.setProperty(
      "--minToolboxHeight",
      `${minToolboxHeight}px`,
    );
    document.documentElement.style.setProperty(
      "--minGateWidth",
      `${minGateWidth}px`,
    );
    document.documentElement.style.setProperty(
      "--gateHeight",
      `${gateHeight}px`,
    );

    // Add body elements
    elements.forEach((element: SVGElement) => svg.appendChild(element));

    return svg;
  }

  /**
   * Depth-first traversal to assign unique location string to `operation`.
   * The operation is assigned the location `location` and its `i`th child
   * in its `colIndex` column is recursively given the location
   * `${location}-${colIndex},${i}`.
   *
   * @param operation Operation to be assigned.
   * @param location: Location to assign to `operation`.
   *
   */
  private fillGateRegistry(operation: Operation, location: string): void {
    if (operation.dataAttributes == null) operation.dataAttributes = {};
    operation.dataAttributes["location"] = location;
    // By default, operations cannot be zoomed-out
    operation.dataAttributes["zoom-out"] = "false";
    this.gateRegistry[location] = operation;
    operation.children?.forEach((col, colIndex) =>
      col.components.forEach((childOp, i) => {
        this.fillGateRegistry(childOp, `${location}-${colIndex},${i}`);
        if (childOp.dataAttributes == null) childOp.dataAttributes = {};
        // Children operations can be zoomed out
        childOp.dataAttributes["zoom-out"] = "true";
      }),
    );
    // Composite operations can be zoomed in
    operation.dataAttributes["zoom-in"] = (
      operation.children != null
    ).toString();
  }

  /**
   * Add interactive click handlers to circuit HTML elements.
   *
   * @param container HTML element containing visualized circuit.
   * @param circuit Circuit to be visualized.
   *
   */
  private addGateClickHandlers(container: HTMLElement, circuit: Circuit): void {
    this.addClassicalControlHandlers(container);
    this.addZoomHandlers(container, circuit);
  }

  /**
   * Add interactive click handlers for classically-controlled operations.
   *
   * @param container HTML element containing visualized circuit.
   *
   */
  private addClassicalControlHandlers(container: HTMLElement): void {
    container.querySelectorAll(".classically-controlled-btn").forEach((btn) => {
      // Zoom in on clicked gate
      btn.addEventListener("click", (evt: Event) => {
        const textSvg = btn.querySelector("text");
        const group = btn.parentElement;
        if (textSvg == null || group == null) return;

        const currValue = textSvg.firstChild?.nodeValue;
        const zeroGates = group?.querySelector(".gates-zero");
        const oneGates = group?.querySelector(".gates-one");
        switch (currValue) {
          case "?":
            textSvg.childNodes[0].nodeValue = "1";
            group.classList.remove("classically-controlled-unknown");
            group.classList.remove("classically-controlled-zero");
            group.classList.add("classically-controlled-one");
            zeroGates?.classList.add("hidden");
            oneGates?.classList.remove("hidden");
            break;
          case "1":
            textSvg.childNodes[0].nodeValue = "0";
            group.classList.remove("classically-controlled-unknown");
            group.classList.add("classically-controlled-zero");
            group.classList.remove("classically-controlled-one");
            zeroGates?.classList.remove("hidden");
            oneGates?.classList.add("hidden");
            break;
          case "0":
            textSvg.childNodes[0].nodeValue = "?";
            group.classList.add("classically-controlled-unknown");
            group.classList.remove("classically-controlled-zero");
            group.classList.remove("classically-controlled-one");
            zeroGates?.classList.remove("hidden");
            oneGates?.classList.remove("hidden");
            break;
        }
        evt.stopPropagation();
      });
    });
  }

  /**
   * Add interactive click handlers for zoom-in/out functionality.
   *
   * @param container HTML element containing visualized circuit.
   * @param circuit Circuit to be visualized.
   *
   */
  private addZoomHandlers(container: HTMLElement, circuit: Circuit): void {
    container.querySelectorAll(".gate .gate-control").forEach((ctrl) => {
      // Zoom in on clicked gate
      ctrl.addEventListener("click", (ev: Event) => {
        const gateId: string | null | undefined =
          ctrl.parentElement?.getAttribute("data-location");
        if (typeof gateId == "string") {
          if (ctrl.classList.contains("gate-collapse")) {
            this.collapseOperation(circuit.componentGrid, gateId);
          } else if (ctrl.classList.contains("gate-expand")) {
            this.expandOperation(circuit.componentGrid, gateId);
          }
          this.renderCircuit(container, circuit);

          ev.stopPropagation();
        }
      });
    });
  }

  /**
   * Expand selected operation for zoom-in interaction.
   *
   * @param componentGrid Grid of circuit components.
   * @param location Location of operation to expand.
   *
   */
  private expandOperation(
    componentGrid: ComponentGrid,
    location: string,
  ): void {
    componentGrid.forEach((col) =>
      col.components.forEach((op) => {
        if (op.conditionalRender === ConditionalRender.AsGroup)
          this.expandOperation(op.children || [], location);
        if (op.dataAttributes == null) return op;
        const opId: string = op.dataAttributes["location"];
        if (opId === location && op.children != null) {
          op.conditionalRender = ConditionalRender.AsGroup;
          op.dataAttributes["expanded"] = "true";
        }
      }),
    );
  }

  /**
   * Collapse selected operation for zoom-out interaction.
   *
   * @param componentGrid Grid of circuit components.
   * @param parentLoc Location of operation to collapse.
   *
   */
  private collapseOperation(
    componentGrid: ComponentGrid,
    parentLoc: string,
  ): void {
    componentGrid.forEach((col) =>
      col.components.forEach((op) => {
        if (op.conditionalRender === ConditionalRender.AsGroup)
          this.collapseOperation(op.children || [], parentLoc);
        if (op.dataAttributes == null) return op;
        const opId: string = op.dataAttributes["location"];
        // Collapse parent gate and its children
        if (opId.startsWith(parentLoc)) {
          op.conditionalRender = ConditionalRender.Always;
          op.dataAttributes["expanded"] = "false";
        }
      }),
    );
  }

  // Minimize the circuits in a circuit group to remove dataAttributes
  minimizeCircuits(circuitGroup: CircuitGroup): CircuitGroup {
    // Create a deep copy of the circuit group
    const minimizedCircuits: CircuitGroup = JSON.parse(
      JSON.stringify(circuitGroup),
    );
    minimizedCircuits.circuits.forEach((circuit) => {
      circuit.componentGrid.forEach((col) => {
        col.components.forEach(this.minimizeOperation);
      });
    });
    return minimizedCircuits;
  }

  // Minimize the operation to remove dataAttributes
  minimizeOperation = (operation: Operation): void => {
    if (operation.children !== undefined) {
      operation.children.forEach((col) =>
        col.components.forEach(this.minimizeOperation),
      );
    }
    operation.dataAttributes = undefined;
  };
}

/**
 * Recursively computes vertical space required to render group borders.
 *
 * The resulting `heightAboveWire` and `heightBelowWire` values per qubit are
 * later used by `formatInputs` to leave sufficient space between qubit wires.
 *
 * @param qubits Array of qubits in the circuit.
 * @param componentGrid Grid of circuit components to traverse.
 *
 * @returns Mapping from qubit index to required heights above and below their wires.
 */
function getRowHeights(
  qubits: Qubit[],
  componentGrid: ComponentGrid,
): {
  [qubitIndex: number]: {
    heightAboveWire: number;
    heightBelowWire: number;
  };
} {
  const rowHeights: {
    [qubitIndex: number]: {
      currentGroupBordersAboveWire: number;
      currentGroupBordersBelowWire: number;
      heightAboveWire: number;
      heightBelowWire: number;
    };
  } = {};

  for (const q of qubits) {
    const { id } = q;
    rowHeights[id] = {
      currentGroupBordersBelowWire: 0,
      currentGroupBordersAboveWire: 0,
      heightBelowWire: 0,
      heightAboveWire: 0,
    };
  }

  updateRowHeights(componentGrid, rowHeights, qubits.length);
  return rowHeights;
}

function updateRowHeights(
  componentGrid: ComponentGrid,
  rowHeights: {
    [qubitIndex: number]: {
      currentGroupBordersAboveWire: number;
      currentGroupBordersBelowWire: number;
      heightAboveWire: number;
      heightBelowWire: number;
    };
  },
  numQubits: number,
) {
  for (const col of componentGrid) {
    for (const component of col.components) {
      if (component.dataAttributes?.["expanded"] === "true") {
        // We're in an expanded group. There is a dashed border above
        // the top qubit, and below the bottom qubit.
        const [topQubit, bottomQubit] = getMinMaxRegIdx(component, numQubits);

        // Increment the current count of dashed group borders for
        // the top and bottom rows for this operation.
        // If the max height for this row has been exceeded above or below the wire,
        // update it.
        rowHeights[topQubit].currentGroupBordersAboveWire++;
        rowHeights[topQubit].heightAboveWire = Math.max(
          rowHeights[topQubit].heightAboveWire,
          rowHeights[topQubit].currentGroupBordersAboveWire,
        );

        rowHeights[bottomQubit].currentGroupBordersBelowWire++;
        rowHeights[bottomQubit].heightBelowWire = Math.max(
          rowHeights[bottomQubit].heightBelowWire,
          rowHeights[bottomQubit].currentGroupBordersBelowWire,
        );

        // recurse
        updateRowHeights(component.children || [], rowHeights, numQubits);

        // decrement
        rowHeights[topQubit].currentGroupBordersAboveWire--;
        rowHeights[bottomQubit].currentGroupBordersBelowWire--;
      }
    }
  }
}
