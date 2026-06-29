// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { Operation } from "../data/circuit.js";

/**
 * `InteractionState` — ephemeral session state for the circuit editor;
 * the Action layer's state container in the Data / Action / View
 * architecture. Holds the mutable fields read and written by the
 * editor's pointer/keyboard handlers.
 *
 * Kept distinct from [`CircuitModel`](circuitModel.ts) because none of
 * these fields belong in saved circuit JSON.
 *
 * Two field lifetimes are mixed here:
 *
 *   - **Persistent** — `selectedOperation` survives the
 *     `resetTransient` reset on mouseup, so the context menu can find
 *     its target after opening. Cleared explicitly by callers.
 *   - **Transient** — `selectedWire`, `movingControl`,
 *     `mouseUpOnCircuit`, `dragging`, `disableLeftAutoScroll`,
 *     `temporaryDropzones`. Owned by the in-flight gesture; reset
 *     between drags.
 *
 * Fields are public for direct read/write from event handlers in
 * `events.ts`. Mutations with non-trivial logic go through
 * [interactionActions.ts](interactionActions.ts) so that
 * `CircuitEvents` doesn't need to know how to e.g. tear down
 * temporary dropzones.
 *
 * No methods on this class: it's pure data paired with the free
 * functions in `interactionActions.ts` (mirroring `CircuitModel` +
 * `circuitActions.ts`), which keeps unit tests trivial.
 */
export class InteractionState {
  /**
   * The operation the user last clicked / mousedown'd. Persistent
   * across `resetTransient` so the context menu can use it. Cleared
   * explicitly by callers (toolbox-drop completion, qubit-line drag
   * start, control-add/remove completion) when no longer relevant.
   */
  selectedOperation: Operation | null = null;

  /**
   * The wire index the user mousedown'd on. Transient — used during
   * a single drag gesture to know which wire of a multi-target gate
   * is being grabbed (and is therefore exempt from getting a
   * temporary dropzone of its own). Cleared on every mouseup.
   */
  selectedWire: number | null = null;

  /**
   * `true` when the dragged element is a control dot (vs. a target
   * box). Drives whether `dropzoneMouseupHandler` calls
   * `addControl`/`removeControl` semantics or the regular
   * move-operation path. Transient — reset on mouseup.
   */
  movingControl: boolean = false;

  /**
   * `true` once a mouseup is received over the circuit SVG itself
   * (vs. outside the canvas). Used by `documentMouseupHandler` to
   * distinguish "dropped on canvas" from "dragged out and dropped"
   * (which triggers delete-on-release). Transient.
   */
  mouseUpOnCircuit: boolean = false;

  /**
   * `true` while a drag is in progress. Transient — set by
   * `_createGhostElement` when the ghost appears, cleared on mouseup.
   */
  dragging: boolean = false;

  /**
   * One-shot flag suppressing left-edge auto-scroll for the current
   * drag. Set when a toolbox drag starts inside the toolbox panel
   * (whose right edge is near the canvas's left edge), so the user
   * doesn't get a runaway scroll while still over the toolbox. The
   * flag clears itself once the cursor moves comfortably past the
   * left auto-scroll threshold (see `_enableAutoScroll`). Transient.
   */
  disableLeftAutoScroll: boolean = false;

  /**
   * DOM elements added during a drag (per-op multi-target dropzones,
   * qubit-line dropzones) that need to be removed on mouseup. Owned
   * here because their lifetime matches the gesture; cleared by
   * [`clearTemporaryDropzones`](interactionActions.ts), the only
   * function in the Action layer that touches the DOM.
   */
  temporaryDropzones: SVGElement[] = [];
}
