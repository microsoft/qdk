// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { Operation } from "../data/circuit.js";

/**
 * `InteractionState` — ephemeral session state for the circuit editor.
 *
 * This is the **Action layer's state container** in the three-layer
 * architecture (Data / Action / View — see
 * [CIRCUIT_EDITOR_TODO.md](CIRCUIT_EDITOR_TODO.md)). It collects the
 * loose mutable fields that previously lived directly on
 * `CircuitEvents` and were tweaked by every pointer/keyboard handler.
 *
 * **Distinct from [`CircuitModel`](circuitModel.ts) on purpose** —
 * neither `selectedOperation` nor `dragging` belongs in saved circuit
 * JSON. Keeping them in a separate type makes that boundary obvious
 * and prevents accidental round-tripping through serialization.
 *
 * Transient vs. persistent fields. Two lifetimes mixed in one object:
 *
 *   - **Persistent** — `selectedOperation` survives across the
 *     `resetTransient` reset that fires on every mouseup. The
 *     selection persists so that the context menu (which is built
 *     against the most-recently-mousedown'd op) can still find its
 *     target after the menu opens. Cleared explicitly by callers.
 *   - **Transient** — `selectedWire`, `movingControl`,
 *     `mouseUpOnCircuit`, `dragging`, `disableLeftAutoScroll`,
 *     `temporaryDropzones`. Reset between drags. Owned by the
 *     in-flight gesture, not the user's selection.
 *
 * The fields are public for direct read/write from event handlers in
 * `events.ts`. State *mutations* with non-trivial logic go through
 * [interactionActions.ts](interactionActions.ts) so that
 * `CircuitEvents` doesn't need to know how to e.g. tear down
 * temporary dropzones.
 *
 * **Why no methods on this class.** The roadmap pushes us toward a
 * pure data + free-function pattern (mirrors `CircuitModel` +
 * `circuitActions.ts`). It also keeps unit tests trivial — see
 * [test/interactionActions.test.mjs](../../test/interactionActions.test.mjs).
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
   * — the latter triggers the delete-on-release behavior. Transient.
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
   * by `InteractionState` because their lifetime exactly matches the
   * gesture; cleared by [`clearTemporaryDropzones`](interactionActions.ts).
   *
   * Holding DOM refs here is a deliberate compromise — the
   * alternative is putting them on `CircuitEvents` (the View layer),
   * which forks "transient state owned by the gesture" across two
   * objects. The unit-test cost is small: `clearTemporaryDropzones`
   * is the only function that touches the DOM, and tests for the
   * pure-data resets work without it.
   */
  temporaryDropzones: SVGElement[] = [];
}
