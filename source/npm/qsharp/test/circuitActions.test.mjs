// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// circuitActions tests: exercises the Action-layer of the circuit
// editor (`ux/circuit-vis/circuitActions.ts`) directly against a
// `CircuitModel` (Data layer), with **no JSDOM and no `CircuitEvents`
// stub**.
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

// ---------------------------------------------------------------------------
// Edge cases & alternate paths
// ---------------------------------------------------------------------------

test("addOperation with insertNewColumn=true creates a fresh column", () => {
  const model = new CircuitModel(emptyCircuit(2));
  addOperation(model, unitary("H"), "0,0", 0);
  // Grid: [[H@0]].

  // Drop another op on column 0 with insertNewColumn=true — should
  // push the new op into a fresh column 0 and shift H to column 1.
  addOperation(
    model,
    /** @type {any} */ ({
      kind: "unitary",
      gate: "X",
      targets: [{ qubit: 1 }],
    }),
    "0,0",
    1,
    /* insertNewColumn */ true,
  );

  assert.equal(model.componentGrid.length, 2);
  assert.equal(model.componentGrid[0].components[0].gate, "X");
  assert.equal(model.componentGrid[1].components[0].gate, "H");
  assert.deepEqual(model.qubitUseCounts, [1, 1]);
});

test("addOperation grows qubits to fit a wire beyond the current count", () => {
  const model = new CircuitModel(emptyCircuit(1));
  assert.equal(model.qubits.length, 1);

  addOperation(model, unitary("H"), "0,0", 3);

  assert.equal(model.qubits.length, 4);
  assert.deepEqual(model.qubitUseCounts, [0, 0, 0, 1]);
});

test("addOperation with a missing target location returns null", () => {
  const model = new CircuitModel(emptyCircuit(2));

  // Empty location string parses to root; `Location.parse("").last()`
  // returns null, so addOperation reports failure and the model is
  // unchanged.
  const result = addOperation(model, unitary("H"), "", 0);

  assert.equal(result, null);
  assert.equal(model.componentGrid.length, 0);
});

test("addOperation deep-copies its source operation template", () => {
  const model = new CircuitModel(emptyCircuit(2));
  const template = unitary("H");

  const added = addOperation(model, template, "0,0", 0);

  // Mutating the original template after add must not affect the model.
  template.gate = "MUTATED";
  assert.equal(/** @type {any} */ (added).gate, "H");
  assert.equal(model.componentGrid[0].components[0].gate, "H");
});

test("removeOperation on a root location is a safe no-op", () => {
  const model = new CircuitModel(emptyCircuit(2));
  addOperation(model, unitary("H"), "0,0", 0);

  // Root location "" — findOperation sees `last == null` and returns
  // null cleanly; removeOperation does nothing.
  const result = removeOperation(model, "");

  assert.equal(result, null);
  assert.equal(model.componentGrid.length, 1);
  assert.deepEqual(model.qubitUseCounts, [1, 0]);
});

test("addControl is a no-op when the wire is already a control", () => {
  const model = new CircuitModel(emptyCircuit(2));
  addOperation(model, unitary("X"), "0,0", 0);
  const op = /** @type {any} */ (model.componentGrid[0].components[0]);

  assert.equal(addControl(model, op, 1), true);
  assert.equal(model.qubitUseCounts[1], 1);

  // Second call with the same wire — already exists.
  assert.equal(addControl(model, op, 1), false);
  // Use count NOT bumped a second time.
  assert.equal(model.qubitUseCounts[1], 1);
  assert.equal(op.controls.length, 1);
});

test("removeControl on a wire with no control returns false", () => {
  const model = new CircuitModel(emptyCircuit(2));
  addOperation(model, unitary("X"), "0,0", 0);
  const op = /** @type {any} */ (model.componentGrid[0].components[0]);

  // Op has no controls at all.
  assert.equal(removeControl(model, op, 1), false);

  // Add one, then try to remove a different wire.
  addControl(model, op, 1);
  assert.equal(removeControl(model, op, 0), false);
  assert.equal(op.controls.length, 1);
});

test("findAndRemoveOperations leaves the grid empty when every op matches", () => {
  const model = new CircuitModel(emptyCircuit(2));
  addOperation(model, unitary("H"), "0,0", 0);
  addOperation(model, unitary("X"), "1,0", 1);

  findAndRemoveOperations(model, () => true);

  assert.equal(model.componentGrid.length, 0);
  // findAndRemoveOperations decrements but does not trim trailing wires.
  assert.deepEqual(model.qubitUseCounts, [0, 0]);
});

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

test("moveQubit with isBetween=true inserts before the target wire", () => {
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          { kind: "unitary", gate: "X", targets: [{ qubit: 0 }] },
          { kind: "unitary", gate: "Y", targets: [{ qubit: 1 }] },
          { kind: "unitary", gate: "Z", targets: [{ qubit: 2 }] },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Move wire 0 to just before wire 2 (isBetween=true).
  moveQubit(model, 0, 2, true);

  // Expected new wire order: [Y, X, Z]. After the rewire, ops carry
  // the *new* wire indices for their targets.
  const ops = model.componentGrid[0].components;
  assert.equal(ops[0].gate, "Y");
  assert.equal(/** @type {any} */ (ops[0]).targets[0].qubit, 0);
  assert.equal(ops[1].gate, "X");
  assert.equal(/** @type {any} */ (ops[1]).targets[0].qubit, 1);
  assert.equal(ops[2].gate, "Z");
  assert.equal(/** @type {any} */ (ops[2]).targets[0].qubit, 2);
});

test("moveQubit with sourceWire === targetWire is a no-op", () => {
  /** @type {any} */
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
  const model = new CircuitModel(circuit);
  const before = JSON.stringify(model.componentGrid);

  moveQubit(model, 1, 1, false);

  assert.equal(JSON.stringify(model.componentGrid), before);
});

test("removeQubit decrements use counts for ops that targeted the removed wire", () => {
  // `removeQubit` is a low-level rewire — its callers (e.g. the
  // qubit controller) are responsible for first removing any ops
  // attached to the doomed wire via `findAndRemoveOperations`.
  // This test exercises that combined flow end-to-end.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      { components: [{ kind: "unitary", gate: "X", targets: [{ qubit: 0 }] }] },
      { components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 1 }] }] },
      { components: [{ kind: "unitary", gate: "Z", targets: [{ qubit: 2 }] }] },
    ],
  };
  const model = new CircuitModel(circuit);
  assert.deepEqual(model.qubitUseCounts, [1, 1, 1]);

  // Step 1: remove ops referencing wire 1.
  findAndRemoveOperations(model, (/** @type {any} */ op) =>
    /** @type {any} */ (op).targets?.some(
      (/** @type {any} */ t) => t.qubit === 1,
    ),
  );
  // Step 2: drop the wire itself.
  removeQubit(model, 1);

  assert.equal(model.qubits.length, 2);
  // Two columns remain (X@wire 0 and Z@wire 1 after shift).
  assert.equal(model.componentGrid.length, 2);
  const ops = model.componentGrid.map((c) => c.components[0]);
  assert.equal(/** @type {any} */ (ops[0]).gate, "X");
  assert.equal(/** @type {any} */ (ops[0]).targets[0].qubit, 0);
  assert.equal(/** @type {any} */ (ops[1]).gate, "Z");
  assert.equal(/** @type {any} */ (ops[1]).targets[0].qubit, 1);
});
