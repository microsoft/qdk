// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// operationPrompts tests — pins the prompt-aware delete/move
// wrappers in `editor/operationPrompts.ts`:
//
//   - `_deleteOperationWithConfirmation`: the fast paths
//     (non-measurement, M with no classical consumers) skip the
//     prompt and mutate + render immediately; the M-with-consumers
//     path opens a confirm dialog whose message singularizes /
//     pluralizes the consumer count, and only commits the cascade
//     delete on OK. Cancel = no mutation, no `renderFn`.
//
//   - `_moveOperationWithConfirmation`: same fast-path / prompt
//     split, plus the three message-shape branches in
//     `_buildMoveMConsumerMessage`:
//       (a) pure survivors — "will be updated to reference this
//           measurement's new wire";
//       (b) pure invalidated — "will be deleted";
//       (c) mixed — both clauses, joined with "; ".
//     OK runs the partitioned cascade; Cancel = no mutation, no
//     `renderFn`. The `movingControl` parameter is threaded to
//     `moveOperation` unchanged on the fast path (the B11a
//     control-dot-of-a-CNOT regression guard).
//
// Both wrappers reach `_createConfirmPrompt` from
// [prompts.ts](../../ux/circuit-vis/editor/prompts.ts), which
// builds a `.prompt-overlay` DOM subtree — these tests run under
// JSDOM and drive the dialog by querying for the rendered
// `.prompt-button` elements and clicking them.

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { CircuitModel } from "../../dist/ux/circuit-vis/data/circuitModel.js";
import {
  _deleteOperationWithConfirmation,
  _moveOperationWithConfirmation,
} from "../../dist/ux/circuit-vis/editor/operationPrompts.js";
import { findOperation } from "../../dist/ux/circuit-vis/utils.js";

/** @type {JSDOM | null} */
let jsdom = null;

beforeEach(() => {
  jsdom = new JSDOM(`<!doctype html><html><body></body></html>`);
  // @ts-expect-error - jsdom typings vs DOM lib mismatch
  globalThis.window = jsdom.window;
  globalThis.document = jsdom.window.document;
  globalThis.HTMLElement = jsdom.window.HTMLElement;
  globalThis.KeyboardEvent = jsdom.window.KeyboardEvent;
});

afterEach(() => {
  // Tear down DOM-level singletons aggressively — `_createConfirmPrompt`
  // installs a document-level keydown listener that's only removed
  // on OK/Cancel click, so a test that asserts a prompt was NEVER
  // opened still needs a clean slate for the next test.
  jsdom?.window.close();
  jsdom = null;
});

/**
 * Query the currently-rendered confirm prompt. Returns null if
 * none is open; useful both for asserting "no prompt was opened"
 * and for grabbing the message / buttons in the open-prompt tests.
 */
function getOpenPrompt() {
  const overlay = document.querySelector(".prompt-overlay");
  if (!overlay) return null;
  const messageElem = overlay.querySelector(".prompt-message");
  const buttons = overlay.querySelectorAll(".prompt-button");
  // Two buttons in this prompt: OK first, Cancel second.
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

// ---------------------------------------------------------------------------
// _deleteOperationWithConfirmation
// ---------------------------------------------------------------------------

test("_deleteOperationWithConfirmation: non-measurement op deletes immediately, no prompt", () => {
  // Fast path: any non-M op (here a plain H) bypasses the
  // consumer-collection branch entirely and dispatches straight
  // to `removeOperation` + `renderFn`. No DOM overlay is created.
  const model = new CircuitModel({
    qubits: [{ id: 0 }],
    componentGrid: [
      {
        components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 0 }] }],
      },
    ],
  });
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
  // returns `[]` (no consumer reads its classical result) still
  // skips the prompt — there's nothing the user could lose by
  // deleting it.
  const model = new CircuitModel({
    qubits: [{ id: 0, numResults: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "measurement",
            gate: "Measure",
            qubits: [{ qubit: 0 }],
            // Produces a result but nobody consumes it.
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
    ],
  });
  const render = makeRenderSpy();

  _deleteOperationWithConfirmation(model, "0,0", render.fn);

  assert.equal(getOpenPrompt(), null, "no prompt for an unread measurement");
  assert.equal(model.componentGrid.length, 0, "M should be removed");
  assert.equal(render.count, 1);
});

test("_deleteOperationWithConfirmation: M with 1 consumer opens a SINGULAR prompt; OK cascades", () => {
  // M produces (qubit=0, result=0); one classically-controlled X
  // consumes it. The prompt body must read in the singular form
  // ("1 dependent operation"), and OK must run the cascade through
  // `removeMeasurementWithDependents` so both ops vanish.
  const model = new CircuitModel({
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "measurement",
            gate: "Measure",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 1 }],
            controls: [{ qubit: 0, result: 0 }],
          },
        ],
      },
    ],
  });
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
  // Pluralization branch: three consumers all reading the same
  // (qubit=0, result=0) classical register. Asserting only on the
  // message form here — the OK-cascade behavior is the same as
  // the singular case.
  const model = new CircuitModel({
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          {
            kind: "measurement",
            gate: "Measure",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 1 }],
            controls: [{ qubit: 0, result: 0 }],
          },
          {
            kind: "unitary",
            gate: "Y",
            targets: [{ qubit: 2 }],
            controls: [{ qubit: 0, result: 0 }],
          },
          {
            kind: "unitary",
            gate: "Z",
            targets: [{ qubit: 3 }],
            controls: [{ qubit: 0, result: 0 }],
          },
        ],
      },
    ],
  });
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
  // The audit's quick-win #2 specifically: pin the cancel path.
  // Model state must be byte-for-byte identical before and after,
  // and `renderFn` must NOT have been called.
  const model = new CircuitModel({
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "measurement",
            gate: "Measure",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 1 }],
            controls: [{ qubit: 0, result: 0 }],
          },
        ],
      },
    ],
  });
  const beforeJSON = JSON.stringify({
    grid: model.componentGrid,
    qubits: model.qubits,
  });
  const render = makeRenderSpy();

  _deleteOperationWithConfirmation(model, "0,0", render.fn);

  const prompt = getOpenPrompt();
  assert.ok(prompt);
  prompt.cancelButton.click();

  assert.equal(getOpenPrompt(), null, "prompt should close on Cancel");
  assert.equal(render.count, 0, "Cancel must NOT trigger a re-render");
  assert.equal(
    JSON.stringify({ grid: model.componentGrid, qubits: model.qubits }),
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
  // threaded as-is (this is the B11a guard for CNOT-control-dot
  // drags — see the doc comment on the wrapper).
  const model = new CircuitModel({
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 0 }] }],
      },
      {
        components: [{ kind: "unitary", gate: "X", targets: [{ qubit: 1 }] }],
      },
    ],
  });
  const render = makeRenderSpy();

  // Swap H from wire 0 → wire 1 (no consumers involved).
  _moveOperationWithConfirmation(
    model,
    "0,0",
    "0,0",
    0,
    1,
    false,
    false,
    render.fn,
  );

  assert.equal(getOpenPrompt(), null, "no prompt for a non-M move");
  // H landed on wire 1; X is still in column 1 (no insertNewColumn).
  const movedH = /** @type {any} */ (findOperation(model.componentGrid, "0,0"));
  assert.equal(movedH.gate, "H");
  assert.equal(movedH.targets[0].qubit, 1);
  assert.equal(render.count, 1);
});

test("_moveOperationWithConfirmation: M with NO consumers moves immediately, no prompt", () => {
  // Second fast path: an M with no classical consumers can move
  // freely. Same passthrough as the non-M case.
  const model = new CircuitModel({
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "measurement",
            gate: "Measure",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
      {
        components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 1 }] }],
      },
    ],
  });
  const render = makeRenderSpy();

  // Move M to column 1 (it'd swap with H there); no consumers,
  // no prompt.
  _moveOperationWithConfirmation(
    model,
    "0,0",
    "1,0",
    0,
    0,
    false,
    false,
    render.fn,
  );

  assert.equal(getOpenPrompt(), null);
  assert.equal(render.count, 1);
});

test("_moveOperationWithConfirmation: M with pure-SURVIVORS consumers shows the update-only message", () => {
  // Survivors-only partition: target column < every consumer's
  // column. The M is moving FORWARD (or staying in place) so
  // every consumer still comes after it; nothing gets deleted.
  // Message must contain the "will be updated" clause and NOT the
  // "will be deleted" clause.
  const model = new CircuitModel({
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        // Column 0: the M.
        components: [
          {
            kind: "measurement",
            gate: "Measure",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
      {
        // Column 1: a consumer.
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 1 }],
            controls: [{ qubit: 0, result: 0 }],
          },
        ],
      },
      {
        // Column 2: another consumer.
        components: [
          {
            kind: "unitary",
            gate: "Y",
            targets: [{ qubit: 2 }],
            controls: [{ qubit: 0, result: 0 }],
          },
        ],
      },
    ],
  });
  const render = makeRenderSpy();

  // Move the M to column 0 (its current spot — but target column
  // is still strictly before columns 1 and 2). Both consumers
  // partition into survivors.
  _moveOperationWithConfirmation(
    model,
    "0,0",
    "0,0",
    0,
    0,
    false,
    false,
    render.fn,
  );

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
  // column (i.e. moving the M past all its consumers). Every
  // consumer flips into the "will be deleted" bucket; no
  // "will be updated" clause appears.
  const model = new CircuitModel({
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        // Column 0: the M.
        components: [
          {
            kind: "measurement",
            gate: "Measure",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
      {
        // Column 1: only consumer.
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 1 }],
            controls: [{ qubit: 0, result: 0 }],
          },
        ],
      },
    ],
  });
  const render = makeRenderSpy();

  // Move M into column 1 (where the consumer currently sits).
  // Target column = consumer's column → `inEarlierColumnThan`
  // is false → consumer is invalidated.
  _moveOperationWithConfirmation(
    model,
    "0,0",
    "1,0",
    0,
    0,
    false,
    false,
    render.fn,
  );

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
  // clauses and the explicit '; ' separator from
  // `_buildMoveMConsumerMessage`.
  const model = new CircuitModel({
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        // Column 0: the M.
        components: [
          {
            kind: "measurement",
            gate: "Measure",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
      {
        // Column 1: consumer #1 — will be invalidated (column ==
        // target column 1 → not in earlier column).
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 1 }],
            controls: [{ qubit: 0, result: 0 }],
          },
        ],
      },
      {
        // Column 2: consumer #2 — will survive (column 2 > target
        // column 1).
        components: [
          {
            kind: "unitary",
            gate: "Y",
            targets: [{ qubit: 2 }],
            controls: [{ qubit: 0, result: 0 }],
          },
        ],
      },
    ],
  });
  const render = makeRenderSpy();

  // Target column 1 → consumer at "1,0" invalidates, consumer at
  // "2,0" survives.
  _moveOperationWithConfirmation(
    model,
    "0,0",
    "1,0",
    0,
    0,
    false,
    false,
    render.fn,
  );

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
  const model = new CircuitModel({
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "measurement",
            gate: "Measure",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 1 }],
            controls: [{ qubit: 0, result: 0 }],
          },
        ],
      },
    ],
  });
  const beforeJSON = JSON.stringify({
    grid: model.componentGrid,
    qubits: model.qubits,
  });
  const render = makeRenderSpy();

  _moveOperationWithConfirmation(
    model,
    "0,0",
    "1,0",
    0,
    0,
    false,
    false,
    render.fn,
  );

  const prompt = getOpenPrompt();
  assert.ok(prompt);
  prompt.cancelButton.click();

  assert.equal(render.count, 0);
  assert.equal(
    JSON.stringify({ grid: model.componentGrid, qubits: model.qubits }),
    beforeJSON,
    "model must be unchanged after Cancel on a move prompt",
  );
});

test("_moveOperationWithConfirmation: M-with-consumers OK cascades through moveMeasurementWithDependents", () => {
  // Sanity-check the OK branch: a mixed partition cascade. After
  // commit, the M has moved to the target column, the survivor
  // had its classical control's `qubit` remapped to the M's new
  // wire, and the invalidated consumer is gone.
  const model = new CircuitModel({
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        // Column 0: the M on wire 0.
        components: [
          {
            kind: "measurement",
            gate: "Measure",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
      {
        // Column 1: invalidated consumer.
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 1 }],
            controls: [{ qubit: 0, result: 0 }],
          },
        ],
      },
      {
        // Column 2: survivor consumer.
        components: [
          {
            kind: "unitary",
            gate: "Y",
            targets: [{ qubit: 2 }],
            controls: [{ qubit: 0, result: 0 }],
          },
        ],
      },
    ],
  });
  const render = makeRenderSpy();

  // Move M from (0,0) on wire 0 → target column 1 on wire 0 (no
  // wire change). Consumer at "1,0" is invalidated; consumer at
  // "2,0" survives.
  _moveOperationWithConfirmation(
    model,
    "0,0",
    "1,0",
    0,
    0,
    false,
    false,
    render.fn,
  );

  const prompt = getOpenPrompt();
  assert.ok(prompt);
  prompt.okButton.click();

  assert.equal(render.count, 1, "OK must trigger exactly one re-render");

  // The X (invalidated) must be gone.
  const allOps = [];
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      allOps.push(op);
    }
  }
  assert.equal(
    allOps.find((o) => /** @type {any} */ (o).gate === "X"),
    undefined,
    "invalidated X consumer must have been cascade-deleted",
  );
  // The Y (survivor) must still exist; its classical control
  // qubit will have been remapped via the cascade. We only assert
  // it still exists here — the exact remap is the contract of
  // `moveMeasurementWithDependents` and is covered by
  // [circuitActions.test.mjs](circuitActions.test.mjs).
  assert.ok(
    allOps.find((o) => /** @type {any} */ (o).gate === "Y"),
    "survivor Y consumer must remain",
  );
});
