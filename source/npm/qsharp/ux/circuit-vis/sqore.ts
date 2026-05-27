// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { formatInputs } from "./renderer/formatters/inputFormatter.js";
import { formatGates } from "./renderer/formatters/gateFormatter.js";
import { formatRegisters } from "./renderer/formatters/registerFormatter.js";
import { processOperations } from "./renderer/process.js";
import {
  Circuit,
  CircuitGroup,
  ComponentGrid,
  Operation,
  SourceLocation,
  Qubit,
} from "./data/circuit.js";
import { GateRenderData } from "./renderer/gateRenderData.js";
import { LayoutMap, emptyLayoutMap } from "./renderer/layoutMap.js";
import { Location } from "./data/location.js";
import { ViewState } from "./data/viewState.js";
import {
  gateHeight,
  minGateWidth,
  minToolboxHeight,
  svgNS,
} from "./renderer/constants.js";
import { installEditor } from "./editor/installEditor.js";
import { getOperationRegisters } from "./utils.js";
import type { StateColumn } from "./state-viz/stateViz.js";
import type { PrepareStateVizOptions } from "./state-viz/worker/stateVizPrep.js";

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
  /**
   * Geometry from the layout pass. Captured here so the editor can
   * position dropzones from the same numbers `processOperations`
   * already computed, instead of reverse-engineering them from
   * rendered SVG attributes. See [`layoutMap.ts`](renderer/layoutMap.ts).
   */
  layoutMap: LayoutMap;
}

/**
 * Defines the mapping of unique location to each operation. Used for enabling
 * interactivity.
 */
type GateRegistry = {
  [location: string]: Operation;
};

export type EditorHandlers = {
  editCallback: (circuitGroup: CircuitGroup) => void;
  // When provided, enables the Run button in the toolbox.
  runCallback?: () => void;
  // Optional callback to offload state visualization computation.
  // When provided (e.g., by the VS Code webview), the state visualizer can
  // compute state in a Web Worker without relying on globals.
  computeStateVizColumnsForCircuitModel?: (
    model: Circuit,
    opts?: PrepareStateVizOptions,
  ) => Promise<StateColumn[]>;
};

export type DrawOptions = {
  renderDepth?: number;
  renderLocations?: (l: SourceLocation[]) => { title: string; href: string };
  /**
   * When provided, enables editing behaviors (dropzones, run button, etc.) and
   * requires the callbacks necessary to support those behaviors.
   */
  editor?: EditorHandlers;
  /**
   * When provided, enables zoom-to-fit behavior. The callback is called with the new zoom level whenever it changes.
   */
  onZoomChange?: (zoomLevel: number) => void;
};

/**
 * Entrypoint class for rendering circuit visualizations.
 */
export class Sqore {
  circuit: Circuit;
  gateRegistry: GateRegistry = {};
  renderDepth: number;
  container: HTMLElement | null = null;
  zoomOnResize: boolean = true;
  zoomLevel: number = 100;
  /**
   * Per-session view preferences (e.g. user-toggled expand/collapse
   * state). Survives every `renderCircuit` call but is intentionally
   * NOT serialized into the saved circuit. See
   * [`viewState.ts`](data/viewState.ts).
   */
  readonly viewState: ViewState = new ViewState();
  /**
   * Snapshot of `op object → location string` captured at the end
   * of the most recent render, used to migrate `viewState` keys
   * forward when ops shift position. See
   * [`rebaseViewState`](#method-rebaseViewState).
   *
   * `null` means "no prior render yet" (first draw) or "the prior
   * snapshot is no longer valid" (after `updateCircuit` replaces
   * the underlying tree). In both cases the next render skips the
   * rebase and just refreshes the snapshot.
   */
  private lastLocationMap: Map<Operation, string> | null = null;
  /**
   * Initializes Sqore object.
   *
   * @param circuitGroup Group of circuits to be visualized.
   * @param options Optional rendering/interaction options.
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
    this.renderDepth = options.renderDepth ?? 0;
    // For now we only visualize the first circuit in the group
    this.circuit = this.circuitGroup.circuits[0];
  }

  /**
   * Render circuit into `container` at the specified layer depth.
   *
   * @param container HTML element for rendering visualization into.
   */
  draw(container: HTMLElement) {
    // Inject into container
    if (container == null) throw new Error(`Container not provided.`);

    this.container = container;

    this.renderCircuit(container);

    if (this.options.onZoomChange != null) {
      this.zoomToFit();
      window.addEventListener("resize", () => this.onResize());
    }
  }

  /**
   * Replace the underlying circuit and re-render in place, preserving
   * everything that lives on `this` (most importantly `viewState`,
   * but also the cached container, zoom level, and the editor's
   * disposable event registrations — `installEditor` already disposes
   * the prior `CircuitEvents` on every render).
   *
   * Intended for hosts that receive **external** circuit updates —
   * e.g. the VS Code circuit editor's text-document backing fires an
   * `onDidChangeTextDocument` for undo/redo and external file edits,
   * and the webview parses the new text into a fresh `CircuitGroup`.
   * Without this method the React wrapper was tearing down the SVG
   * and constructing a new `Sqore` for every such update, which
   * destroyed `viewState` (collapsing every user-expanded group) and
   * caused a visible "Rendering..." flicker.
   *
   * Hosts that want a fully clean instance (e.g. opening a different
   * circuit in the same panel) should keep using `qviz.draw(...)`
   * for a fresh `Sqore`.
   *
   * @param circuitGroup The new circuit group to render.
   */
  updateCircuit(circuitGroup: CircuitGroup): void {
    if (
      circuitGroup == null ||
      circuitGroup.circuits == null ||
      circuitGroup.circuits.length === 0
    ) {
      throw new Error(`No circuit found. Please provide a valid circuit.`);
    }
    this.circuitGroup = circuitGroup;
    // We only render the first circuit in the group today; matches
    // the constructor's behavior.
    this.circuit = circuitGroup.circuits[0];
    // External replacement: the new circuit's op object identities
    // have no relation to the prior tree. Drop the rebase snapshot
    // so the next render doesn't try to migrate viewState against
    // stale identities (which would silently drop every entry).
    this.lastLocationMap = null;
    if (this.container != null) {
      this.renderCircuit(this.container);
    }
  }

  /**
   * Window resize handler to recalculate and set the zoom level
   * based on the new window width.
   */
  private onResize() {
    if (!this.zoomOnResize) {
      return;
    }

    // Recalculate the zoom level based on the container width
    this.zoomToFit();
  }

  /**
   * Calculate and set the zoom level to fit the circuit within the container.
   */
  private zoomToFit() {
    if (this.options.onZoomChange == null || this.container == null) {
      return;
    }
    const zoomLevel = this.calculateZoomToFit(this.container);
    this.updateZoomLevel(zoomLevel);
    this.options.onZoomChange?.(zoomLevel);
  }

  /**
   * Update the zoom level setting and apply it to the SVG element.
   */
  updateZoomLevel(zoomLevel: number) {
    this.zoomLevel = zoomLevel;
    const svg = this.container?.querySelector("svg.qviz");
    if (svg) {
      this.updateSvgWidth(svg as SVGElement, zoomLevel);
    }
  }

  /**
   * Update the width of the SVG element based on the zoom level.
   */
  updateSvgWidth(svg: SVGElement, zoomLevel: number) {
    // The width attribute contains the true width.
    // We'll leave this attribute untouched, so we can use it again if the
    // zoom level is ever updated.
    const width = svg.getAttribute("width")!;

    // We'll set the width in the style attribute to (true width * zoom level).
    // This value takes precedence over the true width in the width attribute.
    svg.setAttribute(
      "style",
      `max-width: ${width}; width: ${(parseInt(width) * (zoomLevel || 100)) / 100}; height: auto`,
    );
  }

  /**
   * Calculate the zoom level that will fit the circuit into the current size of the container.
   */
  calculateZoomToFit(container: HTMLElement): number {
    const svg = container.querySelector("svg.qviz") as SVGElement;
    const containerWidth = container.clientWidth;
    // width and height are the true dimensions generated by qviz
    const width = parseInt(svg.getAttribute("width")!);
    const height = svg.getAttribute("height")!;

    svg.setAttribute("viewBox", `0 0 ${width} ${height}`);
    const zoom = Math.min(Math.ceil((containerWidth / width) * 100), 100);
    return zoom;
  }

  /**
   * Render circuit into `container`.
   *
   * Always deep-copies `this.circuit` so the rendered grid can be
   * mutated freely (location stamps, default-expand flags, ViewState
   * overrides) without touching the saved circuit. The previous
   * "reuse the deep copy" overload existed only to keep chevron-click
   * mutations alive across one render; that bookkeeping now lives in
   * `this.viewState` and the workaround is gone.
   *
   * @param container HTML element for rendering visualization into.
   */
  private renderCircuit(container: HTMLElement): void {
    // Migrate viewState keys to track ops whose locations shifted
    // due to mutations between renders (drag-and-drop, gate insert,
    // qubit-line edits, etc.). MUST run BEFORE the deep copy below,
    // because the rebase compares op object identities against the
    // live `this.circuit.componentGrid` — the JSON copy would break
    // that identity link.
    this.rebaseViewState();

    // Create copy of circuit to prevent mutation
    const _circuit: Circuit = JSON.parse(JSON.stringify(this.circuit));

    // Assign unique locations to each operation
    _circuit.componentGrid.forEach((col, colIndex) =>
      col.components.forEach((op, i) =>
        this.fillGateRegistry(op, Location.root().child(colIndex, i)),
      ),
    );

    // Apply default-expansion passes first — these match the original
    // behavior for any op without an explicit user choice.
    this.expandOperationsToDepth(_circuit.componentGrid, this.renderDepth);
    this.expandIfSingleOperation(_circuit.componentGrid);

    // Apply user view-state overrides on top. Anything the user has
    // explicitly expanded or collapsed wins over the defaults.
    this.viewState.applyTo(_circuit.componentGrid);

    // Create visualization components
    const composedSqore: ComposedSqore = this.compose(_circuit);
    const svg: SVGElement = this.generateSvg(composedSqore);
    this.setViewBox(svg);
    if (this.options.onZoomChange != null) {
      this.updateSvgWidth(svg, this.zoomLevel);
    }
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
    this.addGateClickHandlers(container);

    const editor = this.options.editor;
    const isEditable = editor != null;
    if (isEditable) {
      installEditor(container, this, composedSqore.layoutMap, editor, () =>
        this.renderCircuit(container),
      );
    }

    // Snapshot the live op → location map for the next render's
    // rebase. Built from `this.circuit` (the live model), NOT the
    // deep copy, so the op object identities here match the ones
    // the editor's mutations will operate on between now and the
    // next render.
    this.lastLocationMap = this.buildLiveLocationMap(
      this.circuit.componentGrid,
    );
  }

  /**
   * Walk `grid` in render order (the same `Location.root().child(...)`
   * scheme `fillGateRegistry` uses) and build a map from each op
   * object reference to its current location string.
   *
   * Walks the live model — callers must NOT pass a deep copy, since
   * identity-based lookups are the point.
   */
  private buildLiveLocationMap(grid: ComponentGrid): Map<Operation, string> {
    const map = new Map<Operation, string>();
    const walk = (g: ComponentGrid, parent: Location): void => {
      g.forEach((col, colIndex) =>
        col.components.forEach((op, opIndex) => {
          const loc = parent.child(colIndex, opIndex);
          map.set(op, loc.toString());
          if (op.children != null) {
            walk(op.children, loc);
          }
        }),
      );
    };
    walk(grid, Location.root());
    return map;
  }

  /**
   * Migrate `viewState` keys forward across mutations that may have
   * shifted ops to new locations.
   *
   * Uses object identity against `this.lastLocationMap` (captured
   * at the end of the previous render) so that user expand/collapse
   * choices "follow" their op when the op's string location number
   * changes — e.g. dragging a gate into column 0 shifts every other
   * op's column index by 1, and any user-expanded group needs its
   * viewState entry rekeyed from `"<oldCol>,<op>"` to
   * `"<oldCol+1>,<op>"` so it stays expanded.
   *
   * No-op on the first render (no prior snapshot) and after
   * `updateCircuit` invalidates the snapshot. The rebase logic
   * itself lives in [`ViewState.rebase`](data/viewState.ts).
   */
  private rebaseViewState(): void {
    const prev = this.lastLocationMap;
    if (prev == null) return;
    const next = this.buildLiveLocationMap(this.circuit.componentGrid);
    // For every op we tracked at the last render, compute its old
    // and new location. Build the (oldLoc → newLoc | null) remap
    // that `ViewState.rebase` consumes.
    const remap = new Map<string, string | null>();
    for (const [op, oldLoc] of prev) {
      const newLoc = next.get(op);
      remap.set(oldLoc, newLoc ?? null);
    }
    this.viewState.rebase(remap);
  }

  private expandOperationsToDepth(
    componentGrid: ComponentGrid,
    targetDepth: number,
    currentDepth: number = 0,
  ) {
    for (const col of componentGrid) {
      for (const op of col.components) {
        if (currentDepth < targetDepth && op.children != null) {
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
        onlyComponent.dataAttributes["expanded"] !== "false" &&
        onlyComponent.children != null
      ) {
        // We already have the only-component in hand, so set the
        // attr directly rather than walking the grid for it.
        onlyComponent.dataAttributes["expanded"] = "true";
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

    const isEditable = this.options.editor != null;

    // Draw the qubit labels.
    // Also calculate other register render data to be used later in the rendering.
    const { qubitLabels, registers, svgHeight } = formatInputs(
      qubits,
      rowHeights,
      isEditable ? undefined : this.options.renderLocations,
    );

    // Calculate the render data for the operations.
    const topY = qubits[0] ? registers[qubits[0].id].y : -1;
    const bottomY = qubits[qubits.length - 1]
      ? registers[qubits[qubits.length - 1].id].y
      : -1;
    const { renderDataArray, svgWidth, localScope, childScopes } =
      processOperations(
        componentGrid,
        topY,
        bottomY,
        registers,
        isEditable ? undefined : this.options.renderLocations,
      );

    // Assemble the LayoutMap from the layout pass.
    //
    // - The top-level scope is keyed by `""` (matches the existing
    //   `LayoutMap` convention; see [`layoutMap.ts`](renderer/layoutMap.ts)).
    // - `childScopes` is already keyed by each parent op's location
    //   string, with absolute coords.
    // - `wireYs` mirrors the y-coords of the real qubit wires before
    //   any editor chrome (e.g. the ghost qubit wire) is added.
    const layoutMap: LayoutMap = emptyLayoutMap();
    layoutMap.scopes.set("", localScope);
    for (const [key, scope] of childScopes) {
      layoutMap.scopes.set(key, scope);
    }
    layoutMap.wireYs = qubits.map((q) => registers[q.id].y);

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
      layoutMap,
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
   * Depth-first traversal to assign a unique location string to
   * `operation`. The operation is assigned `location.toString()` and
   * its `i`th child in its `colIndex` column is recursively given
   * `location.child(colIndex, i)`.
   *
   * Takes a [`Location`](data/location.ts) value rather than a raw
   * string, so the addressing format is owned by exactly one module.
   * The string form is still what gets stored in
   * `dataAttributes["location"]` / used as `gateRegistry` keys, since
   * the rest of the codebase reads those as strings.
   *
   * @param operation Operation to be assigned.
   * @param location  Hierarchical location to assign to `operation`.
   */
  private fillGateRegistry(operation: Operation, location: Location): void {
    if (operation.dataAttributes == null) operation.dataAttributes = {};
    const locationStr = location.toString();
    operation.dataAttributes["location"] = locationStr;

    // Note: `dataAttributes["expanded"]` is intentionally not defaulted here.
    // Expansion is controlled by:
    // - `renderDepth` (see `expandOperationsToDepth`),
    // - user interaction (expand/collapse), and
    // - `expandIfSingleOperation`, which auto-expands a single top-level op
    //   unless it has been explicitly collapsed.
    this.gateRegistry[locationStr] = operation;
    operation.children?.forEach((col, colIndex) =>
      col.components.forEach((childOp, i) => {
        this.fillGateRegistry(childOp, location.child(colIndex, i));
      }),
    );
  }

  /**
   * Add interactive click handlers to circuit HTML elements.
   *
   * @param container HTML element containing visualized circuit.
   *
   */
  private addGateClickHandlers(container: HTMLElement): void {
    this.addZoomHandlers(container);
  }

  /**
   * Add interactive click handlers for expand/collapse functionality.
   *
   * Each chevron click writes the user's choice into `this.viewState`
   * and then re-renders. ViewState survives the deep-copy that happens
   * inside `renderCircuit`, so the choice persists across editor
   * mutations rather than being lost on the next refresh.
   *
   * @param container HTML element containing visualized circuit.
   */
  private addZoomHandlers(container: HTMLElement): void {
    container.querySelectorAll(".gate .gate-control").forEach((ctrl) => {
      // Zoom in on clicked gate
      ctrl.addEventListener("click", (ev: Event) => {
        const gateId: string | null | undefined =
          ctrl.parentElement?.getAttribute("data-location");
        if (typeof gateId == "string") {
          if (ctrl.classList.contains("gate-collapse")) {
            this.viewState.setExpanded(gateId, false);
          } else if (ctrl.classList.contains("gate-expand")) {
            this.viewState.setExpanded(gateId, true);
          }
          this.zoomOnResize = false;
          this.renderCircuit(container);

          ev.stopPropagation();
        }
      });
    });
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
 * `heightAboveFirstClassical` is similar but applies to the gap between a
 * qubit's wire and its first classical sub-wire — used to reserve room for
 * the label of any classically-controlled group whose box top sits in that
 * gap (the producing measurement's classical sub-wire is the group's
 * `controlY`, and the label lives just above it).
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
    heightAboveFirstClassical: number;
    bottomBordersAboveFirstClassical: number;
  };
} {
  const rowHeights: {
    [qubitIndex: number]: {
      currentGroupBordersAboveWire: number;
      currentGroupBordersBelowWire: number;
      currentClassicalGroupsAboveFirstClassical: number;
      currentBottomBordersAboveFirstClassical: number;
      heightAboveWire: number;
      heightBelowWire: number;
      heightAboveFirstClassical: number;
      bottomBordersAboveFirstClassical: number;
    };
  } = {};

  const numResultsByQubit: { [qubitIndex: number]: number } = {};
  for (const q of qubits) {
    const { id } = q;
    rowHeights[id] = {
      currentGroupBordersBelowWire: 0,
      currentGroupBordersAboveWire: 0,
      currentClassicalGroupsAboveFirstClassical: 0,
      currentBottomBordersAboveFirstClassical: 0,
      heightBelowWire: 0,
      heightAboveWire: 0,
      heightAboveFirstClassical: 0,
      bottomBordersAboveFirstClassical: 0,
    };
    numResultsByQubit[id] = q.numResults ?? 0;
  }

  updateRowHeights(componentGrid, rowHeights, numResultsByQubit);
  return rowHeights;
}

function updateRowHeights(
  componentGrid: ComponentGrid,
  rowHeights: {
    [qubitIndex: number]: {
      currentGroupBordersAboveWire: number;
      currentGroupBordersBelowWire: number;
      currentClassicalGroupsAboveFirstClassical: number;
      currentBottomBordersAboveFirstClassical: number;
      heightAboveWire: number;
      heightBelowWire: number;
      heightAboveFirstClassical: number;
      bottomBordersAboveFirstClassical: number;
    };
  },
  numResultsByQubit: { [qubitIndex: number]: number },
) {
  for (const col of componentGrid) {
    for (const component of col.components) {
      if (isExpandedGroup(component)) {
        // The group's dashed box top is anchored at the topmost
        // reg's y (over targets ∪ controls), and the bottom at
        // the bottommost reg's y. Decide which row-height counter
        // each border should bump by asking *what kind of layout
        // row that y lands in*. The same geometric rule applies
        // to both top and bottom:
        //
        //   - Anchor is a pure qubit ref `{q}` AND q has no
        //     classical sub-wires → border lives in the gap
        //     immediately above (or below) q's wire →
        //     `heightAboveWire[q]` / `heightBelowWire[q]`.
        //   - Anchor is a pure qubit ref `{q}` AND q has
        //     classical sub-wires → for the TOP, border is above
        //     q's wire (`heightAboveWire`); for the BOTTOM,
        //     border lives just below q's wire which lands in
        //     the gap between q's wire and its first classical
        //     sub-wire → `bottomBordersAboveFirstClassical[q]`.
        //   - Anchor is a classical sub-wire ref `{q, r}` → for
        //     the TOP, border is between q's wire and its first
        //     classical sub-wire → `heightAboveFirstClassical[q]`;
        //     for the BOTTOM, border is after all of q's
        //     classical sub-wires → `heightBelowWire[q]`.
        //
        // Note the two distinct counters for the "above first
        // classical" gap: top borders carry labels and stack at
        // `groupTopPadding` (26 px) per level, while bottom
        // borders have no label and stack at `groupBottomPadding`
        // (10 px) per level (set by `_processChildren`'s padding
        // chain). The formatter applies the two with their own
        // multipliers.
        const regs = getOperationRegisters(component);
        if (regs.length === 0) continue;

        const qubits = regs.map((r) => r.qubit);
        const minQubit = Math.min(...qubits);
        const maxQubit = Math.max(...qubits);

        // For minQubit: the *topmost* anchor ref is a pure qubit
        // ref if one exists on minQubit (pure refs sit above any
        // classical sub-wires); otherwise it's a classical ref.
        const minQubitHasPureRef = regs.some(
          (r) => r.qubit === minQubit && r.result == null,
        );

        // For maxQubit: the *bottommost* anchor ref is a classical
        // ref if any exist on maxQubit (classical sub-wires sit
        // below the qubit wire); otherwise it's the pure qubit ref.
        const maxQubitHasClassicalRef = regs.some(
          (r) => r.qubit === maxQubit && r.result != null,
        );

        // Track which counters we bumped so we can decrement
        // after recursion.
        let bumpedAboveWireQ: number | null = null;
        let bumpedTopFirstClassicalQ: number | null = null;
        let bumpedBottomFirstClassicalQ: number | null = null;
        let bumpedBelowWireQ: number | null = null;

        // Top border placement
        if (minQubitHasPureRef) {
          rowHeights[minQubit].currentGroupBordersAboveWire++;
          rowHeights[minQubit].heightAboveWire = Math.max(
            rowHeights[minQubit].heightAboveWire,
            rowHeights[minQubit].currentGroupBordersAboveWire,
          );
          bumpedAboveWireQ = minQubit;
        } else {
          rowHeights[minQubit].currentClassicalGroupsAboveFirstClassical++;
          rowHeights[minQubit].heightAboveFirstClassical = Math.max(
            rowHeights[minQubit].heightAboveFirstClassical,
            rowHeights[minQubit].currentClassicalGroupsAboveFirstClassical,
          );
          bumpedTopFirstClassicalQ = minQubit;
        }

        // Bottom border placement
        if (
          !maxQubitHasClassicalRef &&
          (numResultsByQubit[maxQubit] ?? 0) > 0
        ) {
          // Bottom anchor is a pure qubit ref on a qubit that has
          // classical sub-wires below it. The border y sits in
          // the gap between maxQubit's wire and its first
          // classical sub-wire, but unlike top borders has no
          // label, so it uses the smaller bottom-border counter.
          rowHeights[maxQubit].currentBottomBordersAboveFirstClassical++;
          rowHeights[maxQubit].bottomBordersAboveFirstClassical = Math.max(
            rowHeights[maxQubit].bottomBordersAboveFirstClassical,
            rowHeights[maxQubit].currentBottomBordersAboveFirstClassical,
          );
          bumpedBottomFirstClassicalQ = maxQubit;
        } else {
          rowHeights[maxQubit].currentGroupBordersBelowWire++;
          rowHeights[maxQubit].heightBelowWire = Math.max(
            rowHeights[maxQubit].heightBelowWire,
            rowHeights[maxQubit].currentGroupBordersBelowWire,
          );
          bumpedBelowWireQ = maxQubit;
        }

        // recurse
        updateRowHeights(
          component.children || [],
          rowHeights,
          numResultsByQubit,
        );

        // decrement (mirror the bumps above)
        if (bumpedAboveWireQ != null) {
          rowHeights[bumpedAboveWireQ].currentGroupBordersAboveWire--;
        }
        if (bumpedTopFirstClassicalQ != null) {
          rowHeights[bumpedTopFirstClassicalQ]
            .currentClassicalGroupsAboveFirstClassical--;
        }
        if (bumpedBottomFirstClassicalQ != null) {
          rowHeights[bumpedBottomFirstClassicalQ]
            .currentBottomBordersAboveFirstClassical--;
        }
        if (bumpedBelowWireQ != null) {
          rowHeights[bumpedBelowWireQ].currentGroupBordersBelowWire--;
        }
      }
    }
  }
}

/**
 * An "expanded group" here is any operation that is to be rendered showing
 * its children, with a dashed box around the children.
 */
export function isExpandedGroup(component: Operation) {
  const expandedAttr = component.dataAttributes?.["expanded"];
  if (expandedAttr != null) {
    return expandedAttr === "true";
  }

  const hasChildren =
    component.children != null && component.children.length > 0;
  const hasClassicalControls =
    component.kind === "unitary" &&
    (((component.controls ?? []).some((reg) => reg.result != null) ?? false) ||
      (component.metadata?.controlResultIds?.length ?? 0) > 0);

  // Classically controlled groups default to expanded when not explicitly set.
  return hasChildren && hasClassicalControls;
}
