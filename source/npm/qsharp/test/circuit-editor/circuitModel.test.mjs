// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// CircuitModel tests — exercises the Data layer of the circuit
// editor (`ux/circuit-vis/data/circuitModel.ts`) directly. Pure
// data, no JSDOM. Covers the invariants the model maintains on
// behalf of the Action layer: per-wire use counts, qubit-list
// growth/trim, and the borrow-by-reference contract with the
// underlying `Circuit`.

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import { CircuitModel } from "../../dist/ux/circuit-vis/data/circuitModel.js";

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
 * Build a unitary op targeting `targetQubit`, optionally with
 * controls on `controlQubits`.
 * @param {string} gate
 * @param {number} targetQubit
 * @param {number[]} [controlQubits]
 * @returns {import("../../dist/ux/circuit-vis/index.js").Operation}
 */
function unitary(gate, targetQubit, controlQubits) {
  const op = {
    kind: "unitary",
    gate,
    targets: [{ qubit: targetQubit }],
  };
  if (controlQubits && controlQubits.length > 0) {
    /** @type {any} */ (op).controls = controlQubits.map((qubit) => ({
      qubit,
    }));
  }
  return /** @type {any} */ (op);
}

// ---------------------------------------------------------------------------
// Constructor
// ---------------------------------------------------------------------------

test("constructor on empty circuit produces zero-filled use counts", () => {
  const model = new CircuitModel(emptyCircuit(3));

  assert.equal(model.qubits.length, 3);
  assert.deepEqual(model.qubitUseCounts, [0, 0, 0]);
  assert.deepEqual(model.componentGrid, []);
});

test("constructor seeds qubitUseCounts from existing operations", () => {
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          unitary("X", 0, [1]), // wire 0 (target) + wire 1 (control)
          unitary("H", 2), // wire 2 (target)
        ],
      },
      {
        components: [unitary("Y", 1)], // wire 1 again
      },
    ],
  };

  const model = new CircuitModel(circuit);

  assert.deepEqual(model.qubitUseCounts, [1, 2, 1]);
});

test("constructor borrows componentGrid by reference", () => {
  const circuit = emptyCircuit(2);
  const model = new CircuitModel(circuit);

  // Mutate via the model.
  model.componentGrid.push({ components: [unitary("H", 0)] });

  // Underlying circuit sees the same change.
  assert.equal(circuit.componentGrid.length, 1);
  assert.equal(circuit.componentGrid, model.componentGrid);
});

test("constructor borrows qubits by reference", () => {
  const circuit = emptyCircuit(2);
  const model = new CircuitModel(circuit);

  assert.equal(circuit.qubits, model.qubits);
});

test("constructor with measurement op counts only qubits, not result registers", () => {
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "measurement",
            gate: "Measure",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
    ],
  };

  const model = new CircuitModel(circuit);

  // Wire 0 counted once for the qubit register; the result register
  // (which has `result` defined) is excluded by the bounds check.
  assert.deepEqual(model.qubitUseCounts, [1, 0, 0]);
});

test("constructor silently ignores ops referencing out-of-range wires", () => {
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [unitary("X", 5)], // wire 5 doesn't exist
      },
    ],
  };

  const model = new CircuitModel(circuit);

  // No throw, no growth — out-of-range refs are dropped.
  assert.deepEqual(model.qubitUseCounts, [0, 0]);
  assert.equal(model.qubits.length, 2);
});

// ---------------------------------------------------------------------------
// snapshot
// ---------------------------------------------------------------------------

test("snapshot returns a Circuit aliasing the model's arrays", () => {
  const model = new CircuitModel(emptyCircuit(2));

  const snap = model.snapshot();

  assert.equal(snap.qubits, model.qubits);
  assert.equal(snap.componentGrid, model.componentGrid);
});

// ---------------------------------------------------------------------------
// ensureQubitCount
// ---------------------------------------------------------------------------

test("ensureQubitCount grows qubits and qubitUseCounts to fit a wire index", () => {
  const model = new CircuitModel(emptyCircuit(2));

  model.ensureQubitCount(4);

  assert.equal(model.qubits.length, 5);
  assert.deepEqual(model.qubitUseCounts, [0, 0, 0, 0, 0]);
  // Newly-added qubits get their position as id.
  assert.equal(model.qubits[2].id, 2);
  assert.equal(model.qubits[4].id, 4);
});

test("ensureQubitCount is a no-op when already large enough", () => {
  const model = new CircuitModel(emptyCircuit(3));

  model.ensureQubitCount(1);

  assert.equal(model.qubits.length, 3);
  assert.deepEqual(model.qubitUseCounts, [0, 0, 0]);
});

// ---------------------------------------------------------------------------
// removeTrailingUnusedQubits
// ---------------------------------------------------------------------------

test("removeTrailingUnusedQubits drops only zero-count tail wires", () => {
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [{ components: [unitary("X", 1)] }],
  };
  const model = new CircuitModel(circuit);
  assert.deepEqual(model.qubitUseCounts, [0, 1, 0, 0]);

  model.removeTrailingUnusedQubits();

  // Wires 2 and 3 (trailing zeros) are gone; wires 0 and 1 stay
  // because the trim stops at the first non-zero from the right.
  assert.equal(model.qubits.length, 2);
  assert.deepEqual(model.qubitUseCounts, [0, 1]);
});

test("removeTrailingUnusedQubits is a no-op when all wires are used", () => {
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      { components: [unitary("X", 0)] },
      { components: [unitary("H", 1)] },
    ],
  };
  const model = new CircuitModel(circuit);

  model.removeTrailingUnusedQubits();

  assert.equal(model.qubits.length, 2);
  assert.deepEqual(model.qubitUseCounts, [1, 1]);
});

test("removeTrailingUnusedQubits empties the model when no wires are used", () => {
  const model = new CircuitModel(emptyCircuit(3));

  model.removeTrailingUnusedQubits();

  assert.equal(model.qubits.length, 0);
  assert.deepEqual(model.qubitUseCounts, []);
});

test("removeTrailingUnusedQubits walks nested children, not just qubitUseCounts", () => {
  // The trim must walk the actual op tree (including each group's
  // derived `.targets`), not the incrementally-maintained
  // `qubitUseCounts`. Groups can name wires in their derived
  // `.targets` that the use-count cache no longer reflects;
  // trusting the cache could drop a wire still referenced by a
  // group, leaving the renderer with a stale row index.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Group",
            // Group's own derived targets claim wire 3, even though
            // the only nested child is on wire 0.
            targets: [{ qubit: 0 }, { qubit: 3 }],
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
  const model = new CircuitModel(circuit);
  // The constructor only walks top-level ops, so it counts the
  // Group op's targets [0, 3] → useCounts = [1, 0, 0, 1].
  // Now hand-corrupt qubitUseCounts to model the post-getChildTargets
  // state: imagine a move that rewrote Group.targets and an
  // intervening `_removeOp` zeroed out wire 3's counter even though
  // Group still claims it.
  model.qubitUseCounts = [1, 0, 0, 0];

  model.removeTrailingUnusedQubits();

  // Wire 3 must NOT have been dropped, because Group's `.targets`
  // still names it. (The renderer will read those targets and
  // crash if wire 3 is gone.)
  assert.equal(
    model.qubits.length,
    4,
    "wire 3 must stay alive because Group still references it",
  );
});

test("removeTrailingUnusedQubits is recursive into expanded-group children", () => {
  // Even when the parent's derived `.targets` happens to be in
  // sync, a wire used only deep inside a group's children must
  // still keep the wire alive.
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
                  {
                    kind: "unitary",
                    gate: "Inner",
                    targets: [{ qubit: 0 }],
                    children: [
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
  // Top-level constructor only sees Group's targets [0] → useCounts
  // = [1, 0, 0]. Wire 2 is used only deep inside, so the counter
  // is zero. The grid walk must keep wire 2.
  assert.deepEqual(model.qubitUseCounts, [1, 0, 0]);

  model.removeTrailingUnusedQubits();

  assert.equal(
    model.qubits.length,
    3,
    "wire 2 must stay because a nested child references it",
  );
});

// ---------------------------------------------------------------------------
// increment / decrement
// ---------------------------------------------------------------------------

test("incrementQubitUseCountForOp ignores out-of-range registers", () => {
  const model = new CircuitModel(emptyCircuit(2));

  // Wire 5 doesn't exist — silently skipped.
  model.incrementQubitUseCountForOp(/** @type {any} */ (unitary("X", 5)));

  assert.deepEqual(model.qubitUseCounts, [0, 0]);
});

test("incrementQubitUseCountForOp counts every qubit register", () => {
  const model = new CircuitModel(emptyCircuit(3));

  model.incrementQubitUseCountForOp(
    /** @type {any} */ (unitary("X", 0, [1, 2])),
  );

  assert.deepEqual(model.qubitUseCounts, [1, 1, 1]);
});

test("decrementQubitUseCountForOp mirrors increment", () => {
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [{ components: [unitary("X", 0, [1, 2])] }],
  };
  const model = new CircuitModel(circuit);
  assert.deepEqual(model.qubitUseCounts, [1, 1, 1]);

  model.decrementQubitUseCountForOp(
    /** @type {any} */ (model.componentGrid[0].components[0]),
  );

  assert.deepEqual(model.qubitUseCounts, [0, 0, 0]);
});
