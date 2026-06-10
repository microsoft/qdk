// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Measurement move / delete with downstream consumers.
//
// `collectMeasurementConsumers` walks the grid and finds every op
// whose classical-ref `(qubit, result)` matches one of the M's
// `results` entries. Both the prompt layer and the cascade
// actions consume its output.
//
// `removeMeasurementWithDependents` is a thin orchestration over
// `findAndRemoveOperations` + `removeOperation`. Test surface:
// predicate-match correctness, M-location re-derivation after the
// cascade collapses columns, and the renumber-then-remap pass for
// surviving Ms whose result indices shift.
//
// `moveMeasurementWithDependents` is the bulk of the new logic:
// pre-/post-move (qubit, result) snapshotting, wire-level
// renumbering remap propagation, survivor / invalidated
// partition by object identity, and post-mutation overlap
// resolution for changed visual spans.

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import { CircuitModel } from "../../../dist/ux/circuit-vis/data/circuitModel.js";
import {
  collectMeasurementConsumers,
  moveMeasurementWithDependents,
  removeMeasurementWithDependents,
} from "../../../dist/ux/circuit-vis/actions/circuitActions.js";

const _mGate = (/** @type {number} */ q, /** @type {number} */ r) => ({
  kind: "measurement",
  gate: "Measure",
  qubits: [{ qubit: q }],
  results: [{ qubit: q, result: r }],
});

const _ccx = (
  /** @type {number} */ targetQubit,
  /** @type {number} */ ctrlQubit,
  /** @type {number} */ ctrlResult,
) => ({
  kind: "unitary",
  gate: "X",
  targets: [{ qubit: targetQubit }],
  controls: [{ qubit: ctrlQubit, result: ctrlResult }],
});

// ---------------------------------------------------------------------------
// collectMeasurementConsumers
// ---------------------------------------------------------------------------

test("collectMeasurementConsumers: empty when no consumer references the M", () => {
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      { components: [_mGate(0, 0)] },
      {
        components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 1 }] }],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  assert.equal(
    collectMeasurementConsumers(model.componentGrid, "0,0").length,
    0,
  );
});

test("collectMeasurementConsumers: finds a top-level classically-controlled consumer", () => {
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      { components: [_mGate(0, 0)] },
      { components: [_ccx(1, 0, 0)] },
    ],
  };
  const model = new CircuitModel(circuit);
  const consumers = collectMeasurementConsumers(model.componentGrid, "0,0");
  assert.equal(consumers.length, 1);
  assert.equal(consumers[0].location, "1,0");
});

test("collectMeasurementConsumers: walks into nested children", () => {
  // Consumer is buried two levels deep inside a non-classically-
  // controlled group; the walker still finds it.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      { components: [_mGate(0, 0)] },
      {
        components: [
          {
            kind: "unitary",
            gate: "Outer",
            targets: [{ qubit: 1 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "Inner",
                    targets: [{ qubit: 1 }],
                    children: [{ components: [_ccx(1, 0, 0)] }],
                  },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const consumers = collectMeasurementConsumers(model.componentGrid, "0,0");
  // Only the leaf X is a logical consumer. The Outer / Inner
  // wrappers don't carry the classical ref in their `.controls`,
  // so they don't count — even though `.targets` cache propagation
  // (when present) would include it.
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
  // Simulates the post-`_deepRefreshDerivedTargets` state where
  // the outer group's `.targets` cache has propagated the classical
  // ref upward. Inspecting `.targets` (instead of just leaf
  // consumers) would flag the Outer group and cascade-delete its
  // unrelated sibling Y. The consumer scan must look at leaves only.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      { components: [_mGate(0, 0)] },
      {
        components: [
          {
            kind: "unitary",
            gate: "Outer",
            // PROPAGATED cache: classical ref from inner X is here.
            targets: [{ qubit: 1 }, { qubit: 2 }, { qubit: 0, result: 0 }],
            children: [
              {
                components: [_ccx(1, 0, 0)], // the actual consumer
              },
              {
                components: [
                  // Unrelated sibling — purely quantum, independent
                  // of M. MUST survive a cascade-delete of M.
                  {
                    kind: "unitary",
                    gate: "Y",
                    targets: [{ qubit: 2 }],
                  },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
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

  // End-to-end: removing the M with this consumer set must leave
  // the Y intact inside the (now-shrunken) Outer group.
  removeMeasurementWithDependents(
    model,
    "0,0",
    consumers.map((c) => c.op),
  );
  // Outer should still exist with the Y child preserved.
  const outerOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  assert.equal(outerOp?.gate, "Outer", "Outer group must survive");
  const survivingGates = outerOp.children
    .flatMap((/** @type {any} */ col) => col.components)
    .map((/** @type {any} */ op) => op.gate);
  assert.ok(
    survivingGates.includes("Y"),
    `Unrelated sibling Y must survive; got children ${JSON.stringify(survivingGates)}`,
  );
  assert.ok(
    !survivingGates.includes("X"),
    `X (the true consumer) must be cascade-deleted; got children ${JSON.stringify(survivingGates)}`,
  );
});

test("collectMeasurementConsumers: classical-ref must MATCH (qubit, result); other Ms don't trigger", () => {
  // Two Ms, on different wires. Consumer references only one of
  // them. The other M's `collectMeasurementConsumers` returns
  // empty.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      { components: [_mGate(0, 0)] },
      { components: [_mGate(1, 0)] },
      { components: [_ccx(2, 1, 0)] }, // consumes M_1
    ],
  };
  const model = new CircuitModel(circuit);
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
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      { components: [_mGate(0, 0)] },
      { components: [_ccx(1, 0, 0)] },
      { components: [_ccx(2, 0, 0)] },
    ],
  };
  const model = new CircuitModel(circuit);
  const consumers = collectMeasurementConsumers(model.componentGrid, "0,0");
  assert.equal(consumers.length, 2);
  removeMeasurementWithDependents(
    model,
    "0,0",
    consumers.map((c) => c.op),
  );
  // Every column should be gone.
  assert.equal(
    model.componentGrid.length,
    0,
    `expected empty grid; got ${JSON.stringify(model.componentGrid)}`,
  );
});

test("removeMeasurementWithDependents: M's location is re-derived after the cascade collapses columns", () => {
  // Consumer in col 0 alone collapses col 0; M was in col 1 and
  // shifts down to col 0. Naive use of the original location
  // string "1,0" would either miss M (post-cascade) or hit the
  // wrong op. The action layer re-derives by ref.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      { components: [_ccx(1, 0, 0)] }, // consumer ALONE in col 0
      { components: [_mGate(0, 0)] }, // M in col 1
    ],
  };
  const model = new CircuitModel(circuit);
  const consumers = collectMeasurementConsumers(model.componentGrid, "1,0");
  assert.equal(consumers.length, 1);
  removeMeasurementWithDependents(
    model,
    "1,0",
    consumers.map((c) => c.op),
  );
  assert.equal(
    model.componentGrid.length,
    0,
    `expected empty grid after cascade; got ${JSON.stringify(model.componentGrid)}`,
  );
});

test("removeMeasurementWithDependents: surviving Ms' result-index renumbering propagates to their consumers", () => {
  // Two Ms on wire 0: M_a → result 0, M_b → result 1.
  // A consumer references (0, 1) — i.e. M_b.
  // Delete M_a. Its only consumer is itself (none in this case),
  // so the consumer set is empty. But the tail-end
  // _updateMeasurementLines sweep renumbers M_b from result 1
  // → result 0 to close the gap. The consumer's (0, 1) must be
  // remapped to (0, 0) or the next render throws
  // "Classical register ID 1 invalid for qubit ID 0 with 1
  // classical register(s)".
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      { components: [_mGate(0, 0)] }, // M_a (to be deleted)
      { components: [_mGate(0, 1)] }, // M_b (survives)
      { components: [_ccx(1, 0, 1)] }, // consumes M_b
    ],
  };
  const model = new CircuitModel(circuit);
  // M_a has no consumers (the ccx references M_b, not M_a).
  const consumers = collectMeasurementConsumers(model.componentGrid, "0,0");
  assert.equal(consumers.length, 0, "M_a has no direct consumers");

  removeMeasurementWithDependents(model, "0,0", []);

  // Locate the surviving ccx and verify its classical-ref was
  // remapped from (0, 1) → (0, 0) to track M_b's new result idx.
  /** @type {any} */
  let consumerOp;
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      if (op.kind === "unitary" && op.gate === "X") {
        consumerOp = op;
      }
    }
  }
  assert.ok(consumerOp, "consumer must still exist");
  const classicalRef = consumerOp.controls.find(
    (/** @type {any} */ c) => c.result !== undefined,
  );
  assert.deepEqual(
    { qubit: classicalRef.qubit, result: classicalRef.result },
    { qubit: 0, result: 0 },
    "consumer of M_b must remap (0,1) → (0,0) after M_a's deletion renumbered M_b",
  );
  // And the model's per-wire numResults must reflect the single
  // surviving M.
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
  // M on wire 0, consumer in a later column on wire 2 with
  // classical-ref (0, 0). M moves DOWN to wire 1 (and the column
  // stays at the original col 0, just one wire over).
  // Expected: consumer's classical-ref becomes (1, 0).
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      { components: [_mGate(0, 0)] },
      { components: [_ccx(2, 0, 0)] },
    ],
  };
  const model = new CircuitModel(circuit);
  // Survivor partition: target column 0 is strictly before
  // consumer column 1 → consumer survives.
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

  // The consumer is in col 1 (target's col was 0, consumer was
  // at col 1 pre-move). Find it.
  const consumerOp = /** @type {any} */ (model.componentGrid[1].components[0]);
  const classicalRef = consumerOp.controls.find(
    (/** @type {any} */ c) => c.result !== undefined,
  );
  assert.deepEqual(
    { qubit: classicalRef.qubit, result: classicalRef.result },
    { qubit: 1, result: 0 },
    "consumer's classical-ref must track M's new wire",
  );
});

test("moveMeasurementWithDependents: invalidated consumer is cascade-deleted", () => {
  // M at col 0, ccx (consumer) at col 1, unrelated H at col 2.
  // Move M to "2,0" (target slot in col 2 alongside H on wire 2,
  // no collision since M's wire 0 is disjoint from H's wire 2).
  //
  // Post-move (pre-cascade): col 0=ccx (was col 1; original col 0
  // collapsed when M moved out), col 1=[M, H] (was col 2 with M
  // inserted at slot 0). Now M is in col 1 and ccx is in col 0
  // — consumer is in an earlier column, invalidated by definition.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      { components: [_mGate(0, 0)] },
      { components: [_ccx(1, 0, 0)] },
      { components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 2 }] }] },
    ],
  };
  const model = new CircuitModel(circuit);
  const consumers = collectMeasurementConsumers(model.componentGrid, "0,0");
  // Caller (prompt layer) would partition; here we hand the
  // single consumer in as invalidated directly.
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
  // Two Ms on wire 0 (results 0 and 1). A consumer of the SECOND
  // M references (0, 1). Move the FIRST M to a different wire.
  // The remaining M on wire 0 gets renumbered down to result 0 by
  // _updateMeasurementLines. The consumer must be remapped from
  // (0, 1) → (0, 0) to track the renumbering.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      { components: [_mGate(0, 0)] }, // M_first, result 0
      { components: [_mGate(0, 1)] }, // M_second, result 1
      { components: [_ccx(2, 0, 1)] }, // consumer of M_second
    ],
  };
  const model = new CircuitModel(circuit);

  // Move M_first from wire 0 to wire 1. Its result key changes
  // from (0, 0) to (1, 0). M_second on wire 0 was result 1;
  // after the move it gets renumbered to result 0.
  // The consumer's (0, 1) must become (0, 0) to track M_second.
  //
  // Pass invalidatedConsumers=[] — we're not invalidating
  // anything; the consumer is downstream of M_second (unmoved),
  // not M_first (moved).
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

  // Find the consumer in the grid (its location may have shifted
  // — locate by gate).
  /** @type {any} */
  let consumerOp;
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      if (op.kind === "unitary" && op.gate === "X") {
        consumerOp = op;
        break;
      }
    }
    if (consumerOp) break;
  }
  assert.ok(consumerOp, "consumer must still exist");
  const classicalRef = consumerOp.controls.find(
    (/** @type {any} */ c) => c.result !== undefined,
  );
  assert.deepEqual(
    { qubit: classicalRef.qubit, result: classicalRef.result },
    { qubit: 0, result: 0 },
    "consumer of M_second must remap (0,1) → (0,0) after M_first's move triggered the wire-0 renumber",
  );
});

test("moveMeasurementWithDependents: M with no consumers behaves like a regular move", () => {
  // Sanity check: the cascade overhead is a no-op when there's
  // no consumer to remap or invalidate.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [{ components: [_mGate(0, 0)] }],
  };
  const model = new CircuitModel(circuit);
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
  const m = /** @type {any} */ (model.componentGrid[0].components[0]);
  assert.equal(m.qubits[0].qubit, 1);
});

test("moveMeasurementWithDependents: moving an M onto a wire that already has multiple Ms-with-consumers does not double-remap M results", () => {
  // `_applyClassicalRefRemap` must skip producer registers
  // (`.results` on measurements) and only remap consumer
  // classical refs. Otherwise, after `_updateMeasurementLines`
  // authoritatively renumbers result indices on the affected
  // wire, walking those producer values back through the
  // consumer remap can chain-react: each M's new result index
  // happens to match another M's pre-move key, so `.results`
  // gets remapped a second time — collapsing into duplicate
  // result indices and orphaning consumers whose target M had
  // its `.results` clobbered.
  //
  // Setup: three Ms with consumers spread across two wires.
  // Wire 0 already has M_a (r=0) and M_b (r=1), each with a
  // downstream classically-controlled gate. Wire 1 has M_c
  // (r=0) with its own consumer. We move M_c onto wire 0 in
  // front of M_a, which forces _updateMeasurementLines to
  // renumber wire 0 as: M_c=0, M_a=1, M_b=2.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      { components: [_mGate(0, 0)] }, // col 0: M_a (wire 0, r=0)
      { components: [_mGate(0, 1)] }, // col 1: M_b (wire 0, r=1)
      { components: [_mGate(1, 0)] }, // col 2: M_c (wire 1, r=0)
      { components: [_ccx(2, 0, 0)] }, // col 3: C_a → "0:0"
      { components: [_ccx(2, 0, 1)] }, // col 4: C_b → "0:1"
      { components: [_ccx(2, 1, 0)] }, // col 5: C_c → "1:0"
    ],
  };
  const model = new CircuitModel(circuit);

  // Move M_c (col 2, idx 0) to wire 0, inserting a fresh column
  // at position 0. After the move, wire 0's doc order is
  // M_c, M_a, M_b → _updateMeasurementLines assigns
  // r=0, 1, 2 respectively. The keyRemap must rewrite every
  // consumer:
  //   C_a "0:0" → "0:1" (M_a moved down)
  //   C_b "0:1" → "0:2" (M_b moved down)
  //   C_c "1:0" → "0:0" (M_c switched wires)
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

  // Collect every M and every classically-controlled consumer
  // in the post-move grid.
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

  // INVARIANT 1: every M's `.results` entry has a unique
  // (qubit, result) key. The bug previously caused two Ms to
  // share the same `.results` value.
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

  // INVARIANT 2: every consumer's classical ref points at a key
  // that some M actually produces. The bug previously left
  // consumers pointing at result indices no M owned (orphaned
  // classical-control indicator).
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

  // INVARIANT 3: on wire 0, result indices are assigned in
  // doc order starting at 0 (the contract of
  // _updateMeasurementLines). Verifies the renumbering itself
  // wasn't corrupted by the remap walk.
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
