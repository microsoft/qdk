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
  Column,
  SourceLocation,
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

    // Render operations starting at given depth
    _circuit.componentGrid = this.selectOpsAtDepth(
      _circuit.componentGrid,
      this.renderDepth,
    );

    // If only one top-level operation, expand automatically:
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
      createPanel(container);
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
        // Recursively expand if the only child is also a single operation
        this.expandIfSingleOperation(onlyComponent.children || []);
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
    const { qubitWires, registers, svgHeight } = formatInputs(
      qubits,
      this.options.isEditable ? undefined : this.options.renderLocations,
    );
    const { renderDataArray, svgWidth } = processOperations(
      componentGrid,
      registers,
      this.options.isEditable ? undefined : this.options.renderLocations,
    );
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
      elements: [qubitWires, formattedRegs, formattedGates],
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
   * Pick out operations that are at or below `renderDepth`.
   *
   * @param componentGrid Circuit components.
   * @param renderDepth Initial layer depth at which to render gates.
   *
   * @returns Grid of components at or below specified depth.
   */
  private selectOpsAtDepth(
    componentGrid: ComponentGrid,
    renderDepth: number,
  ): ComponentGrid {
    if (renderDepth < 0)
      throw new Error(
        `Invalid renderDepth of ${renderDepth}. Needs to be >= 0.`,
      );
    if (renderDepth === 0) return componentGrid;
    const selectedOps: ComponentGrid = [];
    componentGrid.forEach((col) => {
      const selectedCol: Operation[] = [];
      const extraCols: Column[] = [];
      col.components.forEach((op) => {
        if (op.children != null) {
          const selectedChildren = this.selectOpsAtDepth(
            op.children,
            renderDepth - 1,
          );
          if (selectedChildren.length > 0) {
            selectedCol.push(...selectedChildren[0].components);
            selectedChildren.slice(1).forEach((col, colIndex) => {
              if (extraCols[colIndex] == null) extraCols[colIndex] = col;
              // NOTE: I'm unsure if this is a safe way to combine column arrays
              else extraCols[colIndex].components.push(...col.components);
            });
          }
        } else {
          selectedCol.push(op);
        }
      });
      selectedOps.push({ components: selectedCol });
      if (extraCols.length > 0) {
        selectedOps.push(...extraCols);
      }
    });
    return selectedOps;
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
