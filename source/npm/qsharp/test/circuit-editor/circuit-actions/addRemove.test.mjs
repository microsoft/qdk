// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Add/remove mutator tests on flat (non-grouped) shapes against `CircuitModel`. Group recursion and
// group-internal span widening live in `groupAddRemove.test.mjs`.

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
import {
  at,
  circuit,
  expectGrid,
  expectOp,
  gate,
  group,
  meas,
  qubits,
} from "../_helpers.mjs";

/** Fresh empty circuit literal with `n` qubits and no operations. */
const emptyCircuit = (/** @type {number} */ n) => circuit(n, []);

/** Single-target unitary template on wire 0 (what `addOperation` copies). */
const unitary = (/** @type {string} */ g) => gate(g, 0);

// ---------------------------------------------------------------------------
// addOperation
// ---------------------------------------------------------------------------

test("addOperation appends to the target column and bumps qubitUseCounts", () => {
  const model = new CircuitModel(emptyCircuit(2));

  const added = addOperation(model, unitary("H"), "0,0", 0);

  assert.ok(added, "addOperation should return the new operation");
  expectGrid(model, [["H"]]);
  // Returned op is the inserted reference (deep-copied from template).
  assert.equal(added, at(model, "0,0"));
  assert.deepEqual(model.qubitUseCounts, [1, 0]);
});

test("addOperation on an existing wire bumps qubitUseCounts without growing qubits", () => {
  const model = new CircuitModel(emptyCircuit(2));
  addOperation(model, unitary("H"), "0,0", 0);
  assert.equal(model.qubits.length, 2);
  assert.deepEqual(model.qubitUseCounts, [1, 0]);

  // Second op on the SAME wire (column 1 to avoid same-column overlap).
  addOperation(model, unitary("X"), "1,0", 0);
  assert.equal(model.qubits.length, 2);
  assert.deepEqual(model.qubitUseCounts, [2, 0]);
});

test("addOperation on a wire several IDs beyond the end bulk-grows qubits", () => {
  const model = new CircuitModel(emptyCircuit(1));
  assert.equal(model.qubits.length, 1);

  // Drop on wire 5 — ensureQubitCount(5) adds wires 1..5 in one shot.
  addOperation(model, unitary("H"), "0,0", 5);

  assert.equal(model.qubits.length, 6);
  assert.deepEqual(model.qubitUseCounts, [0, 0, 0, 0, 0, 1]);
  for (let i = 0; i < model.qubits.length; i++) {
    assert.equal(model.qubits[i].id, i);
  }
});

test("addOperation with insertNewColumn=true creates a fresh column", () => {
  const model = new CircuitModel(emptyCircuit(2));
  addOperation(model, unitary("H"), "0,0", 0);

  // insertNewColumn pushes X into a fresh column 0, shifting H right.
  addOperation(model, gate("X", 1), "0,0", 1, /* insertNewColumn */ true);

  expectGrid(model, [["X"], ["H"]]);
  assert.deepEqual(model.qubitUseCounts, [1, 1]);
});

test("addOperation with insertNewColumn=true moves other operations to the right", () => {
  const model = new CircuitModel(emptyCircuit(2));
  addOperation(model, unitary("H"), "0,0", 0);
  addOperation(model, unitary("S"), "1,0", 0);

  addOperation(model, gate("X", 1), "1,0", 1, /* insertNewColumn */ true);

  expectGrid(model, [["H"], ["X"], ["S"]]);
  assert.deepEqual(model.qubitUseCounts, [2, 1]);
});

test("addOperation deep-copies its source operation template", () => {
  const model = new CircuitModel(emptyCircuit(2));
  const template = unitary("H");

  const added = addOperation(model, template, "0,0", 0);

  // Mutating the template after add must not affect the model.
  template.gate = "MUTATED";
  assert.equal(/** @type {any} */ (added).gate, "H");
  expectOp(at(model, "0,0"), "H");
});

test("addOperation with a missing target location returns null", () => {
  const model = new CircuitModel(emptyCircuit(2));

  // Empty location parses to root; last() is null → failure, no change.
  const result = addOperation(model, unitary("H"), "", 0);

  assert.equal(result, null);
  expectGrid(model, []);
});

// ---------------------------------------------------------------------------
// removeOperation
// ---------------------------------------------------------------------------

test("removeOperation drops the op and decrements qubitUseCounts", () => {
  const model = new CircuitModel(emptyCircuit(2));
  addOperation(model, unitary("H"), "0,0", 0);
  addOperation(model, unitary("X"), "1,0", 1);
  assert.equal(model.componentGrid.length, 2);
  assert.deepEqual(model.qubitUseCounts, [1, 1]);

  // Remove the X (column 1).
  removeOperation(model, "1,0");

  expectGrid(model, [["H"]]);
  // Wire 1 dropped to 0 uses → trailing-wire trim removes it.
  assert.deepEqual(model.qubitUseCounts, [1]);
  assert.equal(model.qubits.length, 1);
});

test("removeOperation from an interior wire leaves qubits.length untouched", () => {
  const model = new CircuitModel(emptyCircuit(3));
  addOperation(model, unitary("H"), "0,0", 0);
  addOperation(model, unitary("X"), "1,0", 1);
  addOperation(model, unitary("Z"), "2,0", 2);
  assert.equal(model.qubits.length, 3);

  // Remove the middle op. Wire 1 is interior (wire 2 still used), so the trim leaves qubits.length
  // at 3.
  removeOperation(model, "1,0");

  assert.equal(model.qubits.length, 3);
  assert.deepEqual(model.qubitUseCounts, [1, 0, 1]);
});

test("removeOperation bulk-trims every trailing unused wire down to the next anchor", () => {
  const model = new CircuitModel(emptyCircuit(2));
  addOperation(model, unitary("H"), "0,0", 0);
  // Far-out op gives a wide trailing gap when removed.
  addOperation(model, unitary("Z"), "1,0", 5);
  assert.equal(model.qubits.length, 6);
  assert.deepEqual(model.qubitUseCounts, [1, 0, 0, 0, 0, 1]);

  // Remove the far op. Trim walks back from the end, stops at wire 0 (H).
  removeOperation(model, "1,0");

  assert.equal(model.qubits.length, 1);
  assert.deepEqual(model.qubitUseCounts, [1]);
});

test("removeOperation trims to a mid-stack anchor introduced by an in-gap add", () => {
  const model = new CircuitModel(emptyCircuit(1));
  addOperation(model, unitary("H"), "0,0", 0); // anchor at wire 0
  addOperation(model, unitary("A"), "1,0", 8); // far out → grows to 9
  assert.equal(model.qubits.length, 9);

  // B inside the grown range (wire 4); no growth.
  addOperation(model, unitary("B"), "2,0", 4);
  assert.equal(model.qubits.length, 9);
  assert.deepEqual(model.qubitUseCounts, [1, 0, 0, 0, 1, 0, 0, 0, 1]);

  // Remove A. Trim stops at wire 4 (B), not the wire-0 anchor.
  removeOperation(model, "1,0");

  assert.equal(model.qubits.length, 5);
  assert.deepEqual(model.qubitUseCounts, [1, 0, 0, 0, 1]);
});

test("removeOperation on a root location is a safe no-op", () => {
  const model = new CircuitModel(emptyCircuit(2));
  addOperation(model, unitary("H"), "0,0", 0);

  // Root location "" → last == null, safe no-op.
  const result = removeOperation(model, "");

  assert.equal(result, null);
  expectGrid(model, [["H"]]);
  assert.deepEqual(model.qubitUseCounts, [1, 0]);
});

// ---------------------------------------------------------------------------
// addControl
// ---------------------------------------------------------------------------

test("addControl on an existing wire bumps qubitUseCounts without growing qubits", () => {
  const model = new CircuitModel(emptyCircuit(2));
  addOperation(model, unitary("X"), "0,0", 0);
  const op = at(model, "0,0");
  assert.equal(model.qubits.length, 2);

  // Control on wire 1 — already in the qubit list, so no growth.
  assert.equal(addControl(model, op, 1), true);
  assert.equal(model.qubits.length, 2);
  assert.deepEqual(model.qubitUseCounts, [1, 1]);
  expectOp(op, { X: { ctrls: [1] } });
});

test("addControl on a wire several IDs beyond the end bulk-grows qubits", () => {
  const model = new CircuitModel(emptyCircuit(1));
  addOperation(model, unitary("X"), "0,0", 0);
  const op = at(model, "0,0");
  assert.equal(model.qubits.length, 1);

  // Control on wire 5 — ensureQubitCount(5) growth, same as addOperation.
  assert.equal(addControl(model, op, 5), true);

  assert.equal(model.qubits.length, 6);
  assert.deepEqual(model.qubitUseCounts, [1, 0, 0, 0, 0, 1]);
  for (let i = 0; i < model.qubits.length; i++) {
    assert.equal(model.qubits[i].id, i);
  }
  expectOp(op, { X: { ctrls: [5] } });
});

test("addControl is a no-op when the wire is already a control", () => {
  const model = new CircuitModel(emptyCircuit(2));
  addOperation(model, unitary("X"), "0,0", 0);
  const op = at(model, "0,0");

  assert.equal(addControl(model, op, 1), true);
  assert.equal(model.qubitUseCounts[1], 1);

  // Second call on the same wire — already a control, no re-bump.
  assert.equal(addControl(model, op, 1), false);
  assert.equal(model.qubitUseCounts[1], 1);
  expectOp(op, { X: { ctrls: [1] } });
});

// ---------------------------------------------------------------------------
// removeControl
// ---------------------------------------------------------------------------

test("removeControl from an interior wire leaves qubits.length untouched", () => {
  const model = new CircuitModel(emptyCircuit(1));
  addOperation(model, unitary("X"), "0,0", 0);
  const op = at(model, "0,0");
  // Controls on wires 1 (interior) and 2 (trailing).
  addControl(model, op, 1);
  addControl(model, op, 2);
  assert.equal(model.qubits.length, 3);
  assert.deepEqual(model.qubitUseCounts, [1, 1, 1]);

  // Remove the interior control (wire 1); wire 2 still anchors length.
  assert.equal(removeControl(model, op, 1), true);
  assert.equal(model.qubits.length, 3);
  assert.deepEqual(model.qubitUseCounts, [1, 0, 1]);
  expectOp(op, { X: { ctrls: [2] } });
});

test("removeControl on the trailing wire bulk-trims every trailing unused wire", () => {
  const model = new CircuitModel(emptyCircuit(1));
  addOperation(model, unitary("X"), "0,0", 0);
  const op = at(model, "0,0");
  // Single far-out control on wire 5; wires 1..4 are zero-use trailers.
  addControl(model, op, 5);
  assert.equal(model.qubits.length, 6);
  assert.deepEqual(model.qubitUseCounts, [1, 0, 0, 0, 0, 1]);

  // Remove the trailing control; trim walks back to wire 0 (target).
  assert.equal(removeControl(model, op, 5), true);

  assert.equal(model.qubits.length, 1);
  assert.deepEqual(model.qubitUseCounts, [1]);
  expectOp(op, { X: { ctrls: [] } });
});

test("removeControl trims to a mid-stack anchor introduced by an in-gap addControl", () => {
  const model = new CircuitModel(emptyCircuit(1));
  addOperation(model, unitary("X"), "0,0", 0);
  const op = at(model, "0,0");

  addControl(model, op, 8); // grows to 9
  addControl(model, op, 4); // mid-stack anchor; no growth
  assert.equal(model.qubits.length, 9);
  assert.deepEqual(model.qubitUseCounts, [1, 0, 0, 0, 1, 0, 0, 0, 1]);

  // Remove the trailing control; trim walks back to wire 4.
  assert.equal(removeControl(model, op, 8), true);
  assert.equal(model.qubits.length, 5);
  assert.deepEqual(model.qubitUseCounts, [1, 0, 0, 0, 1]);
  expectOp(op, { X: { ctrls: [4] } });
});

test("removeControl on a wire with no control returns false", () => {
  const model = new CircuitModel(emptyCircuit(2));
  addOperation(model, unitary("X"), "0,0", 0);
  const op = at(model, "0,0");

  // No controls at all.
  assert.equal(removeControl(model, op, 1), false);

  // Add one, then try to remove a different wire.
  addControl(model, op, 1);
  assert.equal(removeControl(model, op, 0), false);
  expectOp(op, { X: { ctrls: [1] } });
});

// ---------------------------------------------------------------------------
// addControl / removeControl: classical-ref entries don't shadow quantum controls
// ---------------------------------------------------------------------------
//
// A classically-controlled op carries a classical-ref `{qubit, result}` in both `.targets` and
// `.controls`. The control actions filter to pure-quantum entries (`result === undefined`), so
// add/remove on the classical-owner wire touches only the quantum entry.

test("addControl: adding a quantum control on a wire that already has a classical-ref control succeeds", () => {
  // M on q0 produces c_0.0; conditional X on q1 reads it. Adding a quantum control on q0 must
  // succeed (the existing q0 entry is classical).
  const model = new CircuitModel(
    circuit(qubits(2, { 0: 1 }), [
      [meas(0)],
      [gate("X", 1, { ctrls: [{ q: 0, r: 0 }], conditional: true })],
    ]),
  );
  const condX = at(model, "1,0");

  const ok = addControl(model, condX, 0);

  assert.equal(ok, true, "addControl must succeed on the classical-owner wire");
  // Both the classical-ref and the new quantum entry are present.
  expectOp(condX, { X: { ctrls: [0, { q: 0, r: 0 }] } });
});

test("removeControl: removing a quantum control on a wire that also has a classical-ref control leaves the classical ref intact", () => {
  // Conditional X on q2 has a quantum control on q0 AND reads c_0.0. Removing the q0 control drops
  // only the quantum entry.
  const model = new CircuitModel(
    circuit(qubits(3, { 0: 1 }), [
      [meas(0)],
      [gate("X", 2, { ctrls: [0, { q: 0, r: 0 }], conditional: true })],
    ]),
  );
  const condX = at(model, "1,0");

  const ok = removeControl(model, condX, 0);

  assert.equal(ok, true);
  expectOp(condX, { X: { ctrls: [{ q: 0, r: 0 }] } });
});

test("removeControl: removing a control on a wire that only has a classical-ref returns false (no-op)", () => {
  // The classical-ref is the conditional dependency, not a removable control.
  const model = new CircuitModel(
    circuit(qubits(2, { 0: 1 }), [
      [meas(0)],
      [gate("X", 1, { ctrls: [{ q: 0, r: 0 }], conditional: true })],
    ]),
  );
  const condX = at(model, "1,0");

  const ok = removeControl(model, condX, 0);

  assert.equal(
    ok,
    false,
    "removeControl must refuse to remove a classical-ref",
  );
  expectOp(condX, { X: { ctrls: [{ q: 0, r: 0 }] } });
});

// ---------------------------------------------------------------------------
// addControl: collision-split with a same-column sibling (flat grid)
// ---------------------------------------------------------------------------

test("addControl: top-level widening into a same-column sibling splits the column", () => {
  // CNOT(target q0, control q1) shares col 0 with H@q2. Adding a control on q3 widens the CNOT to
  // span q0..q3, overlapping H.
  const model = new CircuitModel(
    circuit(4, [[gate("X", 0, { ctrls: [1] }), gate("H", 2)]]),
  );
  const cnotOp = at(model, "0,0");
  const ok = addControl(model, cnotOp, 3);
  assert.ok(ok, "addControl should succeed on a fresh wire");

  // Column splits into [CNOT] then [H]; CNOT carries both controls.
  expectGrid(model, [[{ X: { ctrls: [1, 3] } }], ["H"]]);
});

test("addControl: no overlap means no split", () => {
  // CNOT(target q0, control q1) shares col 0 with H@q3. Adding a control on q2 keeps the CNOT span
  // clear of H — no split.
  const model = new CircuitModel(
    circuit(4, [[gate("X", 0, { ctrls: [1] }), gate("H", 3)]]),
  );
  const cnotOp = at(model, "0,0");
  const ok = addControl(model, cnotOp, 2);
  assert.ok(ok);

  expectGrid(model, [["X", "H"]]);
});

test("addControl: widening past MULTIPLE same-column siblings shifts every sibling right", () => {
  // col 0 = [CNOT(target q0, control q1), Y@q2, Z@q3]. A control on the clear wire q4 widens the
  // CNOT over both Y and Z. The split inserts ONE fresh column for the CNOT; siblings stay paired.
  const model = new CircuitModel(
    circuit(5, [[gate("X", 0, { ctrls: [1] }), gate("Y", 2), gate("Z", 3)]]),
  );
  const cnotOp = at(model, "0,0");
  const ok = addControl(model, cnotOp, 4);
  assert.ok(ok);

  expectGrid(model, [["X"], ["Y", "Z"]]);
});

// ---------------------------------------------------------------------------
// addControl / removeControl: shape refusals (multi-target ops & groups)
// ---------------------------------------------------------------------------

test("addControl: refuses on a classically-controlled GROUP (groups never carry quantum controls by design)", () => {
  // Groups (any op with children) may carry classical controls only — the editor refuses to author
  // quantum controls on them.
  const model = new CircuitModel(
    circuit(qubits(4, { 0: 1 }), [
      [meas(0)],
      [
        group("CondGroup", [[gate("H", 1), gate("X", 2)]], {
          ctrls: [{ q: 0, r: 0 }],
          conditional: true,
        }),
      ],
    ]),
  );
  const groupOp = at(model, "1,0");

  const ok = addControl(model, groupOp, 3);

  assert.equal(ok, false, "addControl must refuse on a group");
  // Only the original classical-ref control survives, untouched.
  expectOp(groupOp, { CondGroup: { ctrls: [{ q: 0, r: 0 }] } });
});

test("addControl: still succeeds on a classically-controlled single-target UNITARY (no children)", () => {
  // A single-target classically-controlled unitary isn't multi-target, so a quantum control on a
  // fresh wire is allowed.
  const model = new CircuitModel(
    circuit(qubits(3, { 0: 1 }), [
      [meas(0)],
      [gate("X", 1, { ctrls: [{ q: 0, r: 0 }], conditional: true })],
    ]),
  );
  const op = at(model, "1,0");

  const ok = addControl(model, op, 2);

  assert.equal(ok, true);
  // New quantum control plus the original classical-ref.
  expectOp(op, { X: { ctrls: [2, { q: 0, r: 0 }] } });
});

test("addControl: refuses on a multi-target unitary even without children", () => {
  // SWAP: targets.length === 2, no children — multi-leg, so refused.
  const model = new CircuitModel(circuit(3, [[gate("SWAP", [0, 1])]]));
  const swap = at(model, "0,0");

  const ok = addControl(model, swap, 2);

  assert.equal(ok, false);
  expectOp(swap, { SWAP: { targets: [0, 1], ctrls: [] } });
});

test("addControl: refuses on a plain group (no classical conditions)", () => {
  // Pure organizational group — children, no controls. Multi-leg, refused.
  const model = new CircuitModel(
    circuit(3, [[group("Foo", [[gate("H", 0), gate("X", 1)]])]]),
  );
  const groupOp = at(model, "0,0");

  const ok = addControl(model, groupOp, 2);

  assert.equal(ok, false);
  expectOp(groupOp, { Foo: { ctrls: [] } });
});

test("removeControl: refuses on a multi-target / group op, leaving existing controls in place", () => {
  // A group loaded with a pre-existing quantum control (e.g. from external data): the editor
  // refuses to remove it.
  const model = new CircuitModel(
    circuit(4, [
      [group("Foo", [[gate("H", 1), gate("X", 2)]], { ctrls: [0] })],
    ]),
  );
  const groupOp = at(model, "0,0");

  const ok = removeControl(model, groupOp, 0);

  assert.equal(ok, false);
  // The pre-existing control survives a refused removeControl.
  expectOp(groupOp, { Foo: { ctrls: [0] } });
});
