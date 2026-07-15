// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Measurement move / delete with downstream consumers.
//
// `collectMeasurementConsumers` walks the grid and finds every op whose classical-ref `(qubit,
// result)` matches one of the M's `results` entries. Both the prompt layer and the cascade actions
// consume its output.
//
// `removeMeasurementWithDependents` deletes a measurement together with its downstream consumers,
// then keeps the surviving circuit consistent. Test surface: predicate-match correctness,
// M-location re-derivation after the cascade collapses columns, and the renumber-then-remap pass
// for surviving Ms whose result indices shift.
//
// `moveMeasurementWithDependents` is the bulk of the new logic: pre-/post-move (qubit, result)
// snapshotting, wire-level renumbering remap propagation, survivor / invalidated partition by
// object identity, and post-mutation overlap resolution for changed visual spans.

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  collectMeasurementConsumers,
  moveMeasurementWithDependents,
  removeMeasurementWithDependents,
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
// collectMeasurementConsumers
// ---------------------------------------------------------------------------

test("collectMeasurementConsumers: empty when no consumer references the M", () => {
  const model = build(circuit(2, [[_mGate(0, 0)], [gate("H", 1)]]));
  assert.equal(
    collectMeasurementConsumers(model.componentGrid, "0,0").length,
    0,
  );
});

test("collectMeasurementConsumers: finds a top-level classically-controlled consumer", () => {
  const model = build(circuit(2, [[_mGate(0, 0)], [_ccx(1, 0, 0)]]));
  const consumers = collectMeasurementConsumers(model.componentGrid, "0,0");
  assert.equal(consumers.length, 1);
  assert.equal(consumers[0].location, "1,0");
});

test("collectMeasurementConsumers: walks into nested children", () => {
  // Consumer is buried two levels deep inside non-classically-controlled groups; the walker still
  // finds it. The wrappers carry no classical ref in their `.controls`.
  const model = build(
    circuit(2, [
      [_mGate(0, 0)],
      [group("Outer", [[group("Inner", [[_ccx(1, 0, 0)]])]])],
    ]),
  );
  const consumers = collectMeasurementConsumers(model.componentGrid, "0,0");
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

test("collectMeasurementConsumers: ancestor groups with propagated .targets are NOT flagged", () => {
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
  const consumers = collectMeasurementConsumers(model.componentGrid, "0,0");
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
  removeMeasurementWithDependents(
    model,
    "0,0",
    consumers.map((c) => c.op),
  );
  // Outer survives with only its unrelated Y child; the consumer X is gone.
  expectOp(at(model, "0,0"), { Outer: { children: [["Y"]] } });
});

test("collectMeasurementConsumers: classical-ref must MATCH (qubit, result); other Ms don't trigger", () => {
  // Two Ms on different wires; the consumer references only M_1.
  const model = build(
    circuit(3, [[_mGate(0, 0)], [_mGate(1, 0)], [_ccx(2, 1, 0)]]),
  );
  assert.equal(
    collectMeasurementConsumers(model.componentGrid, "0,0").length,
    0,
    "M_0 has no consumer (the consumer references M_1's (q1, r0))",
  );
  assert.equal(
    collectMeasurementConsumers(model.componentGrid, "1,0").length,
    1,
    "M_1's consumer is the classically-controlled X",
  );
});

// ---------------------------------------------------------------------------
// removeMeasurementWithDependents
// ---------------------------------------------------------------------------

test("removeMeasurementWithDependents: deletes M and all classical-ref consumers", () => {
  const model = build(
    circuit(3, [[_mGate(0, 0)], [_ccx(1, 0, 0)], [_ccx(2, 0, 0)]]),
  );
  const consumers = collectMeasurementConsumers(model.componentGrid, "0,0");
  assert.equal(consumers.length, 2);
  removeMeasurementWithDependents(
    model,
    "0,0",
    consumers.map((c) => c.op),
  );
  // Every column should be gone.
  expectGrid(model, []);
});

test("removeMeasurementWithDependents: M's location is re-derived after the cascade collapses columns", () => {
  // Consumer alone in col 0 collapses col 0; M shifts from col 1 down to col 0. The action layer
  // re-derives M by ref, not by the now-stale "1,0".
  const model = build(circuit(2, [[_ccx(1, 0, 0)], [_mGate(0, 0)]]));
  const consumers = collectMeasurementConsumers(model.componentGrid, "1,0");
  assert.equal(consumers.length, 1);
  removeMeasurementWithDependents(
    model,
    "1,0",
    consumers.map((c) => c.op),
  );
  expectGrid(model, []);
});

test("removeMeasurementWithDependents: surviving Ms' result-index renumbering propagates to their consumers", () => {
  // M_a → result 0, M_b → result 1, both on wire 0. A consumer references (0, 1) — M_b. Deleting
  // M_a renumbers M_b from result 1 → 0; the consumer's ref must remap to (0, 0) or the next render
  // throws "Classical register ID 1 invalid".
  const model = build(
    circuit(2, [[_mGate(0, 0)], [_mGate(0, 1)], [_ccx(1, 0, 1)]]),
  );
  // M_a has no consumers (the ccx references M_b, not M_a).
  const consumers = collectMeasurementConsumers(model.componentGrid, "0,0");
  assert.equal(consumers.length, 0, "M_a has no direct consumers");

  removeMeasurementWithDependents(model, "0,0", []);

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
// moveMeasurementWithDependents
// ---------------------------------------------------------------------------

test("moveMeasurementWithDependents: surviving consumer's classical-ref tracks the M's new wire", () => {
  // M on wire 0, consumer in a later column on wire 2 with classical-ref (0, 0). M moves down to
  // wire 1; the consumer's ref must become (1, 0).
  const model = build(circuit(3, [[_mGate(0, 0)], [_ccx(2, 0, 0)]]));
  // Target column 0 is strictly before consumer column 1 → survives.
  const moved = moveMeasurementWithDependents(
    model,
    "0,0",
    "0,0",
    0,
    1,
    /* insertNewColumn */ false,
    [],
  );
  assert.ok(moved);

  // Consumer's classical-ref must track M's new wire: (0,0) → (1,0).
  expectOp(at(model, "1,0"), { X: { ctrls: [{ q: 1, r: 0 }] } });
});

test("moveMeasurementWithDependents: invalidated consumer is cascade-deleted", () => {
  // M@col 0, ccx consumer@col 1, unrelated H@col 2. Moving M to "2,0" lands it in a column after
  // the ccx, so the consumer is now in an earlier column — invalidated — and gets deleted.
  const model = build(
    circuit(3, [[_mGate(0, 0)], [_ccx(1, 0, 0)], [gate("H", 2)]]),
  );
  const consumers = collectMeasurementConsumers(model.componentGrid, "0,0");
  // Hand the single consumer in as invalidated directly.
  const moved = moveMeasurementWithDependents(
    model,
    "0,0",
    "2,0",
    0,
    0,
    /* insertNewColumn */ false,
    consumers.map((c) => c.op),
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

test("moveMeasurementWithDependents: consumer of an UNMOVED M whose result index gets renumbered is also remapped", () => {
  // Two Ms on wire 0 (results 0 and 1). A consumer of the SECOND M references (0, 1). Moving the
  // FIRST M to wire 1 renumbers the remaining wire-0 M down to result 0, so the consumer must remap
  // (0, 1) → (0, 0).
  const model = build(
    circuit(3, [[_mGate(0, 0)], [_mGate(0, 1)], [_ccx(2, 0, 1)]]),
  );

  // Move M_first from wire 0 to wire 1. invalidatedConsumers=[] — the consumer is downstream of
  // M_second (unmoved).
  const moved = moveMeasurementWithDependents(
    model,
    "0,0",
    "0,0",
    0,
    1,
    /* insertNewColumn */ false,
    [],
  );
  assert.ok(moved);

  // Consumer of M_second must remap (0,1) → (0,0) after M_first's move triggered the wire-0
  // renumber.
  expectOp(at(model, "2,0"), { X: { ctrls: [{ q: 0, r: 0 }] } });
});

test("moveMeasurementWithDependents: M with no consumers behaves like a regular move", () => {
  // Sanity check: the cascade overhead is a no-op when there's no consumer to remap or invalidate.
  const model = build(circuit(2, [[_mGate(0, 0)]]));
  const moved = moveMeasurementWithDependents(
    model,
    "0,0",
    "0,0",
    0,
    1,
    /* insertNewColumn */ false,
    [],
  );
  assert.ok(moved);
  // M moved from wire 0 to wire 1; no consumer to remap.
  expectOp(at(model, "0,0"), { Measure: { qubits: [1] } });
});

test("moveMeasurementWithDependents: moving an M onto a wire that already has multiple Ms-with-consumers does not double-remap M results", () => {
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
  const moved = moveMeasurementWithDependents(
    model,
    "2,0",
    "0,0",
    1,
    0,
    /* insertNewColumn */ true,
    [],
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
