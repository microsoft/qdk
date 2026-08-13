// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Group move tests: moving a child out of a group, dragging the group as a rigid unit,
// classical-control anchoring on the group's children, empty-group cleanup, trailing inner-column
// dropzone, and quantum control-leg drags on multi-target gates. Groups themselves carry classical
// controls only — the authoring layer refuses quantum controls on groups — so the control-leg drag
// mechanics are exercised on multi-target gates, which share the same multi-wire-leg shape and
// single-leg drag path (`_moveAsUnit` returns false whenever a control is moving). Single-target
// (CNOT / CCX) control-leg drags are covered separately in the `circuit-actions/` suite.

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import { moveOperation } from "../../../dist/ux/circuit-vis/actions/circuitActions.js";
import {
  at,
  build,
  circuit,
  expectGrid,
  expectOp,
  gate,
  group,
  meas,
  qubits,
} from "../_helpers.mjs";

// ---------------------------------------------------------------------------
// `moveOperation` cross-scope correctness.
//
// After a successful move, the original location's grid no longer contains the op, and the target
// grid contains exactly one copy. `moveOperation` resolves the source op's parent grid BEFORE
// `_moveX` mutates the model so that splicing a new column ahead of the source's path (e.g. moving
// a child out of a group to a fresh top-level column at index 0) doesn't stale the source location
// lookup and leave a duplicate behind.
// ---------------------------------------------------------------------------

test("moveOperation: moving a child out of a group to a new column ahead of the group does NOT leave a duplicate behind", () => {
  const model = build(
    circuit(3, [
      [gate("X", 2)],
      [group("Group", [[gate("H", 0), gate("Z", 1)]])],
    ]),
  );

  // move H to a fresh top-level column ahead of the group
  const moved = moveOperation(model, "1,0-0,0", "0,0", 0, 0, false, true);
  assert.ok(moved, "move should return the new operation");

  // H lands in the new lead column; X and the surviving Group shift right by one. Exactly one H —
  // no duplicate left behind.
  expectGrid(model, [[{ H: 0 }], [{ X: 2 }], ["Group"]]);
  expectOp(at(model, "2,0"), { Group: { children: [[{ Z: 1 }]] } });
});

test("moveOperation: moving a child out of a group updates the group's targets to drop the departed wire", () => {
  // The parent group's `targets` is a derived render-extent claim: it must reflect the union of its
  // remaining children's wires.
  const model = build(
    circuit(3, [[group("Group", [[gate("H", 0), gate("Z", 1)]])]]),
  );

  // Move H out to top-level on wire 2.
  moveOperation(model, "0,0-0,0", "1,0", 0, 2, false, true);

  // Group now only contains Z on wire 1.
  expectOp(at(model, "0,0"), { Group: { targets: [1] } });
});

// ---------------------------------------------------------------------------
// Dragging a group as a rigid unit.
//
// Moving a group shifts the group's own `.targets` AND recursively
// every register reference in its children grid by the same delta,
// so the box and its contents stay aligned.
// ---------------------------------------------------------------------------

test("moveOperation: dragging a group shifts the box AND all child register refs", () => {
  // Group with children H@0, CNOT(target=1, ctrl=0). Drag wire 0
  // → wire 2 (delta = +2). Box and children all shift by +2.
  const model = build(
    circuit(4, [
      [group("Group", [[gate("H", 0), gate("CNOT", 1, { ctrls: [0] })]])],
    ]),
  );

  const moved = moveOperation(model, "0,0", "0,0", 0, 2, false, false);
  assert.ok(moved);

  expectOp(at(model, "0,0"), {
    Group: {
      targets: [2, 3],
      children: [[{ H: 2 }, { CNOT: { targets: [3], ctrls: [2] } }]],
    },
  });
});

// ---------------------------------------------------------------------------
// Classical-control anchoring on a moved group's children.
// ---------------------------------------------------------------------------

test("moveOperation: moving a group with a classically-controlled child anchors the classical control", () => {
  // External M produces the classical reg; the producer stays put, so
  // X's target shifts but its classical control must anchor on q0.
  const model = build(
    circuit(qubits(4, { 0: 1 }), [
      [meas(0)],
      [group("Group", [[gate("X", 1, { ctrls: [{ q: 0, r: 0 }] })]])],
    ]),
  );

  // drag the group q1 → q2 (delta = +1)
  moveOperation(model, "1,0", "1,0", 1, 2, false, false);

  expectOp(at(model, "1,0"), {
    Group: {
      children: [[{ X: { targets: [2], ctrls: [{ q: 0, r: 0 }] } }]],
    },
  });
});

test("moveOperation: moving a group whose internal measurement produces the classical reg shifts the consumer", () => {
  // The producing M is INSIDE the moved subtree, so the classical reg
  // moves too; the consumer's classical control shifts in lockstep.
  const model = build(
    circuit(qubits(4, { 1: 1 }), [
      [
        group("Group", [
          [meas(1)],
          [gate("X", 1, { ctrls: [{ q: 1, r: 0 }] })],
        ]),
      ],
    ]),
  );

  // drag the group q1 → q2 (delta = +1)
  moveOperation(model, "0,0", "0,0", 1, 2, false, false);

  expectOp(at(model, "0,0"), {
    Group: {
      children: [
        [{ M: { qubits: [2], results: [{ q: 2, r: 0 }] } }],
        [{ X: { targets: [2], ctrls: [{ q: 2, r: 0 }] } }],
      ],
    },
  });

  // numResults bookkeeping must follow the measurement.
  assert.equal(
    model.qubits[1].numResults,
    undefined,
    "wire 1 must no longer claim a classical register",
  );
  assert.equal(
    model.qubits[2].numResults,
    1,
    "wire 2 must now claim the classical register",
  );
});

test("moveOperation: unit-moving a group with TWO internal producers onto a wire that already has a measurement reindexes both", () => {
  // The group holds two internal producers M_a and M_b, each with its own consumer. Landing on wire
  // 6 (which already has M_ext) makes document order M_ext, M_a, M_b, so M_a → r1 and M_b → r2; each
  // consumer must track its own producer.
  const model = build(
    circuit(10, [
      [meas(6, { result: 0 })],
      [
        group("G", [
          [meas(4, { result: 0 })],
          [gate("X", 5, { ctrls: [{ q: 4, r: 0 }], conditional: true })],
          [meas(4, { result: 1 })],
          [gate("Y", 5, { ctrls: [{ q: 4, r: 1 }], conditional: true })],
        ]),
      ],
    ]),
  );

  const moved = moveOperation(model, "1,0", "1,0", 4, 6, false, false);
  assert.ok(moved, "unit-move must succeed");

  const mA = moved.children[0].components[0];
  const consumerA = moved.children[1].components[0];
  const mB = moved.children[2].components[0];
  const consumerB = moved.children[3].components[0];
  const external = at(model, "0,0");

  // All three measurements now live on wire 6, ordered M_ext(r0), M_a(r1), M_b(r2).
  assert.equal(external.results[0].result, 0, "external stays r0");
  assert.equal(mA.results[0].qubit, 6);
  assert.equal(mA.results[0].result, 1, "M_a becomes r1");
  assert.equal(mB.results[0].qubit, 6);
  assert.equal(mB.results[0].result, 2, "M_b becomes r2");

  // Each consumer must track its OWN producer to the new index.
  assert.equal(consumerA.controls[0].qubit, 6);
  assert.equal(
    consumerA.controls[0].result,
    mA.results[0].result,
    `consumer A must track M_a: refs r${consumerA.controls[0].result}, M_a is r${mA.results[0].result}`,
  );
  assert.equal(consumerB.controls[0].qubit, 6);
  assert.equal(
    consumerB.controls[0].result,
    mB.results[0].result,
    `consumer B must track M_b: refs r${consumerB.controls[0].result}, M_b is r${mB.results[0].result}`,
  );
});

// ---------------------------------------------------------------------------
// A 3-wire producer/consumer lattice, moved once, asserted from many angles.
//
// On every wire: M0 → C0(reads M0) → [group G] → M1 → C1(reads M1) → C2(reads M0). Group G spans
// q0–q1; on each of its wires: MG → CG0(reads MG) → CG1(reads the external M0). Moving G by +1 (to
// q1–q2) frees an MG slot on q0 (M1 r2→r1) and inserts one on q2 (M1 r1→r2); q1 is unchanged. CG0
// follows its MG onto the new wire; CG1 stays anchored to the M0 it originally read (now a
// cross-wire control, shown @qN).
//
//   Legend:  Mx(n) = producer Mx, result index n      Cx→My = consumer Cx reads producer My
//            @qN   = classical control anchored to a producer on a different wire qN
//
//   BEFORE  (G spans q0–q1):
//                         ┌─────── group G ───────┐
//     q0   M0(0)   C0→M0  │ MG(1)  CG0→MG  CG1→M0 │   M1(2)   C1→M1   C2→M0
//     q1   M0(0)   C0→M0  │ MG(1)  CG0→MG  CG1→M0 │   M1(2)   C1→M1   C2→M0
//                         └─────── group G ───────┘
//     q2   M0(0)   C0→M0                              M1(1)   C1→M1   C2→M0
//
//   AFTER  (+1 → G spans q1–q2):
//     q0   M0(0)   C0→M0                                 M1(1)   C1→M1   C2→M0   (M1: r2→r1)
//                         ┌───────── group G ────────┐
//     q1   M0(0)   C0→M0  │ MG(1)  CG0→MG  CG1→M0@q0 │   M1(2)   C1→M1   C2→M0
//     q2   M0(0)   C0→M0  │ MG(1)  CG0→MG  CG1→M0@q1 │   M1(2)   C1→M1   C2→M0   (M1: r1→r2)
//                         └───────── group G ────────┘
//
// Each test below shares this exact setup + move (via `buildLatticeAndMove`) and asserts one slice
// of the resulting producer/consumer graph, so a failure names precisely which invariant broke.
// ---------------------------------------------------------------------------

/**
 * Build the 3-wire lattice above and slide group G from q0–q1 down to q1–q2 (delta +1). Returns the
 * model, the moved group, and lookup helpers (`measOn`/`gateOn` for top-level ops by gate name and
 * wire; `innerMeasOn`/`innerGateOn` for the moved group's children by inner-column and wire).
 */
const buildLatticeAndMove = () => {
  const model = build(
    circuit(3, [
      [
        meas(0, { gate: "M0", result: 0 }),
        meas(1, { gate: "M0", result: 0 }),
        meas(2, { gate: "M0", result: 0 }),
      ],
      [
        gate("C0", 0, { ctrls: [{ q: 0, r: 0 }], conditional: true }),
        gate("C0", 1, { ctrls: [{ q: 1, r: 0 }], conditional: true }),
        gate("C0", 2, { ctrls: [{ q: 2, r: 0 }], conditional: true }),
      ],
      [
        group("G", [
          [
            meas(0, { gate: "MG", result: 1 }),
            meas(1, { gate: "MG", result: 1 }),
          ],
          [
            gate("CG0", 0, { ctrls: [{ q: 0, r: 1 }], conditional: true }),
            gate("CG0", 1, { ctrls: [{ q: 1, r: 1 }], conditional: true }),
          ],
          [
            gate("CG1", 0, { ctrls: [{ q: 0, r: 0 }], conditional: true }),
            gate("CG1", 1, { ctrls: [{ q: 1, r: 0 }], conditional: true }),
          ],
        ]),
      ],
      [
        meas(0, { gate: "M1", result: 2 }),
        meas(1, { gate: "M1", result: 2 }),
        meas(2, { gate: "M1", result: 1 }),
      ],
      [
        gate("C1", 0, { ctrls: [{ q: 0, r: 2 }], conditional: true }),
        gate("C1", 1, { ctrls: [{ q: 1, r: 2 }], conditional: true }),
        gate("C1", 2, { ctrls: [{ q: 2, r: 1 }], conditional: true }),
      ],
      [
        gate("C2", 0, { ctrls: [{ q: 0, r: 0 }], conditional: true }),
        gate("C2", 1, { ctrls: [{ q: 1, r: 0 }], conditional: true }),
        gate("C2", 2, { ctrls: [{ q: 2, r: 0 }], conditional: true }),
      ],
    ]),
  );

  const moved = moveOperation(model, "2,0", "2,0", 0, 1, false, false);
  assert.ok(moved, "precondition: the unit-move must succeed");

  const top = () => model.componentGrid.flatMap((c) => c.components);
  const measOn = (name, w) =>
    top().find((op) => op.gate === name && op.qubits?.[0]?.qubit === w);
  const gateOn = (name, w) =>
    top().find((op) => op.gate === name && op.targets?.[0]?.qubit === w);
  const innerMeasOn = (col, w) =>
    moved.children[col].components.find((op) => op.qubits?.[0]?.qubit === w);
  const innerGateOn = (col, w) =>
    moved.children[col].components.find((op) => op.targets?.[0]?.qubit === w);

  return { model, moved, measOn, gateOn, innerMeasOn, innerGateOn };
};

/**
 * Render a classical leg (`{ qubit, result }`) as `qNrM` for readable failure messages.
 * @param {any} reg
 */
const legStr = (reg) =>
  reg == null ? "<none>" : `q${reg.qubit}r${reg.result}`;

/**
 * Assert that `consumer`'s classical control points at `producer`'s result register — i.e. the
 * producer→consumer link survived the move — with a message that spells out both endpoints.
 * @param {any} consumer  op with a classical `.controls[0]`
 * @param {any} producer  measurement with a `.results[0]`
 * @param {string} label  human name for the link, e.g. "C1 on q0 → M1"
 */
const assertLinked = (consumer, producer, label) => {
  const c = consumer?.controls?.[0];
  const p = producer?.results?.[0];
  assert.deepEqual(
    { qubit: c?.qubit, result: c?.result },
    { qubit: p?.qubit, result: p?.result },
    `${label}: consumer control is ${legStr(c)} but its producer sits at ${legStr(p)}`,
  );
};

test("moveOperation: [lattice] producer result indices reindex on the source and destination wires", () => {
  const { measOn, innerMeasOn } = buildLatticeAndMove();

  // q0 lost its MG, so M1 slides down one slot; q1 is untouched; q2 gained an MG, so M1 slides up.
  assert.equal(
    measOn("M1", 0).results[0].result,
    1,
    `M1 on q0 must decrement r2→r1 once MG leaves q0, got ${legStr(measOn("M1", 0).results[0])}`,
  );
  assert.equal(
    measOn("M1", 1).results[0].result,
    2,
    `M1 on q1 must stay r2 (q1's measurement column is unchanged), got ${legStr(measOn("M1", 1).results[0])}`,
  );
  assert.equal(
    measOn("M1", 2).results[0].result,
    2,
    `M1 on q2 must increment r1→r2 once MG lands on q2, got ${legStr(measOn("M1", 2).results[0])}`,
  );

  // Each MG rode the move onto the next wire and keeps its position-1 index (M0 is r0 ahead of it).
  assert.equal(
    innerMeasOn(0, 1).results[0].result,
    1,
    `MG landing on q1 must be r1 (behind M0 r0), got ${legStr(innerMeasOn(0, 1).results[0])}`,
  );
  assert.equal(
    innerMeasOn(0, 2).results[0].result,
    1,
    `MG landing on q2 must be r1 (behind M0 r0), got ${legStr(innerMeasOn(0, 2).results[0])}`,
  );
});

test("moveOperation: [lattice] consumers of the never-moved M0 (C0, C2) keep their references", () => {
  const { measOn, gateOn } = buildLatticeAndMove();

  // M0 never moves and stays r0 on every wire, so both of its consumers are untouched.
  for (const w of [0, 1, 2]) {
    assertLinked(gateOn("C0", w), measOn("M0", w), `C0 on q${w} → M0`);
    assertLinked(gateOn("C2", w), measOn("M0", w), `C2 on q${w} → M0`);
  }
});

test("moveOperation: [lattice] consumers of M1 (C1) follow M1's reindex on every wire", () => {
  const { measOn, gateOn } = buildLatticeAndMove();

  // C1 reads M1, whose index shifted on q0 and q2; the consumer must track it.
  for (const w of [0, 1, 2]) {
    assertLinked(gateOn("C1", w), measOn("M1", w), `C1 on q${w} → M1`);
  }
});

test("moveOperation: [lattice] group-internal consumer CG0 follows its producer MG onto the new wire", () => {
  const { innerMeasOn, innerGateOn } = buildLatticeAndMove();

  // CG0 and its producer MG both live in the group, so CG0 rides down onto q1/q2 with it.
  assertLinked(innerGateOn(1, 1), innerMeasOn(0, 1), "CG0 on q1 → MG");
  assertLinked(innerGateOn(1, 2), innerMeasOn(0, 2), "CG0 on q2 → MG");
});

test("moveOperation: [lattice] group-internal consumer CG1 stays anchored to its external producer M0", () => {
  const { measOn, innerGateOn } = buildLatticeAndMove();

  // CG1 depends on the external M0. Its target rides down (q0→q1, q1→q2) but the classical control
  // stays anchored to the ORIGINAL M0 (q0, then q1), which never moved — a cross-wire control.
  assertLinked(
    innerGateOn(2, 1),
    measOn("M0", 0),
    "CG1 (target now q1) → M0 anchored on q0",
  );
  assertLinked(
    innerGateOn(2, 2),
    measOn("M0", 1),
    "CG1 (target now q2) → M0 anchored on q1",
  );
});

test("moveOperation: unit-moving a multi-target gate with an external classical control anchors that control", () => {
  // Multi-target gates take the same rigid unit-shift path as groups.
  // External M produces the classical reg, so the quantum targets
  // shift but the classical control must anchor on q0.
  const model = build(
    circuit(qubits(5, { 0: 1 }), [
      [meas(0)],
      [gate("Foo", [1, 2], { ctrls: [{ q: 0, r: 0 }], conditional: true })],
    ]),
  );

  // drag the gate q1 → q3 (delta = +2)
  moveOperation(model, "1,0", "1,0", 1, 3, false, false);

  // targets shift q1→q3, q2→q4; classical control anchored at q0.r0.
  expectOp(at(model, "1,0"), {
    Foo: { targets: [3, 4], ctrls: [{ q: 0, r: 0 }], conditional: true },
  });
});

// ---------------------------------------------------------------------------
// Bounds-checking for unit-shift moves on groups.
// ---------------------------------------------------------------------------

test("moveOperation: refuses a unit-shift that would push wires below 0", () => {
  // Group spans wires 1-2. Grabbing on q2 and dropping on q0 is a
  // delta = -2 shift, which would push the group's low wire (1) to -1.
  const circuitLiteral = circuit(4, [
    [group("Group", [[gate("X", 1), gate("Y", 2)]])],
  ]);
  const before = JSON.stringify(circuitLiteral);
  const model = build(circuitLiteral);

  // grab q2, drop on q0 → delta = -2, low wire 1 would underflow to -1
  const result = moveOperation(model, "0,0", "0,0", 2, 0, false, false);

  assert.equal(result, null, "move must be refused");
  assert.equal(
    JSON.stringify({
      qubits: model.qubits,
      componentGrid: model.componentGrid,
    }),
    before,
    "refusal must not mutate the model",
  );
});

test("moveOperation: a unit-shift whose lowest wire lands exactly on 0 is allowed", () => {
  // Boundary: span [1, 2] shifted by -1 lands on [0, 1] — exactly on 0
  // is still in-range, so the move succeeds.
  const model = build(
    circuit(4, [[group("Group", [[gate("X", 1), gate("Y", 2)]])]]),
  );

  // grab q1, drop on q0 (delta = -1)
  const result = moveOperation(model, "0,0", "0,0", 1, 0, false, false);
  assert.ok(result, "move must succeed when min post-shift wire is exactly 0");

  expectOp(at(model, "0,0"), { Group: { targets: [0, 1] } });
});

test("moveOperation: a unit-shift on a single-child group is bounded by the derived min wire", () => {
  // The bounds check uses the derived min wire (here [1], from the lone
  // X@1), not any pre-declared span: shift -1 → [0] is in-range.
  const model = build(circuit(4, [[group("Group", [[gate("X", 1)]])]]));

  // grab q1, drop on q0 (delta = -1)
  const result = moveOperation(model, "0,0", "0,0", 1, 0, false, false);
  assert.ok(result, "move must succeed when derived min post-shift wire is 0");

  expectOp(at(model, "0,0"), {
    Group: { targets: [0], children: [[{ X: 0 }]] },
  });
});

// ---------------------------------------------------------------------------
// Empty-group cleanup.
// ---------------------------------------------------------------------------

test("moveOperation: moving the last child out deletes the empty group", () => {
  const model = build(circuit(3, [[group("Group", [[gate("H", 0)]])]]));

  // move H out to a new top-level column on q1
  moveOperation(model, "0,0-0,0", "0,1", 0, 1, false, true);

  expectGrid(model, [[{ H: 1 }]]);
});

test("moveOperation: empty-group cleanup cascades through nested groups", () => {
  // Inner is Outer's only child, so emptying Inner prunes BOTH groups.
  const model = build(
    circuit(2, [[group("Outer", [[group("Inner", [[gate("H", 0)]])]])]]),
  );

  // move the deepest leaf out to a new top-level column on q1
  moveOperation(model, "0,0-0,0-0,0", "0,1", 0, 1, false, true);

  expectGrid(model, [[{ H: 1 }]]);
});

test("moveOperation: cleanup STOPS at the first non-empty ancestor", () => {
  // Y keeps Outer alive after Inner is pruned, so cleanup must not over-delete: only the emptied
  // Inner disappears.
  const model = build(
    circuit(2, [
      [group("Outer", [[group("Inner", [[gate("H", 0)]]), gate("Y", 0)]])],
    ]),
  );

  // move H out; insertNewColumn shifts Outer to col 1
  moveOperation(model, "0,0-0,0-0,0", "0,1", 0, 1, false, true);

  expectOp(at(model, "1,0"), { Outer: { children: [[{ Y: 0 }]] } });
});

// ---------------------------------------------------------------------------
// Trailing inner-column dropzone of an expanded group.
// ---------------------------------------------------------------------------

test("moveOperation: moving an external gate to a group's trailing inner-column slot pulls it into the group", () => {
  const model = build(
    circuit(2, [[group("Foo", [[gate("H", 0)]])], [gate("Y", 0)]]),
  );

  // move Y into Foo's trailing inner-column slot "0,0-1,0"
  const moved = moveOperation(model, "1,0", "0,0-1,0", 0, 0, false, false);
  assert.ok(moved, "move should return the moved op");

  expectGrid(model, [["Foo"]]);
  expectOp(at(model, "0,0"), {
    Foo: { children: [[{ H: 0 }], [{ Y: 0 }]] },
  });
});

test("moveOperation: moving an internal gate to its group's trailing inner-column slot keeps it inside the group", () => {
  // The exact post-move column count is an implementation detail; what matters is the flat gate
  // sequence ends up [X, H].
  const model = build(
    circuit(2, [[group("Foo", [[gate("H", 0)], [gate("X", 1)]])]]),
  );

  // move H to Foo's trailing inner slot "0,0-2,0"
  const moved = moveOperation(model, "0,0-0,0", "0,0-2,0", 0, 0, false, false);
  assert.ok(moved, "move should return the moved op");

  expectGrid(model, [["Foo"]]);

  const fooOp = at(model, "0,0");
  /** @type {string[]} */
  const innerGates = [];
  for (const col of fooOp.children) {
    for (const op of col.components) {
      innerGates.push(op.gate);
    }
  }
  assert.deepEqual(
    innerGates,
    ["X", "H"],
    "H must land after X in the inner grid; no duplicate H, no stray",
  );
});

test("moveOperation: promoting a gate into its grandparent group lands it beside the parent group", () => {
  // Outer ▷ Inner ▷ [H | Z]. Dropping H on Outer's trailing inner slot
  // "0,0-1,0" pulls H up one level into Outer, as a sibling of Inner.
  // Inner keeps Z, so it survives the promotion (no empty-group prune).
  const model = build(
    circuit(3, [
      [group("Outer", [[group("Inner", [[gate("H", 0)], [gate("Z", 0)]])]])],
    ]),
  );

  const moved = moveOperation(
    model,
    "0,0-0,0-0,0",
    "0,0-1,0",
    0,
    0,
    false,
    false,
  );
  assert.ok(moved, "promotion into the grandparent group must succeed");

  // Outer now holds [Inner(Z)] then [H]; Inner survives with just Z.
  expectOp(at(model, "0,0"), {
    Outer: {
      children: [[{ Inner: { children: [[{ Z: 0 }]] } }], [{ H: 0 }]],
    },
  });
});

test("moveOperation: moving a gate into a sibling group relocates it across scopes", () => {
  // A ▷ [H] and B ▷ [X] are sibling top-level groups. Dropping H on B's
  // trailing inner slot "0,1-1,0" moves it out of A and into B, rewired
  // to B's wire (q2). A is emptied and pruned, so B collapses to "0,0".
  const model = build(
    circuit(4, [[group("A", [[gate("H", 0)]]), group("B", [[gate("X", 2)]])]]),
  );

  const moved = moveOperation(model, "0,0-0,0", "0,1-1,0", 0, 2, false, false);
  assert.ok(moved, "move into the sibling group must succeed");

  // A is gone; B holds [X] then the relocated [H] on wire 2.
  expectGrid(model, [["B"]]);
  expectOp(at(model, "0,0"), {
    B: { children: [[{ X: 2 }], [{ H: 2 }]] },
  });
});

// ---------------------------------------------------------------
// Multi-target gate + quantum-control drag.
//
// Control-leg drags always take the single-leg path (`_moveAsUnit` returns false when a control is
// moving), so a multi-target gate with a quantum control exercises the same mechanics a group would
// — but it's a shape the editor can actually author. Groups support classical controls only,
// covered by the anchoring tests above.
// ---------------------------------------------------------------

test("moveOperation: vertical control drag on a multi-target gate rewires only the control, leaving the body untouched", () => {
  const model = build(circuit(4, [[gate("Foo", [1, 2], { ctrls: [0] })]]));

  // drag the control q0 → q3 (vertical: targets stay put)
  const moved = moveOperation(model, "0,0", "0,0", 0, 3, true, false);
  assert.ok(moved);

  expectOp(at(model, "0,0"), {
    Foo: {
      targets: [1, 2], // body wires unchanged
      ctrls: [3], // control rewired
    },
  });
});

test("moveOperation: dropping a multi-target gate's control onto a body wire swaps the control with that wire", () => {
  const model = build(circuit(3, [[gate("Foo", [1, 2], { ctrls: [0] })]]));

  // drop the control on q2 (a target wire) → control and target q2 swap
  const moved = moveOperation(model, "0,0", "0,0", 0, 2, true, false);
  assert.ok(moved);

  expectOp(at(model, "0,0"), {
    Foo: {
      targets: [0, 1], // target q2 moved to the old control wire q0
      ctrls: [2], // control moved to q2
    },
  });
});

test("moveOperation: dropping a multi-target gate's control onto a wire already occupied by another control is a no-op", () => {
  // Like-register guard: dragging a control onto an existing control.
  const model = build(circuit(5, [[gate("Foo", [3, 4], { ctrls: [1, 2] })]]));

  // drag the control q1 → q2 (already a control) → no-op
  const moved = moveOperation(model, "0,0", "0,0", 1, 2, true, false);
  assert.ok(moved);

  expectOp(at(model, "0,0"), { Foo: { targets: [3, 4], ctrls: [1, 2] } });
});

test("moveOperation: horizontal control drag on a multi-target gate moves the whole op to the new column", () => {
  // Horizontal drag (targetWire === sourceWire, new column) is the regular column-move flow: the
  // whole op relocates. Sibling G@5 shares column 0 with Foo and stays put; Foo moves out to column
  // 1.
  const model = build(
    circuit(6, [[gate("Foo", [1, 2], { ctrls: [0] }), gate("G", 5)]]),
  );

  // drag the control to column 1 (same wire) → whole op relocates
  const moved = moveOperation(model, "0,0", "1,0", 0, 0, true, false);
  assert.ok(moved);

  // G stays in column 0; Foo (topology intact) now occupies column 1.
  expectGrid(model, [[{ G: 5 }], [{ Foo: { targets: [1, 2], ctrls: [0] } }]]);
});
