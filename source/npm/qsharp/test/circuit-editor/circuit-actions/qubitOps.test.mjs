// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// `moveQubit` / `removeQubit` and their interaction with classical-control consumers of
// measurements. Exercises the wire-permutation contract: every register reference (top-level,
// nested, cached `.targets`, and classical-ref consumers) gets rewritten by the same 1-to-1
// function, with no result-index renumbering.

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  moveQubit,
  removeQubit,
  removeQubitWithDependents,
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

// Local shorthands over the shared helpers.
const _mGate = (/** @type {number} */ q, /** @type {number} */ r) =>
  meas(q, { gate: "Measure", result: r });

const _ccx = (
  /** @type {number} */ targetQubit,
  /** @type {number} */ ctrlQubit,
  /** @type {number} */ ctrlResult,
) => gate("X", targetQubit, { ctrls: [{ q: ctrlQubit, r: ctrlResult }] });

// ---------------------------------------------------------------------------
// moveQubit / removeQubit (flat-grid base cases)
// ---------------------------------------------------------------------------

test("moveQubit swaps register references and reorders ops within a column", () => {
  const model = build(circuit(2, [[gate("X", 0), gate("H", 1)]]));

  moveQubit(
    model,
    /* sourceWire */ 0,
    /* targetWire */ 1,
    /* isBetween */ false,
  );

  // Column re-sorts so H (lowest reg) comes first.
  const ops = model.componentGrid[0].components;
  expectOp(ops[0], { H: 0 });
  expectOp(ops[1], { X: 1 });
  // Qubit ids are renumbered to match positions.
  assert.equal(model.qubits[0].id, 0);
  assert.equal(model.qubits[1].id, 1);
});

test("removeQubit shifts higher wire indices down by one", () => {
  const model = build(circuit(3, [[gate("X", 2)]]));
  assert.deepEqual(model.qubitUseCounts, [0, 0, 1]);

  removeQubit(model, 1);

  assert.equal(model.qubits.length, 2);
  // Wire 2's reference shifts down to wire 1.
  expectOp(at(model, "0,0"), { X: 1 });
  assert.deepEqual(model.qubitUseCounts, [0, 1]);
});

test("moveQubit with isBetween=true inserts before the target wire", () => {
  const model = build(
    circuit(4, [[gate("W", 0), gate("X", 1), gate("Y", 2), gate("Z", 3)]]),
  );

  // Move wire 0 to just before wire 3 (isBetween=true).
  moveQubit(model, 0, 3, true);

  // New wire order [X, Y, W, Z]; ops carry their new target indices.
  const ops = model.componentGrid[0].components;
  expectOp(ops[0], { X: 0 });
  expectOp(ops[1], { Y: 1 });
  expectOp(ops[2], { W: 2 });
  expectOp(ops[3], { Z: 3 });
});

test("removeQubitWithDependents strips ops on the wire and drops it", () => {
  // The public cascade: remove every op touching the doomed wire, then rewire the higher indices
  // down.
  const model = build(
    circuit(3, [[gate("X", 0)], [gate("H", 1)], [gate("Z", 2)]]),
  );
  assert.deepEqual(model.qubitUseCounts, [1, 1, 1]);

  removeQubitWithDependents(model, 1);

  assert.equal(model.qubits.length, 2);
  expectGrid(model, [[{ X: 0 }], [{ Z: 1 }]]);
});

test("moveQubit: moving an interior empty wire to the bottom prunes it as a trailing unused wire", () => {
  // Wire 1 is empty; wires 0 and 2 carry ops. Swapping the empty wire down to the bottom leaves it
  // as the highest, unused wire, which `removeTrailingUnusedQubits` drops immediately.
  const model = build(circuit(3, [[gate("X", 0), gate("Z", 2)]]));

  moveQubit(model, 1, 2, false);

  assert.equal(model.qubits.length, 2);
  // Z shifts up from wire 2 to wire 1; the emptied trailing wire is gone.
  expectGrid(model, [[{ X: 0 }, { Z: 1 }]]);
});

// ---------------------------------------------------------------------------
// removeQubit / moveQubit recurse into nested groups
// ---------------------------------------------------------------------------

test("removeQubit: shifts wire indices on ops nested inside groups", () => {
  // Raw JSON: Foo's targets [1,2] aren't derivable from a lone H@2.
  const model = build(
    circuit(3, [
      [
        {
          kind: "unitary",
          gate: "Foo",
          targets: [{ qubit: 1 }, { qubit: 2 }],
          children: [{ components: [gate("H", 2)] }],
        },
      ],
    ]),
  );

  removeQubit(model, 0);

  // Removing wire 0 shifts every >0 wire down: nested H 2 → 1, Foo's cached targets [1,2] → [0,1].
  expectOp(at(model, "0,0"), {
    Foo: { targets: [0, 1], children: [[{ H: 1 }]] },
  });
});

test("moveQubit: swaps wire indices on ops nested inside groups", () => {
  const model = build(
    circuit(2, [[group("Foo", [[gate("H", 0), gate("X", 1)]])]]),
  );

  moveQubit(model, 0, 1, false);

  // Swap propagates into nested ops; column re-sorts so X (now wire 0) precedes H (now wire 1).
  const innerOps = at(model, "0,0").children[0].components;
  expectOp(innerOps[0], { X: 0 });
  expectOp(innerOps[1], { H: 1 });
});

test("moveQubit: refreshes group `.targets` cache after wire swap", () => {
  const model = build(circuit(3, [[group("Foo", [[gate("H", 0)]])]]));

  moveQubit(model, 0, 1, false);

  // Foo's cached `.targets` must be re-derived to [1], not left stale.
  expectOp(at(model, "0,0"), { Foo: { targets: [1], children: [[{ H: 1 }]] } });
});

test("moveQubit: resolves nested-group overlaps introduced by widening", () => {
  // Swapping wires 0 and 1 keeps the H/X span non-overlapping, so the nested column stays single
  // (no split, no corruption).
  const model = build(
    circuit(2, [[group("Foo", [[gate("H", 0), gate("X", 1)]])]]),
  );

  moveQubit(model, 0, 1, false);

  expectOp(at(model, "0,0"), { Foo: { children: [["H", "X"]] } });
});

test("moveQubit: swap inside a group splits a nested column when a child's control moves over a sibling", () => {
  // Swapping wires 1 and 2 widens CX's span to 0-2 (ctrl 1 → 2) and lands H on wire 1, between CX's
  // target and control — forcing a collision-split of the nested column.
  const model = build(
    circuit(3, [
      [group("Foo", [[gate("X", 0, { ctrls: [1] }), gate("H", 2)]])],
    ]),
  );

  moveQubit(model, 1, 2, false);

  const fooOp = at(model, "0,0");
  assert.equal(
    fooOp.children.length,
    2,
    `Foo's nested grid must split into two columns after the wire swap; got ${fooOp.children.length}`,
  );

  // Both children survive with wire refs rewritten by the 1-to-1 permutation.
  const flattened = fooOp.children.flatMap(
    (/** @type {any} */ col) => col.components,
  );
  assert.equal(flattened.length, 2);

  const cx = flattened.find((/** @type {any} */ op) => op.gate === "X");
  const h = flattened.find((/** @type {any} */ op) => op.gate === "H");
  assert.ok(cx, "CX child must survive the split");
  assert.ok(h, "H child must survive the split");
  expectOp(cx, { X: { targets: [0], ctrls: [2] } });
  expectOp(h, { H: 1 });

  // CX and H must end up in different nested columns.
  const cxColIdx = fooOp.children.findIndex((/** @type {any} */ col) =>
    col.components.some((/** @type {any} */ op) => op.gate === "X"),
  );
  const hColIdx = fooOp.children.findIndex((/** @type {any} */ col) =>
    col.components.some((/** @type {any} */ op) => op.gate === "H"),
  );
  assert.notEqual(
    cxColIdx,
    hColIdx,
    "CX and H must be split into separate nested columns",
  );
  // Parent still claims the full wire span it covered before.
  expectOp(fooOp, { Foo: { targets: [0, 1, 2] } });
});

// ---------------------------------------------------------------------------
// moveQubit + Ms-with-classical-consumers
//
// `moveQubit` rewrites every register reference (consumer classical refs AND measurement
// `.results`) by the same 1-to-1 wire-permutation, without renumbering result indices. Invariant:
// every consumer must still reference a real, unique (qubit, result) key some M produces.
// ---------------------------------------------------------------------------

test("moveQubit: classical-control consumer follows a moved M's qubit index", () => {
  const model = build(circuit(3, [[_mGate(0, 0)], [_ccx(2, 0, 0)]]));

  moveQubit(model, 0, 1, false);

  // M (and its `.results`) and the consumer's classical ref all rewire 0 → 1.
  expectOp(at(model, "0,0"), {
    Measure: { qubits: [1], results: [{ q: 1, r: 0 }] },
  });
  expectOp(at(model, "1,0"), { X: { ctrls: [{ q: 1, r: 0 }] } });
});

test("moveQubit: swap of two wires that both have Ms with consumers preserves per-wire uniqueness", () => {
  // M_a (wire 0) and M_b (wire 1) hold the SAME result index 0. The wire-permutation keeps them on
  // distinct wires, so (qubit, result) keys stay unique without any renumbering.
  const model = build(
    circuit(3, [
      [_mGate(0, 0)],
      [_mGate(1, 0)],
      [_ccx(2, 0, 0)], // consumes M_a (wire 0, r=0)
      [_ccx(2, 1, 0)], // consumes M_b (wire 1, r=0)
    ]),
  );

  moveQubit(model, 0, 1, false);

  // M_a 0 → 1, M_b 1 → 0; each consumer follows its M.
  expectOp(at(model, "0,0"), {
    Measure: { qubits: [1], results: [{ q: 1, r: 0 }] },
  });
  expectOp(at(model, "1,0"), {
    Measure: { qubits: [0], results: [{ q: 0, r: 0 }] },
  });
  expectOp(at(model, "2,0"), { X: { ctrls: [{ q: 1, r: 0 }] } });
  expectOp(at(model, "3,0"), { X: { ctrls: [{ q: 0, r: 0 }] } });
});

test("moveQubit: swap of a wire carrying multiple Ms keeps the consumer chain in sync", () => {
  // Wire 0 carries M_a (r=0) and M_b (r=1); wire 1 is empty. Swapping moves both Ms 0 → 1 with
  // result indices preserved (the destination wire had no Ms to collide with).
  const model = build(
    circuit(3, [
      [_mGate(0, 0)],
      [_mGate(0, 1)],
      [_ccx(2, 0, 0)], // consumes M_a
      [_ccx(2, 0, 1)], // consumes M_b
    ]),
  );

  moveQubit(model, 0, 1, false);

  // Both Ms 0 → 1 (results 0 and 1 intact); each consumer follows.
  expectOp(at(model, "0,0"), {
    Measure: { qubits: [1], results: [{ q: 1, r: 0 }] },
  });
  expectOp(at(model, "1,0"), {
    Measure: { qubits: [1], results: [{ q: 1, r: 1 }] },
  });
  expectOp(at(model, "2,0"), { X: { ctrls: [{ q: 1, r: 0 }] } });
  expectOp(at(model, "3,0"), { X: { ctrls: [{ q: 1, r: 1 }] } });
});

test("moveQubit isBetween: moving a wire past one with Ms-with-consumers remaps every party in lockstep", () => {
  // Move wire 0 to between wires 2 and 3 → new order [1, 2, 0, 3], remapping old→new: 0→2, 1→0,
  // 2→1, 3→3.
  const model = build(
    circuit(4, [
      [_mGate(1, 0)],
      [_mGate(2, 0)],
      [_ccx(3, 1, 0)], // consumes M_a
      [_ccx(3, 2, 0)], // consumes M_b
    ]),
  );

  moveQubit(model, 0, 3, true);

  // M_a 1 → 0, M_b 2 → 1; consumers (target wire 3) follow.
  expectOp(at(model, "0,0"), {
    Measure: { qubits: [0], results: [{ q: 0, r: 0 }] },
  });
  expectOp(at(model, "1,0"), {
    Measure: { qubits: [1], results: [{ q: 1, r: 0 }] },
  });
  expectOp(at(model, "2,0"), { X: { targets: [3], ctrls: [{ q: 0, r: 0 }] } });
  expectOp(at(model, "3,0"), { X: { targets: [3], ctrls: [{ q: 1, r: 0 }] } });
});

test("moveQubit: swap remaps a classical-control consumer buried inside a group", () => {
  // Consumer is two groups deep on wire 2; wrapper `.targets` set by hand to keep them on wire 2
  // only. Swapping wires 0 and 1 must still reach the buried consumer's classical ref.
  const model = build(
    circuit(3, [
      [_mGate(0, 0)],
      [
        {
          kind: "unitary",
          gate: "Foo",
          targets: [{ qubit: 2 }],
          children: [
            {
              components: [
                {
                  kind: "unitary",
                  gate: "Bar",
                  targets: [{ qubit: 2 }],
                  children: [{ components: [_ccx(2, 0, 0)] }],
                },
              ],
            },
          ],
        },
      ],
    ]),
  );

  moveQubit(model, 0, 1, false);

  // Buried consumer's classical ref and the M both rewire 0 → 1.
  expectOp(at(model, "1,0-0,0-0,0"), { X: { ctrls: [{ q: 1, r: 0 }] } });
  expectOp(at(model, "0,0"), {
    Measure: { qubits: [1], results: [{ q: 1, r: 0 }] },
  });
});
