// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// circuitActions tests: exercises the Action-layer of the circuit
// editor (`ux/circuit-vis/circuitActions.ts`) directly against a
// `CircuitModel` (Data layer), with **no JSDOM and no `CircuitEvents`
// stub**. That direct testability is the main point of the
// Data/Action/View split landed in R3 — see
// `ux/circuit-vis/CIRCUIT_EDITOR_TODO.md`.
//
// Tests cover the small mutation contracts each action promises:
// componentGrid layout, qubitUseCounts bookkeeping, and the trailing-
// wire trim that several actions trigger as a side effect.

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import { CircuitModel } from "../dist/ux/circuit-vis/data/circuitModel.js";
import {
  addControl,
  addOperation,
  findAndRemoveOperations,
  moveQubit,
  removeControl,
  removeOperation,
  removeQubit,
} from "../dist/ux/circuit-vis/actions/circuitActions.js";

/**
 * Build a fresh empty Circuit with `n` qubits and no operations.
 * @param {number} n
 * @returns {import("../dist/ux/circuit-vis/index.js").Circuit}
 */
function emptyCircuit(n) {
  return {
    qubits: Array.from({ length: n }, (_, id) => ({ id })),
    componentGrid: [],
  };
}

/**
 * Build a unitary-gate template (the shape `addOperation` deep-copies).
 * @param {string} gate
 */
function unitary(gate) {
  return { kind: "unitary", gate, targets: [{ qubit: 0 }] };
}

test("CircuitModel constructor seeds qubitUseCounts from the existing grid", () => {
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 0 }],
            controls: [{ qubit: 1 }],
          },
        ],
      },
    ],
  };

  const model = new CircuitModel(/** @type {any} */ (circuit));

  assert.deepEqual(model.qubitUseCounts, [1, 1, 0]);
});

test("addOperation appends to the target column and bumps qubitUseCounts", () => {
  const model = new CircuitModel(emptyCircuit(2));

  const added = addOperation(model, unitary("H"), "0,0", 0);

  assert.ok(added, "addOperation should return the new operation");
  assert.equal(model.componentGrid.length, 1);
  assert.equal(model.componentGrid[0].components.length, 1);
  assert.equal(model.componentGrid[0].components[0].gate, "H");
  // The op the action returns is the same reference it inserted into
  // the grid — the deep-copy is taken from the input template, not
  // the stored op.
  assert.equal(added, model.componentGrid[0].components[0]);
  assert.deepEqual(model.qubitUseCounts, [1, 0]);
});

test("removeOperation drops the op and decrements qubitUseCounts", () => {
  const model = new CircuitModel(emptyCircuit(2));
  addOperation(model, unitary("H"), "0,0", 0);
  addOperation(model, unitary("X"), "1,0", 1);
  // Each addOperation appends a fresh column → grid is now [[H@0], [X@1]].
  assert.equal(model.componentGrid.length, 2);
  assert.deepEqual(model.qubitUseCounts, [1, 1]);

  // Remove the X (column 1).
  removeOperation(model, "1,0");

  assert.equal(model.componentGrid.length, 1);
  assert.equal(model.componentGrid[0].components[0].gate, "H");
  // Wire 1 went to 0 uses → trailing-wire trim drops it.
  assert.deepEqual(model.qubitUseCounts, [1]);
  assert.equal(model.qubits.length, 1);
});

test("addControl/removeControl maintain qubitUseCounts and trim trailing wires", () => {
  const model = new CircuitModel(emptyCircuit(1));
  addOperation(model, unitary("X"), "0,0", 0);
  assert.deepEqual(model.qubitUseCounts, [1]);

  // Add a control on a brand-new wire. The action should grow the
  // qubit list, bump the use count, and never shrink it back behind
  // wire 1.
  const op = /** @type {any} */ (model.componentGrid[0].components[0]);
  const ok = addControl(model, op, 1);
  assert.equal(ok, true);
  assert.equal(model.qubits.length, 2);
  assert.deepEqual(model.qubitUseCounts, [1, 1]);

  // Adding the same control again is a no-op.
  assert.equal(addControl(model, op, 1), false);
  assert.deepEqual(model.qubitUseCounts, [1, 1]);

  // Removing the control on the trailing wire should also trim it.
  assert.equal(removeControl(model, op, 1), true);
  assert.equal(model.qubits.length, 1);
  assert.deepEqual(model.qubitUseCounts, [1]);
});

test("findAndRemoveOperations decrements qubitUseCounts and prunes empty columns", () => {
  const model = new CircuitModel(emptyCircuit(2));
  addOperation(model, unitary("H"), "0,0", 0);
  addOperation(model, unitary("X"), "1,0", 1);
  // Grid: [[H@0], [X@1]].
  assert.deepEqual(model.qubitUseCounts, [1, 1]);

  findAndRemoveOperations(model, (/** @type {any} */ op) => op.gate === "X");

  assert.equal(model.componentGrid.length, 1);
  assert.equal(model.componentGrid[0].components[0].gate, "H");
  // findAndRemoveOperations only decrements counts — it does NOT trim
  // trailing wires (callers do that explicitly when they need to).
  assert.deepEqual(model.qubitUseCounts, [1, 0]);
});

test("moveQubit swaps register references and reorders ops within a column", () => {
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          { kind: "unitary", gate: "X", targets: [{ qubit: 0 }] },
          { kind: "unitary", gate: "H", targets: [{ qubit: 1 }] },
        ],
      },
    ],
  };
  const model = new CircuitModel(/** @type {any} */ (circuit));

  moveQubit(
    model,
    /* sourceWire */ 0,
    /* targetWire */ 1,
    /* isBetween */ false,
  );

  // After the swap, X targets wire 1 and H targets wire 0; column is
  // re-sorted so H (lowest reg = 0) comes first.
  const ops = model.componentGrid[0].components;
  assert.equal(ops[0].gate, "H");
  assert.equal(/** @type {any} */ (ops[0]).targets[0].qubit, 0);
  assert.equal(ops[1].gate, "X");
  assert.equal(/** @type {any} */ (ops[1]).targets[0].qubit, 1);
  // Qubit ids are renumbered to match positions.
  assert.equal(model.qubits[0].id, 0);
  assert.equal(model.qubits[1].id, 1);
});

test("removeQubit shifts higher wire indices down by one", () => {
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [{ kind: "unitary", gate: "X", targets: [{ qubit: 2 }] }],
      },
    ],
  };
  const model = new CircuitModel(/** @type {any} */ (circuit));
  assert.deepEqual(model.qubitUseCounts, [0, 0, 1]);

  removeQubit(model, 1);

  assert.equal(model.qubits.length, 2);
  // Wire 2's reference shifts to wire 1 (since wire 1 was deleted).
  const op = /** @type {any} */ (model.componentGrid[0].components[0]);
  assert.equal(op.targets[0].qubit, 1);
  // qubitUseCounts unchanged at the removed index, only the slot is gone.
  assert.deepEqual(model.qubitUseCounts, [0, 1]);
});
