// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Group collision-split tests.
//
// When an op's `.targets` grow (shift-extend during a move, or
// `addControl` widening), the action layer's `_resolveSpanChange`
// checks the op against its column siblings and propagates up
// through every ancestor. If the widened drawn span overlaps a
// sibling, the column splits: the widened op gets a fresh column
// at its current index; surviving siblings shift one slot right.
// The flat base case lives in `circuit-actions/addRemove.test.mjs`.

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  addControl,
  moveOperation,
} from "../../../dist/ux/circuit-vis/actions/circuitActions.js";
import {
  at,
  build,
  circuit,
  expectGrid,
  expectOp,
  gate,
  group,
  meas,
} from "../_helpers.mjs";

// ---------------------------------------------------------------------------
// Collision-split when extending a group's span overlaps a sibling.
// ---------------------------------------------------------------------------

test("moveOperation extend: extending across a column-sibling splits the column, group shifts left", () => {
  // Y@0 pins Foo's low end so shift-dropping H from q0→q2 widens
  // Foo to [0, 2], overlapping Z@1.
  const model = build(
    circuit(3, [[group("Foo", [[gate("Y", 0), gate("H", 1)]]), gate("Z", 2)]]),
  );

  // shift-extend H from q0 → q2
  const moved = moveOperation(model, "0,0-0,1", "0,0-0,1", 0, 3, false, false);
  assert.ok(moved);

  expectGrid(model, [[{ Foo: { targets: [0, 3] } }], [{ Z: 2 }]]);
});

test("moveOperation extend: extending-move the only child doesn't split column", () => {
  const model = build(
    circuit(2, [[group("Foo", [[gate("H", 0)]]), gate("Z", 1)]]),
  );

  // shift-extend the lone H from q0 → q2
  const moved = moveOperation(model, "0,0-0,0", "0,0-0,0", 0, 2, false, false);
  assert.ok(moved);

  expectGrid(model, [[{ Foo: { targets: [2] } }, { Z: 1 }]]);
});

test("moveOperation extend: extending without collision does NOT split the column", () => {
  // Sibling Z@3 sits OUTSIDE Foo's post-extend span [0, 2] — no split.
  const model = build(
    circuit(4, [[group("Foo", [[gate("H", 0), gate("Y", 1)]]), gate("Z", 3)]]),
  );

  // shift-extend H from q0 → q2 (stays clear of Z@3)
  const moved = moveOperation(model, "0,0-0,0", "0,0-0,0", 0, 2, false, false);
  assert.ok(moved);

  expectGrid(model, [
    [{ Foo: { targets: [1, 2], children: [[{ Y: 1 }, { H: 2 }]] } }, { Z: 3 }],
  ]);
});

test("moveOperation extend: multiple column-siblings all survive the split", () => {
  // X@0 pins Foo's low end; shift-dropping H to q4 widens Foo to
  // [0, 4], overlapping both Y@2 and Z@3.
  const model = build(
    circuit(5, [
      [
        group("Foo", [[gate("X", 0)], [gate("H", 0)]]),
        gate("Y", 2),
        gate("Z", 3),
      ],
    ]),
  );

  // shift-extend H from q0 → q4
  const moved = moveOperation(model, "0,0-1,0", "0,0-1,0", 0, 4, false, false);
  assert.ok(moved);

  // Y and Z preserved in their original relative order.
  expectGrid(model, [["Foo"], ["Y", "Z"]]);
});

test("moveOperation extend: nested ancestor splits its own containing column on cascade", () => {
  // Inner's two children both pin q0; shift-dropping H to q2 widens
  // Inner to [0, 2], overlapping Z@1 inside Outer's child grid. The
  // split happens at the inner level. Outer's span stays [0, 2]
  // (S@2 still pins its high end), so the cascade bubbles up but
  // finds nothing to split at the top level.
  const model = build(
    circuit(3, [
      [
        group("Outer", [
          [group("Inner", [[gate("H", 0)], [gate("Y", 0)]]), gate("Z", 1)],
          [gate("S", 2)],
        ]),
      ],
    ]),
  );

  // shift-extend H (deep inside Inner) from q0 → q2
  const moved = moveOperation(
    model,
    "0,0-0,0-0,0",
    "0,0-0,0-0,0",
    0,
    2,
    false,
    false,
  );
  assert.ok(moved);

  expectGrid(model, [["Outer"]]);
  expectOp(at(model, "0,0"), {
    Outer: { children: [["Inner"], ["Z"], ["S"]] },
  });
});

// ---------------------------------------------------------------
// Shift-extend cross-over cases.
//
// When a group is shift-extended past an external sibling on an
// in-between wire, the cascade splits the outer column so the
// sibling slides one column right. These tests pin the case where
// the in-between sibling is itself a multi-wire op.
// ---------------------------------------------------------------

test("moveOperation extend: cross-over a GROUP sibling splits the column, leaving both groups intact", () => {
  // Foo[0,1] and Bar[3,4] sit side-by-side in one outer column.
  // X@1 pins Foo's low end, so moving H to q4 widens Foo to [1, 4]
  // (still one outer column, just more wires), overlapping Bar.
  const model = build(
    circuit(5, [
      [
        group("Foo", [[gate("H", 0), gate("X", 1)]]),
        group("Bar", [[gate("Y", 3), gate("Z", 4)]]),
      ],
    ]),
  );

  // shift-extend H from q0 → q4 (into Foo's trailing inner column)
  const moved = moveOperation(model, "0,0-0,0", "0,0-1,0", 0, 4, false, false);
  assert.ok(moved);

  expectGrid(model, [
    [{ Foo: { targets: [1, 4] } }],
    [{ Bar: { children: [[{ Y: 3 }, { Z: 4 }]] } }],
  ]);
});

test("moveOperation extend: cross-over a sibling on an IN-BETWEEN wire (drop wire is clear past it)", () => {
  // X@0 pins Foo's low end; shift-dropping H past Z@3 onto a clear
  // wire q4 widens Foo to [0, 4], catching Z even though the drop
  // wire itself is clear.
  const model = build(
    circuit(5, [[group("Foo", [[gate("X", 0), gate("H", 1)]]), gate("Z", 3)]]),
  );

  // shift-extend H from q1 → q4 (past Z@3)
  const moved = moveOperation(model, "0,0-0,1", "0,0-1,0", 1, 4, false, false);
  assert.ok(moved);

  // Z stays on wire 3 — the resolver shifts COLUMNS, not WIRES.
  expectGrid(model, [[{ Foo: { targets: [0, 4] } }], [{ Z: 3 }]]);
});

test("moveOperation extend: deeply-nested source past a multi-wire ancestor sibling splits at the top ancestor", () => {
  // 3-deep nesting (Outer > Middle > Foo) with Sib[3,4] a sibling of
  // Outer. X@0 pins Foo's low end; shift-dropping H to q5 widens
  // Foo/Middle/Outer all the way up, so Outer's [0, 5] overlaps Sib.
  // The dest-side cascade must keep walking past unchanged rungs to
  // resolve the collision at the topmost ancestor.
  const model = build(
    circuit(6, [
      [
        group("Outer", [
          [group("Middle", [[group("Foo", [[gate("X", 0), gate("H", 1)]])]])],
        ]),
        group("Sib", [[gate("Y", 3), gate("Z", 4)]]),
      ],
    ]),
  );

  // shift-extend H (deepest leaf) from q1 → q5 (past Sib[3,4])
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

  expectGrid(model, [
    [{ Outer: { targets: [0, 5] } }],
    [{ Sib: { children: [[{ Y: 3 }, { Z: 4 }]] } }],
  ]);
});

// ---------------------------------------------------------------
// Centralized post-widening cleanup.
//
// Any path that widens an op's `.targets` / `.controls` must
// trigger the split-and-shift via `_resolveSpanChange`. These
// cover the `addControl` widening path, group-internal and
// group-via-ancestor.
// ---------------------------------------------------------------

test("addControl: nested widening into a same-column sibling inside a group splits inside the group", () => {
  // Adding a control on q3 to H widens it to q0..q3, overlapping Z@3
  // inside Foo's child grid.
  const model = build(
    circuit(4, [[group("Foo", [[gate("H", 0), gate("Z", 3)]])]]),
  );
  // control H @ q3
  const hOp = at(model, "0,0-0,0");
  assert.ok(addControl(model, hOp, 3));

  expectOp(at(model, "0,0"), { Foo: { children: [["H"], ["Z"]] } });
});

test("addControl: widening that pushes the OUTER GROUP into its top-level sibling also splits the top-level column", () => {
  // Control on q3 widens H to q0..q3 → Foo's cache widens → Foo
  // overlaps X@3 at the top level.
  const model = build(
    circuit(4, [[group("Foo", [[gate("H", 0)]]), gate("X", 3)]]),
  );
  // control H @ q3
  const hOp = at(model, "0,0-0,0");
  addControl(model, hOp, 3);

  expectGrid(model, [[{ Foo: { targets: [0, 3] } }], ["X"]]);
});

// ---------------------------------------------------------------
// Overlap-collision check uses the drawn span of siblings.
//
// `getMinMaxRegIdx` includes classical-control connector wires, so
// a sibling whose target is on a high wire but whose classical
// control points at a low-wire measurement visually occupies every
// wire between them. A widened op intersecting that connector
// collides even when it misses the quantum target.
// ---------------------------------------------------------------

test("addControl widening: sibling with classical control on a LOW wire (drawn-span overlap) triggers split even when quantum target is clear", () => {
  // X's quantum span is [q3]; its drawn span is [q1, q3] (connector
  // down to the M@q1 producer). Widening Foo to q0..q2 misses q3 but
  // crosses X's connector.
  const model = build(
    circuit(5, [
      [meas(1)],
      [
        group("Foo", [[gate("H", 0)]]),
        gate("X", 3, { ctrls: [{ q: 1, r: 0 }] }),
      ],
    ]),
  );
  // control H @ q2
  const hOp = at(model, "1,0-0,0");
  addControl(model, hOp, 2);

  // Foo's targets are exactly [0, 2] (NOT q3), so the split was driven
  // ONLY by the drawn-span collision with X's classical connector.
  expectGrid(model, [["M"], [{ Foo: { targets: [0, 2] } }], ["X"]]);
});

test("moveOperation shift-extend: cross-over a sibling whose drawn span includes a classical-control wire", () => {
  // Same drawn-span vs quantum-span distinction, via shift-extend.
  // Foo widens to q0..q2, crossing X's classical connector (q1..q3)
  // without touching X's quantum target q3.
  const model = build(
    circuit(5, [
      [meas(1)],
      [
        group("Foo", [[gate("H", 0)]]),
        gate("X", 3, { ctrls: [{ q: 1, r: 0 }] }),
      ],
    ]),
  );

  // shift-extend H from q0 → q2
  const moved = moveOperation(model, "1,0-0,0", "1,0-1,0", 0, 2, false, false);
  assert.ok(moved, "moveOperation must succeed");

  expectGrid(model, [["M"], [{ Foo: { targets: [2] } }], ["X"]]);
});

test("no false split: widening from ABOVE that lands just short of a classically-controlled sibling's drawn span stays put", () => {
  // The classical row q1.r0 sits BETWEEN q1 and q2 (row order:
  // q0, q1, q1.r0, q2). X's drawn span is rows q1.r0..q2 (connector
  // up from q2, never reaching q1). Widening Z to rows q0..q1 lands
  // strictly ABOVE q1.r0 → no overlap → no split.
  const model = build(
    circuit(3, [
      [meas(1)],
      [gate("Z", 0), gate("X", 2, { ctrls: [{ q: 1, r: 0 }] })],
    ]),
  );
  // control Z @ q1
  const zOp = at(model, "1,0");
  addControl(model, zOp, 1);

  expectGrid(model, [["M"], ["X", "Z"]]);
});

test("split: widening from BELOW that reaches into a classically-controlled sibling's drawn span splits the column", () => {
  // Mirror of the previous test, other direction. X is ABOVE the
  // measurement, reaching DOWN to q1.r0; its drawn span is rows
  // q0..q1.r0. Widening Z to rows q1..q2 includes q1.r0 (between q1
  // and q2) → overlap → split. The widened op (Z) gets the fresh
  // column; X shifts one column right.
  const model = build(
    circuit(5, [
      [meas(1)],
      [gate("X", 0, { ctrls: [{ q: 1, r: 0 }] }), gate("Z", 2)],
    ]),
  );
  // control Z @ q1
  const zOp = at(model, "1,1");
  addControl(model, zOp, 1);

  expectGrid(model, [["M"], [{ Z: { targets: [2], ctrls: [1] } }], ["X"]]);
});

// ---------------------------------------------------------------
// Ordinary (non-shift-extend) move into a sibling-occupied column.
//
// Same `_resolveSpanChange` chokepoint, for source shapes the
// other tests don't cover: a CONTROLLED gate (control leg drives
// the collision) and a MULTI-TARGET gate (SWAP).
// ---------------------------------------------------------------

test("moveOperation: moving a CONTROLLED gate into a sibling-occupied column splits the column", () => {
  // CNOT's span [q0, q3] envelops Y@q1 in the destination column.
  const model = build(
    circuit(4, [
      [gate("CNOT", 0, { ctrls: [2] })],
      [gate("S", 3)],
      [gate("Y", 1)],
    ]),
  );

  // move CNOT into Y's column (q0)
  const moved = moveOperation(model, "0,0", "2,0", 0, 0, false, false);
  assert.ok(moved);

  expectGrid(model, [
    [{ S: 3 }],
    [{ CNOT: { targets: [0], ctrls: [2] } }],
    [{ Y: 1 }],
  ]);
});

test("moveOperation: moving a MULTI-TARGET gate (SWAP) into a sibling-occupied column splits the column", () => {
  // SWAP moves as a unit; its span [q0, q2] envelops Y@q1 in the
  // destination column.
  const model = build(
    circuit(4, [[gate("SWAP", [0, 2])], [gate("S", 3)], [gate("Y", 1)]]),
  );

  // move SWAP into Y's column
  const moved = moveOperation(model, "0,0", "2,0", 0, 0, false, false);
  assert.ok(moved);

  expectGrid(model, [[{ S: 3 }], [{ SWAP: [0, 2] }], [{ Y: 1 }]]);
});
