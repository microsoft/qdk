// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Measurement move / delete with downstream consumers.
//
// `collectSubtreeConsumers` walks the grid and finds every op OUTSIDE a subtree whose classical-ref
// `(qubit, result)` matches a classical register produced anywhere in that subtree. The delete
// prompt layer and the delete cascade action consume its output.
//
// `removeOperationWithDependents` deletes an operation together with its downstream consumers, then
// keeps the surviving circuit consistent. Test surface: predicate-match correctness, location
// re-derivation after the cascade collapses columns, and the renumber-then-remap pass for surviving
// Ms whose result indices shift.
//
// `moveOperationWithDependents` is the bulk of the new logic: it moves, then self-detects the
// consumers the move stranded (via a post-move grid scan) and cascade-deletes them, plus
// post-mutation overlap resolution for changed visual spans. Callers pass no invalidated set; the
// post-move grid is the single source of truth.

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  collectSubtreeConsumers,
  moveOperationWithDependents,
  moveOperation,
  countStrandedConsumers,
  removeOperationWithDependents,
} from "../../../dist/ux/circuit-vis/actions/circuitActions.js";
import {
  at,
  build,
  circuit,
  expectGrid,
  expectOp,
  gate,
  group,
  meas,
} from "../_helpers.mjs";

// Local shorthands over the shared helpers: these suites use the "Measure" gate name (asserted in
// places) and classically-controlled X consumers.
const _mGate = (/** @type {number} */ q, /** @type {number} */ r) =>
  meas(q, { gate: "Measure", result: r });

const _ccx = (
  /** @type {number} */ targetQubit,
  /** @type {number} */ ctrlQubit,
  /** @type {number} */ ctrlResult,
) => gate("X", targetQubit, { ctrls: [{ q: ctrlQubit, r: ctrlResult }] });

// ---------------------------------------------------------------------------
// collectSubtreeConsumers
// ---------------------------------------------------------------------------

test("collectSubtreeConsumers: empty when no consumer references the M", () => {
  const model = build(circuit(2, [[_mGate(0, 0)], [gate("H", 1)]]));
  assert.equal(collectSubtreeConsumers(model.componentGrid, "0,0").length, 0);
});

test("collectSubtreeConsumers: finds a top-level classically-controlled consumer", () => {
  const model = build(circuit(2, [[_mGate(0, 0)], [_ccx(1, 0, 0)]]));
  const consumers = collectSubtreeConsumers(model.componentGrid, "0,0");
  assert.equal(consumers.length, 1);
  assert.equal(consumers[0].location, "1,0");
});

test("collectSubtreeConsumers: walks into nested children", () => {
  // Consumer is buried two levels deep inside non-classically-controlled groups; the walker still
  // finds it. The wrappers carry no classical ref in their `.controls`.
  const model = build(
    circuit(2, [
      [_mGate(0, 0)],
      [group("Outer", [[group("Inner", [[_ccx(1, 0, 0)]])]])],
    ]),
  );
  const consumers = collectSubtreeConsumers(model.componentGrid, "0,0");
  // Only the leaf X is a logical consumer.
  assert.equal(
    consumers.length,
    1,
    `expected only the leaf X to be flagged; got ${JSON.stringify(
      consumers.map((c) => c.op.gate),
    )}`,
  );
  assert.equal(consumers[0].op.gate, "X");
});

test("collectSubtreeConsumers: ancestor groups with propagated .targets are NOT flagged", () => {
  // Simulates the post-`_deepRefreshDerivedTargets` state where the outer group's `.targets` cache
  // has propagated the classical ref upward. Inspecting `.targets` (instead of just leaf consumers)
  // would flag the Outer group and cascade-delete its unrelated sibling Y. The consumer scan must
  // look at leaves only.
  const outer = group("Outer", [
    [_ccx(1, 0, 0)], // the actual consumer
    [gate("Y", 2)], // unrelated sibling — purely quantum, MUST survive
  ]);
  // PROPAGATED cache: rewrite the plain q0 target into the classical ref the inner X would push up
  // through `_deepRefreshDerivedTargets`.
  outer.targets = outer.targets.map((/** @type {any} */ t) =>
    t.qubit === 0 ? { qubit: 0, result: 0 } : t,
  );
  const model = build(circuit(3, [[_mGate(0, 0)], [outer]]));
  const consumers = collectSubtreeConsumers(model.componentGrid, "0,0");
  assert.equal(
    consumers.length,
    1,
    `Outer group with propagated .targets must NOT be flagged; ` +
      `expected only the leaf X. Got ${JSON.stringify(
        consumers.map((c) => c.op.gate),
      )}`,
  );
  assert.equal(consumers[0].op.gate, "X");

  // End-to-end: removing the M with this consumer set must leave the Y intact inside the
  // (now-shrunken) Outer group.
  removeOperationWithDependents(
    model,
    "0,0",
    consumers.map((c) => c.op),
  );
  // Outer survives with only its unrelated Y child; the consumer X is gone.
  expectOp(at(model, "0,0"), { Outer: { children: [["Y"]] } });
});

test("collectSubtreeConsumers: classical-ref must MATCH (qubit, result); other Ms don't trigger", () => {
  // Two Ms on different wires; the consumer references only M_1.
  const model = build(
    circuit(3, [[_mGate(0, 0)], [_mGate(1, 0)], [_ccx(2, 1, 0)]]),
  );
  assert.equal(
    collectSubtreeConsumers(model.componentGrid, "0,0").length,
    0,
    "M_0 has no consumer (the consumer references M_1's (q1, r0))",
  );
  assert.equal(
    collectSubtreeConsumers(model.componentGrid, "1,0").length,
    1,
    "M_1's consumer is the classically-controlled X",
  );
});

// ---------------------------------------------------------------------------
// removeOperationWithDependents
// ---------------------------------------------------------------------------

test("removeOperationWithDependents: deletes M and all classical-ref consumers", () => {
  const model = build(
    circuit(3, [[_mGate(0, 0)], [_ccx(1, 0, 0)], [_ccx(2, 0, 0)]]),
  );
  const consumers = collectSubtreeConsumers(model.componentGrid, "0,0");
  assert.equal(consumers.length, 2);
  removeOperationWithDependents(
    model,
    "0,0",
    consumers.map((c) => c.op),
  );
  // Every column should be gone.
  expectGrid(model, []);
});

test("removeOperationWithDependents: M's location is re-derived after the cascade collapses columns", () => {
  // Consumer alone in col 0 collapses col 0; M shifts from col 1 down to col 0. The action layer
  // re-derives M by ref, not by the now-stale "1,0".
  const model = build(circuit(2, [[_ccx(1, 0, 0)], [_mGate(0, 0)]]));
  const consumers = collectSubtreeConsumers(model.componentGrid, "1,0");
  assert.equal(consumers.length, 1);
  removeOperationWithDependents(
    model,
    "1,0",
    consumers.map((c) => c.op),
  );
  expectGrid(model, []);
});

test("removeOperationWithDependents: surviving Ms' result-index renumbering propagates to their consumers", () => {
  // M_a → result 0, M_b → result 1, both on wire 0. A consumer references (0, 1) — M_b. Deleting
  // M_a renumbers M_b from result 1 → 0; the consumer's ref must remap to (0, 0) or the next render
  // throws "Classical register ID 1 invalid".
  const model = build(
    circuit(2, [[_mGate(0, 0)], [_mGate(0, 1)], [_ccx(1, 0, 1)]]),
  );
  // M_a has no consumers (the ccx references M_b, not M_a).
  const consumers = collectSubtreeConsumers(model.componentGrid, "0,0");
  assert.equal(consumers.length, 0, "M_a has no direct consumers");

  removeOperationWithDependents(model, "0,0", []);

  // The surviving ccx's classical-ref must remap (0,1) → (0,0) to track M_b's new result index.
  expectOp(at(model, "1,0"), { X: { ctrls: [{ q: 0, r: 0 }] } });
  // And the model's per-wire numResults must reflect the single surviving M.
  assert.equal(
    model.qubits[0].numResults,
    1,
    "wire 0 must report exactly 1 classical register after deletion",
  );
});

// ---------------------------------------------------------------------------
// moveOperationWithDependents
// ---------------------------------------------------------------------------

test("moveOperationWithDependents: surviving consumer's classical-ref tracks the M's new wire", () => {
  // M on wire 0, consumer in a later column on wire 2 with classical-ref (0, 0). M moves down to
  // wire 1; the consumer's ref must become (1, 0).
  const model = build(circuit(3, [[_mGate(0, 0)], [_ccx(2, 0, 0)]]));
  // Target column 0 is strictly before consumer column 1 → survives.
  const moved = moveOperationWithDependents(
    model,
    "0,0",
    "0,0",
    0,
    1,
    /* movingControl */ false,
    /* insertNewColumn */ false,
  );
  assert.ok(moved);

  // Consumer's classical-ref must track M's new wire: (0,0) → (1,0).
  expectOp(at(model, "1,0"), { X: { ctrls: [{ q: 1, r: 0 }] } });
});

test("moveOperationWithDependents: invalidated consumer is cascade-deleted", () => {
  // M@col 0, ccx consumer@col 1, unrelated H@col 2. Moving M to "2,0" lands it in a column after
  // the ccx, so the consumer is now in an earlier column — stranded — and the wrapper self-detects
  // and cascade-deletes it.
  const model = build(
    circuit(3, [[_mGate(0, 0)], [_ccx(1, 0, 0)], [gate("H", 2)]]),
  );
  const moved = moveOperationWithDependents(
    model,
    "0,0",
    "2,0",
    0,
    0,
    /* movingControl */ false,
    /* insertNewColumn */ false,
  );
  assert.ok(moved);
  // The ccx should be gone; only M and H remain.
  const remainingGates = model.componentGrid
    .flatMap((/** @type {any} */ col) => col.components)
    .map((/** @type {any} */ op) => op.gate)
    .sort();
  assert.deepEqual(
    remainingGates,
    ["H", "Measure"],
    `ccx must be cascade-deleted; got ${JSON.stringify(remainingGates)}`,
  );
});

test("moveOperationWithDependents: consumer of an UNMOVED M whose result index gets renumbered is also remapped", () => {
  // Two Ms on wire 0 (results 0 and 1). A consumer of the SECOND M references (0, 1). Moving the
  // FIRST M to wire 1 renumbers the remaining wire-0 M down to result 0, so the consumer must remap
  // (0, 1) → (0, 0).
  const model = build(
    circuit(3, [[_mGate(0, 0)], [_mGate(0, 1)], [_ccx(2, 0, 1)]]),
  );

  // Move M_first from wire 0 to wire 1. The consumer is downstream of M_second (unmoved), so the
  // move strands nothing; the wrapper self-detects zero to delete.
  const moved = moveOperationWithDependents(
    model,
    "0,0",
    "0,0",
    0,
    1,
    /* movingControl */ false,
    /* insertNewColumn */ false,
  );
  assert.ok(moved);

  // Consumer of M_second must remap (0,1) → (0,0) after M_first's move triggered the wire-0
  // renumber.
  expectOp(at(model, "2,0"), { X: { ctrls: [{ q: 0, r: 0 }] } });
});

test("moveOperationWithDependents: M with no consumers behaves like a regular move", () => {
  // Sanity check: the cascade overhead is a no-op when there's no consumer to remap or invalidate.
  const model = build(circuit(2, [[_mGate(0, 0)]]));
  const moved = moveOperationWithDependents(
    model,
    "0,0",
    "0,0",
    0,
    1,
    /* movingControl */ false,
    /* insertNewColumn */ false,
  );
  assert.ok(moved);
  // M moved from wire 0 to wire 1; no consumer to remap.
  expectOp(at(model, "0,0"), { Measure: { qubits: [1] } });
});

test("moveOperationWithDependents: moving an M onto a wire that already has multiple Ms-with-consumers does not double-remap M results", () => {
  // `_applyClassicalRefRemap` must skip producer registers (`.results` on measurements) and only
  // remap consumer classical refs. Otherwise, after `_updateMeasurementLines` authoritatively
  // renumbers result indices on the affected wire, walking those producer values back through the
  // consumer remap can chain-react: each M's new result index happens to match another M's pre-move
  // key, so `.results` gets remapped a second time — collapsing into duplicate result indices and
  // orphaning consumers whose target M had its `.results` clobbered.
  //
  // Setup: three Ms with consumers spread across two wires. Wire 0 already has M_a (r=0) and M_b
  // (r=1), each with a downstream classically-controlled gate. Wire 1 has M_c (r=0) with its own
  // consumer. We move M_c onto wire 0 in front of M_a, which forces _updateMeasurementLines to
  // renumber wire 0 as: M_c=0, M_a=1, M_b=2.
  const model = build(
    circuit(3, [
      [_mGate(0, 0)], // col 0: M_a (wire 0, r=0)
      [_mGate(0, 1)], // col 1: M_b (wire 0, r=1)
      [_mGate(1, 0)], // col 2: M_c (wire 1, r=0)
      [_ccx(2, 0, 0)], // col 3: C_a → "0:0"
      [_ccx(2, 0, 1)], // col 4: C_b → "0:1"
      [_ccx(2, 1, 0)], // col 5: C_c → "1:0"
    ]),
  );

  // Move M_c (col 2, idx 0) to wire 0, inserting a fresh column at position 0. After the move, wire
  // 0's doc order is M_c, M_a, M_b → _updateMeasurementLines assigns r=0, 1, 2 respectively. The
  // keyRemap must rewrite every consumer: C_a "0:0" → "0:1" (M_a moved down) C_b "0:1" → "0:2" (M_b
  // moved down) C_c "1:0" → "0:0" (M_c switched wires)
  const moved = moveOperationWithDependents(
    model,
    "2,0",
    "0,0",
    1,
    0,
    /* movingControl */ false,
    /* insertNewColumn */ true,
  );
  assert.ok(moved);

  // Collect every M and every classically-controlled consumer in the post-move grid.
  /** @type {any[]} */
  const ms = [];
  /** @type {any[]} */
  const consumers = [];
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      if (op.kind === "measurement") {
        ms.push(op);
      } else if (
        op.kind === "unitary" &&
        op.controls &&
        op.controls.some((/** @type {any} */ c) => c.result !== undefined)
      ) {
        consumers.push(op);
      }
    }
  }
  assert.equal(ms.length, 3, "all three Ms must still be present");
  assert.equal(
    consumers.length,
    3,
    "all three consumers must still be present",
  );

  // INVARIANT 1: every M's `.results` entry has a unique (qubit, result) key. The bug previously
  // caused two Ms to share the same `.results` value.
  /** @type {Set<string>} */
  const resultKeys = new Set();
  for (const m of ms) {
    for (const r of m.results) {
      const key = `${r.qubit}:${r.result}`;
      assert.ok(
        !resultKeys.has(key),
        `duplicate M.results key ${key} — at least two Ms claim the same classical register`,
      );
      resultKeys.add(key);
    }
  }

  // INVARIANT 2: every consumer's classical ref points at a key that some M actually produces. The
  // bug previously left consumers pointing at result indices no M owned (orphaned classical-control
  // indicator).
  for (const consumer of consumers) {
    const classicalRef = consumer.controls.find(
      (/** @type {any} */ c) => c.result !== undefined,
    );
    const key = `${classicalRef.qubit}:${classicalRef.result}`;
    assert.ok(
      resultKeys.has(key),
      `consumer references ${key}, but no M produces it (orphaned indicator)`,
    );
  }

  // INVARIANT 3: on wire 0, result indices are assigned in doc order starting at 0 (the contract of
  // _updateMeasurementLines). Verifies the renumbering itself wasn't corrupted by the remap walk.
  /** @type {number[]} */
  const wire0ResultsInDocOrder = [];
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      if (op.kind === "measurement" && op.qubits[0].qubit === 0) {
        wire0ResultsInDocOrder.push(
          /** @type {number} */ (op.results[0].result),
        );
      }
    }
  }
  assert.deepEqual(
    wire0ResultsInDocOrder,
    [0, 1, 2],
    "wire 0's three Ms must have result indices 0, 1, 2 in doc order",
  );
});

// ---------------------------------------------------------------------------
// Plain moveOperation on a bare measurement (no dependents wrapper)
//
// The drag layer now routes every move through `moveOperationWithDependents`, but `moveOperation`
// on its own must still keep classical links consistent: moving a bare M off a shared wire
// renumbers the OTHER Ms on that wire via the classical-result token pass, which must repoint their
// consumers so the links don't dangle. This suite exercises `moveOperation` directly to pin that
// down independent of the wrapper.
// ---------------------------------------------------------------------------

test("moveOperation: bare M with no consumers, moved off a shared wire, reindexes a sibling M's consumer", () => {
  // Wire 0 holds M_a (r=0, NO consumer) then M_b (r=1, consumed by ccx on wire 2). Moving M_a to
  // wire 1 leaves M_b as the only wire-0 M → it reindexes to r=0, so its consumer must follow
  // (0,1) → (0,0).
  const model = build(
    circuit(3, [[_mGate(0, 0)], [_mGate(0, 1)], [_ccx(2, 0, 1)]]),
  );
  const moved = moveOperation(model, "0,0", "0,0", 0, 1, false, false);
  assert.ok(moved);

  // M_a landed on wire 1 as its own r=0.
  expectOp(at(model, "0,0"), { Measure: { qubits: [1] } });
  // M_b's consumer must have been repointed to the sibling's new index.
  expectOp(at(model, "2,0"), { X: { ctrls: [{ q: 0, r: 0 }] } });
});

test("moveOperation: bare M with no consumers, moved onto a wire with a consumed M, reindexes that M's consumer", () => {
  // M_x on wire 1 (r=0, consumed by ccx). A no-consumer M on wire 0 moves onto wire 1 in an earlier
  // column, so M_x reindexes to r=1 and its consumer must follow (1,0) → (1,1).
  const model = build(
    circuit(3, [[_mGate(1, 0)], [_mGate(0, 0)], [_ccx(2, 1, 0)]]),
  );
  // Move the wire-0 M (col 1) onto wire 1, inserting a fresh earlier column so it precedes M_x.
  const moved = moveOperation(model, "1,0", "0,0", 0, 1, false, true);
  assert.ok(moved);

  // Find M_x's consumer and assert it now references (1, 1).
  /** @type {any} */
  let consumer = null;
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      if (op.kind === "unitary" && op.gate === "X") consumer = op;
    }
  }
  assert.ok(consumer, "consumer ccx must still exist");
  const ref = consumer.controls.find(
    (/** @type {any} */ c) => c.result !== undefined,
  );
  assert.deepEqual(
    { qubit: ref.qubit, result: ref.result },
    { qubit: 1, result: 1 },
    "consumer of the pushed-down M must track its new result index",
  );
});

test("moveOperation: bare M carries its own consumer onto the new wire", () => {
  // A single M on wire 0 (r=0) with a downstream consumer. Moving the M to wire 1 must repoint the
  // consumer (0,0) → (1,0).
  const model = build(circuit(3, [[_mGate(0, 0)], [_ccx(2, 0, 0)]]));
  const moved = moveOperation(model, "0,0", "0,0", 0, 1, false, false);
  assert.ok(moved);
  expectOp(at(model, "0,0"), { Measure: { qubits: [1] } });
  expectOp(at(model, "1,0"), { X: { ctrls: [{ q: 1, r: 0 }] } });
});

// ---------------------------------------------------------------------------
// countStrandedConsumers agrees with the committed cascade
//
// The prompt layer counts stranded consumers analytically (no move performed); the commit path
// (`moveOperationWithDependents`) re-derives the set from the real post-move grid. If the two ever
// disagree, the confirmation dialog's "delete N" would lie. These tests pin agreement across the
// injection-decision branches: plain merge, forced new column, overlap-forced injection, and a
// group carrying several producers.
// ---------------------------------------------------------------------------

// Count every classically-controlled consumer op anywhere in the grid. Survivors keep their control;
// only stranded consumers are deleted, so (before − after) is exactly the cascade-delete count.
const _countConsumers = (/** @type {any} */ model) => {
  let n = 0;
  const walk = (/** @type {any[]} */ grid) => {
    for (const col of grid) {
      for (const op of col.components) {
        if (
          op.kind === "unitary" &&
          op.controls?.some((/** @type {any} */ c) => c.result !== undefined)
        ) {
          n++;
        }
        if (op.children) walk(op.children);
      }
    }
  };
  walk(model.componentGrid);
  return n;
};

// Assert the predictor's count equals the number of consumers the real commit deletes. `buildModel`
// is called twice so the prediction and the commit each run on a pristine circuit.
const _assertPreviewMatchesCommit = (
  /** @type {() => any} */ buildModel,
  /** @type {[string, string, number, number, boolean, boolean]} */ moveArgs,
  /** @type {string} */ label,
) => {
  const predicted = countStrandedConsumers(buildModel(), ...moveArgs);
  const model = buildModel();
  const before = _countConsumers(model);
  moveOperationWithDependents(model, ...moveArgs);
  const deleted = before - _countConsumers(model);
  assert.equal(
    predicted,
    deleted,
    `${label}: preview predicted ${predicted}, commit deleted ${deleted}`,
  );
};

test("countStrandedConsumers agrees with commit: M merged past its consumer", () => {
  // M moves into a later column (past its consumer), no injection → consumer stranded.
  _assertPreviewMatchesCommit(
    () => build(circuit(3, [[_mGate(0, 0)], [_ccx(1, 0, 0)], [gate("H", 2)]])),
    ["0,0", "2,0", 0, 0, false, false],
    "merge past consumer",
  );
});

test("countStrandedConsumers agrees with commit: insertNewColumn just left of consumer", () => {
  // Forced new column spliced in AT the consumer's column → M lands strictly before it → survives.
  _assertPreviewMatchesCommit(
    () => build(circuit(3, [[_mGate(0, 0)], [gate("H", 1)], [_ccx(1, 0, 0)]])),
    ["0,0", "2,0", 0, 0, false, true],
    "insertNewColumn before consumer",
  );
});

test("countStrandedConsumers agrees with commit: overlap-forced injection at the target column", () => {
  // The target column already holds a gate on the M's landing wire, so addOp is forced to splice a
  // new column even without the flag — the predictor must replay that overlap decision.
  _assertPreviewMatchesCommit(
    () => build(circuit(3, [[_mGate(0, 0)], [_ccx(1, 0, 0)], [gate("X", 0)]])),
    ["0,0", "2,0", 0, 0, false, false],
    "overlap-forced injection",
  );
});

test("countStrandedConsumers agrees with commit: group carrying two producers past both consumers", () => {
  // The whole group acts as one producer at its landed column; both external consumers strand in a
  // single pass and the predictor must count both.
  _assertPreviewMatchesCommit(
    () =>
      build(
        circuit(4, [
          [group("G", [[_mGate(0, 0), _mGate(1, 0)]])],
          [_ccx(2, 0, 0)],
          [_ccx(3, 1, 0)],
          [gate("H", 0)],
        ]),
      ),
    ["0,0", "3,0", 0, 0, false, false],
    "group two producers past both",
  );
});

test("countStrandedConsumers agrees with commit: move strands nothing (survivor repoint only)", () => {
  // A wire-changing move that keeps the M before its consumer: no deletion, predictor must report 0.
  _assertPreviewMatchesCommit(
    () => build(circuit(3, [[_mGate(0, 0)], [_ccx(1, 0, 0)]])),
    ["0,0", "0,0", 0, 2, false, false],
    "survivor repoint only",
  );
});
