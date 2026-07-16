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
import {
  addOperation,
  moveOperation,
} from "../../../dist/ux/circuit-vis/actions/circuitActions.js";
import {
  at,
  build,
  circuit,
  expectGrid,
  expectOp,
  gate,
  group,
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

test("addOperation: dropping on a group's trailing inner-column slot adds the op as a child", () => {
  const model = build(circuit(2, [[group("Foo", [[gate("H", 0)]])]]));

  // drop Y on Foo's trailing inner-column slot "0,0-1,0"
  const added = addOperation(model, gate("Y", 0), "0,0-1,0", 0);
  assert.ok(added, "addOperation should return the new op");

  expectGrid(model, [["Foo"]]);
  expectOp(at(model, "0,0"), {
    Foo: { children: [[{ H: 0 }], [{ Y: 0 }]] },
  });
});

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

// ---------------------------------------------------------------
// Cross-scope moves between nested groups.
//
// The target location string alone decides which group the op lands in. These pin two shapes the
// dropzone layer can produce but that no other test exercises: promoting a gate up into its
// GRANDPARENT group (one level out, still nested), and moving a gate sideways into a SIBLING group
// (a different group at the same nesting level). A "child group" destination isn't meaningful — a
// gate owns no children — so there's nothing to test there.
// ---------------------------------------------------------------

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
