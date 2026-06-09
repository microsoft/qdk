// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// circuitActions add/remove tests: exercises `addOperation`,
// `removeOperation`, `addControl`, and `removeControl` directly
// against a `CircuitModel` (Data layer), with **no JSDOM and no
// `CircuitEvents` stub**.
//
// Scope is limited to the non-group / non-nested cases. Group
// recursion, ancestor `.targets` refresh, span widening, group
// splits, and cross-scope `moveOperation` cases live in
// `circuitActions.test.mjs`.
//
// Tests cover the small mutation contracts each action promises:
// componentGrid layout, qubitUseCounts bookkeeping, the in-place
// updates to `op.controls`, and the trailing-wire trim that the
// remove paths trigger as a side effect.

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import { CircuitModel } from "../../../dist/ux/circuit-vis/data/circuitModel.js";
import {
  addControl,
  addOperation,
  removeControl,
  removeOperation,
} from "../../../dist/ux/circuit-vis/actions/circuitActions.js";

/**
 * Build a fresh empty Circuit with `n` qubits and no operations.
 * @param {number} n
 * @returns {import("../../../dist/ux/circuit-vis/index.js").Circuit}
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
 * @returns {import("../../../dist/ux/circuit-vis/index.js").Operation}
 */
function unitary(gate) {
  return { kind: "unitary", gate, targets: [{ qubit: 0 }] };
}

// ---------------------------------------------------------------------------
// addOperation
// ---------------------------------------------------------------------------

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

test("addOperation on an existing wire bumps qubitUseCounts without growing qubits", () => {
  const model = new CircuitModel(emptyCircuit(2));
  addOperation(model, unitary("H"), "0,0", 0);
  assert.equal(model.qubits.length, 2);
  assert.deepEqual(model.qubitUseCounts, [1, 0]);

  // Drop a second op on the SAME wire (column 1 to avoid the
  // same-column overlap path). Qubit list stays at 2; use count
  // for wire 0 climbs to 2.
  addOperation(model, unitary("X"), "1,0", 0);
  assert.equal(model.qubits.length, 2);
  assert.deepEqual(model.qubitUseCounts, [2, 0]);
});

test("addOperation on a wire several IDs beyond the end bulk-grows qubits", () => {
  const model = new CircuitModel(emptyCircuit(1));
  assert.equal(model.qubits.length, 1);

  // Drop on wire 5 — the gap is wires 1..4 plus wire 5 itself.
  // `ensureQubitCount(5)` must add all five new wires in one shot.
  addOperation(model, unitary("H"), "0,0", 5);

  assert.equal(model.qubits.length, 6);
  assert.deepEqual(model.qubitUseCounts, [0, 0, 0, 0, 0, 1]);
  // New wires should carry the synthesized ids assigned by
  // ensureQubitCount (id matches position).
  for (let i = 0; i < model.qubits.length; i++) {
    assert.equal(model.qubits[i].id, i);
  }
});

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

test("addOperation with insertNewColumn=true moves other operations to the right", () => {
  const model = new CircuitModel(emptyCircuit(2));
  addOperation(model, unitary("H"), "0,0", 0);
  addOperation(model, unitary("S"), "1,0", 0);
  // Grid: [[H@0], [S@0]].

  addOperation(
    model,
    /** @type {any} */ ({
      kind: "unitary",
      gate: "X",
      targets: [{ qubit: 1 }],
    }),
    "1,0",
    1,
    /* insertNewColumn */ true,
  );

  assert.equal(model.componentGrid.length, 3);
  assert.equal(model.componentGrid[0].components[0].gate, "H");
  assert.equal(model.componentGrid[1].components[0].gate, "X");
  assert.equal(model.componentGrid[2].components[0].gate, "S");
  assert.deepEqual(model.qubitUseCounts, [2, 1]);
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

test("addOperation with a missing target location returns null", () => {
  const model = new CircuitModel(emptyCircuit(2));

  // Empty location string parses to root; `Location.parse("").last()`
  // returns null, so addOperation reports failure and the model is
  // unchanged.
  const result = addOperation(model, unitary("H"), "", 0);

  assert.equal(result, null);
  assert.equal(model.componentGrid.length, 0);
});

// ---------------------------------------------------------------------------
// removeOperation
// ---------------------------------------------------------------------------

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

test("removeOperation from an interior wire leaves qubits.length untouched", () => {
  const model = new CircuitModel(emptyCircuit(3));
  addOperation(model, unitary("H"), "0,0", 0);
  addOperation(model, unitary("X"), "1,0", 1);
  addOperation(model, unitary("Z"), "2,0", 2);
  // Grid: [[H@0], [X@1], [Z@2]]; use counts [1, 1, 1].
  assert.equal(model.qubits.length, 3);

  // Remove the middle op. Wire 1 is interior (wire 2 is still
  // used), so the trailing-wire trim leaves qubits.length at 3.
  removeOperation(model, "1,0");

  assert.equal(model.qubits.length, 3);
  assert.deepEqual(model.qubitUseCounts, [1, 0, 1]);
});

test("removeOperation bulk-trims every trailing unused wire down to the next anchor", () => {
  const model = new CircuitModel(emptyCircuit(2));
  addOperation(model, unitary("H"), "0,0", 0);
  // Drop a far-out op so we have a wide trailing gap when it's removed.
  addOperation(model, unitary("Z"), "1,0", 5);
  assert.equal(model.qubits.length, 6);
  assert.deepEqual(model.qubitUseCounts, [1, 0, 0, 0, 0, 1]);

  // Remove the far op. Wires 1..5 all become zero-use; the trim
  // walks back from the end and stops at wire 0 (still used by H).
  removeOperation(model, "1,0");

  assert.equal(model.qubits.length, 1);
  assert.deepEqual(model.qubitUseCounts, [1]);
});

test("removeOperation trims to a mid-stack anchor introduced by an in-gap add", () => {
  // The composite "add A far out, then B in the middle, then
  // remove A" scenario. Removing A must trim ONLY the wires above
  // B, not back to the original wire-0 anchor.
  const model = new CircuitModel(emptyCircuit(1));
  addOperation(model, unitary("H"), "0,0", 0); // anchor at wire 0
  addOperation(model, unitary("A"), "1,0", 8); // far out → grows to 9
  assert.equal(model.qubits.length, 9);

  // Drop B inside the freshly-grown range (wire 4). Qubit count
  // doesn't change — wire 4 already exists.
  addOperation(model, unitary("B"), "2,0", 4);
  assert.equal(model.qubits.length, 9);
  assert.deepEqual(model.qubitUseCounts, [1, 0, 0, 0, 1, 0, 0, 0, 1]);

  // Remove A. Wires 5..8 become zero-use; trim stops at wire 4 (B).
  removeOperation(model, "1,0");

  assert.equal(model.qubits.length, 5);
  assert.deepEqual(model.qubitUseCounts, [1, 0, 0, 0, 1]);
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

// ---------------------------------------------------------------------------
// addControl
// ---------------------------------------------------------------------------

test("addControl on an existing wire bumps qubitUseCounts without growing qubits", () => {
  const model = new CircuitModel(emptyCircuit(2));
  addOperation(model, unitary("X"), "0,0", 0);
  const op = /** @type {any} */ (model.componentGrid[0].components[0]);
  assert.equal(model.qubits.length, 2);

  // Add a control on wire 1 — already in the qubit list, so no
  // growth; just a use-count bump.
  assert.equal(addControl(model, op, 1), true);
  assert.equal(model.qubits.length, 2);
  assert.deepEqual(model.qubitUseCounts, [1, 1]);
  // The op itself must carry the new control.
  assert.equal(op.controls.length, 1);
  assert.equal(op.controls[0].qubit, 1);
});

test("addControl on a wire several IDs beyond the end bulk-grows qubits", () => {
  const model = new CircuitModel(emptyCircuit(1));
  addOperation(model, unitary("X"), "0,0", 0);
  const op = /** @type {any} */ (model.componentGrid[0].components[0]);
  assert.equal(model.qubits.length, 1);

  // Control on wire 5 — gap of wires 1..4 plus wire 5. Same
  // ensureQubitCount(5) growth as addOperation.
  assert.equal(addControl(model, op, 5), true);

  assert.equal(model.qubits.length, 6);
  assert.deepEqual(model.qubitUseCounts, [1, 0, 0, 0, 0, 1]);
  for (let i = 0; i < model.qubits.length; i++) {
    assert.equal(model.qubits[i].id, i);
  }
  assert.equal(op.controls.length, 1);
  assert.equal(op.controls[0].qubit, 5);
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

// ---------------------------------------------------------------------------
// removeControl
// ---------------------------------------------------------------------------

test("removeControl from an interior wire leaves qubits.length untouched", () => {
  const model = new CircuitModel(emptyCircuit(1));
  addOperation(model, unitary("X"), "0,0", 0);
  const op = /** @type {any} */ (model.componentGrid[0].components[0]);
  // Two controls, one interior (wire 1), one trailing (wire 2).
  addControl(model, op, 1);
  addControl(model, op, 2);
  assert.equal(model.qubits.length, 3);
  assert.deepEqual(model.qubitUseCounts, [1, 1, 1]);

  // Remove the INTERIOR control. The trim is skipped (wire 1 isn't
  // the tail), and wire 2 is still in use by its own control, so
  // qubits.length stays at 3.
  assert.equal(removeControl(model, op, 1), true);
  assert.equal(model.qubits.length, 3);
  assert.deepEqual(model.qubitUseCounts, [1, 0, 1]);
  // Only the wire-2 control survives on the op.
  assert.equal(op.controls.length, 1);
  assert.equal(op.controls[0].qubit, 2);
});

test("removeControl on the trailing wire bulk-trims every trailing unused wire", () => {
  const model = new CircuitModel(emptyCircuit(1));
  addOperation(model, unitary("X"), "0,0", 0);
  const op = /** @type {any} */ (model.componentGrid[0].components[0]);
  // Single far-out control → wires 1..4 are zero-use trailers
  // anchored only by the control on wire 5.
  addControl(model, op, 5);
  assert.equal(model.qubits.length, 6);
  assert.deepEqual(model.qubitUseCounts, [1, 0, 0, 0, 0, 1]);

  // Removing the control on the trailing wire triggers the trim;
  // walks back to wire 0 (still anchored by the target).
  assert.equal(removeControl(model, op, 5), true);

  assert.equal(model.qubits.length, 1);
  assert.deepEqual(model.qubitUseCounts, [1]);
  // The op's controls array must be drained, not just the use count.
  assert.equal(op.controls.length, 0);
});

test("removeControl trims to a mid-stack anchor introduced by an in-gap addControl", () => {
  // Mirror of the addOperation/removeOperation "mid-stack anchor"
  // scenario. Same growth and trim plumbing, just driven through
  // the control path.
  const model = new CircuitModel(emptyCircuit(1));
  addOperation(model, unitary("X"), "0,0", 0);
  const op = /** @type {any} */ (model.componentGrid[0].components[0]);

  addControl(model, op, 8); // grows to 9
  addControl(model, op, 4); // mid-stack anchor; no growth
  assert.equal(model.qubits.length, 9);
  assert.deepEqual(model.qubitUseCounts, [1, 0, 0, 0, 1, 0, 0, 0, 1]);

  // Remove the trailing control. Trim walks back to wire 4.
  assert.equal(removeControl(model, op, 8), true);
  assert.equal(model.qubits.length, 5);
  assert.deepEqual(model.qubitUseCounts, [1, 0, 0, 0, 1]);
  // Only the wire-4 control survives; the wire-8 entry must be gone.
  assert.equal(op.controls.length, 1);
  assert.equal(op.controls[0].qubit, 4);
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

// ---------------------------------------------------------------------------
// addControl / removeControl: classical-ref entries don't shadow quantum controls
// ---------------------------------------------------------------------------
//
// A classically-controlled op carries a classical-ref
// `{qubit: Y, result: N}` in BOTH `.targets` (visual extent claim)
// AND `.controls` (the conditional dependency). The add/remove
// control action layer filters controls to pure-quantum entries
// (`result === undefined`) when checking for existing entries on a
// wire, so:
//   - addControl on wire Y succeeds even when the classical-ref
//     already references that wire
//   - removeControl on wire Y removes only the quantum entry,
//     leaving the classical-ref intact

test("addControl: adding a quantum control on a wire that already has a classical-ref control succeeds", () => {
  // M on wire 0 produces c_0.0. Conditional X on wire 1 reads c_0.0
  // (its `.controls` is `[{qubit:0, result:0}]`). Adding a quantum
  // control on wire 0 should succeed — the existing entry is the
  // classical conditional, not a quantum control.
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
            gate: "X",
            targets: [{ qubit: 1 }],
            controls: [{ qubit: 0, result: 0 }],
            isConditional: true,
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const condX = /** @type {any} */ (model.componentGrid[1].components[0]);

  const ok = addControl(model, condX, 0);

  assert.equal(ok, true, "addControl must succeed on the classical-owner wire");
  // Both entries should now be in .controls: the classical-ref AND
  // the new pure-quantum entry. The order between two same-qubit
  // entries is insertion-stable.
  const qubits = condX.controls.map((/** @type {any} */ c) => c.qubit);
  assert.deepEqual(
    qubits.sort(),
    [0, 0],
    `controls must have two entries on q0; got ${JSON.stringify(condX.controls)}`,
  );
  const hasQuantum = condX.controls.some(
    (/** @type {any} */ c) => c.qubit === 0 && c.result === undefined,
  );
  const hasClassical = condX.controls.some(
    (/** @type {any} */ c) => c.qubit === 0 && c.result === 0,
  );
  assert.ok(hasQuantum, "pure-quantum control on q0 must exist");
  assert.ok(hasClassical, "classical-ref control on q0 must still exist");
});

test("removeControl: removing a quantum control on a wire that also has a classical-ref control leaves the classical ref intact", () => {
  // M on wire 0 produces c_0.0. Conditional X on wire 2 reads c_0.0
  // AND has a quantum control on wire 0. Removing the control on
  // wire 0 must remove ONLY the quantum entry, leaving the
  // classical-ref intact (the gate stays classically-conditional).
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0, numResults: 1 }, { id: 1 }, { id: 2 }],
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
            gate: "X",
            targets: [{ qubit: 2 }],
            controls: [{ qubit: 0 }, { qubit: 0, result: 0 }],
            isConditional: true,
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const condX = /** @type {any} */ (model.componentGrid[1].components[0]);

  const ok = removeControl(model, condX, 0);

  assert.equal(ok, true);
  // Only the classical-ref entry survives.
  assert.equal(condX.controls.length, 1);
  assert.equal(condX.controls[0].qubit, 0);
  assert.equal(condX.controls[0].result, 0);
});

test("removeControl: removing a control on a wire that only has a classical-ref returns false (no-op)", () => {
  // The classical-ref control is the conditional dependency, not a
  // removable quantum control. removeControl must NOT touch it.
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
            gate: "X",
            targets: [{ qubit: 1 }],
            controls: [{ qubit: 0, result: 0 }],
            isConditional: true,
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const condX = /** @type {any} */ (model.componentGrid[1].components[0]);

  const ok = removeControl(model, condX, 0);

  assert.equal(
    ok,
    false,
    "removeControl must refuse to remove a classical-ref",
  );
  assert.equal(condX.controls.length, 1, "classical-ref must still be present");
  assert.equal(condX.controls[0].result, 0);
});
