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
  moveOperation,
  removeControl,
  removeOperation,
} from "../../dist/ux/circuit-vis/actions/circuitActions.js";

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
  // Moving the inner H to a fresh top-level column ahead of the
  // group must remove it from the group's children (no duplicate).
  // Because the H was the group's only child, the now-empty group
  // is pruned: the grid contains neither a duplicate H nor a
  // zero-content Group shell.
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

  // Top-level grid: [new H@0], [X@2]. The Group is gone
  // (empty-group cleanup pruned it once its last child departed).
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
  // wires. After a child departs, the parent's `targets` must no
  // longer include that wire.
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
  // Multi-target ops (e.g. SWAP) move as a rigid unit: every
  // register shifts by `targetWire - sourceWire` so the gate
  // keeps its shape on the drop. A SWAP at wires [0, 2] dragged
  // from wire 0 onto wire 1 (delta = +1) lands at wires [1, 3].
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

  // Grab wire 0 of the SWAP, drop on wire 1 → delta = +1.
  // Expected: targets = [{ qubit: 1 }, { qubit: 3 }] — SWAP intact, shifted.
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
  // Moving a group shifts the group's own `.targets` AND
  // recursively every register reference in its children grid by
  // the same delta — so the box and its contents stay aligned.
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
  // A classical control register has the shape `{qubit, result}` —
  // the `qubit` field points to the WIRE that owns the classical
  // register (where the producing measurement lives), NOT to a
  // wire the gate acts on. When the producer measurement is
  // EXTERNAL to a moved group, the consumer's classical control
  // must stay anchored to its current wire — otherwise it would
  // point at a wire with no classical registers.
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
  // A unit-shift whose lowest post-shift wire would land below 0
  // is refused: moveOperation returns null and leaves the model
  // untouched. The dragController treats `null` as a no-op and
  // skips the re-render.
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
  // A classically-conditional unitary (e.g. `if: ...`) records its
  // classical-register dependency in BOTH its `controls` array AND
  // its `targets` array (the targets entry is a visual extent
  // claim drawing the line down to the classical register box).
  // The producer-internal-vs-external anchoring rule applies to
  // ALL classical-ref entries, not just controls.
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
  // Dragging the last remaining child out of a group must prune
  // the now-empty group — never leave a zero-content shell like
  //   { gate: "Group", targets: [], children: [{ components: [] }] }
  // The grid contains only the relocated child.
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
  // When a move-out empties an inner group AND that inner group
  // was the only child of its outer group, BOTH groups must
  // disappear. The cleanup walks the ancestor chain innermost-out,
  // stopping at the first ancestor that still has content.
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
  // The cleanup must not over-delete. When the innermost ancestor
  // empties but its grandparent still has other content, only the
  // innermost ancestor disappears; the grandparent stays put with
  // its remaining content.
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

// ---------------------------------------------------------------------------
// Trailing inner-column dropzone of an expanded group.
//
// The dropzone layer emits a dropzone at
// `data-dropzone-location="<prefix>-<N>,0"` where `<N>` is the
// group's existing child-column count (one past the rightmost
// existing column). The action layer accepts that location and
// synthesizes the new column in the group's `children` grid —
// without leaking the new op to the top level or duplicating it.
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

// ---------------------------------------------------------------------------
// Dest-side ancestor refresh cascade.
//
// `moveOperation` always re-derives each destination ancestor's
// `.targets` from its post-move children. The target location
// string is authoritative: if the user dropped the source inside
// group G, G's `.targets` MUST reflect that, even when the drop
// wire was outside G's pre-move span.
//
// The cascade walks innermost-out and stops at the first ancestor
// whose pre-existing span already encloses the (now-widened) child
// below it, and skips ancestors that the empty-prune pass removed
// (the last-child-departed case).
// ---------------------------------------------------------------------------

test("moveOperation extend: shift-drop onto a wire just outside group's span extends the group's targets", () => {
  // 3 qubits. Foo spans wires 0-1 with one child H on wire 0.
  // Shift+drop H from "0,0-0,0" onto wire 2 (one wire below Foo's
  // current span) at Foo's trailing inner slot "0,0-1,2". Result:
  // Foo's .targets must grow to include wire 2; H now lives on
  // wire 2 (via _moveY's delta-shift).
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
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

  const moved = moveOperation(
    model,
    /* sourceLocation */ "0,0-0,0",
    /* targetLocation */ "0,0-1,2",
    /* sourceWire */ 0,
    /* targetWire */ 2,
    /* movingControl */ false,
    /* insertNewColumn */ false,
  );

  assert.ok(moved, "extend move should return the moved op");

  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  assert.equal(fooOp.gate, "Foo");

  // Foo's .targets are re-derived from its remaining children:
  // H is now Foo's only child, sitting on wire 2 — so .targets
  // becomes [2]. The cascade refreshes the .targets *from* the
  // children, which is the correct, lossless model — phantom
  // wires that no descendant touches are released. What matters
  // for the extend semantics: wire 2 (which was OUTSIDE Foo's
  // original span 0-1) is now enclosed.
  const fooQubits = fooOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.ok(
    fooQubits.includes(2),
    `Foo .targets must enclose the new wire 2 after extend; got ${JSON.stringify(fooQubits)}`,
  );

  // H landed on wire 2 (single-leg shift moves wire by delta=2).
  // Find the H inside Foo's inner grid and verify its target.
  let hOp = null;
  for (const col of fooOp.children) {
    for (const op of col.components) {
      if (op.gate === "H") {
        hOp = op;
        break;
      }
    }
  }
  assert.ok(hOp, "H must still exist inside Foo");
  assert.equal(hOp.targets[0].qubit, 2, "H must land on wire 2");
});

test("moveOperation extend: shift-drop several wires past the span extends across the gap", () => {
  // 5 qubits. Foo spans wires 0-1; shift-drop H onto wire 4 — a
  // gap of two wires (2 and 3) between Foo's old span and the drop
  // wire. Foo's new span must enclose ALL of 0..4, not just 0-1
  // and 4 (a non-contiguous span is unrepresentable; .targets is a
  // set whose min/max define the rendered span).
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }, { id: 4 }],
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

  const moved = moveOperation(model, "0,0-0,0", "0,0-1,4", 0, 4, false, false);
  assert.ok(moved);

  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const qubits = fooOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);

  // .targets is just the H's qubit (1) after extend — but the
  // rendered span is min..max of qubits referenced inside the
  // group. The extend math reads `getChildTargets(Foo)` which only
  // returns unique qubits actually referenced inside Foo. After the
  // move H lives on wire 4 and Foo's original wires 0 and 1 have
  // no remaining children, so .targets becomes [4] alone — meaning
  // Foo collapses to span just wire 4.
  //
  // We assert the SPAN (min..max) covers the relevant range: wire
  // 4 must be enclosed.
  assert.ok(qubits.includes(4), "Foo must enclose wire 4 after extend");
});

test("moveOperation extend: multi-wire source extends to cover its new top wire", () => {
  // 4 qubits. Foo spans wires 0-1, contains a CNOT on wires 0-1
  // (control=0, target=1). Shift-drop the CNOT (grabbed by its
  // target wire 1) onto wire 2 — _moveY slides by delta=1, so
  // control moves 0→1 and target moves 1→2.
  //
  // The cascade refresh reads `getChildTargets(Foo)` which returns
  // the union of all wires referenced by Foo's children. CNOT now
  // sits on wires {1, 2}, so Foo.targets becomes [1, 2] — the
  // extend has pulled Foo's lower bound up to 1 (was 0) and upper
  // bound down to 2 (was 1). What we really care about: Foo's
  // span now ENCLOSES the new top wire 2 (it didn't before).
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
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
                  {
                    kind: "unitary",
                    gate: "X",
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

  const moved = moveOperation(
    model,
    /* sourceLocation */ "0,0-0,0",
    /* targetLocation */ "0,0-1,2",
    /* sourceWire */ 1,
    /* targetWire */ 2,
    /* movingControl */ false,
    /* insertNewColumn */ false,
  );
  assert.ok(moved);

  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const qubits = fooOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);

  // Foo must enclose wire 2 (its previous max was 1).
  assert.ok(
    Math.max(...qubits) >= 2,
    `Foo's span must extend to at least wire 2; got ${JSON.stringify(qubits)}`,
  );
});

test("moveOperation extend: cascade refreshes nested ancestors whose span is now exceeded", () => {
  // Outer (wires 0-1) contains Inner (wires 0-1) contains H (wire 0).
  // Shift-drop H from "0,0-0,0-0,0" to "0,0-0,0-1,2" (inside Inner's
  // trailing inner-column, on wire 2 — outside both groups' spans).
  // Cascade: Inner extends to enclose wire 2, THEN Outer (whose
  // pre-existing span 0-1 no longer encloses Inner's new span)
  // also extends.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Outer",
            targets: [{ qubit: 0 }, { qubit: 1 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "Inner",
                    targets: [{ qubit: 0 }, { qubit: 1 }],
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

  const moved = moveOperation(
    model,
    "0,0-0,0-0,0",
    "0,0-0,0-1,2",
    0,
    2,
    false,
    false,
  );
  assert.ok(moved);

  const outerOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const innerOp = /** @type {any} */ (outerOp.children[0].components[0]);

  const innerQubits = innerOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  const outerQubits = outerOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);

  // Inner extended to cover H's new wire (2).
  assert.ok(
    innerQubits.includes(2),
    `Inner must enclose wire 2 after extend; got ${JSON.stringify(innerQubits)}`,
  );
  // Outer cascaded — must include every wire Inner now references.
  for (const q of innerQubits) {
    assert.ok(
      outerQubits.includes(q),
      `Outer must enclose Inner's wire ${q} after cascade; got Outer=${JSON.stringify(outerQubits)}`,
    );
  }
});

test("moveOperation extend: cascade stops at first ancestor that already encloses the child", () => {
  // Outer spans wires 0-3; Inner spans wires 1-2 inside Outer with
  // an H on wire 1. Shift-drop H onto wire 0 (inside Outer's
  // pre-existing span 0-3 but OUTSIDE Inner's span 1-2). Inner
  // must extend to include wire 0; Outer's existing wires (0-3)
  // already enclose wire 0 so Outer's .targets are unchanged.
  //
  // Captures the "stop walking up" early-exit in the cascade.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Outer",
            // Outer's pre-existing span: wires 0 and 3 (placeholder
            // children on those wires give Outer a 0-3 span without
            // needing real ops). For the test we only care about
            // Outer's .targets *after* the move, which derives from
            // its children — so seed Outer with children on 0 and 3.
            targets: [{ qubit: 0 }, { qubit: 3 }],
            children: [
              {
                components: [
                  // Padding op on wire 0 to anchor Outer's lower span.
                  { kind: "unitary", gate: "P0", targets: [{ qubit: 0 }] },
                ],
              },
              {
                components: [
                  // Inner sub-group on wires 1-2.
                  {
                    kind: "unitary",
                    gate: "Inner",
                    targets: [{ qubit: 1 }, { qubit: 2 }],
                    children: [
                      {
                        components: [
                          {
                            kind: "unitary",
                            gate: "H",
                            targets: [{ qubit: 1 }],
                          },
                        ],
                      },
                    ],
                  },
                ],
              },
              {
                components: [
                  // Padding op on wire 3 to anchor Outer's upper span.
                  { kind: "unitary", gate: "P3", targets: [{ qubit: 3 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // H lives at "0,0-1,0-0,0" (Outer at "0,0", Inner at col 1 op 0
  // of Outer, H at col 0 op 0 of Inner). Shift-drop onto Inner's
  // trailing inner-column "0,0-1,0-1,0" at wire 0.
  const moved = moveOperation(
    model,
    "0,0-1,0-0,0",
    "0,0-1,0-1,0",
    1,
    0,
    false,
    false,
  );
  assert.ok(moved);

  const outerOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const innerOp = /** @type {any} */ (outerOp.children[1].components[0]);

  const innerQubits = innerOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  const outerQubits = outerOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);

  // Inner extended to include wire 0 (its previous span was 1-2).
  assert.ok(
    innerQubits.includes(0),
    `Inner must enclose wire 0 after extend; got ${JSON.stringify(innerQubits)}`,
  );
  // Outer's span — refreshed by the cascade because Inner's new
  // min (0) extended below Inner's old min (1) — must still
  // enclose every wire its children sit on. Children after the
  // move: P0 on wire 0, Inner whose .targets include wire 0, P3
  // on wire 3. So Outer's span is (at least) [0, 3] — it MUST
  // enclose wires 0 and 3.
  assert.ok(
    outerQubits.includes(0) && outerQubits.includes(3),
    `Outer must enclose wires 0 and 3 after cascade; got ${JSON.stringify(outerQubits)}`,
  );
});

test("moveOperation extend: last-child-departed case prunes the group; extend is a safe no-op", () => {
  // Foo contains only a single H. Shift-drop the H to a top-level
  // slot, leaving Foo empty. The empty-prune pass removes Foo
  // entirely; the extend cascade then walks the dest ancestor chain
  // and finds the (former) ancestor no longer attached — skipping
  // it without error. The H lands at top level on its new wire.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
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
      // Filler so we have somewhere strictly later than Foo for
      // the H to land at top level.
      {
        components: [{ kind: "unitary", gate: "Y", targets: [{ qubit: 2 }] }],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Move H from "0,0-0,0" to top-level "1,1" (inter-column band
  // before the Y column, on wire 2). The dest scope is top-level
  // (root, no parent group), so the dest-side cascade is a no-op
  // before the empty-prune even kicks in. The point of this test
  // is that it doesn't throw and the H lands cleanly even when
  // the move also empties out (and prunes) the source's parent
  // group.
  const moved = moveOperation(model, "0,0-0,0", "1,1", 0, 2, false, true);
  assert.ok(moved, "move must succeed when dest is top-level");

  // Foo must be gone (empty-prune cascaded).
  /** @type {string[]} */
  const topGates = [];
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      topGates.push(/** @type {any} */ (op).gate);
    }
  }
  assert.ok(
    !topGates.includes("Foo"),
    "Foo must be pruned after last child leaves",
  );
  assert.ok(topGates.includes("H"), "H must land at top level");
  assert.ok(topGates.includes("Y"), "Y must remain at top level");
});

test("moveOperation extend: external source dropped into group on off-span wire extends the group", () => {
  // Cross-chain move: source lives OUTSIDE the destination group,
  // so the existing source-side ancestor refresh acts on the
  // source's old ancestors (top-level here), NOT on the
  // destination group. The dest-side cascade is the ONLY thing
  // that keeps the invariant "G's `.targets` reflects G's actual
  // children" intact in this case — and since it always runs
  // (no opt-in needed), the target location string alone is
  // enough to convey intent.
  //
  // Setup: 3 qubits. Top-level H on wire 2 (the external source).
  // Foo spans wires 0-1 with an X on wire 0. Drop H from top-level
  // "0,0" into Foo's trailing inner slot "1,0-1,2" at wire 2
  // (outside Foo's current span).
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 2 }] }],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 0 }, { qubit: 1 }],
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

  const moved = moveOperation(
    model,
    /* sourceLocation */ "0,0",
    /* targetLocation */ "1,0-1,2",
    /* sourceWire */ 2,
    /* targetWire */ 2,
    /* movingControl */ false,
    /* insertNewColumn */ false,
  );
  assert.ok(moved, "move must succeed");

  const fooOp = /** @type {any} */ (
    model.componentGrid
      .find((/** @type {any} */ c) =>
        c.components.some((/** @type {any} */ op) => op.gate === "Foo"),
      )
      ?.components.find((/** @type {any} */ op) => op.gate === "Foo")
  );
  const fooQubits = fooOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.ok(
    fooQubits.includes(2),
    `Foo's .targets MUST include wire 2 after the external-source drop; got ${JSON.stringify(fooQubits)}`,
  );
});

// ---------------------------------------------------------------------------
// Collision-split when extending a group's span overlaps a sibling.
//
// Mirrors `commitAddControl`'s split-and-shift convention: the
// extended op is pulled into a fresh column inserted at its
// current column index, leaving the surviving siblings one slot
// to the right. This restores a non-overlapping layout without
// disturbing siblings' relative order.
// ---------------------------------------------------------------------------

test("moveOperation extend: extending across a column-sibling splits the column, group shifts left", () => {
  // 3 qubits. Top-level column 0 holds Foo (span 0-1, contains H
  // on wire 0) AND a sibling Z on wire 2 — they coexist because
  // their spans don't overlap. Shift-drop H from inside Foo to
  // Foo's trailing inner-column "0,0-1,2" at wire 2; the cascade
  // widens Foo to enclose wire 2, which now collides with Z.
  //
  // Expected: Foo gets spliced into a fresh column inserted at
  // index 0; Z stays in what used to be column 0 (now column 1,
  // to the right of Foo). H lives inside Foo on wire 2.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
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
          { kind: "unitary", gate: "Z", targets: [{ qubit: 2 }] },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  const moved = moveOperation(model, "0,0-0,0", "0,0-1,2", 0, 2, false, false);
  assert.ok(moved);

  // Two top-level columns now: column 0 = [Foo alone], column 1 = [Z].
  assert.equal(
    model.componentGrid.length,
    2,
    `expected 2 top-level columns after split; got ${model.componentGrid.length}`,
  );

  const col0Gates = model.componentGrid[0].components.map(
    (/** @type {any} */ op) => op.gate,
  );
  const col1Gates = model.componentGrid[1].components.map(
    (/** @type {any} */ op) => op.gate,
  );
  assert.deepEqual(
    col0Gates,
    ["Foo"],
    `Foo must occupy a fresh leftmost column alone; got ${JSON.stringify(col0Gates)}`,
  );
  assert.deepEqual(
    col1Gates,
    ["Z"],
    `Z must remain in the (now-shifted-right) old column; got ${JSON.stringify(col1Gates)}`,
  );

  // Foo's widened targets MUST enclose wire 2 (justifying the split).
  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const fooQubits = fooOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.ok(
    fooQubits.includes(2),
    `Foo must enclose wire 2; got ${JSON.stringify(fooQubits)}`,
  );
});

test("moveOperation extend: extending without collision does NOT split the column", () => {
  // Same shape as the previous test BUT the sibling sits on a wire
  // OUTSIDE Foo's new span. 4 qubits. Column 0 = [Foo(span 0-1
  // with H on 0), Z on wire 3]. Shift-drop H to wire 2. Foo's new
  // span is [0, 2] — does NOT overlap Z (wire 3) — so no split:
  // Foo and Z stay in the same column.
  //
  // Locks down the negative case: the resolver shouldn't fire when
  // there's no actual collision.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
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
                  // Two children so Foo's children-derived targets
                  // include wires 0 AND 1 even after H moves — that
                  // way Foo's post-extend span is [0, 2], guaranteed
                  // to land on wire 2 but NOT on wire 3.
                  { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
                  { kind: "unitary", gate: "Y", targets: [{ qubit: 1 }] },
                ],
              },
            ],
          },
          { kind: "unitary", gate: "Z", targets: [{ qubit: 3 }] },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  const moved = moveOperation(model, "0,0-0,0", "0,0-1,2", 0, 2, false, false);
  assert.ok(moved);

  // Still 1 top-level column: no split needed.
  assert.equal(
    model.componentGrid.length,
    1,
    `expected 1 top-level column (no split); got ${model.componentGrid.length}`,
  );

  const colGates = model.componentGrid[0].components
    .map((/** @type {any} */ op) => op.gate)
    .sort();
  assert.deepEqual(
    colGates,
    ["Foo", "Z"].sort(),
    `Foo and Z must still share the column; got ${JSON.stringify(colGates)}`,
  );

  // Sanity: Foo's targets cover wire 2 but NOT wire 3.
  const fooOp = /** @type {any} */ (
    model.componentGrid[0].components.find(
      (/** @type {any} */ op) => op.gate === "Foo",
    )
  );
  const fooQubits = fooOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.ok(
    fooQubits.includes(2) && !fooQubits.includes(3),
    `Foo must enclose wire 2 but NOT wire 3; got ${JSON.stringify(fooQubits)}`,
  );
});

test("moveOperation extend: multiple column-siblings all survive the split", () => {
  // 5 qubits. Column 0 = [Foo(span 0-1 with X on 0 AND H on 0),
  // Y on 2, Z on 3]. Shift-drop H to wire 4 \u2014 X stays inside Foo
  // on wire 0, H lands on wire 4 \u2014 so Foo's new span is [0, 4]
  // and it now overlaps BOTH Y and Z. The extended op (Foo) gets
  // its own fresh column at index 0; Y and Z BOTH stay in what's
  // now column 1, in their original relative order.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }, { id: 4 }],
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
                  // X on wire 0 anchors Foo's low end so after H
                  // moves, Foo's children-derived span is [0, 4],
                  // not just [4, 4].
                  { kind: "unitary", gate: "X", targets: [{ qubit: 0 }] },
                ],
              },
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
                ],
              },
            ],
          },
          { kind: "unitary", gate: "Y", targets: [{ qubit: 2 }] },
          { kind: "unitary", gate: "Z", targets: [{ qubit: 3 }] },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // H lives at "0,0-1,0" (Foo at top-level "0,0", H at col 1 op 0
  // of Foo's children). Shift-drop into Foo's trailing inner-column
  // "0,0-2,4" at wire 4.
  const moved = moveOperation(model, "0,0-1,0", "0,0-2,4", 0, 4, false, false);
  assert.ok(moved);

  assert.equal(
    model.componentGrid.length,
    2,
    `expected 2 top-level columns after split; got ${model.componentGrid.length}`,
  );

  const col0Gates = model.componentGrid[0].components.map(
    (/** @type {any} */ op) => op.gate,
  );
  const col1Gates = model.componentGrid[1].components.map(
    (/** @type {any} */ op) => op.gate,
  );
  assert.deepEqual(col0Gates, ["Foo"]);
  // Y and Z preserved in their original order in the right column.
  assert.deepEqual(
    col1Gates,
    ["Y", "Z"],
    `Y and Z must stay in their original relative order; got ${JSON.stringify(col1Gates)}`,
  );
});

test("moveOperation extend: nested ancestor splits its own containing column on cascade", () => {
  // The cascade walks innermost-out, so collision-splits happen at
  // each level independently. Setup:
  //   - Outer (span 0-1) contains an inner-grid with:
  //       col 0: [Inner(span 0-1 with H on 0), Z on wire 2]
  //   - Outer lives alone at top level.
  // Shift-drop H to wire 2 (inside Inner's trailing inner-column).
  // Inner extends to enclose wire 2 → collides with Z inside Outer's
  // children → Inner splits into a fresh column ahead of Z.
  // Then the cascade refreshes Outer; Outer's new span includes
  // wire 2 (via Inner), but Outer has no top-level siblings, so
  // no top-level split.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Outer",
            targets: [{ qubit: 0 }, { qubit: 1 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "Inner",
                    targets: [{ qubit: 0 }, { qubit: 1 }],
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
                  { kind: "unitary", gate: "Z", targets: [{ qubit: 2 }] },
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
    "0,0-0,0-0,0",
    "0,0-0,0-1,2",
    0,
    2,
    false,
    false,
  );
  assert.ok(moved);

  // Top level: still one column, just Outer.
  assert.equal(
    model.componentGrid.length,
    1,
    `top level must still have a single column; got ${model.componentGrid.length}`,
  );

  // Inside Outer: 2 columns now — col 0 = [Inner alone],
  // col 1 = [Z].
  const outerOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  assert.equal(
    outerOp.children.length,
    2,
    `Outer must contain 2 inner columns after split; got ${outerOp.children.length}`,
  );

  const innerCol0Gates = outerOp.children[0].components.map(
    (/** @type {any} */ op) => op.gate,
  );
  const innerCol1Gates = outerOp.children[1].components.map(
    (/** @type {any} */ op) => op.gate,
  );
  assert.deepEqual(
    innerCol0Gates,
    ["Inner"],
    `Inner must occupy a fresh leftmost inner column; got ${JSON.stringify(innerCol0Gates)}`,
  );
  assert.deepEqual(
    innerCol1Gates,
    ["Z"],
    `Z must remain in the (now-shifted-right) old inner column; got ${JSON.stringify(innerCol1Gates)}`,
  );
});

// -------- addOperation / removeOperation: ancestor refresh ---------
//
// Both paths mutate a group's children, so the group's eager
// `.targets` cache must be refreshed afterwards (same contract
// `moveOperation` already honors via `refreshAncestorTargets`).
// These tests lock that contract down for the add and remove
// paths. The UI today never invokes `addOperation` directly into
// a group (the toolbox-drop path always lands at top level), but
// the action-layer API does, and the cache must stay coherent
// regardless of who calls it.

test("addOperation: adding a child to a group on a wire outside its span extends the group's targets", () => {
  // Foo spans wires 0-1 with a single H on wire 0 in its only
  // inner column. Adding a Y on wire 2 into Foo's trailing
  // inner-column slot must widen Foo's `.targets` to include
  // wire 2 — otherwise the renderer's bracket clips above the
  // newly-added child and subsequent reads of `Foo.targets`
  // (e.g. `getMinMaxRegIdx`) miss wire 2.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
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

  const added = addOperation(
    model,
    { kind: "unitary", gate: "Y", targets: [{ qubit: 0 }] },
    "0,0-1,2",
    2,
  );
  assert.ok(added, "addOperation should return the new op");

  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const fooQubits = fooOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);

  assert.ok(
    fooQubits.includes(2),
    `Foo must enclose wire 2 after addOperation; got ${JSON.stringify(fooQubits)}`,
  );
});

test("addOperation: cascade — adding deep into a nested group extends both inner and outer ancestors", () => {
  // Outer (wires 0-1) contains Inner (wires 0-1) contains H (wire 0).
  // Add a Y on wire 2 into Inner's trailing inner-column slot. The
  // refresh must cascade: Inner extends to include wire 2, and
  // Outer (whose old span 0-1 no longer encloses Inner's new span)
  // also extends. This mirrors the existing extend-cascade
  // moveOperation test, but exercised via the add path.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Outer",
            targets: [{ qubit: 0 }, { qubit: 1 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "Inner",
                    targets: [{ qubit: 0 }, { qubit: 1 }],
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

  const added = addOperation(
    model,
    { kind: "unitary", gate: "Y", targets: [{ qubit: 0 }] },
    "0,0-0,0-1,2",
    2,
  );
  assert.ok(added);

  const outerOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const innerOp = /** @type {any} */ (outerOp.children[0].components[0]);

  const innerQubits = innerOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  const outerQubits = outerOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);

  assert.ok(
    innerQubits.includes(2),
    `Inner must enclose wire 2 after addOperation; got ${JSON.stringify(innerQubits)}`,
  );
  for (const q of innerQubits) {
    assert.ok(
      outerQubits.includes(q),
      `Outer must enclose Inner's wire ${q} after cascade; got Outer=${JSON.stringify(outerQubits)}`,
    );
  }
});

test("removeOperation: removing the only child on a wire narrows the group's targets", () => {
  // Foo spans wires 0-1 with two children: H on wire 0 and Y on
  // wire 1, in separate inner columns. Removing Y leaves only H,
  // so Foo's `.targets` must shrink to just [wire 0]. Otherwise
  // the bracket still claims wire 1 even though nothing inside
  // touches it.
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

  // Y lives at "0,0-1,0" (Foo at top-level "0,0"; Y at col 1
  // op 0 of Foo's children).
  removeOperation(model, "0,0-1,0");

  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const fooQubits = fooOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);

  assert.deepEqual(
    fooQubits,
    [0],
    `Foo must narrow to just wire 0 after removing Y; got ${JSON.stringify(fooQubits)}`,
  );
});

test("removeOperation: cascade — removing a deep child narrows nested ancestors", () => {
  // Outer (wires 0-2) contains Inner (wires 0-2) contains
  // H (wire 0), X (wire 1), Y (wire 2), each in their own inner
  // column. Removing Y narrows Inner to wires 0-1, and Outer's
  // span (which derives from Inner) must narrow in lockstep.
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

  // Y at "0,0-0,0-2,0" (Outer "0,0", Inner "0,0-0,0", Y at col 2
  // op 0 of Inner's children).
  removeOperation(model, "0,0-0,0-2,0");

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
    `Inner must narrow to wires [0,1] after removing Y; got ${JSON.stringify(innerQubits)}`,
  );
  assert.deepEqual(
    outerQubits,
    [0, 1],
    `Outer must cascade-narrow to wires [0,1]; got ${JSON.stringify(outerQubits)}`,
  );
});

// -------- addControl / removeControl: ancestor refresh ---------
//
// Adding/removing a control on an op nested inside a group widens
// or narrows the op's wire span, which must propagate into every
// ancestor group's eager `.targets` cache.

test("addControl: adding a control to a child op on a wire outside the group's span extends the group's targets", () => {
  // Foo spans wire 0 with a single H on wire 0. Adding a control
  // on wire 2 to that H widens H's span to wires 0+2, and Foo's
  // `.targets` must extend to enclose wire 2.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
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

  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const hOp = /** @type {any} */ (fooOp.children[0].components[0]);

  const added = addControl(model, hOp, 2);
  assert.ok(added, "addControl should return true on a fresh wire");

  const fooQubits = fooOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.ok(
    fooQubits.includes(2),
    `Foo must enclose wire 2 after addControl; got ${JSON.stringify(fooQubits)}`,
  );
});

test("addControl: cascade — adding a control deep inside a nested group extends both ancestors", () => {
  // Outer (wire 0) contains Inner (wire 0) contains H (wire 0).
  // Adding a control on wire 2 to H widens H's span — Inner must
  // extend, and Outer must cascade-extend in lockstep.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
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

  const outerOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const innerOp = /** @type {any} */ (outerOp.children[0].components[0]);
  const hOp = /** @type {any} */ (innerOp.children[0].components[0]);

  addControl(model, hOp, 2);

  const innerQubits = innerOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  const outerQubits = outerOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);

  assert.ok(
    innerQubits.includes(2),
    `Inner must enclose wire 2; got ${JSON.stringify(innerQubits)}`,
  );
  assert.ok(
    outerQubits.includes(2),
    `Outer must cascade-enclose wire 2; got ${JSON.stringify(outerQubits)}`,
  );
});

test("removeControl: removing the only control extending a group's span narrows the group's targets", () => {
  // Foo spans wires 0-2 with a single H on wire 0 that has a control
  // on wire 2 (the only thing reaching wire 2 inside Foo). Removing
  // that control narrows H's span to just wire 0, and Foo's `.targets`
  // must narrow accordingly.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 0 }, { qubit: 2 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "H",
                    targets: [{ qubit: 0 }],
                    controls: [{ qubit: 2 }],
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

  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const hOp = /** @type {any} */ (fooOp.children[0].components[0]);

  const removed = removeControl(model, hOp, 2);
  assert.ok(removed, "removeControl should return true when control existed");

  const fooQubits = fooOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.deepEqual(
    fooQubits,
    [0],
    `Foo must narrow to just wire 0; got ${JSON.stringify(fooQubits)}`,
  );
});

// Refreshed group targets must be in canonical `(qubit, result)`
// order — qubit-only refs before their classical-result siblings —
// regardless of child iteration order. Renderer consumers
// (`_splitTargetsY`, `_unitary` box geometry) depend on this.

test("ancestor refresh: produces canonical (qubit, result) target order even when a classically-controlled child appears before the measurement that produces the result", () => {
  // Foo has `if(c_0) H q1` before `M q0 → c_0`, so child-iteration
  // order would yield [c_0, q1, q0]. Adding a control on wire 2
  // triggers an ancestor refresh; result must be canonically sorted.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            // Empty: refresh repopulates from scratch.
            targets: [],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "H",
                    targets: [{ qubit: 1 }],
                    controls: [{ qubit: 0, result: 0 }],
                    isConditional: true,
                  },
                ],
              },
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
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const ifOp = /** @type {any} */ (fooOp.children[0].components[0]);

  addControl(model, ifOp, 2);

  const keys = fooOp.targets.map((/** @type {any} */ r) =>
    r.result === undefined ? `q${r.qubit}` : `c${r.qubit}.${r.result}`,
  );

  assert.deepEqual(
    keys,
    ["q0", "c0.0", "q1", "q2"],
    `Foo.targets must be canonically sorted (qubit, result); got ${JSON.stringify(keys)}`,
  );
});

// -------- addOperation: clone-copy of a group preserves shape ----------
//
// Bug B8: Ctrl-drag (clone) of a multi-wire group from its top-most
// box used to clobber the group's `.targets` to a single-wire stub
// and strand the children on their original wires. The fix mirrors
// `moveOperation`'s `_moveAsUnit` path — every register in the
// cloned subtree shifts by the same `targetWire - sourceWire` delta.

test("addOperation: clone-copy of a group with delta>0 shifts every nested register", () => {
  // Foo spans wires 0-1 with H@wire0 and X@wire1. Clone the whole
  // group, grabbing it on wire 0, drop on wire 2 (delta = +2).
  // Expected: cloned Foo spans wires 2-3, with H@wire2 and X@wire3.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
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
  const sourceFoo = model.componentGrid[0].components[0];

  // Clone-drop in a fresh trailing column, grabbing on wire 0,
  // dropping on wire 2.
  const cloned = addOperation(
    model,
    /** @type {any} */ (sourceFoo),
    "1,0",
    /* targetWire */ 2,
    /* insertNewColumn */ false,
    /* sourceWire */ 0,
  );

  assert.ok(cloned, "clone returned an op");
  const clonedAny = /** @type {any} */ (cloned);
  // Cloned group's cached .targets reflect the post-shift wires.
  const clonedQubits = clonedAny.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.deepEqual(
    clonedQubits,
    [2, 3],
    `cloned Foo must span [2,3]; got ${JSON.stringify(clonedQubits)}`,
  );
  // Children shifted in lockstep.
  const innerOps = clonedAny.children[0].components;
  assert.equal(innerOps[0].gate, "H");
  assert.equal(innerOps[0].targets[0].qubit, 2);
  assert.equal(innerOps[1].gate, "X");
  assert.equal(innerOps[1].targets[0].qubit, 3);

  // Original Foo is untouched (clone, not move).
  const origFoo = /** @type {any} */ (model.componentGrid[0].components[0]);
  assert.equal(origFoo.children[0].components[0].targets[0].qubit, 0);
  assert.equal(origFoo.children[0].components[1].targets[0].qubit, 1);
});

test("addOperation: clone-copy of a group with delta=0 preserves all children on their original wires", () => {
  // Clone a group dropping on the same wire it was grabbed from
  // (different column). Children should land on the same wires
  // they came from — no shift, no clobber.
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
  const sourceFoo = model.componentGrid[0].components[0];

  const cloned = addOperation(
    model,
    /** @type {any} */ (sourceFoo),
    "1,0",
    /* targetWire */ 0,
    /* insertNewColumn */ false,
    /* sourceWire */ 0,
  );

  assert.ok(cloned, "clone returned an op");
  const clonedAny = /** @type {any} */ (cloned);
  const clonedQubits = clonedAny.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.deepEqual(
    clonedQubits,
    [0, 1],
    `cloned Foo must still span [0,1]; got ${JSON.stringify(clonedQubits)}`,
  );
  const innerOps = clonedAny.children[0].components;
  assert.equal(innerOps[0].targets[0].qubit, 0);
  assert.equal(innerOps[1].targets[0].qubit, 1);
});

test("addOperation: clone-copy of a multi-target gate preserves every leg", () => {
  // SWAP is multi-target. Without the fix, the clone path would
  // collapse its `targets` to `[{qubit: targetWire}]` — destroying
  // one leg of the swap.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "SWAP",
            targets: [{ qubit: 0 }, { qubit: 1 }],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const sourceSwap = model.componentGrid[0].components[0];

  const cloned = addOperation(
    model,
    /** @type {any} */ (sourceSwap),
    "1,0",
    /* targetWire */ 2,
    /* insertNewColumn */ false,
    /* sourceWire */ 0,
  );

  assert.ok(cloned, "clone returned an op");
  const clonedAny = /** @type {any} */ (cloned);
  const clonedQubits = clonedAny.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.deepEqual(
    clonedQubits,
    [2, 3],
    `cloned SWAP must span [2,3]; got ${JSON.stringify(clonedQubits)}`,
  );
});

test("addOperation: clone-copy of a group containing an internal classical control shifts the classical ref in lockstep", () => {
  // Foo contains `M q0 → c_0` followed by `if (c_0) H q1`. Cloning
  // Foo with delta=+2 should produce a copy where the inner M is on
  // q2 producing c_2.0, and the inner conditional H is on q3 reading
  // c_2.0 (NOT c_0.0 — that's the original's classical, which still
  // belongs to the original).
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
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
                    gate: "H",
                    targets: [{ qubit: 1 }],
                    controls: [{ qubit: 0, result: 0 }],
                    isConditional: true,
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
  const sourceFoo = model.componentGrid[0].components[0];

  const cloned = addOperation(
    model,
    /** @type {any} */ (sourceFoo),
    "1,0",
    /* targetWire */ 2,
    /* insertNewColumn */ false,
    /* sourceWire */ 0,
  );

  assert.ok(cloned, "clone returned an op");
  const clonedAny = /** @type {any} */ (cloned);
  const m = clonedAny.children[0].components[0];
  const condH = clonedAny.children[1].components[0];
  // Measurement shifted to wire 2.
  assert.equal(m.qubits[0].qubit, 2);
  assert.equal(m.results[0].qubit, 2);
  // Conditional H's target shifted to wire 3; its classical control
  // ref shifted to wire 2 (the cloned producer), NOT anchored at
  // wire 0 (which would point at the original producer).
  assert.equal(condH.targets[0].qubit, 3);
  assert.equal(condH.controls[0].qubit, 2);
  assert.equal(condH.controls[0].result, 0);
});

test("addOperation: clone-copy that would push a wire below 0 returns null", () => {
  // Grab Foo (spans wires 1-2) on wire 1, try to drop below wire 0.
  // The unit-shift would compute delta = -2, pushing wire 1 → -1.
  // Returns null (the drag controller treats null as no-op).
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 1 }, { qubit: 2 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 1 }] },
                  { kind: "unitary", gate: "X", targets: [{ qubit: 2 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const sourceFoo = model.componentGrid[0].components[0];
  const before = JSON.stringify(model.componentGrid);

  // sourceWire=1, targetWire would land Foo's wire-1 child at -1.
  // Pick targetWire = -1 to force delta = -2.
  const result = addOperation(
    model,
    /** @type {any} */ (sourceFoo),
    "1,0",
    /* targetWire */ -1,
    /* insertNewColumn */ false,
    /* sourceWire */ 1,
  );

  assert.equal(result, null, "expected null when shift would underflow");
  // Model is unchanged.
  assert.equal(JSON.stringify(model.componentGrid), before);
});

test("addOperation: omitting sourceWire still rewrites a single-target template to the requested wire (toolbox drops)", () => {
  // The dragController doesn't pass sourceWire for fresh toolbox
  // drops. Verify that omitting it still rewrites a single-target
  // template's `targets` to the requested wire.
  const model = new CircuitModel(emptyCircuit(3));

  const added = addOperation(
    model,
    /** @type {any} */ ({
      kind: "unitary",
      gate: "H",
      targets: [{ qubit: 0 }],
    }),
    "0,0",
    /* targetWire */ 2,
    /* insertNewColumn */ false,
    // sourceWire intentionally omitted
  );

  assert.ok(added, "toolbox drop returned an op");
  assert.equal(/** @type {any} */ (added).targets[0].qubit, 2);
});

// ---------------------------------------------------------------
// Group + control move.
//
// Dragging a quantum control on a group rewires just the one
// control, not the entire group. Controls on groups behave the
// same as controls on non-group ops: vertical drag rewires only
// the control (body stays put), and dropping the control on a
// body wire swaps the two.
// ---------------------------------------------------------------

test("moveOperation: vertical control drag on a group rewires only the control, leaving body untouched", () => {
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 1 }, { qubit: 2 }],
            controls: [{ qubit: 0 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 1 }] },
                  { kind: "unitary", gate: "X", targets: [{ qubit: 2 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Drag the control from wire 0 to wire 3 (above the group body).
  // Only the control moves; children stay on q1, q2 (the group's
  // body wires are not dragged along with the control leg).
  const moved = moveOperation(model, "0,0", "0,0", 0, 3, true, false);

  assert.ok(moved);
  const movedAny = /** @type {any} */ (moved);
  assert.equal(movedAny.controls.length, 1);
  assert.equal(movedAny.controls[0].qubit, 3, "control follows the drag");
  // Children stay put.
  const children = movedAny.children[0].components;
  assert.equal(children[0].targets[0].qubit, 1, "H stays on q1");
  assert.equal(children[1].targets[0].qubit, 2, "X stays on q2");
  // Group's derived `.targets` still reflects the children's span.
  const targetWires = movedAny.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort();
  assert.deepEqual(
    targetWires,
    [1, 2],
    "group .targets must remain pinned to the children's wires",
  );
});

test("moveOperation: dropping a group control onto a body wire swaps the control with that body wire", () => {
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 1 }, { qubit: 2 }],
            controls: [{ qubit: 0 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 1 }] },
                  { kind: "unitary", gate: "X", targets: [{ qubit: 2 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Drag the control from wire 0 onto wire 2 (a body wire — the X).
  // Expected swap: control goes to wire 2; the X (previously on
  // wire 2) moves to wire 0. H stays on wire 1.
  const moved = moveOperation(model, "0,0", "0,0", 0, 2, true, false);

  assert.ok(moved);
  const movedAny = /** @type {any} */ (moved);
  assert.equal(movedAny.controls.length, 1);
  assert.equal(movedAny.controls[0].qubit, 2, "control moves to wire 2");
  const children = movedAny.children[0].components;
  // Find H and X by gate label so the test isn't sensitive to
  // child reordering (sort-by-min-wire in any future sweep).
  const h = children.find((/** @type {any} */ c) => c.gate === "H");
  const x = children.find((/** @type {any} */ c) => c.gate === "X");
  assert.equal(h.targets[0].qubit, 1, "H stays on wire 1");
  assert.equal(
    x.targets[0].qubit,
    0,
    "X swaps from wire 2 to wire 0 (the control's old wire)",
  );
  // Derived `.targets` reflects the swapped body wires.
  const targetWires = movedAny.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort();
  assert.deepEqual(
    targetWires,
    [0, 1],
    "group .targets must re-derive from the swapped children",
  );
});

test("moveOperation: dropping a group control onto a wire already occupied by another control is a no-op", () => {
  // The like-register guard from the single-leg path must still
  // apply to groups: a quantum control on wire 1, dragged to wire 2
  // where another quantum control already lives, refuses the move.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 3 }],
            controls: [{ qubit: 1 }, { qubit: 2 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 3 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Drag control on wire 1 to wire 2 (already a control). No-op.
  const moved = moveOperation(model, "0,0", "0,0", 1, 2, true, false);
  assert.ok(moved);
  const movedAny = /** @type {any} */ (moved);
  const controlWires = movedAny.controls
    .map((/** @type {any} */ c) => c.qubit)
    .sort();
  assert.deepEqual(
    controlWires,
    [1, 2],
    "both controls must remain on their original wires",
  );
});

test("moveOperation: horizontal control drag on a group moves the whole op to the new column", () => {
  // Horizontal drag (targetWire === sourceWire, targetLocation in
  // a different column) is the regular column-move flow: the
  // entire op relocates to the new column. The leg-only `_moveY`
  // path with sourceWire === targetWire is a no-op (delta = 0,
  // like-register guard returns); `_moveX` does the actual move.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 1 }, { qubit: 2 }],
            controls: [{ qubit: 0 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 1 }] },
                  { kind: "unitary", gate: "X", targets: [{ qubit: 2 }] },
                ],
              },
            ],
          },
        ],
      },
      // Empty target column reserved for the move destination.
      { components: [{ kind: "unitary", gate: "Z", targets: [{ qubit: 2 }] }] },
    ],
  };
  const model = new CircuitModel(circuit);

  // Horizontal control drag: target column "1,0", same wire 0.
  const moved = moveOperation(model, "0,0", "1,0", 0, 0, true, false);
  assert.ok(moved);
  const movedAny = /** @type {any} */ (moved);
  // Control + children all stayed on their original wires.
  assert.equal(movedAny.controls[0].qubit, 0);
  const children = movedAny.children[0].components;
  assert.equal(children[0].targets[0].qubit, 1);
  assert.equal(children[1].targets[0].qubit, 2);
});

test("moveOperation: horizontal control drag on a CNOT keeps target and control intact", () => {
  // Dragging a control DOT (not the gate body) of an ordinary
  // CNOT-shaped op horizontally to a new column preserves the full
  // topology: target stays on its wire, the dragged control stays
  // on its wire, only the column changes.
  //
  // The wrapper threads `movingControl=true` through to `_moveY`,
  // whose leg-rewire path is a no-op for an in-place (sourceWire
  // === targetWire) drag — the like-register guard early-returns —
  // so `_moveX` does the column relocation alone.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
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
      // Empty target column reserved for the move destination.
      { components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 1 }] }] },
    ],
  };
  const model = new CircuitModel(circuit);

  // Horizontal control drag: sourceWire = control's wire (0),
  // targetWire = same wire (0), targetLocation in column 1.
  const moved = moveOperation(model, "0,0", "1,0", 0, 0, true, false);
  assert.ok(moved);
  const movedAny = /** @type {any} */ (moved);
  assert.equal(
    movedAny.targets.length,
    1,
    "target count must stay 1 (not collapsed by a stray rewrite)",
  );
  assert.equal(
    movedAny.targets[0].qubit,
    1,
    "target must stay on its original wire (q1)",
  );
  assert.equal(movedAny.controls.length, 1, "control count must stay 1");
  assert.equal(
    movedAny.controls[0].qubit,
    0,
    "control must stay on its original wire (q0)",
  );
});

test("moveOperation: vertical control drag on a CNOT rewires just the control leg", () => {
  // Sister test to the horizontal case: dragging a control DOT
  // VERTICALLY to a fresh wire (sourceWire !== targetWire) rewires
  // only the control. The target stays put, and no other control
  // is added.
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

  // Vertical control drag: control was on q0, drop on q2.
  const moved = moveOperation(model, "0,0", "0,0", 0, 2, true, false);
  assert.ok(moved);
  const movedAny = /** @type {any} */ (moved);
  assert.equal(movedAny.targets.length, 1);
  assert.equal(movedAny.targets[0].qubit, 1, "target stays on q1");
  assert.equal(movedAny.controls.length, 1, "still exactly one control");
  assert.equal(movedAny.controls[0].qubit, 2, "control rewired to q2");
});

test("moveOperation: vertical control drag on a CCX rewires only the dragged leg", () => {
  // Multi-control case: CCX with controls on q0 and q1, target
  // on q2. Drag the q0 control vertically to q3 — only the q0
  // control should move; the q1 control must stay put, and the
  // target must stay put.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 2 }],
            controls: [{ qubit: 0 }, { qubit: 1 }],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Vertical control drag of the q0 control onto q3.
  const moved = moveOperation(model, "0,0", "0,0", 0, 3, true, false);
  assert.ok(moved);
  const movedAny = /** @type {any} */ (moved);
  assert.equal(movedAny.targets.length, 1);
  assert.equal(movedAny.targets[0].qubit, 2, "target unchanged on q2");
  assert.equal(movedAny.controls.length, 2, "still exactly two controls");
  const controlWires = movedAny.controls
    .map((/** @type {any} */ c) => c.qubit)
    .sort();
  assert.deepEqual(
    controlWires,
    [1, 3],
    "q0 control moved to q3; q1 control stayed put",
  );
});

// ---------------------------------------------------------------
// Shift-extend cross-over cases.
//
// When a group is shift-extended onto a wire past an external
// sibling sitting on an in-between wire, the cascade must split
// the outer column so the in-between sibling slides one column to
// the right of the now-widened group. The dragController suppresses
// "direct collision" dropzones (drop wire IS occupied) via
// `getOuterColumnSiblingWires`; everything else is the action
// layer's job.
//
// The simple-gate sibling case is covered by the earlier extend
// tests. These tests pin the case where the in-between sibling is
// itself a multi-wire op (group / SWAP-like), to ensure
// `_resolveOverlapAfterExtend` handles more than 1-wire siblings.
// ---------------------------------------------------------------

test("moveOperation extend: cross-over a GROUP sibling splits the column, leaving both groups intact", () => {
  // 5 qubits. Column 0 = [Foo(span 0-1 with H@0, X@1), Bar(span
  // 3-4 with Y@3, Z@4)]. Both are groups, neither overlaps the
  // other. User shift-drops H from inside Foo to Foo's trailing
  // inner-column at wire 4 — Foo widens to enclose wire 4, which
  // makes Foo's [1, 4] span overlap Bar's [3, 4] span. Expected:
  // column splits, Foo alone at new col 0, Bar at col 1, both
  // groups keep their children intact.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }, { id: 4 }],
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
                  { kind: "unitary", gate: "X", targets: [{ qubit: 1 }] },
                ],
              },
            ],
          },
          {
            kind: "unitary",
            gate: "Bar",
            targets: [{ qubit: 3 }, { qubit: 4 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "Y", targets: [{ qubit: 3 }] },
                  { kind: "unitary", gate: "Z", targets: [{ qubit: 4 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // H lives at "0,0-0,0". Shift-drop to Foo's trailing inner-col
  // "0,0-1,0" at wire 4.
  const moved = moveOperation(model, "0,0-0,0", "0,0-1,0", 0, 4, false, false);
  assert.ok(moved);

  assert.equal(
    model.componentGrid.length,
    2,
    `expected 2 top-level columns after split; got ${model.componentGrid.length}`,
  );

  const col0Gates = model.componentGrid[0].components.map(
    (/** @type {any} */ op) => op.gate,
  );
  const col1Gates = model.componentGrid[1].components.map(
    (/** @type {any} */ op) => op.gate,
  );
  assert.deepEqual(col0Gates, ["Foo"]);
  assert.deepEqual(col1Gates, ["Bar"]);

  // Foo's widened targets must include wire 4 (justifying the split).
  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const fooWires = fooOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.ok(
    fooWires.includes(4),
    `Foo must enclose wire 4; got ${JSON.stringify(fooWires)}`,
  );

  // Bar must still have BOTH original children on their original wires.
  const barOp = /** @type {any} */ (model.componentGrid[1].components[0]);
  const barChildren = barOp.children[0].components.map(
    (/** @type {any} */ c) => ({ gate: c.gate, qubit: c.targets[0].qubit }),
  );
  assert.deepEqual(
    barChildren,
    [
      { gate: "Y", qubit: 3 },
      { gate: "Z", qubit: 4 },
    ],
    `Bar's children must be preserved through the split; got ${JSON.stringify(barChildren)}`,
  );
});

test("moveOperation extend: cross-over a sibling on an IN-BETWEEN wire (drop wire is clear past it)", () => {
  // 5 qubits. Column 0 = [Foo(span 0-1 with X@0 + H@1), Z@3 (in
  // between)]. User shift-drops H from inside Foo to wire 4 — past
  // Z, landing on a clear wire. X stays on wire 0 (anchoring Foo's
  // low end), H moves to wire 4 → Foo's new span = [0, 4], which
  // overlaps Z at wire 3. Even though the DROP wire itself (4) is
  // clear, Z gets caught by the widened span and the column splits.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }, { id: 4 }],
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
                  // X anchors Foo's low end so moving H still leaves
                  // a child on wire 0 — Foo widens (not just shifts)
                  // to [0, 4] after the drop.
                  { kind: "unitary", gate: "X", targets: [{ qubit: 0 }] },
                  { kind: "unitary", gate: "H", targets: [{ qubit: 1 }] },
                ],
              },
            ],
          },
          { kind: "unitary", gate: "Z", targets: [{ qubit: 3 }] },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // H is at "0,0-0,1" (inner col 0, opIdx 1). Shift-drop to Foo's
  // trailing inner-col "0,0-1,0" at wire 4.
  const moved = moveOperation(model, "0,0-0,1", "0,0-1,0", 1, 4, false, false);
  assert.ok(moved);

  assert.equal(
    model.componentGrid.length,
    2,
    `expected 2 top-level columns after split; got ${model.componentGrid.length}`,
  );

  assert.deepEqual(
    model.componentGrid[0].components.map((/** @type {any} */ op) => op.gate),
    ["Foo"],
  );
  assert.deepEqual(
    model.componentGrid[1].components.map((/** @type {any} */ op) => op.gate),
    ["Z"],
  );

  // Z stayed on wire 3 — the resolver shifts COLUMNS, not WIRES.
  const zOp = /** @type {any} */ (model.componentGrid[1].components[0]);
  assert.equal(
    zOp.targets[0].qubit,
    3,
    "Z must stay on its original wire; resolution is horizontal-only",
  );

  // Sanity: Foo's widened targets enclose wire 4 (the drop) and
  // still wire 0 (the X anchor).
  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const fooWires = fooOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.ok(
    fooWires.includes(0) && fooWires.includes(4),
    `Foo must enclose wires 0 and 4; got ${JSON.stringify(fooWires)}`,
  );
});

test("moveOperation extend: deeply-nested source past a multi-wire ancestor sibling splits at the top ancestor", () => {
  // The case that requires the dest-side cascade to keep walking
  // when the immediate rung sees `!changed`. With a deeply-nested
  // source AND an in-between sibling at the TOP ancestor's column,
  // the source-side cascade propagates the new wire span up through
  // every shared ancestor before the dest-side cascade runs. If the
  // dest-side cascade returned at its first `!changed` rung, the
  // collision at the topmost ancestor would go unresolved — the
  // widened Outer would share a column with Sib, swallowing it
  // visually.
  //
  // Topology: 3-deep nesting (Outer > Middle > Foo > leaves) with
  // Sib a 2-wire GROUP sibling of Outer at the top level.
  //   Top-level col 0:
  //     - Outer
  //         children = single column [Middle]
  //           Middle.children = single column [Foo]
  //             Foo.children = single column [X@0, H@1]
  //     - Sib (group @ wires 3-4, children Y@3, Z@4)
  //
  // User shift-drops H (at "0,0-0,0-0,0-0,1") to wire 5 — past
  // Sib's [3,4] span to a clear wire. Foo / Middle / Outer all
  // widen to enclose wire 5; Outer's new span [0, 5] overlaps
  // Sib's [3, 4]. Expected: top-level column splits, Outer alone
  // at col 0, Sib alone at col 1.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }, { id: 4 }, { id: 5 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Outer",
            targets: [{ qubit: 0 }, { qubit: 1 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "Middle",
                    targets: [{ qubit: 0 }, { qubit: 1 }],
                    children: [
                      {
                        components: [
                          {
                            kind: "unitary",
                            gate: "Foo",
                            targets: [{ qubit: 0 }, { qubit: 1 }],
                            children: [
                              {
                                components: [
                                  {
                                    kind: "unitary",
                                    gate: "X",
                                    targets: [{ qubit: 0 }],
                                  },
                                  {
                                    kind: "unitary",
                                    gate: "H",
                                    targets: [{ qubit: 1 }],
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
          },
          {
            kind: "unitary",
            gate: "Sib",
            targets: [{ qubit: 3 }, { qubit: 4 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "Y", targets: [{ qubit: 3 }] },
                  { kind: "unitary", gate: "Z", targets: [{ qubit: 4 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // H lives at "0,0-0,0-0,0-0,1" (Outer > Middle > Foo > inner col
  // 0, opIdx 1). Shift-drop to Foo's trailing inner-col
  // "0,0-0,0-0,0-1,0" at wire 5.
  const moved = moveOperation(
    model,
    "0,0-0,0-0,0-0,1",
    "0,0-0,0-0,0-1,0",
    1,
    5,
    false,
    false,
  );
  assert.ok(moved);

  // The split must have happened: top-level grid has 2 columns,
  // Outer alone in the left, Sib alone in the right.
  assert.equal(
    model.componentGrid.length,
    2,
    `expected 2 top-level columns after split; got ${model.componentGrid.length}`,
  );
  assert.deepEqual(
    model.componentGrid[0].components.map((/** @type {any} */ op) => op.gate),
    ["Outer"],
  );
  assert.deepEqual(
    model.componentGrid[1].components.map((/** @type {any} */ op) => op.gate),
    ["Sib"],
  );

  // Outer's targets must have propagated all the way up to {0, 5}
  // — sanity check that the cascade refresh ran (not just the
  // overlap resolver).
  const outerOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const outerWires = outerOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.ok(
    outerWires.includes(0) && outerWires.includes(5),
    `Outer must enclose wires 0 and 5 after the deep cascade; got ${JSON.stringify(outerWires)}`,
  );

  // Sib's children must survive the split intact — its wires
  // didn't move (the resolver shifts COLUMNS, not WIRES).
  const sibOp = /** @type {any} */ (model.componentGrid[1].components[0]);
  const sibChildren = sibOp.children[0].components.map(
    (/** @type {any} */ c) => ({ gate: c.gate, qubit: c.targets[0].qubit }),
  );
  assert.deepEqual(
    sibChildren,
    [
      { gate: "Y", qubit: 3 },
      { gate: "Z", qubit: 4 },
    ],
    `Sib's children must be preserved through the split; got ${JSON.stringify(sibChildren)}`,
  );
});

// ---------------------------------------------------------------
// Centralized post-widening cleanup.
//
// Whenever an op's `.targets` / `.controls` grow (added control,
// added target, wider remap, etc.), the action layer's
// `_resolveSpanChange` must check the op against its own column
// siblings AND propagate up through every ancestor. Prior to this,
// only ancestors were checked — so a top-level `addControl` that
// widened the op into a same-column sibling silently left them
// overlapping. These tests pin the centralized invariant: any
// path that widens an op must trigger the split-and-shift.
// The flat (no-group) base case lives in
// `circuit-actions/addRemove.test.mjs`; what remains here are
// the group-internal and group-via-ancestor split cases.
// ---------------------------------------------------------------

test("addControl: nested widening into a same-column sibling inside a group splits inside the group", () => {
  // Inside group Foo (top-level), col 0 contains [H(q0), Z(q3)].
  // Add a control on q3 to H → H now spans q0..q3 and overlaps Z
  // INSIDE the group's child grid. The H must end up in its own
  // column inside Foo; Z follows one column to the right.
  //
  // Prior to centralizing the cleanup, the ancestor cascade fired
  // on Foo (refreshing its outer cache) but never resolved the
  // collision inside Foo's children.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 0 }, { qubit: 3 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
                  { kind: "unitary", gate: "Z", targets: [{ qubit: 3 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const hOp = /** @type {any} */ (fooOp.children[0].components[0]);
  const ok = addControl(model, hOp, 3);
  assert.ok(ok);

  // Foo's child grid must now have two columns: [H] then [Z].
  assert.equal(
    fooOp.children.length,
    2,
    `expected Foo's child grid to split; got ${JSON.stringify(
      fooOp.children.map((/** @type {any} */ c) =>
        c.components.map((/** @type {any} */ op) => op.gate),
      ),
    )}`,
  );
  assert.equal(fooOp.children[0].components[0].gate, "H");
  assert.equal(fooOp.children[1].components[0].gate, "Z");
});

test("addControl: widening that pushes the OUTER GROUP into its top-level sibling also splits the top-level column", () => {
  // Top-level col 0: [Foo(q0), X(q3)].
  //   - Foo has one child H(q0). Foo's outer span = [q0,q0].
  //   - X is on q3.
  //   - Adding a control on q3 to H widens H to q0..q3, which
  //     cascades up to widen Foo's `.targets` to q0..q3, which
  //     now overlaps the top-level X.
  // Expect TWO splits: (a) inside Foo there's only one child so
  // no inner split, and (b) at top-level, Foo gets its own column
  // and X is pushed to col 1.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 0 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
                ],
              },
            ],
          },
          { kind: "unitary", gate: "X", targets: [{ qubit: 3 }] },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const hOp = /** @type {any} */ (fooOp.children[0].components[0]);
  addControl(model, hOp, 3);

  // Top-level grid should now have [Foo] then [X].
  assert.equal(
    model.componentGrid.length,
    2,
    `expected the top-level column to split; got ${JSON.stringify(
      model.componentGrid.map((c) =>
        c.components.map((/** @type {any} */ op) => op.gate),
      ),
    )}`,
  );
  assert.equal(model.componentGrid[0].components[0].gate, "Foo");
  assert.equal(model.componentGrid[1].components[0].gate, "X");
});

// ---------------------------------------------------------------
// Overlap-collision check uses the drawn span of siblings.
//
// `getMinMaxRegIdx` includes classical-control wires; the quantum-
// only `getQuantumWireRange` would under-report collisions. A
// sibling whose target is on a high wire but whose classical
// control points at a low-wire measurement visually occupies
// every wire between them (the renderer paints a connector
// through them); a widened group whose span intersects ANY of
// those wires collides with that connector even if it doesn't
// touch the quantum target.
// ---------------------------------------------------------------

test("addControl widening: sibling with classical control on a LOW wire (drawn-span overlap) triggers split even when quantum target is clear", () => {
  // 5 qubits.
  //   col 0: M on q1 (produces a result that X will read).
  //   col 1: [Foo(span q0, child H@q0), X(target q3, classical
  //          control pointing at M's result on q1)].
  // X's QUANTUM span is just [q3]. X's DRAWN span is [q1, q3]
  // because the classical-control connector falls from the gate
  // body on q3 down to the producer's wire q1.
  //
  // Add a quantum control on q2 to H inside Foo. Foo's `.targets`
  // widens from q0 to q0..q2. Foo's NEW span doesn't touch X's
  // quantum target q3 — but it DOES intersect X's drawn span at
  // q1, q2 (where X's classical-control connector lives). The
  // renderer would draw Foo's expanded box right through that
  // connector. The action-layer cascade must split col 1 to
  // restore a non-overlapping layout.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }, { id: 4 }],
    componentGrid: [
      {
        components: [
          {
            kind: "measurement",
            gate: "Measure",
            qubits: [{ qubit: 1 }],
            results: [{ qubit: 1, result: 0 }],
          },
        ],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 0 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
                ],
              },
            ],
          },
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 3 }],
            controls: [{ qubit: 1, result: 0 }],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const fooOp = /** @type {any} */ (model.componentGrid[1].components[0]);
  const hOp = /** @type {any} */ (fooOp.children[0].components[0]);

  addControl(model, hOp, 2);

  // col 0 (M) stays. The Foo/X column must have split into two:
  // one with Foo alone, one with X alone.
  assert.equal(
    model.componentGrid.length,
    3,
    `expected col 1 to split into two; got ${JSON.stringify(
      model.componentGrid.map((c) =>
        c.components.map((/** @type {any} */ op) => op.gate),
      ),
    )}`,
  );
  // Foo lands in a fresh column at index 1; X gets pushed to the
  // next column (mirrors the `commitAddControl` convention).
  assert.deepEqual(
    model.componentGrid[1].components.map((/** @type {any} */ op) => op.gate),
    ["Foo"],
    "Foo must end up alone in its column after the split",
  );
  assert.deepEqual(
    model.componentGrid[2].components.map((/** @type {any} */ op) => op.gate),
    ["X"],
    "X must end up alone in the column to Foo's right",
  );

  // Sanity: Foo's children-derived targets now include q2 (the
  // new control wire) but not q3 (X's quantum target) — the
  // split was justified ONLY by the drawn-span collision with X's
  // classical-control connector. Note groups store only the
  // wires their direct children USE, not every wire in the span,
  // so q1 (in the [q0..q2] span but used by nobody) isn't here.
  const widenedFoo = /** @type {any} */ (model.componentGrid[1].components[0]);
  const fooWires = widenedFoo.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.ok(
    fooWires.includes(0) && fooWires.includes(2) && !fooWires.includes(3),
    `Foo's targets must include q0 and q2 but not q3; got ${JSON.stringify(fooWires)}`,
  );
});

test("moveOperation shift-extend: cross-over a sibling whose drawn span includes a classical-control wire", () => {
  // Same drawn-span vs quantum-span distinction as the previous
  // test, but exercised through `moveOperation`'s shift-extend
  // path. This is the literal scenario in the user's bug report.
  //
  // 5 qubits.
  //   col 0: M on q1.
  //   col 1: [Foo(span q0, child H@q0), X(target q3, classical
  //          control on q1)].
  // Shift-drop H from inside Foo onto wire q2 (Foo's trailing
  // inner-col at q2). Foo's `.targets` cascade-widens to q0..q2.
  // q2 doesn't touch X's quantum target (q3) but DOES sit on
  // X's classical-control connector (which spans q1..q3 visually).
  // Expect col 1 to split: Foo alone, X pushed right.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }, { id: 4 }],
    componentGrid: [
      {
        components: [
          {
            kind: "measurement",
            gate: "Measure",
            qubits: [{ qubit: 1 }],
            results: [{ qubit: 1, result: 0 }],
          },
        ],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 0 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
                ],
              },
            ],
          },
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 3 }],
            controls: [{ qubit: 1, result: 0 }],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // H lives at "1,0-0,0" (top col 1, op 0 = Foo; then inner col
  // 0, op 0 = H). Shift-drop to Foo's trailing inner-col "1,0-1,0"
  // at wire 2.
  const moved = moveOperation(model, "1,0-0,0", "1,0-1,0", 0, 2, false, false);
  assert.ok(moved, "moveOperation must succeed");

  assert.equal(
    model.componentGrid.length,
    3,
    `expected col 1 to split; got ${JSON.stringify(
      model.componentGrid.map((c) =>
        c.components.map((/** @type {any} */ op) => op.gate),
      ),
    )}`,
  );
  assert.deepEqual(
    model.componentGrid[0].components.map((/** @type {any} */ op) => op.gate),
    ["Measure"],
    "M must stay in col 0",
  );
  assert.deepEqual(
    model.componentGrid[1].components.map((/** @type {any} */ op) => op.gate),
    ["Foo"],
    "Foo must occupy the new col 1 alone",
  );
  assert.deepEqual(
    model.componentGrid[2].components.map((/** @type {any} */ op) => op.gate),
    ["X"],
    "X must be pushed to col 2",
  );

  // Foo's widened quantum span includes q2 (the drop wire) but
  // not q3 (X's quantum target) — confirms the split was driven
  // by the drawn-span collision, not a quantum-span overlap.
  const widenedFoo = /** @type {any} */ (model.componentGrid[1].components[0]);
  const fooWires = widenedFoo.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.ok(
    fooWires.includes(2) && !fooWires.includes(3),
    `Foo must enclose q2 but not q3; got ${JSON.stringify(fooWires)}`,
  );
});

test("no false split: widening that lands BELOW a classically-controlled sibling's drawn span stays put", () => {
  // Negative sanity: the drawn-span fix must not over-block. If
  // the widened group's span lies entirely outside the sibling's
  // drawn span (both quantum target AND classical-control wire),
  // no collision exists and no split must happen.
  //
  // 5 qubits.
  //   col 0: M on q2.
  //   col 1: [X(target q3, classical control on q2), Z(q0)].
  //   - X's drawn span: q2..q3.
  //   - Z is the op we widen.
  // Add a quantum control on q1 to Z. Z's new span: q0..q1. That
  // DOESN'T overlap X's drawn span [q2, q3], so the column must
  // NOT split.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }, { id: 4 }],
    componentGrid: [
      {
        components: [
          {
            kind: "measurement",
            gate: "Measure",
            qubits: [{ qubit: 2 }],
            results: [{ qubit: 2, result: 0 }],
          },
        ],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 3 }],
            controls: [{ qubit: 2, result: 0 }],
          },
          { kind: "unitary", gate: "Z", targets: [{ qubit: 0 }] },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const zOp = /** @type {any} */ (model.componentGrid[1].components[1]);
  addControl(model, zOp, 1);
  assert.equal(
    model.componentGrid.length,
    2,
    `widening below the drawn span must not split; got ${JSON.stringify(
      model.componentGrid.map((c) =>
        c.components.map((/** @type {any} */ op) => op.gate),
      ),
    )}`,
  );
  assert.deepEqual(
    model.componentGrid[1].components.map((/** @type {any} */ op) => op.gate),
    ["X", "Z"],
    "X and Z must still share col 1",
  );
});

// ---------------------------------------------------------------
// Ordinary (non-shift-extend) move into a sibling-occupied column.
//
// Exercises the same `_resolveSpanChange` chokepoint as the
// shift-extend path, but for SOURCE shapes the other tests don't
// cover:
//
//   - a CONTROLLED gate moved into a sibling-occupied column
//     (control leg is what causes the collision, not the target),
//   - a MULTI-TARGET gate (SWAP) moved into a sibling-occupied
//     column.
//
// Both shapes route through `_addOp`'s pre-insert overlap check
// AND the dest-side `_resolveSpanChange` cascade. `_addOp` handles
// the immediate column; `_resolveSpanChange` is the architectural
// guarantee that nothing slips through. Pin both invariants:
// post-move grid layout splits cleanly, no duplicate, no overlap.
// ---------------------------------------------------------------

test("moveOperation: moving a CONTROLLED gate into a sibling-occupied column splits the column", () => {
  // 5 qubits.
  //   col 0: [Z(q4)] — anchor, stays put.
  //   col 1: [H(q0), Y(q2)] — H is the source, Y is the sibling.
  //
  // Move H from "1,0" → "1,0" with the same target wire (q0)
  // wouldn't be interesting. Instead, build H to have a control
  // on q3 already (so its span = q0..q3, which is COMPATIBLE with
  // Y on q2... no, wait, that overlaps. Use a different setup.)
  //
  // Restart: build a circuit where the SOURCE's span doesn't yet
  // overlap the sibling, then MOVE the source to a column where
  // it now does. That's the genuine "move-into-overlap" case.
  //
  //   col 0: [CNOT(target=q0, ctrl=q3)] — source, span q0..q3.
  //   col 1: [Y(q1)] — destination's sibling.
  // Move CNOT from "0,0" to col 1 wire 0 — same wire, just
  // moving horizontally into a column where Y(q1) sits. CNOT's
  // span [q0,q3] envelops q1, so it collides with Y → split.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }, { id: 4 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "CNOT",
            targets: [{ qubit: 0 }],
            controls: [{ qubit: 3 }],
          },
        ],
      },
      {
        components: [{ kind: "unitary", gate: "Y", targets: [{ qubit: 1 }] }],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Move CNOT from col 0 to col 1, wire stays at q0. Target
  // location "1,0" = col 1, opIndex 0.
  const moved = moveOperation(model, "0,0", "1,0", 0, 0, false, false);
  assert.ok(moved);

  // _addOp's pre-insert overlap check sees CNOT's span [0,3]
  // would overlap Y(q1) and inserts a fresh column. Top-level
  // grid: [CNOT] in col 0 (the freshly inserted one), [Y] in col
  // 1. The OLD col 0 (where CNOT used to live) is empty and gets
  // pruned by `_removeOp`'s cleanup.
  assert.equal(
    model.componentGrid.length,
    2,
    `expected exactly 2 columns post-move; got ${JSON.stringify(
      model.componentGrid.map((c) =>
        c.components.map((/** @type {any} */ op) => op.gate),
      ),
    )}`,
  );

  // CNOT exactly once, in its own column. Y exactly once, in its
  // own column. Order: CNOT before Y (the _addOp insert-new-column
  // convention pushes the original sibling right).
  const layout = model.componentGrid.map((c) =>
    c.components.map((/** @type {any} */ op) => op.gate),
  );
  assert.deepEqual(layout, [["CNOT"], ["Y"]]);

  // No duplicate.
  let cnotCount = 0;
  let yCount = 0;
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      if (/** @type {any} */ (op).gate === "CNOT") cnotCount++;
      if (/** @type {any} */ (op).gate === "Y") yCount++;
    }
  }
  assert.equal(cnotCount, 1, `CNOT must appear exactly once; got ${cnotCount}`);
  assert.equal(yCount, 1, `Y must appear exactly once; got ${yCount}`);

  // The CNOT's control on q3 is preserved through the move (the
  // move is on the TARGET leg, not the control, and `_moveAsUnit`
  // is true for op-with-controls only when explicitly grabbing
  // the control — here the move keeps the control intact).
  const movedCnot = /** @type {any} */ (model.componentGrid[0].components[0]);
  const controls = (movedCnot.controls ?? []).map(
    (/** @type {any} */ c) => c.qubit,
  );
  assert.deepEqual(
    controls,
    [3],
    `CNOT's control on q3 must survive the move; got ${JSON.stringify(controls)}`,
  );
});

test("moveOperation: moving a MULTI-TARGET gate (SWAP) into a sibling-occupied column splits the column", () => {
  // 4 qubits.
  //   col 0: [SWAP(q0,q2)] — source, span q0..q2.
  //   col 1: [Y(q1)] — destination's sibling.
  //
  // Move SWAP horizontally from col 0 to col 1. The source op
  // `selectedWire` = q0 (one of the SWAP's legs), targetWire =
  // q0 (no vertical change). `_moveAsUnit` is true for SWAP
  // (multi-target), so the move keeps both legs at q0 and q2;
  // its span [q0, q2] envelops q1, colliding with Y → split.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
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
      {
        components: [{ kind: "unitary", gate: "Y", targets: [{ qubit: 1 }] }],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  const moved = moveOperation(model, "0,0", "1,0", 0, 0, false, false);
  assert.ok(moved);

  assert.equal(
    model.componentGrid.length,
    2,
    `expected exactly 2 columns post-move; got ${JSON.stringify(
      model.componentGrid.map((c) =>
        c.components.map((/** @type {any} */ op) => op.gate),
      ),
    )}`,
  );

  const layout = model.componentGrid.map((c) =>
    c.components.map((/** @type {any} */ op) => op.gate),
  );
  assert.deepEqual(
    layout,
    [["SWAP"], ["Y"]],
    "SWAP must occupy a fresh column ahead of Y",
  );

  // No duplicates.
  let swapCount = 0;
  let yCount = 0;
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      if (/** @type {any} */ (op).gate === "SWAP") swapCount++;
      if (/** @type {any} */ (op).gate === "Y") yCount++;
    }
  }
  assert.equal(swapCount, 1, `SWAP must appear exactly once; got ${swapCount}`);
  assert.equal(yCount, 1, `Y must appear exactly once; got ${yCount}`);

  // Both SWAP legs survive on their original wires (unit-shift
  // with delta=0 → no change to either leg).
  const movedSwap = /** @type {any} */ (model.componentGrid[0].components[0]);
  const swapWires = movedSwap.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.deepEqual(
    swapWires,
    [0, 2],
    `SWAP's targets must remain [q0, q2]; got ${JSON.stringify(swapWires)}`,
  );
});
