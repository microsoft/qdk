// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import {
  Circuit,
  CircuitGroup,
  ComponentGrid,
  CURRENT_VERSION,
  Operation,
  SourceLocation,
} from "./data/circuit.js";
import { Location } from "./data/location.js";
import { ViewState } from "./data/viewState.js";
import {
  gateHeight,
  minGateWidth,
  minToolboxHeight,
} from "./renderer/constants.js";
import { toDomSvgElement } from "./renderer/svg.js";
import { installEditor } from "./editor/installEditor.js";
import type { StateColumn } from "./state-viz/stateViz.js";
import type { PrepareStateVizOptions } from "./state-viz/worker/stateVizPrep.js";
import { createStandaloneCircuitSvg } from "./renderer/circuitSvgDocument.js";
import { renderCircuit as renderCircuitSvgTree } from "./renderer/circuitRenderer.js";
import {
  type CircuitRendererSvgOptions,
  validateCircuitRendererSvgOptions,
} from "./renderer/circuitSvgOptions.js";

function registerSemanticKey(register: {
  qubit: number;
  result?: number;
}): [number, number | null] {
  return [register.qubit, register.result ?? null];
}

type LocatedOperation = {
  operation: Operation;
  location: string;
  semanticKey: string;
  subtreeKey: number;
};

type LocatedOperationQueue = {
  operations: LocatedOperation[];
  index: number;
};

type ReplacementCandidates = {
  fallback: LocatedOperationQueue;
  bySubtreeKey: Map<number, LocatedOperationQueue>;
  used: Set<LocatedOperation>;
};

type OperationSubtreeKey = (operation: Operation) => number;

export type EditorHandlers = {
  editCallback: (circuitGroup: CircuitGroup) => void;
  // When provided, enables the Run button in the toolbox.
  runCallback?: () => void;
  // Optional callback to offload state visualization computation. When provided (e.g., by the VS
  // Code webview), the state visualizer can compute state in a Web Worker without relying on
  // globals.
  computeStateVizColumnsForCircuitModel?: (
    model: Circuit,
    opts?: PrepareStateVizOptions,
  ) => Promise<StateColumn[]>;
};

export type DrawOptions = {
  renderDepth?: number;
  renderLocations?: (l: SourceLocation[]) => { title: string; href: string };
  /**
   * When provided, enables editing behaviors (dropzones, run button, etc.) and requires the
   * callbacks necessary to support those behaviors.
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
  renderDepth: number;
  container: HTMLElement | null = null;
  zoomOnResize: boolean = true;
  zoomLevel: number = 100;
  /**
   * Per-session view preferences (e.g. user-toggled expand/collapse state). Survives every
   * `renderCircuit` call but is intentionally NOT serialized into the saved circuit. See
   * [`viewState.ts`](data/viewState.ts).
   */
  readonly viewState: ViewState = new ViewState();
  /**
   * Snapshot of `op object → location string` captured at the end of the most recent render, used
   * to migrate `viewState` keys forward when ops shift position. See `rebaseViewState`.
   *
   * `null` means "no prior render yet" (first draw) or "the prior snapshot is no longer valid"
   * (after `updateCircuit` replaces the underlying tree and separately rebases the view state).
   * In both cases the next render skips the identity-based rebase and just refreshes the snapshot.
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

  /** Render the current circuit model as a standalone SVG document. */
  exportSvg(options: CircuitRendererSvgOptions = {}): string {
    const renderDepth = options.renderDepth ?? this.renderDepth;
    validateCircuitRendererSvgOptions({ ...options, renderDepth });
    const rendered = renderCircuitSvgTree(this.circuit, {
      renderDepth,
      expansion: options.expansion ?? "current",
      applyCurrentExpansion: (grid) => this.viewState.applyTo(grid),
    });
    return createStandaloneCircuitSvg(rendered.svg, options);
  }

  /**
   * Replace the underlying circuit and re-render in place, preserving everything that lives on
   * `this` (most importantly `viewState`, but also the cached container, zoom level, and the
   * editor's event registrations).
   *
   * Intended for hosts that receive **external** circuit updates — e.g. the VS Code editor parsing
   * an `onDidChangeTextDocument` into a fresh `CircuitGroup`. Using this instead of a new `Sqore`
   * preserves `viewState` (so user-expanded groups stay expanded) and avoids a re-render flicker.
   *
   * Hosts that want a fully clean instance (e.g. opening a different circuit in the same panel)
   * should keep using `qviz.draw(...)`.
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
    const nextCircuit = circuitGroup.circuits[0];
    this.rebaseViewState();
    this.rebaseViewStateForReplacement(
      this.circuit.componentGrid,
      nextCircuit.componentGrid,
    );
    this.circuitGroup = circuitGroup;
    // We only render the first circuit in the group today; matches the constructor's behavior.
    this.circuit = nextCircuit;
    // The semantic replacement rebase above has already migrated viewState. Reset the
    // identity-based snapshot because the new tree contains fresh operation objects.
    this.lastLocationMap = null;
    if (this.container != null) {
      this.renderCircuit(this.container);
    }
  }

  /**
   * Window resize handler to recalculate and set the zoom level based on the new window width.
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
    // The width attribute contains the true width. We'll leave this attribute untouched, so we can
    // use it again if the zoom level is ever updated.
    const width = svg.getAttribute("width")!;

    // We'll set the width in the style attribute to (true width * zoom level). This value takes
    // precedence over the true width in the width attribute.
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
   * Always deep-copies `this.circuit` so the rendered grid can be mutated freely (location stamps,
   * default-expand flags, ViewState overrides) without touching the saved circuit.
   *
   * @param container HTML element for rendering visualization into.
   */
  private renderCircuit(container: HTMLElement): void {
    // Migrate viewState keys to track ops whose locations shifted due to mutations between renders
    // (drag-and-drop, gate insert, qubit-line edits, etc.). MUST run BEFORE the deep copy below,
    // because the rebase compares op object identities against the live
    // `this.circuit.componentGrid` — the JSON copy would break that identity link.
    this.rebaseViewState();

    const rendered = renderCircuitSvgTree(this.circuit, {
      renderDepth: this.renderDepth,
      expansion: "current",
      applyCurrentExpansion: (grid) => this.viewState.applyTo(grid),
      renderLocations:
        this.options.editor == null ? this.options.renderLocations : undefined,
    });
    const svg = toDomSvgElement(rendered.svg, container.ownerDocument);
    this.setRendererCssVariables(container.ownerDocument);
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
      installEditor(container, this, rendered.layoutMap, editor, () =>
        this.renderCircuit(container),
      );
    }

    // Snapshot the live op → location map for the next render's rebase. Built from `this.circuit`
    // (the live model), NOT the deep copy, so the op object identities here match the ones the
    // editor's mutations will operate on between now and the next render.
    this.lastLocationMap = this.buildLiveLocationMap(
      this.circuit.componentGrid,
    );
  }

  /**
   * Walk `grid` in render order (the same `Location.root().child(...)` scheme the renderer uses)
   * and build a map from each op object reference to its current location string.
   *
   * Walks the live model — callers must NOT pass a deep copy, since identity-based lookups are the
   * point.
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
   * Rebase view preferences when an external text edit replaces every operation object.
   *
   * Editable hosts keep one Sqore for a document, so equivalent operations are paired by their
   * semantic shape and occurrence within each grid. This preserves expansion choices when an edit
   * inserts or removes neighboring operations while avoiding position-based state leaking onto a
   * different gate. Non-editable hosts use a fresh Sqore for unrelated circuits.
   */
  private rebaseViewStateForReplacement(
    previous: ComponentGrid,
    next: ComponentGrid,
  ): void {
    const remap = new Map<string, string | null>();
    const subtreeKey = this.createOperationSubtreeKey();
    for (const location of this.buildLiveLocationMap(previous).values()) {
      remap.set(location, null);
    }
    this.matchReplacementGrid(
      previous,
      next,
      Location.root(),
      Location.root(),
      remap,
      subtreeKey,
    );
    this.viewState.rebase(remap);
  }

  private matchReplacementGrid(
    previous: ComponentGrid,
    next: ComponentGrid,
    previousParent: Location,
    nextParent: Location,
    remap: Map<string, string | null>,
    subtreeKey: OperationSubtreeKey,
  ): void {
    const previousOperations = this.locatedOperations(
      previous,
      previousParent,
      subtreeKey,
    );
    const nextBySemanticKey = new Map<string, ReplacementCandidates>();
    for (const operation of this.locatedOperations(
      next,
      nextParent,
      subtreeKey,
    )) {
      let candidates = nextBySemanticKey.get(operation.semanticKey);
      if (candidates == null) {
        candidates = {
          fallback: { operations: [], index: 0 },
          bySubtreeKey: new Map(),
          used: new Set(),
        };
        nextBySemanticKey.set(operation.semanticKey, candidates);
      }
      candidates.fallback.operations.push(operation);
      let subtreeQueue = candidates.bySubtreeKey.get(operation.subtreeKey);
      if (subtreeQueue == null) {
        subtreeQueue = { operations: [], index: 0 };
        candidates.bySubtreeKey.set(operation.subtreeKey, subtreeQueue);
      }
      subtreeQueue.operations.push(operation);
    }

    for (const previousOperation of previousOperations) {
      const candidates = nextBySemanticKey.get(previousOperation.semanticKey);
      if (candidates == null) {
        continue;
      }
      const exactQueue = candidates.bySubtreeKey.get(
        previousOperation.subtreeKey,
      );
      const nextOperation =
        this.takeReplacementCandidate(exactQueue, candidates.used) ??
        this.takeReplacementCandidate(candidates.fallback, candidates.used);
      if (nextOperation == null) {
        continue;
      }
      candidates.used.add(nextOperation);

      remap.set(previousOperation.location, nextOperation.location);
      if (
        previousOperation.operation.children != null &&
        nextOperation.operation.children != null
      ) {
        this.matchReplacementGrid(
          previousOperation.operation.children,
          nextOperation.operation.children,
          Location.parse(previousOperation.location),
          Location.parse(nextOperation.location),
          remap,
          subtreeKey,
        );
      }
    }
  }

  private takeReplacementCandidate(
    queue: LocatedOperationQueue | undefined,
    used: ReadonlySet<LocatedOperation>,
  ): LocatedOperation | undefined {
    if (queue == null) {
      return undefined;
    }
    while (
      queue.index < queue.operations.length &&
      used.has(queue.operations[queue.index])
    ) {
      queue.index += 1;
    }
    const operation = queue.operations[queue.index];
    queue.index += 1;
    return operation;
  }

  private locatedOperations(
    grid: ComponentGrid,
    parent: Location,
    subtreeKey: OperationSubtreeKey,
  ): LocatedOperation[] {
    const operations: LocatedOperation[] = [];
    grid.forEach((column, columnIndex) =>
      column.components.forEach((operation, operationIndex) => {
        operations.push({
          operation,
          location: parent.child(columnIndex, operationIndex).toString(),
          semanticKey: this.operationSemanticKey(operation),
          subtreeKey: subtreeKey(operation),
        });
      }),
    );
    return operations;
  }

  private createOperationSubtreeKey(): OperationSubtreeKey {
    const cache = new WeakMap<Operation, number>();
    const canonicalIds = new Map<string, number>();

    const getKey = (operation: Operation): number => {
      const cached = cache.get(operation);
      if (cached !== undefined) {
        return cached;
      }

      const signature = JSON.stringify([
        this.operationSemanticKey(operation),
        operation.children?.map((column) => column.components.map(getKey)) ??
          null,
      ]);
      let key = canonicalIds.get(signature);
      if (key === undefined) {
        key = canonicalIds.size;
        canonicalIds.set(signature, key);
      }
      cache.set(operation, key);
      return key;
    };

    return getKey;
  }

  private operationSemanticKey(operation: Operation): string {
    const common = [
      operation.kind,
      operation.gate,
      operation.args ?? null,
      operation.params?.map(({ name, type }) => [name, type]) ?? null,
      operation.isConditional ?? false,
    ];
    switch (operation.kind) {
      case "unitary":
        return JSON.stringify([
          ...common,
          operation.isAdjoint ?? false,
          operation.targets.map(registerSemanticKey),
          operation.controls?.map((register) => [
            ...registerSemanticKey(register),
            register.inverted ?? false,
          ]) ?? null,
        ]);
      case "measurement":
        return JSON.stringify([
          ...common,
          operation.qubits.map(registerSemanticKey),
          operation.results.map(registerSemanticKey),
        ]);
      case "ket":
        return JSON.stringify([
          ...common,
          operation.targets.map(registerSemanticKey),
        ]);
    }
  }

  /**
   * Migrate `viewState` keys forward across mutations that may have shifted ops to new locations.
   *
   * Uses object identity against `this.lastLocationMap` (captured at the end of the previous
   * render) so user expand/collapse choices follow their op when its string location changes — e.g.
   * dragging a gate into column 0 shifts every other op's column index by 1.
   *
   * No-op on the first render and immediately after `updateCircuit`, which performs a separate
   * semantic rebase before replacing the operation objects. The key rewrite itself lives in
   * [`ViewState.rebase`](data/viewState.ts).
   */
  private rebaseViewState(): void {
    const prev = this.lastLocationMap;
    if (prev == null) return;
    const next = this.buildLiveLocationMap(this.circuit.componentGrid);

    // Build a (prev-location → new-location) fallback map from any ops that carry a
    // `sqore-prev-location` stamp. The stamp is set by [`moveOperation`](actions/circuitActions.ts)
    // when it deep-clones the source op — the clone has a new object identity so the identity
    // lookup against `next` would miss and drop the ViewState entry; the stamp lets us recover the
    // choice by matching on the pre-move location. Consumed (deleted) here so it never leaks into
    // the rendered SVG.
    const prevLocationFallback = new Map<string, string>();
    for (const [op, newLoc] of next) {
      const stamp = op.dataAttributes?.["sqore-prev-location"];
      if (typeof stamp === "string") {
        prevLocationFallback.set(stamp, newLoc);
        delete op.dataAttributes!["sqore-prev-location"];
      }
    }

    // For every op we tracked at the last render, compute its old and new location. Build the
    // (oldLoc → newLoc | null) remap that `ViewState.rebase` consumes.
    const remap = new Map<string, string | null>();
    for (const [op, oldLoc] of prev) {
      const newLoc = next.get(op) ?? prevLocationFallback.get(oldLoc);
      remap.set(oldLoc, newLoc ?? null);
    }
    this.viewState.rebase(remap);
  }

  private setRendererCssVariables(ownerDocument: Document): void {
    ownerDocument.documentElement.style.setProperty(
      "--minToolboxHeight",
      `${minToolboxHeight}px`,
    );
    ownerDocument.documentElement.style.setProperty(
      "--minGateWidth",
      `${minGateWidth}px`,
    );
    ownerDocument.documentElement.style.setProperty(
      "--gateHeight",
      `${gateHeight}px`,
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
   * Each chevron click writes the user's choice into `this.viewState` and then re-renders.
   * ViewState survives the deep-copy that happens inside `renderCircuit`, so the choice persists
   * across editor mutations rather than being lost on the next refresh.
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
    minimizedCircuits.version = CURRENT_VERSION;
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
