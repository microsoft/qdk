// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { Circuit, ComponentGrid, Qubit, Unitary } from "../data/circuit.js";
import { CircuitModel } from "../data/circuitModel.js";
import { DragController } from "./controllers/dragController.js";
import { InteractionContext } from "./controllers/interactionContext.js";
import { InteractionState } from "../actions/interactionState.js";
import { KeyboardController } from "./controllers/keyboardController.js";
import { LayoutMap } from "../renderer/layoutMap.js";
import { QubitController } from "./controllers/qubitController.js";
import { SelectionController } from "./controllers/selectionController.js";
import { Sqore } from "../sqore.js";
import { getWireData } from "./domUtils.js";

let events: CircuitEvents | null = null;
let currentCircuitSvg: SVGElement | null = null;

/**
 * Creates and attaches the events that allow editing of the circuit.
 *
 * @param container     HTML element for rendering visualization into
 * @param sqore         Sqore object
 * @param layoutMap     Geometry from the layout pass
 */
const enableEvents = (
  container: HTMLElement,
  sqore: Sqore,
  layoutMap: LayoutMap,
  useRefresh: () => void,
): void => {
  if (events != null) {
    events.dispose();
  }
  events = new CircuitEvents(container, sqore, layoutMap, useRefresh);

  // Lets other modules avoid reading a stale model during a re-render where the SVG was replaced
  // but enableEvents hasn't run yet.
  currentCircuitSvg = container.querySelector("svg.qviz") as SVGElement | null;

  // Signal that the model is ready so state-viz can re-render without polling.
  try {
    const CustomEventCtor = (globalThis as any).CustomEvent as
      | (new (type: string, init?: CustomEventInit) => CustomEvent)
      | undefined;
    if (CustomEventCtor && typeof container.dispatchEvent === "function") {
      container.dispatchEvent(
        new CustomEventCtor("qsharp:circuit:modelReady", { bubbles: true }),
      );
    }
  } catch {
    // ignore
  }
};

/**
 * `CircuitEvents` — thin coordinator for the editor's View layer.
 *
 * Pure wiring: builds the shared `InteractionContext` and instantiates one focused controller per
 * slice of pointer / keyboard interaction. Controllers own their listeners and lifecycle;
 * `dispose()` chains through to them. The event logic lives in `controllers/`.
 *
 * Compatibility shims kept on this class:
 *
 * - `componentGrid` / `qubits` / `qubitUseCounts` getters delegate to `model` for
 *   `getCurrentCircuitModel` and `contextMenu.ts`.
 * - `_startAddingControl` / `_startRemovingControl` delegate to the drag controller so the context
 *   menu can invoke them by name.
 */
class CircuitEvents {
  /** The Data layer. See [circuitModel.ts](../data/circuitModel.ts). */
  readonly model: CircuitModel;
  /** Ephemeral session state. See [interactionState.ts](../actions/interactionState.ts). */
  readonly interaction: InteractionState = new InteractionState();
  readonly renderFn: () => void;

  /** Convenience — read by `getCurrentCircuitModel` + `contextMenu.ts`. */
  get componentGrid(): ComponentGrid {
    return this.model.componentGrid;
  }
  get qubits(): Qubit[] {
    return this.model.qubits;
  }
  get qubitUseCounts(): number[] {
    return this.model.qubitUseCounts;
  }

  private readonly keyboard: KeyboardController;
  private readonly drag: DragController;
  // Held only because DragController injects it for the qubit-drag-out-delete path.
  private readonly qubit: QubitController;
  // No public methods; exists to install host-element listeners on construction.
  private readonly selection: SelectionController;

  constructor(
    container: HTMLElement,
    sqore: Sqore,
    layoutMap: LayoutMap,
    useRefresh: () => void,
  ) {
    this.renderFn = useRefresh;
    this.model = new CircuitModel(sqore.circuit);
    this.model.removeTrailingUnusedQubits();

    const circuitSvg = container.querySelector("svg.qviz") as SVGElement;
    // The editor overlay (and its dropzone + ghost-qubit sub-layers) is built by createDropzones
    // before enableEvents runs.
    const overlayLayer = container.querySelector(
      ".editor-overlay",
    ) as SVGGElement;
    const dropzoneLayer = container.querySelector(
      ".dropzone-layer",
    ) as SVGGElement;
    const ghostQubitLayer = container.querySelector(
      ".ghost-qubit-layer",
    ) as SVGGElement;

    if (this.qubits.length === 0) {
      ghostQubitLayer.style.display = "block";
    }

    // Build the shared context once and hand it to every controller. `wireData` is read from the
    // DOM since the ghost wire's y is only available there at this point.
    const ctx: InteractionContext = {
      model: this.model,
      interaction: this.interaction,
      layoutMap,
      container,
      circuitSvg,
      overlayLayer,
      dropzoneLayer,
      ghostQubitLayer,
      wireData: getWireData(container),
      renderFn: useRefresh,
    };

    // Order matters: DragController takes the QubitController.
    this.qubit = new QubitController(ctx);
    this.drag = new DragController(ctx, this.qubit);
    this.keyboard = new KeyboardController(ctx);
    this.selection = new SelectionController(ctx, this);
  }

  /** Disposes the controllers that own document-level listeners. */
  dispose() {
    this.keyboard.dispose();
    this.drag.dispose();
  }

  /**
   * Begin the wire-pick add-control flow. Delegates to the drag controller; kept here because
   * `contextMenu.ts` invokes it by name.
   */
  _startAddingControl(selectedOperation: Unitary) {
    this.drag.startAddingControl(selectedOperation);
  }

  /**
   * Begin the wire-pick remove-control flow. Delegates to the drag controller; kept here because
   * `contextMenu.ts` invokes it by name.
   */
  _startRemovingControl(selectedOperation: Unitary) {
    this.drag.startRemovingControl(selectedOperation);
  }
}

export { enableEvents, CircuitEvents };

// Returns the current circuit model, but only if it matches the currently-rendered SVG — prevents
// state-viz from computing against a previous render's model mid-re-render.
export function getCurrentCircuitModel(
  expectedSvg?: SVGElement | null,
): Circuit | null {
  if (events == null) return null;
  if (expectedSvg && currentCircuitSvg && expectedSvg !== currentCircuitSvg) {
    return null;
  }
  return { qubits: events.qubits, componentGrid: events.componentGrid };
}
