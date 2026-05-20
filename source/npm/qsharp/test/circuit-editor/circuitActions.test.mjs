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
import { CircuitModel } from "../../dist/ux/circuit-vis/data/circuitModel.js";
import {
  addControl,
  addOperation,
  collectExternalProducerLocations,
  findAndRemoveOperations,
  moveOperation,
  moveQubit,
  removeControl,
  removeOperation,
  removeQubit,
} from "../../dist/ux/circuit-vis/actions/circuitActions.js";
import { Location } from "../../dist/ux/circuit-vis/data/location.js";

/**
 * Build a fresh empty Circuit with `n` qubits and no operations.
 * @param {number} n
 * @returns {import("../../dist/ux/circuit-vis/index.js").Circuit}
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

// ---------------------------------------------------------------------------
// `moveOperation` cross-scope correctness.
//
// The earlier implementation looked up the source op's parent grid
// AFTER `_moveX` had already mutated the model. When `_moveX` spliced
// a new column ahead of the source's path (e.g. moving a child out of
// a group to a fresh top-level column at index 0), the source's
// location string went stale and `findParentArray` either returned
// the wrong grid or null — leaving a duplicate of the source op in
// the original group.
//
// These tests pin down the cross-scope contract: after a successful
// move, the original location's grid no longer contains the op, and
// the target grid contains exactly one copy.
// ---------------------------------------------------------------------------

test("moveOperation: moving a child out of a group to a new column ahead of the group does NOT leave a duplicate behind", () => {
  // Top-level grid layout:
  //   col 0: X on wire 2
  //   col 1: Group on wires 0+1, with one child H on wire 0.
  // The bug: moving the inner H from inside the group to a fresh
  // top-level column at index 0 used to leave the original H still
  // inside the group's children (a duplicate).
  //
  // Post-D1 (empty-group cleanup) the group itself disappears
  // because moving out its only child empties it. The "no
  // duplicate" guarantee is strengthened: there's neither a
  // duplicate H nor an empty Group shell.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [{ kind: "unitary", gate: "X", targets: [{ qubit: 2 }] }],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "Group",
            targets: [{ qubit: 0 }, { qubit: 1 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Move H from "1,0-0,0" (inside group) to top-level "0,0".
  // insertNewColumn=true mirrors what the drag controller does for
  // an inter-column dropzone — it forces a fresh column ahead of
  // the existing col 0, shifting every existing top-level index by 1.
  const moved = moveOperation(
    model,
    "1,0-0,0",
    "0,0",
    /* sourceWire */ 0,
    /* targetWire */ 0,
    /* movingControl */ false,
    /* insertNewColumn */ true,
  );

  assert.ok(moved, "move should return the new operation");

  // Top-level grid: [new H@0], [X@2]. The Group is gone (D1
  // cleanup pruned it because its last child departed).
  assert.equal(
    model.componentGrid.length,
    2,
    "two top-level columns: the relocated H and the X (Group is gone)",
  );
  assert.equal(
    /** @type {any} */ (model.componentGrid[0].components[0]).gate,
    "H",
  );
  assert.equal(
    /** @type {any} */ (model.componentGrid[1].components[0]).gate,
    "X",
  );

  // No Group anywhere in the grid (and no second H — the original
  // duplicate-bug guarantee).
  /** @type {string[]} */
  const allGates = [];
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      allGates.push(/** @type {any} */ (op).gate);
    }
  }
  assert.deepEqual(
    allGates.sort(),
    ["H", "X"],
    "no duplicate H and no stale Group shell",
  );
});

test("moveOperation: moving a child out of a group updates the group's targets to drop the departed wire", () => {
  // The parent group's `targets` array is a derived render-extent
  // claim: it must reflect the union of its remaining children's
  // wires. Pre-fix, the parent's `targets` was recomputed BEFORE the
  // child was removed, so it still included the departed child's
  // wire — leaving the group claiming a wire it no longer contains.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Group",
            targets: [{ qubit: 0 }, { qubit: 1 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
                  { kind: "unitary", gate: "Z", targets: [{ qubit: 1 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Move the H (wire 0 inside the group) out to top-level on wire 2.
  moveOperation(
    model,
    "0,0-0,0",
    "1,0",
    /* sourceWire */ 0,
    /* targetWire */ 2,
    /* movingControl */ false,
    /* insertNewColumn */ true,
  );

  // Group should now only contain Z on wire 1.
  const groupOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const wires = groupOp.targets.map((/** @type {any} */ t) => t.qubit).sort();
  assert.deepEqual(
    wires,
    [1],
    "group's targets must reflect only its remaining children's wires",
  );
});

test("moveOperation: returns null when sourceLocation does not resolve", () => {
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }],
    componentGrid: [
      {
        components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 0 }] }],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  const result = moveOperation(model, "5,5-9,9", "0,0", 0, 0, false, false);

  assert.equal(result, null);
  // Model untouched.
  assert.equal(model.componentGrid.length, 1);
  assert.equal(
    /** @type {any} */ (model.componentGrid[0].components[0]).gate,
    "H",
  );
});

// ---------------------------------------------------------------------------
// moveOperation: multi-wire ops (groups, SWAP-like multi-target gates)
// move as a rigid unit
// ---------------------------------------------------------------------------

test("moveOperation: dragging a multi-target gate (SWAP) shifts all targets by the delta", () => {
  // Regression: pre-fix, `_moveY` did `targets = [{ qubit: targetWire }]`
  // unconditionally, which collapsed a SWAP at wires [0, 2] down to a
  // single-target gate on the drop wire — destroying half the gate.
  // The fix detects multi-target ops and shifts every register by
  // `targetWire - sourceWire` so the whole gate moves as a unit.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "SWAP",
            targets: [{ qubit: 0 }, { qubit: 2 }],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // User grabbed wire 0 of the SWAP and dropped on wire 1 → delta = +1.
  // Pre-fix: targets = [{ qubit: 1 }] (single-target, gate destroyed).
  // Post-fix: targets = [{ qubit: 1 }, { qubit: 3 }] (SWAP intact, shifted).
  const moved = moveOperation(
    model,
    "0,0",
    "0,0",
    /* sourceWire */ 0,
    /* targetWire */ 1,
    /* movingControl */ false,
    /* insertNewColumn */ false,
  );

  assert.ok(moved);
  const wires = /** @type {any} */ (moved).targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.deepEqual(
    wires,
    [1, 3],
    "both SWAP targets must shift by the delta; gate keeps its 2-wire shape",
  );
  // The shift introduces wire 3 — model must have grown to accommodate it.
  assert.equal(model.qubits.length, 4, "wire 3 must exist after the shift");
});

test("moveOperation: dragging a group shifts the box AND all child register refs", () => {
  // Regression: pre-fix, moving a group rewrote the group's
  // `.targets` to a single wire and left every child op pointing
  // at the original wires. The visible symptom was "the group box
  // moves but the contents stay put". The fix shifts every
  // register on the group AND recursively every register in the
  // group's children grid by the same delta.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Group",
            // Group spans wires 0..1 via its children.
            targets: [{ qubit: 0 }, { qubit: 1 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
                  {
                    kind: "unitary",
                    gate: "CNOT",
                    targets: [{ qubit: 1 }],
                    controls: [{ qubit: 0 }],
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

  // User grabbed wire 0 of the group and dropped on wire 2 → delta = +2.
  // Expected: group.targets shifts to [2, 3], H child shifts to wire 2,
  // CNOT child shifts to target=3 control=2.
  const moved = moveOperation(
    model,
    "0,0",
    "0,0",
    /* sourceWire */ 0,
    /* targetWire */ 2,
    /* movingControl */ false,
    /* insertNewColumn */ false,
  );

  assert.ok(moved);
  const movedAny = /** @type {any} */ (moved);
  const groupWires = movedAny.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.deepEqual(
    groupWires,
    [2, 3],
    "group's derived targets must shift by the delta",
  );

  // Children must have followed the box.
  const h = movedAny.children[0].components[0];
  const cnot = movedAny.children[0].components[1];
  assert.equal(h.gate, "H");
  assert.equal(h.targets[0].qubit, 2, "H child must shift from wire 0 → 2");
  assert.equal(cnot.gate, "CNOT");
  assert.equal(
    cnot.targets[0].qubit,
    3,
    "CNOT target must shift from wire 1 → 3",
  );
  assert.equal(
    cnot.controls[0].qubit,
    2,
    "CNOT control must shift from wire 0 → 2",
  );
});

test("moveOperation: moving a SWAP down by one creates the new bottom wire", () => {
  // Anchoring sanity check: shifting bumps the model's wire count
  // to accommodate the new high wire (here wire 3, which didn't
  // exist pre-move). Without the ensureQubitCount fix in
  // moveOperation, _moveX would file the op into the grid before
  // the model knew wire 3 existed, leaving qubitUseCounts out of
  // step with the actual register set.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "SWAP",
            targets: [{ qubit: 0 }, { qubit: 2 }],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  assert.equal(model.qubits.length, 3, "precondition: 3 wires");

  moveOperation(model, "0,0", "0,0", 0, 1, false, false);

  assert.equal(
    model.qubits.length,
    4,
    "model must have grown to make room for the shifted high wire",
  );
});

test("moveOperation: single-target controlled-gate move still rewires just one leg (no regression)", () => {
  // Defensive: the unit-shift path must NOT engage for ordinary
  // CNOT-style gates (1 target + N controls). Dragging the target
  // of a CNOT to a new wire should leave the control alone — the
  // long-established "rewire one leg" interaction.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 1 }],
            controls: [{ qubit: 0 }],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Drag the target from wire 1 to wire 2. The control on wire 0
  // must stay put (single-leg path).
  const moved = moveOperation(model, "0,0", "0,0", 1, 2, false, false);

  assert.ok(moved);
  const movedAny = /** @type {any} */ (moved);
  assert.equal(movedAny.targets.length, 1);
  assert.equal(movedAny.targets[0].qubit, 2, "target follows the drag");
  assert.equal(movedAny.controls.length, 1);
  assert.equal(
    movedAny.controls[0].qubit,
    0,
    "control must NOT have moved (single-leg behavior preserved)",
  );
});

test("moveOperation: moving a group with a classically-controlled child anchors the classical control", () => {
  // Regression for the render crash:
  //   "Classical register ID 0 invalid for qubit ID N with 0 classical register(s)"
  //
  // A classical control register has the shape `{qubit, result}` —
  // the `qubit` field points to the WIRE that owns the classical
  // register (i.e. where the producing measurement lives),
  // **not** to a wire the gate acts on. When a group with a
  // classically-controlled child moves but the producing
  // measurement is EXTERNAL to the group, the classical control
  // must stay anchored to its current wire — otherwise it gets
  // re-pointed at a wire with no classical registers and the
  // renderer throws on the next paint.
  /** @type {any} */
  const circuit = {
    qubits: [
      { id: 0, numResults: 1 }, // wire 0 owns one classical register (the M's result)
      { id: 1 },
      { id: 2 },
      { id: 3 },
    ],
    componentGrid: [
      {
        // Top-level (EXTERNAL to the group) measurement on wire 0 —
        // the producer of the classical register the group's X is
        // conditioned on.
        components: [
          {
            kind: "measurement",
            gate: "Measure",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "Group",
            targets: [{ qubit: 1 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "X",
                    targets: [{ qubit: 1 }],
                    // Classical control: conditioned on the
                    // result of the EXTERNAL M (on wire 0).
                    controls: [{ qubit: 0, result: 0 }],
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

  // Drag the group from wire 1 to wire 2 → delta = +1.
  moveOperation(model, "1,0", "1,0", 1, 2, false, false);

  // The group's child X must have its TARGET shifted to wire 2,
  // but its CLASSICAL CONTROL must STILL point at wire 0 (the
  // measurement didn't move).
  const groupOp = /** @type {any} */ (model.componentGrid[1].components[0]);
  const x = groupOp.children[0].components[0];
  assert.equal(x.gate, "X");
  assert.equal(x.targets[0].qubit, 2, "X target must shift with the group");
  assert.equal(
    x.controls[0].qubit,
    0,
    "classical control must STAY anchored to wire 0 — its classical register did not move",
  );
  assert.equal(
    x.controls[0].result,
    0,
    "classical control's result index is unchanged",
  );
});

test("moveOperation: moving a group whose internal measurement produces the classical reg shifts the consumer", () => {
  // The mirror case: the producing measurement is INSIDE the moved
  // subtree, so the classical register it produces moves with it.
  // The consumer's classical control must shift by the same delta
  // to stay aligned with its producer; if we anchored it here
  // we'd leave a dangling reference to a wire that no longer has
  // any classical register at all.
  /** @type {any} */
  const circuit = {
    qubits: [
      { id: 0 },
      { id: 1, numResults: 1 }, // wire 1 owns the M's classical register
      { id: 2 },
      { id: 3 },
    ],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Group",
            targets: [{ qubit: 1 }],
            children: [
              {
                // Internal measurement on wire 1.
                components: [
                  {
                    kind: "measurement",
                    gate: "M",
                    qubits: [{ qubit: 1 }],
                    results: [{ qubit: 1, result: 0 }],
                  },
                ],
              },
              {
                // Internal X classically-controlled on the
                // INTERNAL M (matching (qubit, result) tuple).
                components: [
                  {
                    kind: "unitary",
                    gate: "X",
                    targets: [{ qubit: 1 }],
                    controls: [{ qubit: 1, result: 0 }],
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

  // Drag the group from wire 1 to wire 2 → delta = +1.
  moveOperation(model, "0,0", "0,0", 1, 2, false, false);

  const groupOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const m = groupOp.children[0].components[0];
  const x = groupOp.children[1].components[0];

  // M and its result moved to wire 2.
  assert.equal(m.qubits[0].qubit, 2, "internal M's qubit must shift");
  assert.equal(m.results[0].qubit, 2, "internal M's result must shift");

  // X target shifts AND the classical control follows because
  // its producer (the internal M) is also inside the moved
  // subtree.
  assert.equal(x.targets[0].qubit, 2, "X target must shift");
  assert.equal(
    x.controls[0].qubit,
    2,
    "classical control must FOLLOW its internal producer to wire 2",
  );
  assert.equal(x.controls[0].result, 0, "result index unchanged");

  // numResults bookkeeping must follow the measurement: wire 1
  // is no longer a producer, wire 2 is.
  assert.equal(
    model.qubits[1].numResults,
    undefined,
    "wire 1 must no longer claim a classical register (M moved away)",
  );
  assert.equal(
    model.qubits[2].numResults,
    1,
    "wire 2 must now claim the classical register (M lives here now)",
  );
});

test("moveOperation: refuses a unit-shift that would push wires below 0", () => {
  // Regression: a unit-shift with negative delta whose minimum
  // wire would land below 0 was previously executed anyway,
  // leaving the subtree with `qubit: -N` register refs. The next
  // render then either threw "Qubit register with ID -N not found"
  // OR, more often, threw the misleading
  // "Classical register ID X invalid for qubit ID Y with 0
  // classical register(s)" after `removeTrailingUnusedQubits`
  // trimmed the model in response to the corruption.
  //
  // The fix: refuse the move (return null, leave the model
  // untouched) when the unit-shift's lowest wire would land
  // below 0. The dragController treats a `null` return as a
  // no-op and skips the re-render.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Group",
            // Multi-target gate; touches wires 0..3. Lowest wire is 0,
            // so ANY negative delta would push below 0.
            targets: [{ qubit: 0 }, { qubit: 1 }, { qubit: 2 }, { qubit: 3 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "X", targets: [{ qubit: 0 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const before = JSON.stringify(circuit);
  const model = new CircuitModel(circuit);

  // User grabs wire 2 and drops on wire 0 → delta = -2, which
  // would push the Group's lowest wire (0) to -2.
  const result = moveOperation(model, "0,0", "0,0", 2, 0, false, false);

  assert.equal(result, null, "move must be refused");
  // Model must be untouched.
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
  // Boundary case: delta = -1 with min wire 1 → lowest post-shift
  // wire is 0. That's still in-range; the move must succeed.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Group",
            // Lowest wire is 1; delta = -1 lands it at 0.
            targets: [{ qubit: 1 }, { qubit: 2 }],
            children: [
              {
                components: [
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

  const result = moveOperation(model, "0,0", "0,0", 1, 0, false, false);

  assert.ok(result, "move must succeed when min post-shift wire is exactly 0");
  const group = /** @type {any} */ (model.componentGrid[0].components[0]);
  const wires = group.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.deepEqual(wires, [0, 1], "group's targets must shift to [0, 1]");
});

test("moveOperation: classical-ref in targets of a conditional anchors when producer is external", () => {
  // Regression: a classically-conditional unitary (e.g. `if: ...`)
  // records its classical-register dependency in BOTH its
  // `controls` array AND its `targets` array (the targets entry
  // is a visual extent claim that draws the line down to the
  // classical register box). The producer-internal-vs-external
  // rule applies to ALL such classical-ref entries, not just
  // controls.
  //
  // Bug: `_doShift` previously shifted `targets` unconditionally,
  // so a unit-shift of a conditional whose producer M was a
  // SIBLING (outside the moved subtree) re-pointed the targets
  // classical-ref at a wire that has no classical registers.
  // The renderer then threw:
  //   "Classical register ID 0 invalid for qubit ID 1 with 0
  //   classical register(s)"
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          // Producer M lives at the OUTER level on wire 0, col 0.
          {
            kind: "measurement",
            gate: "M",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
      {
        components: [
          // Conditional unitary in a STRICTLY LATER column. targets
          // include both a quantum ref AND a classical-ref pointing
          // back at the M above.
          {
            kind: "unitary",
            gate: "if",
            isConditional: true,
            targets: [{ qubit: 0 }, { qubit: 0, result: 0 }],
            controls: [{ qubit: 0, result: 0 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Move just the conditional from wire 0 to wire 1, staying in
  // its own column (col 1). Producer at col 0 → consumer at col 1
  // is strictly later, so the move is allowed by the producer-
  // column-ordering rule and `_doShift` runs.
  moveOperation(model, "1,0", "1,0", 0, 1, false, false);

  // After the move the columns may have reshuffled (a fresh
  // column can be inserted to hold the moved op or the M). Find
  // the conditional wherever it landed.
  /** @type {any} */
  let cond;
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      if (op.gate === "if") {
        cond = op;
        break;
      }
    }
    if (cond) break;
  }
  assert.ok(cond, "conditional must still be present in the grid");
  // The quantum target follows the move.
  assert.equal(cond.targets[0].qubit, 1, "quantum target must shift to wire 1");
  // The classical-ref target STAYS at wire 0 (where the producer M lives).
  assert.equal(
    cond.targets[1].qubit,
    0,
    "classical-ref in targets must anchor at producer wire 0",
  );
  assert.equal(cond.targets[1].result, 0, "result index unchanged");
  // Same rule for controls.
  assert.equal(
    cond.controls[0].qubit,
    0,
    "classical control must anchor at producer wire 0",
  );
  // The H inside the children shifts (its target is quantum).
  const h = cond.children[0].components[0];
  assert.equal(h.targets[0].qubit, 1, "child H's quantum target must shift");
});

test("moveOperation: moving the last child out deletes the empty group", () => {
  // Regression (D1): before this fix, dragging the last remaining
  // child out of a group left the group as
  //   { gate: "Group", targets: [], children: [{components:[]}] }
  // The next render either threw on the empty `targets` or
  // produced a zero-wire phantom that the user couldn't reach to
  // delete.
  //
  // Expected: the group quietly disappears once empty. The grid
  // contains only the relocated child.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
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
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Move the only child (H at 0,0-0,0) out to a new top-level
  // column on wire 1.
  moveOperation(model, "0,0-0,0", "0,1", 0, 1, false, true);

  // No `Group` should remain anywhere in the top-level grid.
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      assert.notEqual(
        /** @type {any} */ (op).gate,
        "Group",
        "empty group must be deleted, not left as a zero-content shell",
      );
    }
  }
  // The H must be present at wire 1.
  /** @type {any} */
  let h;
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      if (/** @type {any} */ (op).gate === "H") {
        h = op;
        break;
      }
    }
    if (h) break;
  }
  assert.ok(h, "H must still be present at top level");
  assert.equal(h.targets[0].qubit, 1, "H must have landed on wire 1");
});

test("moveOperation: empty-group cleanup cascades through nested groups", () => {
  // Regression (D1): when the move-out empties an inner group
  // AND the inner group was the only child of an outer group,
  // BOTH groups must disappear. The cleanup walks the ancestor
  // chain innermost-out, stopping at the first ancestor that
  // still has content.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Outer",
            targets: [{ qubit: 0 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "Inner",
                    targets: [{ qubit: 0 }],
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

  // Move the deepest leaf (H at 0,0-0,0-0,0) out to a new
  // top-level column on wire 1.
  moveOperation(model, "0,0-0,0-0,0", "0,1", 0, 1, false, true);

  // Both Outer and Inner must be gone.
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      const gate = /** @type {any} */ (op).gate;
      assert.ok(
        gate !== "Outer" && gate !== "Inner",
        `${gate} group must be deleted (cascading cleanup)`,
      );
    }
  }
});

test("moveOperation: cleanup STOPS at the first non-empty ancestor", () => {
  // Regression (D1): the cleanup must not over-delete. When the
  // innermost ancestor empties but its grandparent still has
  // other content, only the innermost ancestor disappears; the
  // grandparent stays put with its remaining content.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Outer",
            targets: [{ qubit: 0 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "Inner",
                    targets: [{ qubit: 0 }],
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
                    ],
                  },
                  // Sibling of Inner: keeps Outer alive after
                  // Inner is pruned.
                  { kind: "unitary", gate: "Y", targets: [{ qubit: 0 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  moveOperation(model, "0,0-0,0-0,0", "0,1", 0, 1, false, true);

  // Inner must be gone.
  /** @type {any} */
  let outer;
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      if (/** @type {any} */ (op).gate === "Outer") {
        outer = op;
      }
      assert.notEqual(
        /** @type {any} */ (op).gate,
        "Inner",
        "Inner must be deleted (it's now empty)",
      );
    }
  }
  // Outer must still be present, now containing just Y.
  assert.ok(outer, "Outer must survive (it still contains Y)");
  const survivors = outer.children[0].components.map(
    (/** @type {any} */ c) => c.gate,
  );
  assert.deepEqual(survivors, ["Y"], "Outer's only remaining child is Y");
});

// ============================================================
// D2: classical-condition before producer
// ============================================================

test("Location.before: document-order comparison", () => {
  // Quick sanity tests for the helper that backs the
  // dropzone-filter and moveOperation safety-net.
  const L = (s) => Location.parse(s);
  // Top-level columns.
  assert.equal(L("0,0").before(L("0,1")), true, "same col, smaller op first");
  assert.equal(L("0,1").before(L("0,0")), false, "same col, larger op last");
  assert.equal(L("0,0").before(L("1,0")), true, "smaller col first");
  assert.equal(L("1,0").before(L("0,0")), false, "larger col last");
  // Equal -> strict-before is false.
  assert.equal(
    L("0,1").before(L("0,1")),
    false,
    "equal is not strictly before",
  );
  // Ancestor / descendant.
  assert.equal(
    L("0,0").before(L("0,0-0,0")),
    true,
    "ancestor renders before descendant",
  );
  assert.equal(
    L("0,0-0,0").before(L("0,0")),
    false,
    "descendant does not come before ancestor",
  );
  // Cross-level comparison.
  assert.equal(
    L("0,0-5,5").before(L("0,1")),
    true,
    "deeply nested inside col 0 comes before col 1",
  );
  assert.equal(
    L("0,1").before(L("0,0-5,5")),
    false,
    "col 1 does not come before anything inside col 0",
  );
});

test("collectExternalProducerLocations: classical control with external M", () => {
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          // Top-level M produces (0,0).
          {
            kind: "measurement",
            gate: "M",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
          // Conditional unitary consumes (0,0).
          {
            kind: "unitary",
            gate: "if",
            isConditional: true,
            targets: [{ qubit: 0 }, { qubit: 0, result: 0 }],
            controls: [{ qubit: 0, result: 0 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const producers = collectExternalProducerLocations(
    model.componentGrid,
    "0,1",
  );
  assert.deepEqual(
    producers,
    ["0,0"],
    "external producer M at top-level col 0 must be reported",
  );
});

test("collectExternalProducerLocations: internal producer M is excluded", () => {
  // Producer M lives INSIDE the moved subtree → it travels with
  // the consumer, so it imposes no drop-target constraint.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }],
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
                  // Producer inside the group.
                  {
                    kind: "measurement",
                    gate: "M",
                    qubits: [{ qubit: 0 }],
                    results: [{ qubit: 0, result: 0 }],
                  },
                  // Consumer also inside the group.
                  {
                    kind: "unitary",
                    gate: "X",
                    targets: [{ qubit: 0 }],
                    controls: [{ qubit: 0, result: 0 }],
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
  const producers = collectExternalProducerLocations(
    model.componentGrid,
    "0,0",
  );
  assert.deepEqual(
    producers,
    [],
    "producer is internal to the moved subtree → not reported",
  );
});

test("moveOperation: refuses dropping a conditional before its producer M", () => {
  // Regression (D2): dragging a classically-conditional unitary
  // (or a group containing one) to a column before its producing
  // measurement leaves the renderer with classical refs pointing
  // at registers that don't exist yet at the consumer's position.
  // moveOperation refuses such drops (return null, no-op).
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "measurement",
            gate: "M",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "if",
            isConditional: true,
            targets: [{ qubit: 0 }, { qubit: 0, result: 0 }],
            controls: [{ qubit: 0, result: 0 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const before = JSON.stringify(circuit);
  const model = new CircuitModel(circuit);

  // Try to drop the conditional at top-level col 0 with
  // insertNewColumn=true — would put consumer at col 0, pushing M
  // to col 1. Move must be refused.
  const result = moveOperation(model, "1,0", "0,0", 0, 0, false, true);

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

test("moveOperation: allows dropping a conditional AFTER its producer M", () => {
  // Boundary check: the same conditional dropped to a column
  // after the producer M must succeed. The refusal mustn't
  // over-trigger.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "measurement",
            gate: "M",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
      // Unrelated filler column so we have somewhere to drop AFTER M.
      {
        components: [{ kind: "unitary", gate: "Y", targets: [{ qubit: 1 }] }],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "if",
            isConditional: true,
            targets: [{ qubit: 0 }, { qubit: 0, result: 0 }],
            controls: [{ qubit: 0, result: 0 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Drop the conditional at col 1 with insertNewColumn=true —
  // lands at new col 1 (after M at col 0). Must succeed.
  const result = moveOperation(model, "2,0", "1,0", 0, 0, false, true);

  assert.ok(result, "move must succeed: consumer remains after producer");
});

test("moveOperation: allows moving a group whose classical producer is INTERNAL", () => {
  // When the producer M lives inside the moved subtree, the
  // subtree is self-contained — there's no external constraint
  // and the move can go anywhere (including to the beginning of
  // the grid).
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }],
    componentGrid: [
      // Filler column so col 0 is occupied.
      {
        components: [{ kind: "unitary", gate: "Y", targets: [{ qubit: 1 }] }],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "Group",
            targets: [{ qubit: 0 }],
            children: [
              {
                components: [
                  // Producer inside the group.
                  {
                    kind: "measurement",
                    gate: "M",
                    qubits: [{ qubit: 0 }],
                    results: [{ qubit: 0, result: 0 }],
                  },
                  // Consumer also inside the group.
                  {
                    kind: "unitary",
                    gate: "X",
                    targets: [{ qubit: 0 }],
                    controls: [{ qubit: 0, result: 0 }],
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

  // Drop the group at col 0 with insertNewColumn=true — would
  // normally be "before its producer", but since the producer is
  // internal, the move is allowed.
  const result = moveOperation(model, "1,0", "0,0", 0, 0, false, true);

  assert.ok(
    result,
    "move must succeed: internal producer travels with the consumer",
  );
});

test("Location.inEarlierColumnThan: column-strict, ancestor-aware", () => {
  // Backs the dropzone-filter and moveOperation safety-net for the
  // "producer must precede consumer" rule. Different from plain
  // document-order `before`: two ops in the same column are
  // simultaneous, and ancestor groups project their column down
  // onto everything they contain.
  const L = (s) => Location.parse(s);

  // Different top-level columns.
  assert.equal(
    L("0,0").inEarlierColumnThan(L("1,0")),
    true,
    "earlier top-level column",
  );
  assert.equal(
    L("1,0").inEarlierColumnThan(L("0,0")),
    false,
    "later top-level column",
  );

  // Same column, different op-index — different ops in the same
  // column are simultaneous, so neither is "earlier" than the other.
  assert.equal(
    L("0,0").inEarlierColumnThan(L("0,1")),
    false,
    "same col, different op is simultaneous",
  );
  assert.equal(
    L("0,1").inEarlierColumnThan(L("0,0")),
    false,
    "same col, different op is simultaneous (reverse)",
  );

  // Identical locations — strictly earlier is false.
  assert.equal(
    L("0,0").inEarlierColumnThan(L("0,0")),
    false,
    "identical is not strictly earlier",
  );

  // Ancestor vs descendant — both occupy the same outer column.
  assert.equal(
    L("0,0").inEarlierColumnThan(L("0,0-1,0")),
    false,
    "ancestor shares outer column with descendant",
  );
  assert.equal(
    L("0,0-1,0").inEarlierColumnThan(L("0,0")),
    false,
    "descendant shares outer column with ancestor",
  );

  // The user's "promote-around-the-rule" scenario. Producer M
  // deeply nested at "0,0-1,0-0,0-1,0" (inside a for loop at
  // top-level col 0). Promoting the consumer to a sibling op of
  // the for loop at top-level col 0 must NOT count as earlier:
  // they're still in the same top-level time-step.
  assert.equal(
    L("0,0-1,0-0,0-1,0").inEarlierColumnThan(L("0,5")),
    false,
    "promoting consumer to producer's outer column does not bypass the rule",
  );
  assert.equal(
    L("0,0-1,0-0,0-1,0").inEarlierColumnThan(L("1,0")),
    true,
    "promoting consumer to a strictly later outer column is fine",
  );

  // Same outer group, later inner column.
  assert.equal(
    L("0,0-1,0").inEarlierColumnThan(L("0,0-2,0")),
    true,
    "same outer group, later inner column",
  );

  // Same outer group, same inner column, different op-index.
  assert.equal(
    L("0,0-1,0").inEarlierColumnThan(L("0,0-1,1")),
    false,
    "same inner column is simultaneous",
  );
});

test("moveOperation: refuses promoting a conditional to a sibling of the producer's outer group", () => {
  // Regression for the "promote-around-the-rule" scenario. Producer
  // M lives inside an outer group at top-level col 0; the consumer
  // also starts inside that group. Dragging the consumer OUT of the
  // group and dropping it as a sibling at top-level col 0 must be
  // refused — the consumer would land in the same top-level
  // time-step as the producer, even though it's a different op
  // position.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Outer",
            targets: [{ qubit: 0 }],
            children: [
              {
                components: [
                  // Producer at "0,0-0,0".
                  {
                    kind: "measurement",
                    gate: "M",
                    qubits: [{ qubit: 0 }],
                    results: [{ qubit: 0, result: 0 }],
                  },
                ],
              },
              {
                components: [
                  // Consumer at "0,0-1,0".
                  {
                    kind: "unitary",
                    gate: "if",
                    isConditional: true,
                    targets: [{ qubit: 0 }, { qubit: 0, result: 0 }],
                    controls: [{ qubit: 0, result: 0 }],
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
  const before = JSON.stringify({
    qubits: circuit.qubits,
    componentGrid: circuit.componentGrid,
  });
  const model = new CircuitModel(circuit);

  // Try to promote the consumer ("0,0-1,0") to a sibling of the
  // outer group at top-level col 0 ("0,X"). The target column is
  // the same as the outer group's column → simultaneous → refuse.
  const result = moveOperation(model, "0,0-1,0", "0,0", 0, 0, false, true);
  assert.equal(result, null, "must refuse: same top-level column as producer");
  assert.equal(
    JSON.stringify({
      qubits: model.qubits,
      componentGrid: model.componentGrid,
    }),
    before,
    "refusal must not mutate the model",
  );
});

test("moveOperation: allows promoting a conditional to a strictly later top-level column", () => {
  // Boundary check: the same promotion to top-level col 1 (strictly
  // after the producer's outer group at col 0) must succeed.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Outer",
            targets: [{ qubit: 0 }],
            children: [
              {
                components: [
                  {
                    kind: "measurement",
                    gate: "M",
                    qubits: [{ qubit: 0 }],
                    results: [{ qubit: 0, result: 0 }],
                  },
                ],
              },
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "if",
                    isConditional: true,
                    targets: [{ qubit: 0 }, { qubit: 0, result: 0 }],
                    controls: [{ qubit: 0, result: 0 }],
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
                    ],
                  },
                ],
              },
            ],
          },
        ],
      },
      // Filler at top-level col 1 so we have somewhere strictly
      // later than the outer group at col 0 to drop onto.
      {
        components: [{ kind: "unitary", gate: "Y", targets: [{ qubit: 1 }] }],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Promote consumer ("0,0-1,0") to top-level col 1 (after the
  // outer group). Must succeed.
  const result = moveOperation(model, "0,0-1,0", "1,0", 0, 0, false, true);
  assert.ok(result, "move must succeed: strictly later outer column");
});

// ---------------------------------------------------------------------------
// D4 Stage A: action-layer support for the right-edge trailing
// inner-column dropzone of an expanded group. The dropzone layer
// emits a dropzone at `data-dropzone-location="<prefix>-<N>,0"`
// where `<N>` is the group's existing child-column count (i.e. one
// past the rightmost existing column). The action layer must accept
// that location string and synthesize the new column in the group's
// `children` grid — without leaking the new op to the top level or
// creating a duplicate.
//
// `_addOp`'s existing "create column if absent" branch is what makes
// this work; these tests pin down the wire-format contract between
// the dropzone layer and the action layer.
// ---------------------------------------------------------------------------

test("addOperation: dropping on a group's trailing inner-column slot adds the op as a child", () => {
  // Foo spans wires 0-1 with one child column (a single H on wire 0).
  // Trailing inner-column dropzone location is "0,0-1,0".
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
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Drop a Y onto Foo's trailing inner column at wire 0. `addOperation`
  // sees a location with prefix "0,0" and colIndex=1 (one past the
  // single existing inner column); `_addOp` synthesizes the new
  // inner column.
  const added = addOperation(
    model,
    { kind: "unitary", gate: "Y", targets: [{ qubit: 0 }] },
    "0,0-1,0",
    0,
  );

  assert.ok(added, "addOperation should return the new op");

  // Top level is unchanged: still just Foo.
  assert.equal(model.componentGrid.length, 1);
  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  assert.equal(fooOp.gate, "Foo");

  // Foo now has 2 inner columns: the original H, and the new Y.
  assert.equal(
    fooOp.children.length,
    2,
    "Foo's children grid should have grown by one column",
  );
  assert.equal(fooOp.children[0].components[0].gate, "H");
  assert.equal(fooOp.children[1].components[0].gate, "Y");
});

test("moveOperation: moving an external gate to a group's trailing inner-column slot pulls it into the group", () => {
  // Top-level layout:
  //   col 0: Foo group on wires 0-1 with one child H on wire 0.
  //   col 1: Y on wire 0 (the external gate we'll move into Foo).
  // Trailing inner-column slot of Foo is "0,0-1,0".
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
            ],
          },
        ],
      },
      {
        components: [{ kind: "unitary", gate: "Y", targets: [{ qubit: 0 }] }],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Move Y from "1,0" to Foo's trailing inner slot "0,0-1,0", wire 0.
  // `insertNewColumn=false` is what the trailing-band dropzones set
  // (they're tagged `data-dropzone-inter-column="false"` — drop, not
  // insert-between).
  const moved = moveOperation(
    model,
    /* sourceLocation */ "1,0",
    /* targetLocation */ "0,0-1,0",
    /* sourceWire */ 0,
    /* targetWire */ 0,
    /* movingControl */ false,
    /* insertNewColumn */ false,
  );

  assert.ok(moved, "move should return the moved op");

  // Top level: just Foo. The external Y column is gone (its only op
  // moved into Foo).
  assert.equal(model.componentGrid.length, 1);
  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  assert.equal(fooOp.gate, "Foo");

  // Foo's children: [[H], [Y]].
  assert.equal(fooOp.children.length, 2);
  assert.equal(fooOp.children[0].components[0].gate, "H");
  assert.equal(fooOp.children[1].components[0].gate, "Y");

  // And there's no duplicate Y at top level.
  /** @type {string[]} */
  const topGates = [];
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      topGates.push(/** @type {any} */ (op).gate);
    }
  }
  assert.deepEqual(topGates, ["Foo"], "Y must not remain at top level");
});

test("moveOperation: moving an internal gate to its group's trailing inner-column slot keeps it inside the group", () => {
  // Foo spans wires 0-1 with two child columns:
  //   inner col 0: H on wire 0
  //   inner col 1: X on wire 1
  // Move the H from "0,0-0,0" to Foo's trailing inner slot "0,0-2,0"
  // (colIndex 2 = one past the existing inner colCount of 2). The
  // gate should land in a new inner col 2, and the source inner col
  // 0 should be cleaned up (now empty).
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

  const moved = moveOperation(
    model,
    /* sourceLocation */ "0,0-0,0",
    /* targetLocation */ "0,0-2,0",
    /* sourceWire */ 0,
    /* targetWire */ 0,
    /* movingControl */ false,
    /* insertNewColumn */ false,
  );

  assert.ok(moved, "move should return the moved op");

  // Top level: still just Foo (not dissolved — it still has X).
  assert.equal(model.componentGrid.length, 1);
  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  assert.equal(fooOp.gate, "Foo");

  // Foo's children: collect the gate sequence column-by-column.
  // The exact column count is an implementation detail (cleanup of
  // the now-empty inner col 0 may or may not collapse it), but the
  // gate sequence in order must be [X, H] — X was originally in
  // inner col 1, and H landed in the new inner col 2.
  /** @type {string[]} */
  const innerGates = [];
  for (const col of fooOp.children) {
    for (const op of col.components) {
      innerGates.push(/** @type {any} */ (op).gate);
    }
  }
  assert.deepEqual(
    innerGates,
    ["X", "H"],
    "H must land after X in the inner grid; no duplicate H, no stray",
  );
});
