// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Add/remove mutator tests on grouped shapes against `CircuitModel`.
// Counterpart to `addRemove.test.mjs` (which covers the flat,
// non-grouped case). Focuses on `findAndRemoveOperations`'s
// recursion into group children and the ancestor-`.targets`
// narrowing that follows.

// @ts-check

import { test } from "node:test";
import { findAndRemoveOperations } from "../../../dist/ux/circuit-vis/actions/circuitActions.js";
import { at, build, circuit, expectOp, gate, group } from "../_helpers.mjs";

test("findAndRemoveOperations recurses into expanded-group children", () => {
  const model = build(
    circuit(2, [[group("Group", [[gate("H", 0), gate("X", 1)]])]]),
  );

  findAndRemoveOperations(model, (/** @type {any} */ op) => op.gate === "X");

  // Outer group remains; the X inside its single child column is gone.
  expectOp(at(model, "0,0"), { Group: { children: [[{ H: 0 }]] } });
});

test("findAndRemoveOperations: removing a deep child narrows the group's targets", () => {
  // Foo spans wires 0-1; predicate-remove Y must narrow its cached
  // targets to just [0].
  const model = build(
    circuit(2, [[group("Foo", [[gate("H", 0)], [gate("Y", 1)]])]]),
  );

  findAndRemoveOperations(model, (/** @type {any} */ op) => op.gate === "Y");

  expectOp(at(model, "0,0"), { Foo: { targets: [0] } });
});

test("findAndRemoveOperations: cascade — removing across multiple nested groups narrows every ancestor", () => {
  // Outer ⊃ Inner, one gate per inner column. Removing Y narrows
  // Inner to [0,1] and Outer cascade-narrows in lockstep.
  const model = build(
    circuit(3, [
      [
        group("Outer", [
          [group("Inner", [[gate("H", 0)], [gate("X", 1)], [gate("Y", 2)]])],
        ]),
      ],
    ]),
  );

  findAndRemoveOperations(model, (/** @type {any} */ op) => op.gate === "Y");

  expectOp(at(model, "0,0"), { Outer: { targets: [0, 1] } });
  expectOp(at(model, "0,0-0,0"), { Inner: { targets: [0, 1] } });
});
