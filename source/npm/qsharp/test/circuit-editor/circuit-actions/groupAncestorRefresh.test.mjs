// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Ancestor `.targets` cache refresh after a child mutation: when
// a group's children change shape (via `addOperation`,
// `removeOperation`, `addControl`, `removeControl`, or
// `moveOperation`), every ancestor's eager `.targets` is
// re-derived bottom-up in canonical `(qubit, result)` order that
// the renderer (`_splitTargetsY`, `_unitary` box geometry) depends on.

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  addControl,
  addOperation,
  moveOperation,
  removeControl,
  removeOperation,
} from "../../../dist/ux/circuit-vis/actions/circuitActions.js";
import {
  assertEnclosesWires,
  at,
  build,
  circuit,
  expectOp,
  gate,
  group,
  meas,
  topShape,
  wires,
} from "./_helpers.mjs";

// ---------------------------------------------------------------------------
// addOperation / removeOperation: ancestor refresh.
//
// Both paths mutate a group's children, so the group's eager
// `.targets` cache must be refreshed afterwards.
// ---------------------------------------------------------------------------

test("addOperation: adding a child to a group on a wire outside its span extends the group's targets", () => {
  const model = build(circuit(3, [[group("Foo", [[gate("H", 0)]])]]));

  // add Y into Foo's trailing inner slot on q2 (outside its span)
  const added = addOperation(model, gate("Y", 0), "0,0-1,2", 2);
  assert.ok(added, "addOperation should return the new op");

  assertEnclosesWires(at(model, "0,0"), 2);
});

test("addOperation: cascade — adding deep into a nested group extends both inner and outer ancestors", () => {
  const model = build(
    circuit(3, [[group("Outer", [[group("Inner", [[gate("H", 0)]])]])]]),
  );

  // add Y deep inside Inner on q2 (outside both spans)
  const added = addOperation(model, gate("Y", 0), "0,0-0,0-1,2", 2);
  assert.ok(added);

  assertEnclosesWires(at(model, "0,0-0,0"), 2);
  assertEnclosesWires(at(model, "0,0"), 2);
});

test("removeOperation: removing the only child on a wire narrows the group's targets", () => {
  const model = build(
    circuit(2, [[group("Foo", [[gate("H", 0)], [gate("Y", 1)]])]]),
  );

  // remove Y@q1 — Foo's span must shrink to just [0]
  removeOperation(model, "0,0-1,0");

  expectOp(at(model, "0,0"), { Foo: { targets: [0] } });
});

test("removeOperation: cascade — removing a deep child narrows nested ancestors", () => {
  const model = build(
    circuit(3, [
      [
        group("Outer", [
          [group("Inner", [[gate("H", 0)], [gate("X", 1)], [gate("Y", 2)]])],
        ]),
      ],
    ]),
  );

  // remove Y@q2 — Inner and Outer both narrow to [0, 1]
  removeOperation(model, "0,0-0,0-2,0");

  expectOp(at(model, "0,0"), { Outer: { targets: [0, 1] } });
  expectOp(at(model, "0,0-0,0"), { Inner: { targets: [0, 1] } });
});

// ---------------------------------------------------------------------------
// addControl / removeControl: ancestor refresh.
//
// Adding/removing a control widens or narrows the op's wire span,
// which must propagate into every ancestor's `.targets` cache.
// ---------------------------------------------------------------------------

test("addControl: adding a control to a child op on a wire outside the group's span extends the group's targets", () => {
  const model = build(circuit(3, [[group("Foo", [[gate("H", 0)]])]]));

  // control H @ q2 (outside Foo's span)
  const hOp = at(model, "0,0-0,0");
  const added = addControl(model, hOp, 2);
  assert.ok(added, "addControl should return true on a fresh wire");

  assertEnclosesWires(at(model, "0,0"), 2);
});

test("addControl: cascade — adding a control deep inside a nested group extends both ancestors", () => {
  const model = build(
    circuit(3, [[group("Outer", [[group("Inner", [[gate("H", 0)]])]])]]),
  );

  // control the deepest H @ q2 — Inner and Outer both extend
  addControl(model, at(model, "0,0-0,0-0,0"), 2);

  assertEnclosesWires(at(model, "0,0-0,0"), 2);
  assertEnclosesWires(at(model, "0,0"), 2);
});

test("removeControl: removing the only control extending a group's span narrows the group's targets", () => {
  const model = build(
    circuit(3, [[group("Foo", [[gate("H", 0, { ctrls: [2] })]])]]),
  );

  // remove H's q2 control — the only thing reaching q2 inside Foo
  const hOp = at(model, "0,0-0,0");
  const removed = removeControl(model, hOp, 2);
  assert.ok(removed, "removeControl should return true when control existed");

  expectOp(at(model, "0,0"), { Foo: { targets: [0] } });
});

// ---------------------------------------------------------------------------
// moveOperation: ancestor refresh.
//
// `moveOperation` re-derives each destination ancestor's `.targets`
// from its post-move children. The target location string is
// authoritative: dropping into group G makes G's `.targets` reflect
// that, even when the drop wire was outside G's pre-move span. The
// cascade walks innermost-out and stops at the first ancestor that
// already encloses the widened child below it.
// ---------------------------------------------------------------------------

test("moveOperation extend: shift-drop onto a wire just outside group's span extends the group's targets", () => {
  // With H now Foo's only child, Foo re-derives to [2]; the point is
  // that q2 (outside the old span 0-1) is enclosed.
  const model = build(circuit(3, [[group("Foo", [[gate("H", 0)]])]]));

  // shift-extend H from q0 → q2
  const moved = moveOperation(
    model,
    /* sourceLocation */ "0,0-0,0",
    /* targetLocation */ "0,0-0,0",
    /* sourceWire */ 0,
    /* targetWire */ 2,
    /* movingControl */ false,
    /* insertNewColumn */ false,
  );
  assert.ok(moved, "extend move should return the moved op");

  const fooOp = at(model, "0,0");
  assertEnclosesWires(fooOp, 2);
  expectOp(fooOp, { Foo: { children: [[{ H: 2 }]] } });
});

test("moveOperation extend: shift-drop several wires past the span extends across the gap", () => {
  // A non-contiguous span is unrepresentable; .targets is a set
  // whose min/max define the rendered span, so it must reach q4.
  const model = build(circuit(5, [[group("Foo", [[gate("H", 0)]])]]));

  // shift-extend H from q0 → q4 (two-wire gap)
  const moved = moveOperation(model, "0,0-0,0", "0,0-0,0", 0, 4, false, false);
  assert.ok(moved);

  assertEnclosesWires(at(model, "0,0"), 4);
});

test("moveOperation extend: multi-wire source extends to cover its new top wire", () => {
  // Grabbing CNOT by its target (q1) and dropping on q2 slides
  // control 0→1 and target 1→2; Foo's new max must reach q2.
  const model = build(
    circuit(4, [[group("Foo", [[gate("X", 1, { ctrls: [0] })]])]]),
  );

  // shift-extend CNOT, grabbed by target q1, dropped on q2 (delta=1)
  const moved = moveOperation(
    model,
    /* sourceLocation */ "0,0-0,0",
    /* targetLocation */ "0,0-0,0",
    /* sourceWire */ 1,
    /* targetWire */ 2,
    /* movingControl */ false,
    /* insertNewColumn */ false,
  );
  assert.ok(moved);

  const fooWires = wires(at(model, "0,0"));
  assert.ok(
    Math.max(...fooWires) >= 2,
    `Foo's span must extend to at least wire 2; got ${JSON.stringify(fooWires)}`,
  );
});

test("moveOperation extend: cascade refreshes nested ancestors whose span is now exceeded", () => {
  // Cascade: Inner extends to enclose q2, then Outer (no longer
  // enclosing Inner's new span) extends too.
  const model = build(
    circuit(3, [[group("Outer", [[group("Inner", [[gate("H", 0)]])]])]]),
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

  assertEnclosesWires(at(model, "0,0-0,0"), 2);
  assertEnclosesWires(at(model, "0,0"), 2);
});

test("moveOperation extend: cascade stops at first ancestor that already encloses the child", () => {
  // Outer spans 0-3 (padding P0@q0, P3@q3); Inner spans 1-2. Dropping
  // H onto q0 is inside Outer but outside Inner: Inner extends, Outer
  // is already wide enough and the cascade early-exits.
  const model = build(
    circuit(4, [
      [
        group("Outer", [
          [gate("P0", 0)],
          [group("Inner", [[gate("H", 1)]])],
          [gate("P3", 3)],
        ]),
      ],
    ]),
  );

  // shift-extend H from q1 → q0 (inside Outer, outside Inner)
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

  assertEnclosesWires(at(model, "0,0-1,0"), 0);
  // Outer still anchored by P0 and P3 — cascade didn't need to widen it.
  assertEnclosesWires(at(model, "0,0"), 0, 3);
});

test("moveOperation extend: dest cascade is a no-op when dest is top-level, even when source-side prune empties the source group", () => {
  // Moving Foo's only child to top level empties Foo: the source-side
  // prune removes Foo while the dest-side cascade (top-level dest)
  // is a no-op. Confirms the two halves don't interfere.
  const model = build(
    circuit(3, [[group("Foo", [[gate("H", 0)]])], [gate("Y", 2)]]),
  );

  // move H from inside Foo to top-level "1,1" (q2), emptying Foo
  const moved = moveOperation(model, "0,0-0,0", "1,1", 0, 2, false, true);
  assert.ok(moved, "move must succeed when dest is top-level");

  const top = topShape(model).flat();
  assert.ok(!top.includes("Foo"), "Foo must be pruned after last child leaves");
  assert.ok(top.includes("H"), "H must land at top level");
  assert.ok(top.includes("Y"), "Y must remain at top level");
});

test("moveOperation extend: external source dropped into group on off-span wire extends the group", () => {
  // Cross-chain move: source lives OUTSIDE Foo, so the source-side
  // refresh acts on H's old top-level ancestors. The dest-side
  // cascade is the only thing keeping Foo's `.targets` honest here.
  const model = build(
    circuit(3, [[gate("H", 2)], [group("Foo", [[gate("X", 0)]])]]),
  );

  // move top-level H@q2 into Foo's trailing inner col on q2 (off-span)
  const moved = moveOperation(
    model,
    /* sourceLocation */ "0,0",
    /* targetLocation */ "1,0-1,0",
    /* sourceWire */ 2,
    /* targetWire */ 2,
    /* movingControl */ false,
    /* insertNewColumn */ false,
  );
  assert.ok(moved, "move must succeed");

  // Top-level col 0 (only had H) is now empty and pruned, so Foo
  // lands at top-level "0,0".
  assertEnclosesWires(at(model, "0,0"), 2);
});

// ---------------------------------------------------------------------------
// Canonical target-order invariant.
//
// Refreshed group targets must be in canonical `(qubit, result)`
// order — qubit-only refs before their classical-result siblings —
// regardless of child iteration order. Renderer consumers
// (`_splitTargetsY`, `_unitary` box geometry) depend on this.
// ---------------------------------------------------------------------------

test("ancestor refresh: produces canonical (qubit, result) target order regardless of child-iteration order", () => {
  // Child-iteration visits refs as [q2, c2.0, q0]; the refresh must
  // re-sort to canonical (qubit index first; qubit-only before
  // result-bearing for the same qubit).
  const model = build(
    circuit(3, [
      [
        group("Foo", [
          [meas(2)],
          [gate("H", 0, { ctrls: [{ q: 2, r: 0 }], conditional: true })],
        ]),
      ],
    ]),
  );

  // control H @ q1 — triggers an ancestor refresh
  addControl(model, at(model, "0,0-1,0"), 1);

  const keys = at(model, "0,0").targets.map((/** @type {any} */ r) =>
    r.result === undefined ? `q${r.qubit}` : `c${r.qubit}.${r.result}`,
  );

  assert.deepEqual(
    keys,
    ["q0", "q1", "q2", "c2.0"],
    `Foo.targets must be canonically sorted (qubit, result); got ${JSON.stringify(keys)}`,
  );
});
