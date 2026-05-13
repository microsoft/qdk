// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// interactionActions tests — exercises the Action layer for the
// editor's ephemeral session state (`InteractionState`) directly,
// with **no JSDOM** for the pure-data helpers and a tiny stub
// `parentNode` for the one DOM-touching helper. Together with the
// `circuitActions` tests, this means the only editor logic that
// still needs JSDOM is the actual event-listener wiring in
// `CircuitEvents`.
//

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import { InteractionState } from "../../dist/ux/circuit-vis/actions/interactionState.js";
import {
  beginToolboxDrag,
  clearSelection,
  clearTemporaryDropzones,
  markDragging,
  markMouseUpOnCircuit,
  markMovingControl,
  markSelected,
  resetTransient,
  trackTemporaryDropzone,
} from "../../dist/ux/circuit-vis/actions/interactionActions.js";

/** Minimal stub matching the shape `clearTemporaryDropzones` looks at. */
function fakeDropzone() {
  const removed = { value: false };
  const elem = /** @type {any} */ ({
    parentNode: {
      removeChild: () => {
        removed.value = true;
      },
    },
    /** Test-only flag for assertions. */
    _removed: removed,
  });
  return elem;
}

test("InteractionState defaults all fields to a 'no gesture' state", () => {
  const s = new InteractionState();

  assert.equal(s.selectedOperation, null);
  assert.equal(s.selectedWire, null);
  assert.equal(s.movingControl, false);
  assert.equal(s.mouseUpOnCircuit, false);
  assert.equal(s.dragging, false);
  assert.equal(s.disableLeftAutoScroll, false);
  assert.deepEqual(s.temporaryDropzones, []);
});

test("resetTransient clears every transient flag but preserves selectedOperation", () => {
  const s = new InteractionState();
  // Persistent — must survive the reset.
  const op = /** @type {any} */ ({ kind: "unitary", gate: "H" });
  s.selectedOperation = op;
  // Transient — must be cleared.
  s.selectedWire = 2;
  s.movingControl = true;
  s.mouseUpOnCircuit = true;
  s.dragging = true;
  s.disableLeftAutoScroll = true;
  s.temporaryDropzones.push(fakeDropzone());

  resetTransient(s);

  // Persistent selection survives — that's the contract callers rely
  // on so the context menu can still find its target after a reset.
  assert.equal(s.selectedOperation, op);
  // Everything else cleared.
  assert.equal(s.selectedWire, null);
  assert.equal(s.movingControl, false);
  assert.equal(s.mouseUpOnCircuit, false);
  assert.equal(s.dragging, false);
  assert.equal(s.disableLeftAutoScroll, false);
  assert.deepEqual(s.temporaryDropzones, []);
});

test("clearSelection drops only selectedOperation", () => {
  const s = new InteractionState();
  s.selectedOperation = /** @type {any} */ ({ kind: "unitary" });
  s.selectedWire = 1;
  s.dragging = true;

  clearSelection(s);

  assert.equal(s.selectedOperation, null);
  // clearSelection is targeted; transient flags untouched.
  assert.equal(s.selectedWire, 1);
  assert.equal(s.dragging, true);
});

test("markSelected sets selectedOperation; null is allowed", () => {
  const s = new InteractionState();
  const op = /** @type {any} */ ({ kind: "unitary", gate: "X" });

  markSelected(s, op);
  assert.equal(s.selectedOperation, op);

  markSelected(s, null);
  assert.equal(s.selectedOperation, null);
});

test("beginToolboxDrag sets selection AND suppresses left auto-scroll", () => {
  const s = new InteractionState();
  const template = /** @type {any} */ ({ kind: "unitary", gate: "T" });

  beginToolboxDrag(s, template);

  assert.equal(s.selectedOperation, template);
  // The whole point of the helper: these two have to move together,
  // because forgetting the suppress-flag causes a runaway scroll bug.
  assert.equal(s.disableLeftAutoScroll, true);
});

test("markMovingControl / markMouseUpOnCircuit / markDragging set their respective flags", () => {
  const s = new InteractionState();

  markMovingControl(s);
  markMouseUpOnCircuit(s);
  markDragging(s);

  assert.equal(s.movingControl, true);
  assert.equal(s.mouseUpOnCircuit, true);
  assert.equal(s.dragging, true);
});

test("trackTemporaryDropzone appends to the list without removing existing entries", () => {
  const s = new InteractionState();
  const a = fakeDropzone();
  const b = fakeDropzone();

  trackTemporaryDropzone(s, a);
  trackTemporaryDropzone(s, b);

  assert.equal(s.temporaryDropzones.length, 2);
  assert.equal(s.temporaryDropzones[0], a);
  assert.equal(s.temporaryDropzones[1], b);
});

test("clearTemporaryDropzones removes each element from its parent and clears the list", () => {
  const s = new InteractionState();
  const a = fakeDropzone();
  const b = fakeDropzone();
  trackTemporaryDropzone(s, a);
  trackTemporaryDropzone(s, b);

  clearTemporaryDropzones(s);

  assert.equal(s.temporaryDropzones.length, 0);
  assert.equal(a._removed.value, true);
  assert.equal(b._removed.value, true);
});

test("clearTemporaryDropzones is safe on dropzones with no parentNode", () => {
  const s = new InteractionState();
  // Simulates an element that's already been removed by something else
  // — `clearTemporaryDropzones` must not throw on it.
  s.temporaryDropzones.push(/** @type {any} */ ({ parentNode: null }));

  clearTemporaryDropzones(s);

  assert.equal(s.temporaryDropzones.length, 0);
});

test("clearTemporaryDropzones is idempotent", () => {
  const s = new InteractionState();

  clearTemporaryDropzones(s);
  clearTemporaryDropzones(s);

  assert.deepEqual(s.temporaryDropzones, []);
});
