// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Add/remove mutator tests on grouped shapes against `CircuitModel`.
// Counterpart to `addRemove.test.mjs` (which covers the flat,
// non-grouped case). Focuses on `findAndRemoveOperations`'s
// recursion into group children and the ancestor-`.targets`
// narrowing that follows.

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import { CircuitModel } from "../../../dist/ux/circuit-vis/data/circuitModel.js";
import { findAndRemoveOperations } from "../../../dist/ux/circuit-vis/actions/circuitActions.js";

test("findAndRemoveOperations recurses into expanded-group children", () => {
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Group",
            targets: [{ qubit: 0 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
                  { kind: "unitary", gate: "X", targets: [{ qubit: 1 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  findAndRemoveOperations(model, (/** @type {any} */ op) => op.gate === "X");

  // Outer group remains; the X inside its single child column is gone.
  const groupOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  assert.equal(groupOp.gate, "Group");
  assert.equal(groupOp.children.length, 1);
  assert.equal(groupOp.children[0].components.length, 1);
  assert.equal(groupOp.children[0].components[0].gate, "H");
});

test("findAndRemoveOperations: removing a deep child narrows the group's targets", () => {
  // Foo spans wires 0-1 with H on wire 0 and Y on wire 1. Predicate-
  // remove Y; Foo's cached targets must narrow to just [wire 0].
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 0 }, { qubit: 1 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
                ],
              },
              {
                components: [
                  { kind: "unitary", gate: "Y", targets: [{ qubit: 1 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  findAndRemoveOperations(model, (/** @type {any} */ op) => op.gate === "Y");

  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const fooQubits = fooOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.deepEqual(
    fooQubits,
    [0],
    `Foo must narrow to [0] after removing Y; got ${JSON.stringify(fooQubits)}`,
  );
});

test("findAndRemoveOperations: cascade — removing across multiple nested groups narrows every ancestor", () => {
  // Outer (wires 0-2) contains Inner (wires 0-2) with H on 0, X on
  // 1, Y on 2 — one per inner column. Predicate-remove Y. Inner
  // narrows to [0,1] and Outer cascade-narrows in lockstep.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Outer",
            targets: [{ qubit: 0 }, { qubit: 1 }, { qubit: 2 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "Inner",
                    targets: [{ qubit: 0 }, { qubit: 1 }, { qubit: 2 }],
                    children: [
                      {
                        components: [
                          {
                            kind: "unitary",
                            gate: "H",
                            targets: [{ qubit: 0 }],
                          },
                        ],
                      },
                      {
                        components: [
                          {
                            kind: "unitary",
                            gate: "X",
                            targets: [{ qubit: 1 }],
                          },
                        ],
                      },
                      {
                        components: [
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
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  findAndRemoveOperations(model, (/** @type {any} */ op) => op.gate === "Y");

  const outerOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const innerOp = /** @type {any} */ (outerOp.children[0].components[0]);

  const innerQubits = innerOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  const outerQubits = outerOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);

  assert.deepEqual(
    innerQubits,
    [0, 1],
    `Inner must narrow to [0,1]; got ${JSON.stringify(innerQubits)}`,
  );
  assert.deepEqual(
    outerQubits,
    [0, 1],
    `Outer must cascade-narrow to [0,1]; got ${JSON.stringify(outerQubits)}`,
  );
});
