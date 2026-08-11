// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Add- and remove-mutator tests on grouped shapes, driven through the public `addOperation` and
// `removeOperation` actions. Counterpart to `addRemove.test.mjs` (which covers the flat,
// non-grouped case).

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  addOperation,
  removeOperation,
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

test("addOperation: dropping on a group's trailing inner-column slot adds the op as a child", () => {
  const model = build(circuit(2, [[group("Foo", [[gate("H", 0)]])]]));

  // Drop Y on Foo's trailing inner-column slot "0,0-1,0".
  const added = addOperation(model, gate("Y", 0), "0,0-1,0", 0);
  assert.ok(added, "addOperation should return the new op");

  expectGrid(model, [["Foo"]]);
  expectOp(at(model, "0,0"), {
    Foo: { children: [[{ H: 0 }], [{ Y: 0 }]] },
  });
});

test("addOperation: adding to an interior inner column on a clear wire merges into that column", () => {
  // Foo's inner grid is [[H@0], [Z@0]]. Adding Y@1 at inner column 1 (a real, populated column)
  // with no overlap merges Y alongside Z rather than splitting.
  const model = build(
    circuit(2, [[group("Foo", [[gate("H", 0)], [gate("Z", 0)]])]]),
  );

  const added = addOperation(model, gate("Y", 1), "0,0-1,0", 1);
  assert.ok(added);

  expectOp(at(model, "0,0"), {
    Foo: { children: [[{ H: 0 }], [{ Y: 1 }, { Z: 0 }]] },
  });
});

test("addOperation: insertNewColumn splits an interior inner column, shifting later columns right", () => {
  // Foo's inner grid is [[H@0], [Z@0]]. Inserting Y at inner column 1 with insertNewColumn pushes
  // the existing Z column one step right inside the group.
  const model = build(
    circuit(2, [[group("Foo", [[gate("H", 0)], [gate("Z", 0)]])]]),
  );

  const added = addOperation(
    model,
    gate("Y", 0),
    "0,0-1,0",
    0,
    /* insertNewColumn */ true,
  );
  assert.ok(added);

  expectOp(at(model, "0,0"), {
    Foo: { children: [[{ H: 0 }], [{ Y: 0 }], [{ Z: 0 }]] },
  });
});

test("addOperation: an overlapping insert inside a group forces a new inner column", () => {
  // Inner column 0 already holds H@0. Adding X@0 there would collide on wire 0, so the add splits
  // into a fresh inner column ahead of H.
  const model = build(circuit(2, [[group("Foo", [[gate("H", 0)]])]]));

  const added = addOperation(model, gate("X", 0), "0,0-0,0", 0);
  assert.ok(added, "an overlapping insert is resolved, not rejected");

  expectOp(at(model, "0,0"), {
    Foo: { children: [[{ X: 0 }], [{ H: 0 }]] },
  });
});

test("removeOperation strips a leaf inside an expanded group", () => {
  const model = build(
    circuit(2, [[group("Group", [[gate("H", 0), gate("X", 1)]])]]),
  );

  // Remove the nested X (group col 0, row 1).
  removeOperation(model, "0,0-0,1");

  // Outer group remains; the X inside its single child column is gone.
  expectOp(at(model, "0,0"), { Group: { children: [[{ H: 0 }]] } });
});

test("removeOperation: removing a deep child narrows the group's targets", () => {
  // Foo spans wires 0-1; removing the nested Y must narrow Foo's cached targets to just [0].
  const model = build(
    circuit(2, [[group("Foo", [[gate("H", 0)], [gate("Y", 1)]])]]),
  );

  removeOperation(model, "0,0-1,0");

  expectOp(at(model, "0,0"), { Foo: { targets: [0] } });
});

test("removeOperation: cascade — removing across multiple nested groups narrows every ancestor", () => {
  // Outer ⊃ Inner, one gate per inner column. Removing the nested Y narrows Inner to [0,1] and
  // Outer in lockstep.
  const model = build(
    circuit(3, [
      [
        group("Outer", [
          [group("Inner", [[gate("H", 0)], [gate("X", 1)], [gate("Y", 2)]])],
        ]),
      ],
    ]),
  );

  removeOperation(model, "0,0-0,0-2,0");

  expectOp(at(model, "0,0"), { Outer: { targets: [0, 1] } });
  expectOp(at(model, "0,0-0,0"), { Inner: { targets: [0, 1] } });
});

test("removeOperation: removing a group's last child prunes the emptied group", () => {
  const model = build(circuit(2, [[group("Foo", [[gate("H", 0)]])]]));

  removeOperation(model, "0,0-0,0");

  // Foo held only H; with H gone the group is deleted, leaving an empty circuit.
  expectGrid(model, []);
});

test("removeOperation: prune cascades through nested groups emptied in lockstep", () => {
  // Inner is Outer's only child, so removing Inner's only gate empties Inner, which empties Outer.
  const model = build(
    circuit(2, [[group("Outer", [[group("Inner", [[gate("H", 0)]])]])]]),
  );

  removeOperation(model, "0,0-0,0-0,0");

  expectGrid(model, []);
});

test("removeOperation: prune STOPS at the first non-empty ancestor", () => {
  // Y keeps Outer alive after Inner is emptied, so only Inner is pruned.
  const model = build(
    circuit(2, [
      [group("Outer", [[group("Inner", [[gate("H", 0)]]), gate("Y", 0)]])],
    ]),
  );

  removeOperation(model, "0,0-0,0-0,0");

  // Inner is gone; Outer survives holding just Y.
  expectOp(at(model, "0,0"), { Outer: { children: [[{ Y: 0 }]] } });
});
