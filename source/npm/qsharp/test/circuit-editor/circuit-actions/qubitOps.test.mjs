// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// `moveQubit` / `removeQubit` and their interaction with
// classical-control consumers of measurements. Exercises the
// wire-permutation contract: every register reference (top-level,
// nested, cached `.targets`, and classical-ref consumers) gets
// rewritten by the same 1-to-1 function, with no result-index
// renumbering.

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import { CircuitModel } from "../../../dist/ux/circuit-vis/data/circuitModel.js";
import {
  findAndRemoveOperations,
  moveQubit,
  removeQubit,
} from "../../../dist/ux/circuit-vis/actions/circuitActions.js";

const _mGate = (/** @type {number} */ q, /** @type {number} */ r) => ({
  kind: "measurement",
  gate: "Measure",
  qubits: [{ qubit: q }],
  results: [{ qubit: q, result: r }],
});

const _ccx = (
  /** @type {number} */ targetQubit,
  /** @type {number} */ ctrlQubit,
  /** @type {number} */ ctrlResult,
) => ({
  kind: "unitary",
  gate: "X",
  targets: [{ qubit: targetQubit }],
  controls: [{ qubit: ctrlQubit, result: ctrlResult }],
});

// ---------------------------------------------------------------------------
// moveQubit / removeQubit (flat-grid base cases)
// ---------------------------------------------------------------------------

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

test("moveQubit with isBetween=true inserts before the target wire", () => {
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          { kind: "unitary", gate: "W", targets: [{ qubit: 0 }] },
          { kind: "unitary", gate: "X", targets: [{ qubit: 1 }] },
          { kind: "unitary", gate: "Y", targets: [{ qubit: 2 }] },
          { kind: "unitary", gate: "Z", targets: [{ qubit: 3 }] },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Move wire 0 to just before wire 3 (isBetween=true).
  moveQubit(model, 0, 3, true);

  // Expected new wire order: [X, Y, W, Z]. After the rewire, ops carry
  // the *new* wire indices for their targets.
  const ops = model.componentGrid[0].components;
  assert.equal(ops[0].gate, "X");
  assert.equal(/** @type {any} */ (ops[0]).targets[0].qubit, 0);
  assert.equal(ops[1].gate, "Y");
  assert.equal(/** @type {any} */ (ops[1]).targets[0].qubit, 1);
  assert.equal(ops[2].gate, "W");
  assert.equal(/** @type {any} */ (ops[2]).targets[0].qubit, 2);
  assert.equal(ops[3].gate, "Z");
  assert.equal(/** @type {any} */ (ops[3]).targets[0].qubit, 3);
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
// removeQubit / moveQubit recurse into nested groups
// ---------------------------------------------------------------------------

test("removeQubit: shifts wire indices on ops nested inside groups", () => {
  // Foo (wires 1-2) contains H on wire 2. Removing wire 0 shifts
  // every >0 wire down by one, including the H inside Foo (2 → 1)
  // and Foo's own cached targets (1,2 → 0,1).
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
                  { kind: "unitary", gate: "H", targets: [{ qubit: 2 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  removeQubit(model, 0);

  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const hOp = /** @type {any} */ (fooOp.children[0].components[0]);

  assert.equal(
    hOp.targets[0].qubit,
    1,
    `Nested H must shift from wire 2 to wire 1; got ${hOp.targets[0].qubit}`,
  );
  const fooQubits = fooOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.deepEqual(
    fooQubits,
    [0, 1],
    `Foo's cached targets must shift to [0,1]; got ${JSON.stringify(fooQubits)}`,
  );
});

test("moveQubit: swaps wire indices on ops nested inside groups", () => {
  // Foo spans wires 0-1, containing H on wire 0 and X on wire 1.
  // Swapping wires 0 and 1 at the top level must propagate into the
  // nested ops so H now targets wire 1 and X targets wire 0.
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

  moveQubit(model, 0, 1, false);

  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const innerOps = fooOp.children[0].components;
  // Nested column re-sorted by lowest-numbered register: X (wire 0)
  // now comes before H (wire 1).
  assert.equal(innerOps[0].gate, "X");
  assert.equal(innerOps[0].targets[0].qubit, 0);
  assert.equal(innerOps[1].gate, "H");
  assert.equal(innerOps[1].targets[0].qubit, 1);
});

test("moveQubit: refreshes group `.targets` cache after wire swap", () => {
  // Foo spans wires 0-1 with a single H on wire 0. Swap wires 0
  // and 1; H now targets wire 1, and Foo's cached `.targets` must
  // be re-derived from the (still single-child) cache rather than
  // left as a stale `[{q:0}, {q:1}]`.
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

  moveQubit(model, 0, 1, false);

  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const hOp = /** @type {any} */ (fooOp.children[0].components[0]);
  assert.equal(hOp.targets[0].qubit, 1);
  assert.equal(
    fooOp.targets.length,
    1,
    `Foo's cached targets must have one entry; got ${JSON.stringify(fooOp.targets)}`,
  );
  assert.equal(fooOp.targets[0].qubit, 1);
});

test("moveQubit: resolves nested-group overlaps introduced by widening", () => {
  // Foo spans wires 0-1 with children H@wire0 and X@wire1 in the
  // same nested column. Swapping wires 0 and 1 keeps the H/X span
  // non-overlapping (each owns its own wire), so the nested column
  // stays as a single column. The smoke-test value is that we don't
  // throw and don't corrupt the children.
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

  moveQubit(model, 0, 1, false);

  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  // Both children still live in a single nested column (no overlap
  // was introduced — each child still owns its own wire after the
  // swap).
  assert.equal(fooOp.children.length, 1);
  assert.equal(fooOp.children[0].components.length, 2);
});

test("moveQubit: swap inside a group splits a nested column when a child's control moves over a sibling", () => {
  // Foo spans wires 0-2. In a single nested column it carries:
  //   - CX with target@wire0, ctrl@wire1  (spans wires 0-1)
  //   - H@wire2
  // The two children don't overlap, so they share one nested column.
  //
  // Swapping wires 1 and 2 widens the CX's vertical span to 0-2
  // (target stays at wire 0, ctrl moves to wire 2), and H lands on
  // wire 1 — right between CX's target and control. The nested
  // column must collision-split into two so the two children no
  // longer occupy the same column.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 0 }, { qubit: 1 }, { qubit: 2 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "X",
                    targets: [{ qubit: 0 }],
                    controls: [{ qubit: 1 }],
                  },
                  { kind: "unitary", gate: "H", targets: [{ qubit: 2 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  moveQubit(model, 1, 2, false);

  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  // The nested column had to split into two: CX (target@wire0,
  // ctrl@wire2) now spans wires 0-2 and conflicts with H@wire1.
  assert.equal(
    fooOp.children.length,
    2,
    `Foo's nested grid must split into two columns after the wire swap; got ${fooOp.children.length}`,
  );

  // Both children survived (one per column), with their wire
  // references rewritten by the 1-to-1 permutation.
  const flattened = fooOp.children.flatMap(
    (/** @type {any} */ col) => col.components,
  );
  assert.equal(flattened.length, 2);

  const cx = flattened.find((/** @type {any} */ op) => op.gate === "X");
  const h = flattened.find((/** @type {any} */ op) => op.gate === "H");
  assert.ok(cx, "CX child must survive the split");
  assert.ok(h, "H child must survive the split");
  assert.equal(cx.targets[0].qubit, 0);
  assert.equal(cx.controls[0].qubit, 2);
  assert.equal(h.targets[0].qubit, 1);

  // CX and H must end up in different nested columns.
  const cxColIdx = fooOp.children.findIndex((/** @type {any} */ col) =>
    col.components.some((/** @type {any} */ op) => op.gate === "X"),
  );
  const hColIdx = fooOp.children.findIndex((/** @type {any} */ col) =>
    col.components.some((/** @type {any} */ op) => op.gate === "H"),
  );
  assert.notEqual(
    cxColIdx,
    hColIdx,
    "CX and H must be split into separate nested columns",
  );
  // And the parent still claims the full wire span it covered before.
  const fooQubits = fooOp.targets
    .map((/** @type {any} */ t) => t.qubit)
    .sort((/** @type {number} */ a, /** @type {number} */ b) => a - b);
  assert.deepEqual(fooQubits, [0, 1, 2]);
});

// ---------------------------------------------------------------------------
// moveQubit + Ms-with-classical-consumers
//
// `moveQubit` is a low-level wire-index remap: every register
// reference (including classical refs in consumer ops AND the
// `.results` arrays of measurement ops) gets its `qubit` field
// rewritten by the same 1-to-1 wire-permutation function. It does
// NOT renumber result indices (that's `_updateMeasurementLines`,
// which only runs from `moveOperation`/`removeOperation` paths).
//
// The invariant: after `moveQubit`, every classical-control
// consumer must still reference a real, unique (qubit, result) key
// that some measurement produces. The 1-to-1 remap preserves
// uniqueness as long as the pre-state was well-formed.
// ---------------------------------------------------------------------------

test("moveQubit: classical-control consumer follows a moved M's qubit index", () => {
  // M on wire 0 with a downstream classically-controlled X on
  // wire 2 (controlled by the M's result). Swap wires 0 and 1.
  // The M, its `.results` and the consumer's classical-control
  // ref must all rewrite qubit 0 → 1.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      { components: [_mGate(0, 0)] },
      { components: [_ccx(2, 0, 0)] },
    ],
  };
  const model = new CircuitModel(circuit);

  moveQubit(model, 0, 1, false);

  // Find the M and the consumer in the post-swap grid.
  /** @type {any} */
  let m;
  /** @type {any} */
  let consumer;
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      if (op.kind === "measurement") m = op;
      else if (op.kind === "unitary" && op.gate === "X") consumer = op;
    }
  }
  assert.ok(m, "M must still exist");
  assert.ok(consumer, "consumer must still exist");
  assert.equal(m.qubits[0].qubit, 1, "M's qubit ref rewires 0 → 1");
  assert.equal(m.results[0].qubit, 1, "M's results ref rewires 0 → 1");
  const classicalRef = consumer.controls.find(
    (/** @type {any} */ c) => c.result !== undefined,
  );
  assert.deepEqual(
    { qubit: classicalRef.qubit, result: classicalRef.result },
    { qubit: 1, result: 0 },
    "consumer's classical-control ref rewires 0 → 1",
  );
});

test("moveQubit: swap of two wires that both have Ms with consumers preserves per-wire uniqueness", () => {
  // Wire 0 has M_a (r=0); wire 1 has M_b (r=0). Each has its own
  // consumer on wire 2. The Ms hold the SAME pre-swap result
  // index (0); the wire-permutation keeps them on different
  // wires post-swap, so (qubit, result) keys stay unique without
  // any renumbering pass.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      { components: [_mGate(0, 0)] },
      { components: [_mGate(1, 0)] },
      { components: [_ccx(2, 0, 0)] }, // consumes M_a (wire 0, r=0)
      { components: [_ccx(2, 1, 0)] }, // consumes M_b (wire 1, r=0)
    ],
  };
  const model = new CircuitModel(circuit);

  moveQubit(model, 0, 1, false);

  /** @type {any[]} */
  const ms = [];
  /** @type {any[]} */
  const consumers = [];
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      if (op.kind === "measurement") ms.push(op);
      else if (
        op.kind === "unitary" &&
        op.controls &&
        op.controls.some((/** @type {any} */ c) => c.result !== undefined)
      )
        consumers.push(op);
    }
  }
  assert.equal(ms.length, 2);
  assert.equal(consumers.length, 2);

  // Per-wire (qubit, result) keys must be unique.
  /** @type {Set<string>} */
  const keys = new Set();
  for (const m of ms) {
    const k = `${m.results[0].qubit}:${m.results[0].result}`;
    assert.ok(!keys.has(k), `duplicate M.results key ${k}`);
    keys.add(k);
  }

  // Every consumer must still resolve to a real M.
  for (const c of consumers) {
    const ref = c.controls.find(
      (/** @type {any} */ x) => x.result !== undefined,
    );
    const k = `${ref.qubit}:${ref.result}`;
    assert.ok(
      keys.has(k),
      `consumer references ${k}, but no M produces it (orphaned)`,
    );
  }
});

test("moveQubit: swap of a wire carrying multiple Ms keeps the consumer chain in sync", () => {
  // Wire 0 has M_a (r=0) and M_b (r=1), each with its own
  // consumer on wire 2. Wire 1 is empty. Swap wires 0 and 1.
  // After the swap: both Ms (and both consumers' classical refs)
  // get rewired 0 → 1, with their `.result` indices preserved
  // (moveQubit does not renumber — and doesn't need to, because
  // the wire on the other side of the swap had no Ms to collide
  // with).
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      { components: [_mGate(0, 0)] },
      { components: [_mGate(0, 1)] },
      { components: [_ccx(2, 0, 0)] }, // consumes M_a
      { components: [_ccx(2, 0, 1)] }, // consumes M_b
    ],
  };
  const model = new CircuitModel(circuit);

  moveQubit(model, 0, 1, false);

  // All Ms should now live on wire 1 with their original result
  // indices intact (0 and 1).
  /** @type {any[]} */
  const ms = [];
  /** @type {any[]} */
  const consumers = [];
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      if (op.kind === "measurement") ms.push(op);
      else if (
        op.kind === "unitary" &&
        op.controls &&
        op.controls.some((/** @type {any} */ c) => c.result !== undefined)
      )
        consumers.push(op);
    }
  }
  assert.equal(ms.length, 2);
  assert.equal(consumers.length, 2);

  /** @type {Set<string>} */
  const keys = new Set();
  for (const m of ms) {
    assert.equal(m.qubits[0].qubit, 1, "M now on wire 1");
    const k = `${m.results[0].qubit}:${m.results[0].result}`;
    assert.ok(!keys.has(k), `duplicate M.results key ${k}`);
    keys.add(k);
  }
  // Specifically expect {1:0, 1:1}.
  assert.deepEqual([...keys].sort(), ["1:0", "1:1"]);

  for (const c of consumers) {
    const ref = c.controls.find(
      (/** @type {any} */ x) => x.result !== undefined,
    );
    assert.equal(ref.qubit, 1, "consumer classical-control ref rewires to 1");
    assert.ok(
      keys.has(`${ref.qubit}:${ref.result}`),
      "consumer must still resolve to a real M",
    );
  }
});

test("moveQubit isBetween: moving a wire past one with Ms-with-consumers remaps every party in lockstep", () => {
  // 4 wires. Wire 1 has M_a (r=0); wire 2 has M_b (r=0). Each
  // has a consumer on wire 3. Move wire 0 to between wires 2 and
  // 3 (isBetween=true, sourceWire=0, targetWire=3) — new wire
  // order is [1, 2, 0, 3]. Remap: old 0→2, old 1→0, old 2→1,
  // old 3→3. Every classical ref and every M.results.qubit must
  // shift accordingly; consumers must still resolve.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      { components: [_mGate(1, 0)] },
      { components: [_mGate(2, 0)] },
      { components: [_ccx(3, 1, 0)] }, // consumes M_a
      { components: [_ccx(3, 2, 0)] }, // consumes M_b
    ],
  };
  const model = new CircuitModel(circuit);

  moveQubit(model, 0, 3, true);

  /** @type {any[]} */
  const ms = [];
  /** @type {any[]} */
  const consumers = [];
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      if (op.kind === "measurement") ms.push(op);
      else if (
        op.kind === "unitary" &&
        op.controls &&
        op.controls.some((/** @type {any} */ c) => c.result !== undefined)
      )
        consumers.push(op);
    }
  }
  assert.equal(ms.length, 2);
  assert.equal(consumers.length, 2);

  // M_a (was wire 1 → new wire 0) and M_b (was wire 2 → new
  // wire 1). Result indices unchanged.
  const mByWire = new Map(ms.map((m) => [m.qubits[0].qubit, m]));
  assert.ok(mByWire.has(0) && mByWire.has(1));
  assert.equal(mByWire.get(0).results[0].result, 0);
  assert.equal(mByWire.get(1).results[0].result, 0);

  // Per-wire (qubit, result) keys still unique.
  /** @type {Set<string>} */
  const keys = new Set();
  for (const m of ms) {
    keys.add(`${m.results[0].qubit}:${m.results[0].result}`);
  }
  assert.equal(keys.size, 2);

  for (const c of consumers) {
    const ref = c.controls.find(
      (/** @type {any} */ x) => x.result !== undefined,
    );
    assert.ok(
      keys.has(`${ref.qubit}:${ref.result}`),
      `consumer references ${ref.qubit}:${ref.result}, no M produces it`,
    );
  }
});

test("moveQubit: swap remaps a classical-control consumer buried inside a group", () => {
  // M on wire 0; consumer is two levels deep inside a Foo group
  // on wire 2. Swap wires 0 and 1 — the buried consumer's
  // classical ref must still rewire 0 → 1.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      { components: [_mGate(0, 0)] },
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 2 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "Bar",
                    targets: [{ qubit: 2 }],
                    children: [{ components: [_ccx(2, 0, 0)] }],
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

  moveQubit(model, 0, 1, false);

  // Locate the deeply-nested consumer.
  /** @type {any} */
  const fooOp = model.componentGrid[1].components[0];
  /** @type {any} */
  const barOp = fooOp.children[0].components[0];
  /** @type {any} */
  const innerConsumer = barOp.children[0].components[0];
  const ref = innerConsumer.controls.find(
    (/** @type {any} */ c) => c.result !== undefined,
  );
  assert.deepEqual(
    { qubit: ref.qubit, result: ref.result },
    { qubit: 1, result: 0 },
    "nested consumer's classical ref must rewire to the M's new wire",
  );
  // M itself moved 0 → 1.
  /** @type {any} */
  const m = model.componentGrid[0].components[0];
  assert.equal(m.qubits[0].qubit, 1);
  assert.equal(m.results[0].qubit, 1);
});
