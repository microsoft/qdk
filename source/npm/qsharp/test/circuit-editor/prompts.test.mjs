// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// prompts tests — covers the confirm-dialog side of `editor/prompts.ts`: the generic confirm-dialog
// primitive `createConfirmPrompt`, plus the operation-specific delete/move flows built on it. The
// text-input primitive `_createInputPrompt` and the `promptForArguments` flow also live in that
// file but are not yet unit-tested.
//
// `createConfirmPrompt`:
//   - DOM shape: `.prompt-overlay > .prompt-container > .prompt-message + .prompt-buttons > [OK,
//     Cancel]`. The widget classes are load-bearing — the host page's CSS styles by them and the
//     operation-flow tests locate buttons by them.
//   - Click semantics: OK → `callback(true)` + overlay removed; Cancel → `callback(false)` +
//     overlay removed.
//   - Keyboard semantics: Enter → OK, Escape → Cancel, wired through a document-level capture-phase
//     keydown listener so the prompt wins over any descendant input handler.
//   - Listener lifecycle: the keydown listener is removed when the prompt closes (clicking OK or
//     Cancel — including via Enter or Escape), so a subsequent key press doesn't re-invoke the
//     callback.
//
// `deleteOperationWithConfirmation`: the no-consumer path (any op whose subtree strands nothing)
// skips the prompt and mutates + render immediately; the with-consumers path opens a confirm dialog
// whose message singularizes / pluralizes the consumer count, and only commits the cascade on OK.
//
// `moveOperationWithConfirmation`: previews the move on a clone to decide whether it strands any
// consumer. Strands none -> commit directly, no prompt (including a wire-changing move whose
// consumers are all silently repointed). Strands some -> prompt with the single delete-count
// message, and cascade-delete on OK. `movingControl` is threaded through to `moveOperation`
// unchanged.
//
// Tests run under JSDOM and drive the dialog by querying for `.prompt-button` elements.

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import {
  createConfirmPrompt,
  deleteOperationWithConfirmation,
  moveOperationWithConfirmation,
} from "../../dist/ux/circuit-vis/editor/prompts.js";
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
  jsdom?.window.close();
  jsdom = null;
});

/** Locate the open prompt's structural pieces by class. */
function getPrompt() {
  const overlay = /** @type {HTMLElement | null} */ (
    document.querySelector(".prompt-overlay")
  );
  if (!overlay) return null;
  const container = overlay.querySelector(".prompt-container");
  const message = overlay.querySelector(".prompt-message");
  const buttons = overlay.querySelectorAll(".prompt-button");
  return {
    overlay,
    container,
    message,
    okButton: /** @type {HTMLButtonElement} */ (buttons[0]),
    cancelButton: /** @type {HTMLButtonElement} */ (buttons[1]),
  };
}

/**
 * Open a confirm prompt over the current document and return a handle to the located DOM parts plus
 * two accessors:
 *   - `result()`    — the value the callback last received (null until fired)
 *   - `callCount()` — how many times the callback has fired
 */
function openPrompt(message = "ok?") {
  /** @type {boolean | null} */
  let captured = null;
  let callCount = 0;
  createConfirmPrompt(message, (c) => {
    captured = c;
    callCount++;
  });
  const parts = getPrompt();
  assert.ok(parts, "prompt overlay should be open after createConfirmPrompt");
  return { ...parts, result: () => captured, callCount: () => callCount };
}

/** Dispatch a document-level keydown — the path the prompt listens on. */
function pressKey(/** @type {string} */ key) {
  document.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
}

/** Assert the prompt overlay has been removed from the DOM. */
function assertPromptClosed(label = "prompt overlay must be removed") {
  assert.equal(document.querySelector(".prompt-overlay"), null, label);
}

/** Assert the prompt overlay is still present in the DOM. */
function assertPromptOpen(label = "prompt overlay must still be open") {
  assert.ok(document.querySelector(".prompt-overlay"), label);
}

test("createConfirmPrompt: builds the expected DOM subtree under document.body", () => {
  // Pinning the DOM shape because both the host page's CSS and the delete/move flow tests below
  // rely on these specific class names and the button ordering (OK first, Cancel second).
  const p = openPrompt("Confirm something?");

  assert.equal(p.overlay.parentNode, document.body);
  assert.ok(p.container, "container should exist inside overlay");
  assert.equal(
    p.message?.textContent,
    "Confirm something?",
    "message element should carry the caller's text verbatim",
  );
  assert.equal(p.okButton.textContent, "OK");
  assert.equal(p.cancelButton.textContent, "Cancel");
  // Callback shouldn't have fired yet — just the construction.
  assert.equal(p.result(), null);
});

test("createConfirmPrompt: OK button click fires callback(true) and removes the overlay", () => {
  const p = openPrompt();
  p.okButton.click();

  assert.equal(p.result(), true, "OK click must pass true to callback");
  assertPromptClosed("overlay must be removed from the DOM after OK");
});

test("createConfirmPrompt: Cancel button click fires callback(false) and removes the overlay", () => {
  const p = openPrompt();
  p.cancelButton.click();

  assert.equal(p.result(), false, "Cancel click must pass false to callback");
  assertPromptClosed("overlay must be removed from the DOM after Cancel");
});

test("createConfirmPrompt: Enter key commits as if OK was clicked", () => {
  // The document-level keydown listener is registered in capture phase, so dispatching a `keydown`
  // from `document` directly exercises the same path real key events take in the browser.
  const p = openPrompt();
  pressKey("Enter");

  assert.equal(p.result(), true, "Enter must commit (callback(true))");
  assertPromptClosed("Enter must close the prompt");
});

test("createConfirmPrompt: Escape key cancels as if Cancel was clicked", () => {
  const p = openPrompt();
  pressKey("Escape");

  assert.equal(p.result(), false, "Escape must cancel (callback(false))");
  assertPromptClosed("Escape must close the prompt");
});

test("createConfirmPrompt: keydown listener is removed after close — subsequent keys do not fire callback again", () => {
  // After OK closes the prompt, the document-level handler MUST be uninstalled — otherwise a stray
  // Enter elsewhere on the page would try to click a now-detached button and (worse) could
  // double-fire the callback if a second prompt has since opened. The implementation uses
  // `removeEventListener` with matching capture flag inside both click handlers; here we pin that
  // contract.
  const p = openPrompt();

  // First Enter → OK → callback fires once, prompt closes.
  pressKey("Enter");
  assert.equal(p.callCount(), 1);
  assertPromptClosed();

  // Subsequent Enter must NOT fire the now-closed prompt's callback again.
  pressKey("Enter");
  assert.equal(
    p.callCount(),
    1,
    "callback must NOT fire after the prompt is closed",
  );
});

test("createConfirmPrompt: keys other than Enter/Escape are ignored", () => {
  // Defense-in-depth: typing inside the prompt (e.g. someone accidentally hitting a letter key)
  // must not close it. Only Enter and Escape are honored.
  const p = openPrompt();

  pressKey("a");
  pressKey(" ");
  pressKey("Tab");

  assert.equal(
    p.result(),
    null,
    "callback must not fire for non-Enter/Escape keys",
  );
  assertPromptOpen("prompt must still be open after stray keypresses");
});

// ═══════════════════════════════════════════════════════════════════
//  Operation flows — deleteOperationWithConfirmation / moveOperationWithConfirmation
// ═══════════════════════════════════════════════════════════════════

/**
 * Query the currently-rendered confirm prompt. Returns null when none is open. The first button is
 * OK, the second is Cancel.
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
 * A unitary classically controlled by the measurement at "0,0" (result register `(qubit 0, result
 * 0)`). Every consumer in these tests reads that same register, so this captures the shared shape.
 *
 * @param {string} name  gate name
 * @param {number} target  target wire
 */
const consumer = (name, target) => gate(name, target, { ctrls: [{ q: 0 }] });

/**
 * Thin wrapper over `moveOperationWithConfirmation` that names its positional argument soup.
 * Defaults cover the common case (wires unchanged, not moving a control, no new column).
 *
 * @param {any} model
 * @param {{ from: string, to: string, fromWire?: number, toWire?: number, movingControl?: boolean,
 *           insertNewColumn?: boolean }} opts
 * @param {() => void} renderFn
 */
function moveWithConfirm(model, opts, renderFn) {
  moveOperationWithConfirmation(
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
// deleteOperationWithConfirmation
// ---------------------------------------------------------------------------

test("deleteOperationWithConfirmation: non-measurement op deletes immediately, no prompt", () => {
  // Fast path: any non-M op bypasses the consumer-collection branch and dispatches straight to
  // `removeOperation` + `renderFn`.
  const model = build(circuit(1, [[gate("H", 0)]]));
  const render = makeRenderSpy();

  deleteOperationWithConfirmation(model, "0,0", render.fn);

  assert.equal(getOpenPrompt(), null, "no confirm prompt should be opened");
  assert.equal(
    model.componentGrid.length,
    0,
    "the H should have been removed and the empty column collapsed",
  );
  assert.equal(render.count, 1, "renderFn must run exactly once on success");
});

test("deleteOperationWithConfirmation: measurement with NO classical consumers deletes immediately", () => {
  // Second fast path: an M whose subtree strands no consumer (nothing reads its result) also skips
  // the prompt.
  const model = build(circuit(qubits(1, { 0: 1 }), [[meas(0)]]));
  const render = makeRenderSpy();

  deleteOperationWithConfirmation(model, "0,0", render.fn);

  assert.equal(getOpenPrompt(), null, "no prompt for an unread measurement");
  assert.equal(model.componentGrid.length, 0, "M should be removed");
  assert.equal(render.count, 1);
});

test("deleteOperationWithConfirmation: M with 1 consumer opens a SINGULAR prompt; OK cascades", () => {
  // M produces (qubit=0, result=0); one classically-controlled X consumes it. Message must use the
  // singular form; OK must cascade both ops away.
  const model = build(
    circuit(qubits(2, { 0: 1 }), [[meas(0)], [consumer("X", 1)]]),
  );
  const render = makeRenderSpy();

  deleteOperationWithConfirmation(model, "0,0", render.fn);

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

test("deleteOperationWithConfirmation: M with 3 consumers opens a PLURAL prompt", () => {
  // Pluralization branch: three consumers reading the same (qubit=0, result=0) register. OK-cascade
  // behavior matches the singular case; this test asserts only on the message form.
  const model = build(
    circuit(qubits(4, { 0: 1 }), [
      [meas(0)],
      [consumer("X", 1), consumer("Y", 2), consumer("Z", 3)],
    ]),
  );
  const render = makeRenderSpy();

  deleteOperationWithConfirmation(model, "0,0", render.fn);

  const prompt = getOpenPrompt();
  assert.ok(prompt);
  assert.match(
    prompt.message,
    /3 dependent operations that reference/,
    "message must use the plural 'operations' form and the literal count",
  );
});

test("deleteOperationWithConfirmation: M-with-consumers Cancel makes NO mutations and does NOT render", () => {
  // Pins the cancel path: model state byte-for-byte identical before and after, and `renderFn` was
  // never called.
  const model = build(
    circuit(qubits(2, { 0: 1 }), [[meas(0)], [consumer("X", 1)]]),
  );
  const beforeJSON = snapshot(model);
  const render = makeRenderSpy();

  deleteOperationWithConfirmation(model, "0,0", render.fn);

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
// moveOperationWithConfirmation
// ---------------------------------------------------------------------------

test("moveOperationWithConfirmation: non-measurement op moves immediately, no prompt", () => {
  // Fast path: ordinary unitary, no consumers to consider. The wrapper passes through to
  // `moveOperation` with `movingControl` threaded as-is.
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

test("moveOperationWithConfirmation: M with NO consumers moves immediately, no prompt", () => {
  // Second fast path: an M with no classical consumers can move freely. Same passthrough as the
  // non-M case.
  const model = build(
    circuit(qubits(2, { 0: 1 }), [[meas(0)], [gate("H", 1)]]),
  );
  const render = makeRenderSpy();

  // Move M to column 1 (it'd swap with H there); no consumers, no prompt.
  moveWithConfirm(model, { from: "0,0", to: "1,0" }, render.fn);

  assert.equal(getOpenPrompt(), null);
  assert.equal(render.count, 1);
});

test("moveOperationWithConfirmation: M with pure-SURVIVORS consumers on a HORIZONTAL move runs without a prompt", () => {
  // Survivors-only, same-wire move: target column < every consumer's column, so nothing is deleted,
  // and the M stays on its wire so consumers only need result-index renumbering. That's cosmetic —
  // no confirmation — the surviving consumers' refs are reconciled automatically.
  const model = build(
    circuit(qubits(3, { 0: 1 }), [
      [meas(0)], // column 0: the M
      [consumer("X", 1)], // column 1: a consumer
      [consumer("Y", 2)], // column 2: another consumer
    ]),
  );
  const render = makeRenderSpy();

  // Move the M to column 0 (its current spot), same wire — still strictly before columns 1 and 2.
  // Both consumers survive and the wire is unchanged, so the move applies with no prompt.
  moveWithConfirm(model, { from: "0,0", to: "0,0" }, render.fn);

  assert.equal(
    getOpenPrompt(),
    null,
    "a same-wire, non-destructive M move must not prompt",
  );
  assert.equal(render.count, 1);
});

test("moveOperationWithConfirmation: M with pure-SURVIVORS consumers on a VERTICAL move runs without a prompt", () => {
  // Survivors-only but the M changes wire: no consumer is stranded, so nothing is deleted. The move
  // repoints every surviving consumer at the measurement's new qubit automatically — a silent,
  // non-destructive change — so it commits with no prompt.
  const model = build(
    circuit(qubits(3, { 0: 1 }), [
      [meas(0)], // column 0: the M on wire 0
      [consumer("X", 1)], // column 1: consumes M0, survives (later column)
    ]),
  );
  const render = makeRenderSpy();

  // Move the M from wire 0 to wire 2, staying in column 0 (still before the consumer's column). The
  // consumer survives and is silently repointed to wire 2 — no prompt.
  moveWithConfirm(model, { from: "0,0", to: "0,0", toWire: 2 }, render.fn);

  assert.equal(
    getOpenPrompt(),
    null,
    "a non-destructive wire-changing M move must not prompt",
  );
  assert.equal(render.count, 1);
  // The survivor's classical control must track the M's new wire: (0,0) → (2,0).
  assert.equal(at(model, "1,0").controls[0].qubit, 2);
});

test("moveOperationWithConfirmation: M dropped just left of its consumer (insertNewColumn) does not delete it", () => {
  // Off-by-one guard: dropping the M into a FRESH column spliced in at the consumer's column places
  // the M strictly before the consumer, so the consumer survives — no deletion, no prompt. Without
  // the `insertNewColumn` awareness this partitioned the consumer as invalidated (same column).
  const model = build(
    circuit(qubits(2, { 0: 1 }), [
      [meas(0)], // column 0: the M
      [gate("H", 1)], // column 1: filler so the M has somewhere to move from
      [consumer("X", 1)], // column 2: the consumer
    ]),
  );
  const render = makeRenderSpy();

  // Drop the M into a new column spliced in AT the consumer's column ("2,0"). The M lands strictly
  // before the consumer, which shifts to column 3 — the consumer survives.
  moveWithConfirm(
    model,
    { from: "0,0", to: "2,0", insertNewColumn: true },
    render.fn,
  );

  assert.equal(
    getOpenPrompt(),
    null,
    "dropping the M just before its consumer must not invalidate it",
  );
  assert.equal(render.count, 1);
});

test("moveOperationWithConfirmation: M with pure-INVALIDATED consumers shows the delete message", () => {
  // Invalidated-only: the M moves PAST its consumer (into a later column), so no producer remains in
  // a strictly earlier column — the consumer is stranded and flagged for deletion.
  const model = build(
    circuit(qubits(3, { 0: 1 }), [
      [meas(0)], // column 0: the M
      [consumer("X", 1)], // column 1: only consumer
      [gate("H", 2)], // column 2: filler so the M can land after the consumer
    ]),
  );
  const render = makeRenderSpy();

  // Move M into column 2 (after the consumer). Producer no longer strictly earlier → stranded.
  moveWithConfirm(model, { from: "0,0", to: "2,0" }, render.fn);

  const prompt = getOpenPrompt();
  assert.ok(prompt);
  assert.match(
    prompt.message,
    /will also delete 1 dependent operation that references a measurement result/,
    "must surface the singular delete message",
  );
});

test("moveOperationWithConfirmation: M with MIXED consumers on a VERTICAL move shows only the delete count", () => {
  // Mixed on a wire-changing move: one consumer ends up stranded (deleted), one stays after
  // (silently repointed to the new qubit). The unified prompt reports only the stranded count —
  // survivor repointing is silent — so exactly one deletion is mentioned and no update clause.
  const model = build(
    circuit(qubits(3, { 0: 1 }), [
      [meas(0)], // column 0: the M on wire 0
      [consumer("X", 1)], // column 1: stranded (column == target)
      [consumer("Y", 2)], // column 2: survives (column > target)
    ]),
  );
  const render = makeRenderSpy();

  // Target column 1, wire 0 → 2 → consumer at "1,0" strands, consumer at "2,0" survives and is
  // repointed to wire 2.
  moveWithConfirm(model, { from: "0,0", to: "1,0", toWire: 2 }, render.fn);

  const prompt = getOpenPrompt();
  assert.ok(prompt);
  assert.match(
    prompt.message,
    /will also delete 1 dependent operation that references a measurement result/,
    "must report exactly the one stranded consumer",
  );
  assert.doesNotMatch(
    prompt.message,
    /updated/,
    "survivor repointing is silent — no update clause",
  );
});

test("moveOperationWithConfirmation: M with MIXED consumers on a HORIZONTAL move shows only the delete count", () => {
  // Mixed but SAME wire: one consumer is stranded (triggering the prompt) while the survivor stays
  // on the same qubit and only needs result-index renumbering. The unified prompt reports only the
  // stranded count.
  const model = build(
    circuit(qubits(3, { 0: 1 }), [
      [meas(0)], // column 0: the M
      [consumer("X", 1)], // column 1: stranded (M lands after it)
      [gate("H", 2)], // column 2: filler — the M's landing column
      [consumer("Y", 1)], // column 3: survives (still after the M)
    ]),
  );
  const render = makeRenderSpy();

  // Move M into column 2 (same wire) → consumer at "1,0" strands, consumer at "3,0" survives.
  moveWithConfirm(model, { from: "0,0", to: "2,0" }, render.fn);

  const prompt = getOpenPrompt();
  assert.ok(prompt);
  assert.match(
    prompt.message,
    /will also delete 1 dependent operation that references a measurement result/,
    "must report exactly the one stranded consumer",
  );
  assert.doesNotMatch(
    prompt.message,
    /updated/,
    "a horizontal deletion prompt must NOT mention survivor updates",
  );
});

test("moveOperationWithConfirmation: M-with-consumers Cancel makes NO mutations and does NOT render", () => {
  // Cancel-path symmetry with the delete wrapper: model frozen, renderFn untouched.
  const model = build(
    circuit(qubits(2, { 0: 1 }), [
      [meas(0)], // column 0: the M
      [consumer("X", 1)], // column 1: consumer stranded by the move
      [gate("H", 1)], // column 2: filler — the M's landing column
    ]),
  );
  const beforeJSON = snapshot(model);
  const render = makeRenderSpy();

  moveWithConfirm(model, { from: "0,0", to: "2,0" }, render.fn);

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

test("moveOperationWithConfirmation: M-with-consumers OK cascades through moveOperationWithDependents", () => {
  // Sanity check on the OK branch with a mixed partition. After commit: the M moved to the target
  // column, the survivor's classical control was remapped to the M's new wire, and the stranded
  // consumer is gone.
  const model = build(
    circuit(qubits(3, { 0: 1 }), [
      [meas(0)], // column 0: the M on wire 0
      [consumer("X", 1)], // column 1: stranded consumer (M lands after it)
      [gate("H", 2)], // column 2: filler — the M's landing column
      [consumer("Y", 1)], // column 3: survivor consumer
    ]),
  );
  const render = makeRenderSpy();

  // Move M from (0,0) on wire 0 → target column 2 on wire 0 (no wire change). Consumer at "1,0" is
  // stranded; consumer at "3,0" survives.
  moveWithConfirm(model, { from: "0,0", to: "2,0" }, render.fn);

  const prompt = getOpenPrompt();
  assert.ok(prompt);
  prompt.okButton.click();

  assert.equal(render.count, 1, "OK must trigger exactly one re-render");

  // The X (stranded) must be gone.
  const allOps = flattenOps(model);
  assert.equal(
    allOps.find((o) => /** @type {any} */ (o).gate === "X"),
    undefined,
    "stranded X consumer must have been cascade-deleted",
  );
  // The Y (survivor) must still exist. The exact remap is the contract of
  // `moveOperationWithDependents`, covered in the circuit-actions/ suite
  // (measurementCascade.test.mjs).
  assert.ok(
    allOps.find((o) => /** @type {any} */ (o).gate === "Y"),
    "survivor Y consumer must remain",
  );
});
