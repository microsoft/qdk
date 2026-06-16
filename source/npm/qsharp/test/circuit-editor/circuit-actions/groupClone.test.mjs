// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// addOperation: clone-copy of a group preserves shape.
//
// Ctrl-drag (clone) of a multi-wire op routes through the same
// rigid-shift path as `moveOperation`'s `_moveAsUnit`: every
// register in the cloned subtree shifts by the same
// `targetWire - sourceWire` delta, keeping `.targets` and every
// nested child wire aligned.

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import { addOperation } from "../../../dist/ux/circuit-vis/actions/circuitActions.js";
import {
  at,
  build,
  circuit,
  expectOp,
  gate,
  group,
  meas,
  qubits,
} from "./_helpers.mjs";

test("addOperation: clone-copy of a group with delta>0 shifts every nested register", () => {
  const model = build(
    circuit(4, [[group("Foo", [[gate("H", 0), gate("X", 1)]])]]),
  );
  const sourceFoo = at(model, "0,0");

  // clone Foo, grab on q0, drop on q2 (delta = +2)
  const cloned = addOperation(
    model,
    sourceFoo,
    "1,0",
    /* targetWire */ 2,
    /* insertNewColumn */ false,
    /* sourceWire */ 0,
  );

  assert.ok(cloned, "clone returned an op");
  expectOp(cloned, {
    Foo: { targets: [2, 3], children: [[{ H: 2 }, { X: 3 }]] },
  });
  // Original Foo is untouched (clone, not move).
  expectOp(at(model, "0,0"), {
    Foo: { targets: [0, 1], children: [[{ H: 0 }, { X: 1 }]] },
  });
});

test("addOperation: clone-copy of a group with delta=0 preserves all children on their original wires", () => {
  const model = build(
    circuit(2, [[group("Foo", [[gate("H", 0), gate("X", 1)]])]]),
  );
  const sourceFoo = at(model, "0,0");

  // clone Foo, grab on q0, drop on q0 (delta = 0, different column)
  const cloned = addOperation(
    model,
    sourceFoo,
    "1,0",
    /* targetWire */ 0,
    /* insertNewColumn */ false,
    /* sourceWire */ 0,
  );

  assert.ok(cloned, "clone returned an op");
  expectOp(cloned, {
    Foo: { targets: [0, 1], children: [[{ H: 0 }, { X: 1 }]] },
  });
});

test("addOperation: clone-copy of a multi-target gate preserves every leg", () => {
  // The clone path must rigid-shift every leg by the same delta —
  // collapsing `targets` to a single-wire stub would destroy a leg.
  const model = build(circuit(4, [[gate("SWAP", [0, 1])]]));
  const sourceSwap = at(model, "0,0");

  // clone SWAP, grab on q0, drop on q2 (delta = +2)
  const cloned = addOperation(
    model,
    sourceSwap,
    "1,0",
    /* targetWire */ 2,
    /* insertNewColumn */ false,
    /* sourceWire */ 0,
  );

  assert.ok(cloned, "clone returned an op");
  expectOp(cloned, { SWAP: [2, 3] });
});

test("addOperation: clone-copy of a group containing an internal classical control shifts the classical ref in lockstep", () => {
  // The cloned conditional H must read the CLONED measurement's
  // classical register (c_2.0), not the original's (c_0.0).
  const model = build(
    circuit(4, [
      [
        group("Foo", [
          [meas(0)],
          [gate("H", 1, { ctrls: [{ q: 0, r: 0 }], conditional: true })],
        ]),
      ],
    ]),
  );
  const sourceFoo = at(model, "0,0");

  // clone Foo, grab on q0, drop on q2 (delta = +2)
  const cloned = addOperation(
    model,
    sourceFoo,
    "1,0",
    /* targetWire */ 2,
    /* insertNewColumn */ false,
    /* sourceWire */ 0,
  );

  assert.ok(cloned, "clone returned an op");
  expectOp(cloned, {
    Foo: {
      children: [
        [{ M: { qubits: [2], results: [{ q: 2, r: 0 }] } }],
        [{ H: { targets: [3], ctrls: [{ q: 2, r: 0 }], conditional: true } }],
      ],
    },
  });
});

test("addOperation: clone-copy of a classically-controlled op anchors its classical ref when the producer M is not cloned", () => {
  // Mirror of the lockstep test, but the producing M lives OUTSIDE
  // the cloned op. Foo reads c_0.0 produced by an external M. Cloning
  // Foo with a wire delta must shift its quantum legs but ANCHOR the
  // classical ref at q0.r0 — the original producer still owns it.
  const model = build(
    circuit(qubits(5, { 0: 1 }), [
      [meas(0)],
      [gate("Foo", [1, 2], { ctrls: [{ q: 0, r: 0 }], conditional: true })],
    ]),
  );
  const sourceFoo = at(model, "1,0");

  // clone Foo, grab on q1, drop on q3 (delta = +2)
  const cloned = addOperation(
    model,
    sourceFoo,
    "2,0",
    /* targetWire */ 3,
    /* insertNewColumn */ false,
    /* sourceWire */ 1,
  );

  assert.ok(cloned, "clone returned an op");
  // Quantum legs shifted q1→q3, q2→q4; classical ctrl anchored at q0.r0.
  expectOp(cloned, {
    Foo: { targets: [3, 4], ctrls: [{ q: 0, r: 0 }], conditional: true },
  });
  // Original Foo and the external M are untouched.
  expectOp(at(model, "1,0"), {
    Foo: { targets: [1, 2], ctrls: [{ q: 0, r: 0 }], conditional: true },
  });
  expectOp(at(model, "0,0"), { M: { qubits: [0], results: [{ q: 0, r: 0 }] } });
});

test("addOperation: clone-copy of a group anchors an internal child's classical ref when the producer M is outside the group", () => {
  // The classically-dependent op is INSIDE the cloned group, but the
  // producing M is OUTSIDE it (not cloned). Cloning the group with a
  // wire delta shifts the child's quantum target but anchors its
  // classical ref at q0.r0 — the external producer still owns it.
  const model = build(
    circuit(qubits(5, { 0: 1 }), [
      [meas(0)],
      [
        group("Foo", [
          [gate("X", 1, { ctrls: [{ q: 0, r: 0 }], conditional: true })],
        ]),
      ],
    ]),
  );
  const sourceFoo = at(model, "1,0");

  // clone Foo, grab on q1, drop on q3 (delta = +2)
  const cloned = addOperation(
    model,
    sourceFoo,
    "2,0",
    /* targetWire */ 3,
    /* insertNewColumn */ false,
    /* sourceWire */ 1,
  );

  assert.ok(cloned, "clone returned an op");
  // Child X's target shifted q1→q3; classical ctrl anchored at q0.r0.
  expectOp(cloned, {
    Foo: {
      children: [
        [{ X: { targets: [3], ctrls: [{ q: 0, r: 0 }], conditional: true } }],
      ],
    },
  });
  // Original Foo and the external M are untouched.
  expectOp(at(model, "1,0"), {
    Foo: {
      children: [
        [{ X: { targets: [1], ctrls: [{ q: 0, r: 0 }], conditional: true } }],
      ],
    },
  });
  expectOp(at(model, "0,0"), { M: { qubits: [0], results: [{ q: 0, r: 0 }] } });
});

test("addOperation: clone-copy that would push a wire below 0 returns null", () => {
  // Grabbing Foo (wires 1-2) on q1 and dropping at q-1 computes
  // delta = -2, underflowing wire 1 → -1. Returns null (no-op).
  const model = build(
    circuit(3, [[group("Foo", [[gate("H", 1), gate("X", 2)]])]]),
  );
  const sourceFoo = at(model, "0,0");
  const before = JSON.stringify(model.componentGrid);

  // clone Foo, grab on q1, drop on q-1 (delta = -2, underflows)
  const result = addOperation(
    model,
    sourceFoo,
    "1,0",
    /* targetWire */ -1,
    /* insertNewColumn */ false,
    /* sourceWire */ 1,
  );

  assert.equal(result, null, "expected null when shift would underflow");
  assert.equal(JSON.stringify(model.componentGrid), before);
});
