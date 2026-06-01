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
import { getWireData } from "../utils.js";

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

  // Track which rendered SVG the current `events` instance is associated with.
  // This lets other modules avoid reading a stale model during a re-render where
  // the SVG has been replaced but `enableEvents` hasn't run yet.
  currentCircuitSvg = container.querySelector("svg.qviz") as SVGElement | null;

  // Signal that the circuit model (events + model snapshot) is now ready.
  // The state visualization uses this to re-render without relying on polling.
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
 * This class is **only** wiring: it builds the `InteractionContext`
 * (shared deps for every controller) and instantiates a focused
 * controller for each slice of pointer / keyboard interaction.
 * Controllers own their own listeners and lifecycle; `dispose()`
 * here just chains through to them.
 *
 * The actual event logic lives in:
 *
 * - [keyboardController.ts](keyboardController.ts) — Ctrl-toggle
 *   between move and copy modes.
 * - [selectionController.ts](selectionController.ts) — host-element
 *   mousedown (sets `selectedWire` / `movingControl`) and the
 *   context-menu attachment.
 * - [dragController.ts](dragController.ts) — the gate-drag surface:
 *   gate-element mousedown, toolbox mousedown, dropzone mouseup,
 *   document-level cleanup, ghost element creation, and the
 *   wire-pick add-control / remove-control flow that the context
 *   menu invokes.
 * - [qubitController.ts](qubitController.ts) — qubit-label drag
 *   and `removeQubitLineWithConfirmation`.
 * - [scrollController.ts](scrollController.ts) — the auto-scroll
 *   function shared by gate-drag and qubit-drag.
 *
 * Compatibility shims kept on this class:
 *
 * - `componentGrid` / `qubits` / `qubitUseCounts` getters delegate
 *   to `model` so `getCurrentCircuitModel` and the
 *   `contextMenu.ts` consumer keep working unchanged.
 * - `_startAddingControl` / `_startRemovingControl` delegate to
 *   the drag controller so the context menu can keep invoking them
 *   by name. These will go away once `addContextMenuToHostElem`
 *   itself is migrated to a controller-shaped API.
 *
 */
class CircuitEvents {
  /** The Data layer. See [circuitModel.ts](circuitModel.ts). */
  readonly model: CircuitModel;
  /** Ephemeral session state. See [interactionState.ts](interactionState.ts). */
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
  // QubitController is held even though `CircuitEvents` doesn't
  // call into it directly — DragController references it via
  // constructor injection for the qubit-drag-out-delete path.
  private readonly qubit: QubitController;
  // SelectionController has no public methods; it only exists to
  // install host-element listeners on construction.
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
    // Every editor-only DOM node lives inside this overlay group.
    // createDropzones builds it, the dropzone + ghost-qubit sub-layers,
    // and attaches it to circuitSvg before enableEvents runs — so it's
    // always present here.
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

    // Build the shared context once and hand it to every controller.
    // `wireData` mirrors the layout pass's `wireYs` plus the trailing
    // ghost wire; the DOM is the only source for the ghost-wire's y
    // at this point (it was just inserted), so we read via getWireData.
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
   * Begin the wire-pick add-control flow. Delegates to the drag
   * controller; kept on `CircuitEvents` because `contextMenu.ts`
   * still invokes it by name.
   */
  _startAddingControl(selectedOperation: Unitary) {
    this.drag.startAddingControl(selectedOperation);
  }

  /**
   * Begin the wire-pick remove-control flow. Delegates to the drag
   * controller; kept on `CircuitEvents` for the same reason as
   * `_startAddingControl`.
   */
  _startRemovingControl(selectedOperation: Unitary) {
    this.drag.startRemovingControl(selectedOperation);
  }
}

export { enableEvents, CircuitEvents };

// Provide access to the current circuit model, but only if it matches the
// currently-rendered SVG element. This prevents state visualization from
// computing against the previous render's model during a re-render.
export function getCurrentCircuitModel(
  expectedSvg?: SVGElement | null,
): Circuit | null {
  if (events == null) return null;
  if (expectedSvg && currentCircuitSvg && expectedSvg !== currentCircuitSvg) {
    return null;
  }
  return { qubits: events.qubits, componentGrid: events.componentGrid };
}
