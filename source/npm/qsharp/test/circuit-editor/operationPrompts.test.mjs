// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// operationPrompts tests — covers the prompt-aware delete/move
// wrappers in `editor/operationPrompts.ts`.
//
//   - `_deleteOperationWithConfirmation`: fast paths (non-M, M
//     with no classical consumers) skip the prompt and mutate +
//     render immediately; the M-with-consumers path opens a
//     confirm dialog whose message singularizes / pluralizes the
//     consumer count, and only commits the cascade on OK.
//
//   - `_moveOperationWithConfirmation`: same fast-path / prompt
//     split, plus the three message-shape branches in
//     `_buildMoveMConsumerMessage` (pure survivors, pure
//     invalidated, mixed). `movingControl` is threaded through to
//     `moveOperation` unchanged on the fast path.
//
// Both wrappers reach `_createConfirmPrompt` from `prompts.ts`,
// which builds a `.prompt-overlay` DOM subtree. Tests run under
// JSDOM and drive the dialog by querying for `.prompt-button`
// elements.

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import {
  _deleteOperationWithConfirmation,
  _moveOperationWithConfirmation,
} from "../../dist/ux/circuit-vis/editor/operationPrompts.js";
import { at, build, circuit, gate, meas, qubits } from "./_helpers.mjs";

/** @type {JSDOM | null} */
let jsdom = null;

beforeEach(() => {
  jsdom = new JSDOM(`<!doctype html><html><body></body></html>`);
  globalThis.window = jsdom.window;
  globalThis.document = jsdom.window.document;
  globalThis.HTMLElement = jsdom.window.HTMLElement;
  globalThis.KeyboardEvent = jsdom.window.KeyboardEvent;
});

afterEach(() => {
  // `_createConfirmPrompt` installs a document-level keydown
  // listener only removed on OK/Cancel click; closing JSDOM
  // ensures a clean slate even if a prompt was never opened.
  jsdom?.window.close();
  jsdom = null;
});

/**
 * Query the currently-rendered confirm prompt. Returns null when
 * none is open. The first button is OK, the second is Cancel.
 */
function getOpenPrompt() {
  const overlay = document.querySelector(".prompt-overlay");
  if (!overlay) return null;
  const messageElem = overlay.querySelector(".prompt-message");
  const buttons = overlay.querySelectorAll(".prompt-button");
  return {
    overlay,
    message: messageElem?.textContent ?? "",
    okButton: /** @type {HTMLButtonElement} */ (buttons[0]),
    cancelButton: /** @type {HTMLButtonElement} */ (buttons[1]),
  };
}

/** Make a render-callback spy that counts invocations. */
function makeRenderSpy() {
  const spy = /** @type {{ count: number; fn: () => void }} */ ({ count: 0 });
  spy.fn = () => {
    spy.count++;
  };
  return spy;
}

/**
 * A unitary classically controlled by the measurement at "0,0"
 * (result register `(qubit 0, result 0)`). Every consumer in these
 * tests reads that same register, so this captures the shared shape.
 *
 * @param {string} name  gate name
 * @param {number} target  target wire
 */
const consumer = (name, target) => gate(name, target, { ctrls: [{ q: 0 }] });

/**
 * Thin wrapper over `_moveOperationWithConfirmation` that names its
 * positional argument soup. Defaults cover the common case (wires
 * unchanged, not moving a control, no new column).
 *
 * @param {any} model
 * @param {{ from: string, to: string, fromWire?: number, toWire?: number,
 *           movingControl?: boolean, insertNewColumn?: boolean }} opts
 * @param {() => void} renderFn
 */
function moveWithConfirm(model, opts, renderFn) {
  _moveOperationWithConfirmation(
    model,
    opts.from,
    opts.to,
    opts.fromWire ?? 0,
    opts.toWire ?? 0,
    opts.movingControl ?? false,
    opts.insertNewColumn ?? false,
    renderFn,
  );
}

/** Serialize a model's grid + qubits for byte-for-byte equality checks. */
function snapshot(/** @type {any} */ model) {
  return JSON.stringify({ grid: model.componentGrid, qubits: model.qubits });
}

/** Flatten every op across all columns into a single array. */
function flattenOps(/** @type {any} */ model) {
  const ops = [];
  for (const col of model.componentGrid) {
    for (const op of col.components) ops.push(op);
  }
  return ops;
}

// ---------------------------------------------------------------------------
// _deleteOperationWithConfirmation
// ---------------------------------------------------------------------------

test("_deleteOperationWithConfirmation: non-measurement op deletes immediately, no prompt", () => {
  // Fast path: any non-M op bypasses the consumer-collection branch
  // and dispatches straight to `removeOperation` + `renderFn`.
  const model = build(circuit(1, [[gate("H", 0)]]));
  const render = makeRenderSpy();

  _deleteOperationWithConfirmation(model, "0,0", render.fn);

  assert.equal(getOpenPrompt(), null, "no confirm prompt should be opened");
  assert.equal(
    model.componentGrid.length,
    0,
    "the H should have been removed and the empty column collapsed",
  );
  assert.equal(render.count, 1, "renderFn must run exactly once on success");
});

test("_deleteOperationWithConfirmation: measurement with NO classical consumers deletes immediately", () => {
  // Second fast path: an M whose `collectMeasurementConsumers`
  // returns `[]` (no consumer reads its result) also skips the
  // prompt.
  const model = build(circuit(qubits(1, { 0: 1 }), [[meas(0)]]));
  const render = makeRenderSpy();

  _deleteOperationWithConfirmation(model, "0,0", render.fn);

  assert.equal(getOpenPrompt(), null, "no prompt for an unread measurement");
  assert.equal(model.componentGrid.length, 0, "M should be removed");
  assert.equal(render.count, 1);
});

test("_deleteOperationWithConfirmation: M with 1 consumer opens a SINGULAR prompt; OK cascades", () => {
  // M produces (qubit=0, result=0); one classically-controlled X
  // consumes it. Message must use the singular form; OK must
  // cascade both ops away.
  const model = build(
    circuit(qubits(2, { 0: 1 }), [[meas(0)], [consumer("X", 1)]]),
  );
  const render = makeRenderSpy();

  _deleteOperationWithConfirmation(model, "0,0", render.fn);

  const prompt = getOpenPrompt();
  assert.ok(prompt, "prompt should be open");
  assert.match(
    prompt.message,
    /1 dependent operation that references/,
    "message must use the singular 'operation' form",
  );
  assert.equal(
    render.count,
    0,
    "renderFn must NOT run until the user confirms",
  );

  prompt.okButton.click();

  assert.equal(getOpenPrompt(), null, "prompt should close on OK");
  assert.equal(
    model.componentGrid.length,
    0,
    "both the M and its consumer should be cascade-deleted",
  );
  assert.equal(render.count, 1, "renderFn fires exactly once after cascade");
});

test("_deleteOperationWithConfirmation: M with 3 consumers opens a PLURAL prompt", () => {
  // Pluralization branch: three consumers reading the same
  // (qubit=0, result=0) register. OK-cascade behavior matches the
  // singular case; this test asserts only on the message form.
  const model = build(
    circuit(qubits(4, { 0: 1 }), [
      [meas(0)],
      [consumer("X", 1), consumer("Y", 2), consumer("Z", 3)],
    ]),
  );
  const render = makeRenderSpy();

  _deleteOperationWithConfirmation(model, "0,0", render.fn);

  const prompt = getOpenPrompt();
  assert.ok(prompt);
  assert.match(
    prompt.message,
    /3 dependent operations that reference/,
    "message must use the plural 'operations' form and the literal count",
  );
});

test("_deleteOperationWithConfirmation: M-with-consumers Cancel makes NO mutations and does NOT render", () => {
  // Pins the cancel path: model state byte-for-byte identical
  // before and after, and `renderFn` was never called.
  const model = build(
    circuit(qubits(2, { 0: 1 }), [[meas(0)], [consumer("X", 1)]]),
  );
  const beforeJSON = snapshot(model);
  const render = makeRenderSpy();

  _deleteOperationWithConfirmation(model, "0,0", render.fn);

  const prompt = getOpenPrompt();
  assert.ok(prompt);
  prompt.cancelButton.click();

  assert.equal(getOpenPrompt(), null, "prompt should close on Cancel");
  assert.equal(render.count, 0, "Cancel must NOT trigger a re-render");
  assert.equal(
    snapshot(model),
    beforeJSON,
    "model must be unchanged after Cancel",
  );
});

// ---------------------------------------------------------------------------
// _moveOperationWithConfirmation
// ---------------------------------------------------------------------------

test("_moveOperationWithConfirmation: non-measurement op moves immediately, no prompt", () => {
  // Fast path: ordinary unitary, no consumers to consider. The
  // wrapper passes through to `moveOperation` with `movingControl`
  // threaded as-is.
  const model = build(circuit(2, [[gate("H", 0)], [gate("X", 1)]]));
  const render = makeRenderSpy();

  // Swap H from wire 0 → wire 1 (no consumers involved).
  moveWithConfirm(model, { from: "0,0", to: "0,0", toWire: 1 }, render.fn);

  assert.equal(getOpenPrompt(), null, "no prompt for a non-M move");
  // H landed on wire 1; X is still in column 1 (no insertNewColumn).
  const movedH = at(model, "0,0");
  assert.equal(movedH.gate, "H");
  assert.equal(movedH.targets[0].qubit, 1);
  assert.equal(render.count, 1);
});

test("_moveOperationWithConfirmation: M with NO consumers moves immediately, no prompt", () => {
  // Second fast path: an M with no classical consumers can move
  // freely. Same passthrough as the non-M case.
  const model = build(
    circuit(qubits(2, { 0: 1 }), [[meas(0)], [gate("H", 1)]]),
  );
  const render = makeRenderSpy();

  // Move M to column 1 (it'd swap with H there); no consumers,
  // no prompt.
  moveWithConfirm(model, { from: "0,0", to: "1,0" }, render.fn);

  assert.equal(getOpenPrompt(), null);
  assert.equal(render.count, 1);
});

test("_moveOperationWithConfirmation: M with pure-SURVIVORS consumers shows the update-only message", () => {
  // Survivors-only partition: target column < every consumer's
  // column. The M moves forward (or stays) so every consumer still
  // comes after it; nothing gets deleted.
  const model = build(
    circuit(qubits(3, { 0: 1 }), [
      [meas(0)], // column 0: the M
      [consumer("X", 1)], // column 1: a consumer
      [consumer("Y", 2)], // column 2: another consumer
    ]),
  );
  const render = makeRenderSpy();

  // Move the M to column 0 (its current spot) — still strictly
  // before columns 1 and 2. Both consumers partition into survivors.
  moveWithConfirm(model, { from: "0,0", to: "0,0" }, render.fn);

  const prompt = getOpenPrompt();
  assert.ok(prompt);
  assert.match(
    prompt.message,
    /2 dependent operations will be updated to reference this measurement's new wire/,
    "must surface the survivors-update clause with the plural count",
  );
  assert.doesNotMatch(
    prompt.message,
    /will be deleted/,
    "pure-survivors message must NOT mention deletion",
  );
});

test("_moveOperationWithConfirmation: M with pure-INVALIDATED consumers shows the delete-only message", () => {
  // Invalidated-only partition: target column >= every consumer's
  // column — the M moves past all its consumers. Every consumer
  // flips into the "will be deleted" bucket.
  const model = build(
    circuit(qubits(3, { 0: 1 }), [
      [meas(0)], // column 0: the M
      [consumer("X", 1)], // column 1: only consumer
    ]),
  );
  const render = makeRenderSpy();

  // Move M into column 1 (the consumer's column). Target column ==
  // consumer's column → `inEarlierColumnThan` is false → consumer
  // is invalidated.
  moveWithConfirm(model, { from: "0,0", to: "1,0" }, render.fn);

  const prompt = getOpenPrompt();
  assert.ok(prompt);
  assert.match(
    prompt.message,
    /1 dependent operation would end up before this measurement in document order and will be deleted/,
    "must surface the invalidated-delete clause in singular form",
  );
  assert.doesNotMatch(
    prompt.message,
    /will be updated/,
    "pure-invalidated message must NOT mention updates",
  );
});

test("_moveOperationWithConfirmation: M with MIXED consumers shows both clauses joined with '; '", () => {
  // Mixed partition: target column splits the consumer list —
  // some stay after (survivors → updated), some end up at-or-
  // before (invalidated → deleted). Message must include BOTH
  // clauses and the explicit '; ' separator.
  const model = build(
    circuit(qubits(3, { 0: 1 }), [
      [meas(0)], // column 0: the M
      [consumer("X", 1)], // column 1: invalidated (column == target)
      [consumer("Y", 2)], // column 2: survives (column > target)
    ]),
  );
  const render = makeRenderSpy();

  // Target column 1 → consumer at "1,0" invalidates, consumer at
  // "2,0" survives.
  moveWithConfirm(model, { from: "0,0", to: "1,0" }, render.fn);

  const prompt = getOpenPrompt();
  assert.ok(prompt);
  assert.match(
    prompt.message,
    /will be updated to reference this measurement's new wire/,
    "must include the survivors clause",
  );
  assert.match(
    prompt.message,
    /will be deleted/,
    "must include the invalidated clause",
  );
  assert.match(
    prompt.message,
    /; /,
    "the two clauses must be joined with '; '",
  );
});

test("_moveOperationWithConfirmation: M-with-consumers Cancel makes NO mutations and does NOT render", () => {
  // Cancel-path symmetry with the delete wrapper: model frozen,
  // renderFn untouched.
  const model = build(
    circuit(qubits(2, { 0: 1 }), [[meas(0)], [consumer("X", 1)]]),
  );
  const beforeJSON = snapshot(model);
  const render = makeRenderSpy();

  moveWithConfirm(model, { from: "0,0", to: "1,0" }, render.fn);

  const prompt = getOpenPrompt();
  assert.ok(prompt);
  prompt.cancelButton.click();

  assert.equal(render.count, 0);
  assert.equal(
    snapshot(model),
    beforeJSON,
    "model must be unchanged after Cancel on a move prompt",
  );
});

test("_moveOperationWithConfirmation: M-with-consumers OK cascades through moveMeasurementWithDependents", () => {
  // Sanity check on the OK branch with a mixed partition. After
  // commit: the M moved to the target column, the survivor's
  // classical control was remapped to the M's new wire, and the
  // invalidated consumer is gone.
  const model = build(
    circuit(qubits(3, { 0: 1 }), [
      [meas(0)], // column 0: the M on wire 0
      [consumer("X", 1)], // column 1: invalidated consumer
      [consumer("Y", 2)], // column 2: survivor consumer
    ]),
  );
  const render = makeRenderSpy();

  // Move M from (0,0) on wire 0 → target column 1 on wire 0 (no
  // wire change). Consumer at "1,0" is invalidated; consumer at
  // "2,0" survives.
  moveWithConfirm(model, { from: "0,0", to: "1,0" }, render.fn);

  const prompt = getOpenPrompt();
  assert.ok(prompt);
  prompt.okButton.click();

  assert.equal(render.count, 1, "OK must trigger exactly one re-render");

  // The X (invalidated) must be gone.
  const allOps = flattenOps(model);
  assert.equal(
    allOps.find((o) => /** @type {any} */ (o).gate === "X"),
    undefined,
    "invalidated X consumer must have been cascade-deleted",
  );
  // The Y (survivor) must still exist. The exact remap is the
  // contract of `moveMeasurementWithDependents`, covered in
  // circuitActions.test.mjs.
  assert.ok(
    allOps.find((o) => /** @type {any} */ (o).gate === "Y"),
    "survivor Y consumer must remain",
  );
});
