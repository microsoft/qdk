// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { Operation } from "../data/circuit.js";
import { InteractionState } from "./interactionState.js";

/*
 * `interactionActions.ts` — the Action layer for ephemeral editor
 * session state (drag/selection/temporary-overlay tracking). Mirrors
 * [circuitActions.ts](circuitActions.ts): each function takes an
 * `InteractionState` first and mutates it in place, returning `void`.
 *
 * Most functions are pure data helpers (no DOM) and unit-testable
 * without JSDOM; `clearTemporaryDropzones` is the only DOM-touching
 * one. Direct setters (`state.selectedOperation = ...`) are fine for
 * one-line writes in event handlers; the wrappers exist to centralize
 * the multi-step sequences (e.g. `beginToolboxDrag` sets two fields
 * together) so they aren't reinvented inconsistently.
 */

/**
 * Clear all transient drag/gesture flags. **Does not** clear
 * `selectedOperation` — that survives across resets so the context
 * menu can use it.
 */
export function resetTransient(state: InteractionState): void {
  state.selectedWire = null;
  state.movingControl = false;
  state.mouseUpOnCircuit = false;
  state.dragging = false;
  state.disableLeftAutoScroll = false;
  clearTemporaryDropzones(state);
}

/**
 * Clear the persistent selection. Called when the selected op no
 * longer represents a meaningful target — e.g. after committing a
 * toolbox drop, after starting an add-control flow, when the user
 * clicks on the canvas background.
 */
export function clearSelection(state: InteractionState): void {
  state.selectedOperation = null;
}

/**
 * Set the persistent selection to `op`. Used by the various
 * mousedown handlers when the user grabs a gate or starts a
 * control add/remove flow.
 */
export function markSelected(
  state: InteractionState,
  op: Operation | null,
): void {
  state.selectedOperation = op;
}

/**
 * Begin a drag from the toolbox. Records the toolbox-template
 * operation as the selection and suppresses left-edge auto-scroll
 * for this drag (so the user doesn't get a runaway scroll while
 * still over the toolbox panel near the canvas's left edge).
 */
export function beginToolboxDrag(
  state: InteractionState,
  templateOp: Operation,
): void {
  state.selectedOperation = templateOp;
  state.disableLeftAutoScroll = true;
}

/** Track that the user is dragging a control dot. */
export function markMovingControl(state: InteractionState): void {
  state.movingControl = true;
}

/**
 * Track that a mouseup landed on the circuit SVG (vs. outside).
 * Read by `documentMouseupHandler` to decide whether to commit
 * the drop or treat it as a "dragged out" delete.
 */
export function markMouseUpOnCircuit(state: InteractionState): void {
  state.mouseUpOnCircuit = true;
}

/**
 * Track that a drag is now in flight. Set by `_createGhostElement`
 * once the visual ghost is up.
 */
export function markDragging(state: InteractionState): void {
  state.dragging = true;
}

/**
 * Append `dz` to the list of temporary dropzones to be torn down on
 * the next reset. Caller is still responsible for inserting the
 * element into the DOM; this is just a bookkeeping handle.
 */
export function trackTemporaryDropzone(
  state: InteractionState,
  dz: SVGElement,
): void {
  state.temporaryDropzones.push(dz);
}

/**
 * Remove every tracked temporary dropzone from its parent node and
 * clear the tracking list. Safe to call when the list is already
 * empty. The only DOM-touching function in this module.
 */
export function clearTemporaryDropzones(state: InteractionState): void {
  for (const dz of state.temporaryDropzones) {
    if (dz.parentNode) {
      dz.parentNode.removeChild(dz);
    }
  }
  state.temporaryDropzones = [];
}
