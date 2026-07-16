// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Remove-mutator tests on grouped shapes, driven through the public `removeOperation` action.
// Counterpart to `addRemove.test.mjs` (which covers the flat, non-grouped case). Focuses on
// stripping a leaf inside a group and the ancestor-`.targets` narrowing that follows the removal.

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import { removeOperation } from "../../../dist/ux/circuit-vis/actions/circuitActions.js";
import {
  at,
  build,
  circuit,
  expectGrid,
  expectOp,
  gate,
  group,
} from "../_helpers.mjs";

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
