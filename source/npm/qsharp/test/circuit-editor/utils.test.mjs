// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Tests for the lower-level helpers in `utils.ts` — the ones that can be exercised without a
// `CircuitModel` or a rendered SVG tree. Most are pure data (`pickClosestWireIndex`,
// `getChildTargets`, the column-sibling helpers); `parseWireYs` just reads an attribute off an
// Element, so a minimal JSDOM is spun up for those few tests. The heavier paths that walk the
// rendered SVG (host-element lookup, wire-Y resolution from a real circuit DOM) live in the
// controller-level suites where a full render fixture exists.

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import {
  getChildTargets,
  getAncestorColumnSiblingWires,
  getOuterColumnSiblingWires,
  getWireRange,
  pickClosestWireIndex,
} from "../../dist/ux/circuit-vis/utils.js";
import { parseWireYs } from "../../dist/ux/circuit-vis/editor/domUtils.js";

// ============================================================
// pickClosestWireIndex
// ============================================================

test("pickClosestWireIndex: degenerate inputs return the sentinel / first-match contract", () => {
  // Empty span -> -1.
  assert.equal(pickClosestWireIndex(0, [], [40, 100, 160]), -1);
  // Closest wireY has no matching entry in wireData -> -1.
  assert.equal(pickClosestWireIndex(50, [40, 100], [200, 300]), -1);
  // Duplicate Y in wireData resolves to the FIRST index (indexOf).
  assert.equal(pickClosestWireIndex(45, [40], [40, 40, 40]), 0);
});

test("pickClosestWireIndex: single-wire span ignores clickY", () => {
  // Single-wire host elements (control dots, target circles, ket boxes) trivially resolve to their
  // one wire-Y regardless of where the click landed.
  assert.equal(
    pickClosestWireIndex(999, [100], [40, 100, 160]),
    1,
    "clickY far below wire 100 must still resolve to that wire",
  );
  assert.equal(
    pickClosestWireIndex(-999, [40], [40, 100, 160]),
    0,
    "clickY far above wire 40 must still resolve to that wire",
  );
});

test("pickClosestWireIndex: multi-wire picks the closest by absolute distance", () => {
  const wireYs = [40, 100, 160];
  const wireData = [40, 100, 160];
  assert.equal(pickClosestWireIndex(45, wireYs, wireData), 0, "near top");
  assert.equal(pickClosestWireIndex(95, wireYs, wireData), 1, "near middle");
  assert.equal(pickClosestWireIndex(155, wireYs, wireData), 2, "near bottom");
  assert.equal(
    pickClosestWireIndex(70, wireYs, wireData),
    0,
    "exactly equidistant -> smaller wireY wins (deterministic)",
  );
  assert.equal(
    pickClosestWireIndex(72, wireYs, wireData),
    1,
    "tilt one px toward the middle wire -> middle wins",
  );
});

test("pickClosestWireIndex: clicks outside the span clamp to the nearest end", () => {
  const wireYs = [40, 100, 160];
  const wireData = [40, 100, 160];
  assert.equal(
    pickClosestWireIndex(-50, wireYs, wireData),
    0,
    "click far above clamps to topmost wire",
  );
  assert.equal(
    pickClosestWireIndex(500, wireYs, wireData),
    2,
    "click far below clamps to bottommost wire",
  );
});

test("pickClosestWireIndex: wireYs ordering does not affect the result", () => {
  const wireData = [40, 100, 160];
  // Span listed in non-sorted order — algorithm is comparison-based, not order-dependent.
  assert.equal(pickClosestWireIndex(45, [160, 40, 100], wireData), 0);
  assert.equal(pickClosestWireIndex(155, [100, 40, 160], wireData), 2);
});

// ============================================================
// parseWireYs
// ============================================================

/** @type {JSDOM | null} */
let jsdom = null;
beforeEach(() => {
  jsdom = new JSDOM(`<!doctype html><html><body></body></html>`);
  globalThis.document = jsdom.window.document;
});
afterEach(() => {
  jsdom?.window.close();
  jsdom = null;
});

const makeElem = (/** @type {string | null} */ attr) => {
  const el = document.createElementNS("http://www.w3.org/2000/svg", "rect");
  if (attr != null) el.setAttribute("data-wire-ys", attr);
  return el;
};

test("parseWireYs: absent or invalid attribute returns []", () => {
  // Missing attribute, malformed JSON, non-number entries (whole array rejected), and non-array
  // JSON all yield [].
  assert.deepEqual(parseWireYs(makeElem(null)), []);
  assert.deepEqual(parseWireYs(makeElem("not json")), []);
  assert.deepEqual(parseWireYs(makeElem('[40, "100", 160]')), []);
  assert.deepEqual(parseWireYs(makeElem("42")), []);
  assert.deepEqual(parseWireYs(makeElem('"40"')), []);
  assert.deepEqual(parseWireYs(makeElem("{}")), []);
});

test("parseWireYs: valid number-array round-trips", () => {
  assert.deepEqual(parseWireYs(makeElem("[40, 100, 160]")), [40, 100, 160]);
  assert.deepEqual(parseWireYs(makeElem("[40]")), [40]);
});

// ============================================================
// getChildTargets
// ============================================================

// Helper: wrap a list of children into the single-column shape `Operation.children` expects.
const group = (
  /** @type {string} */ gate,
  /** @type {any[]} */ targets,
  /** @type {any[]} */ children,
) =>
  /** @type {import("../../dist/ux/circuit-vis/index.js").Operation} */ ({
    kind: "unitary",
    gate,
    targets,
    children: [{ components: children }],
  });
const u = (
  /** @type {string} */ gate,
  /** @type {any[]} */ targets,
  /** @type {any[] | undefined} */ controls = undefined,
) => {
  /** @type {any} */
  const op = { kind: "unitary", gate, targets };
  if (controls != null) op.controls = controls;
  return /** @type {import("../../dist/ux/circuit-vis/index.js").Operation} */ (
    op
  );
};
const m = (/** @type {any[]} */ qubits, /** @type {any[]} */ results) =>
  /** @type {import("../../dist/ux/circuit-vis/index.js").Operation} */ ({
    kind: "measurement",
    gate: "Measure",
    qubits,
    results,
  });

test("getChildTargets: returns [] when op has no children", () => {
  // Leaf ops aren't groups; the action-layer cascade only calls `getChildTargets` on ops that have
  // a `children` grid. The `[]` return models that contract.
  const leaf = u("H", [{ qubit: 0 }]);
  assert.deepEqual(getChildTargets(leaf), []);
});

test("getChildTargets: dedupes overlapping bare-qubit refs", () => {
  // Foo contains H on wire 1 and RX on wires 1, 2. The union is {1, 2} — wire 1 must appear exactly
  // once, not twice.
  const foo = group(
    "Foo",
    [{ qubit: 1 }, { qubit: 2 }],
    [u("H", [{ qubit: 1 }]), u("RX", [{ qubit: 1 }, { qubit: 2 }])],
  );
  assert.deepEqual(getChildTargets(foo), [{ qubit: 1 }, { qubit: 2 }]);
});

test("getChildTargets: walks into nested groups", () => {
  // Wire union must cross group boundaries — the cascade refresh assigns `getChildTargets(outer)`
  // straight into `outer.targets`, and the outer span has to enclose every descendant wire no
  // matter how deep.
  const inner = group("Inner", [{ qubit: 2 }], [u("H", [{ qubit: 2 }])]);
  const outer = group(
    "Outer",
    [{ qubit: 0 }, { qubit: 1 }, { qubit: 2 }],
    [u("H", [{ qubit: 0 }]), inner],
  );
  assert.deepEqual(getChildTargets(outer), [{ qubit: 0 }, { qubit: 2 }]);
});

test("getChildTargets: preserves measurement result registers as distinct entries", () => {
  // A child measurement on wire 0 produces classical result 0; the measurement contributes BOTH
  // `{qubit:0}` (the quantum input, pushed from `operation.qubits`) AND `{qubit:0, result:0}` (the
  // classical output, pushed from `operation.results`). The dedup pass keys on `(qubit, result)` so
  // the two distinct registers survive as separate entries.
  const foo = group(
    "Foo",
    [{ qubit: 0 }],
    [m([{ qubit: 0 }], [{ qubit: 0, result: 0 }])],
  );
  const out = getChildTargets(foo);
  // Order: measurement.qubits comes before measurement.results in the recursion's push order, so
  // the bare-qubit entry comes first.
  assert.deepEqual(out, [{ qubit: 0 }, { qubit: 0, result: 0 }]);
});

test("getChildTargets: preserves classical-control refs from classically-conditional unitaries", () => {
  // Classically-conditional unitaries record their classical dependency in BOTH `controls` and
  // `targets` (the `targets` entries are visual-extent claims that draw the line down to the
  // classical register box — see `_shiftAllRegisters` in circuitActions.ts). If a group contains
  // such a unitary, the group's refreshed `.targets` MUST carry the classical ref through, or the
  // renderer drops the line.
  const cond = u(
    "X",
    [{ qubit: 1 }, { qubit: 0, result: 0 }],
    [{ qubit: 0, result: 0 }],
  );
  const foo = group("Foo", [{ qubit: 0 }, { qubit: 1 }], [cond]);
  const out = getChildTargets(foo);
  // The bare-qubit target `{qubit:1}` and the classical ref `{qubit:0, result:0}` are both present.
  // The classical ref appears once even though it was pushed twice (from `targets` and from
  // `controls`).
  assert.ok(
    out.some((r) => r.qubit === 1 && r.result === undefined),
    `expected bare-qubit {qubit:1}, got ${JSON.stringify(out)}`,
  );
  assert.ok(
    out.some((r) => r.qubit === 0 && r.result === 0),
    `expected classical-ref {qubit:0, result:0}, got ${JSON.stringify(out)}`,
  );
  assert.equal(
    out.filter((r) => r.qubit === 0 && r.result === 0).length,
    1,
    "classical ref should be deduped to a single entry",
  );
});

test("getChildTargets: returns fresh register objects, not aliases of child registers", () => {
  // Callers assign the returned array straight into `parent.targets` / `parent.results`. If the
  // entries aliased the child's own register objects, a later in-place edit on the child's register
  // (e.g. `_shiftAllRegisters` bumping `qubit`) would silently mutate the parent's cached extent
  // too.
  const childTargets = [{ qubit: 0 }];
  const foo = group("Foo", [{ qubit: 0 }], [u("H", childTargets)]);
  const out = getChildTargets(foo);
  assert.notEqual(
    out[0],
    childTargets[0],
    "returned register must be a fresh object, not a reference to the child's register",
  );
  // Belt-and-suspenders: mutate the returned entry and confirm the child's register is unchanged.
  out[0].qubit = 999;
  assert.equal(childTargets[0].qubit, 0, "child register must be untouched");
});

// ============================================================
// getWireRange
// ============================================================
//
// Vertical extent of an op as a pair of `Register` endpoints. Either endpoint may be a qubit row
// (no `.result`) or a classical-result row (`.result` set). Classical rows sit IMMEDIATELY BELOW
// their owning qubit row \u2014 the stack on a qubit `q_c` with results `r0..rN` reads, top to
// bottom: q_c, q_c.r0, q_c.r1, ..., q_c.rN, q_(c+1), ...

test("getWireRange: single-qubit unitary returns the qubit row at both endpoints", () => {
  const op = u("H", [{ qubit: 2 }]);
  assert.deepEqual(getWireRange(op), [{ qubit: 2 }, { qubit: 2 }]);
});

test("getWireRange: multi-qubit unitary spans min..max", () => {
  // SWAP-like op on q3 and q5 \u2014 endpoints are the outer qubits.
  const op = u("SWAP", [{ qubit: 3 }, { qubit: 5 }]);
  assert.deepEqual(getWireRange(op), [{ qubit: 3 }, { qubit: 5 }]);
});

test("getWireRange: classically-controlled gate with low classical ref \u2014 ref is the MIN endpoint", () => {
  // Z @ q3 cref q0r0 \u2014 the classical row sits below q0, so it's the lowest visual position.
  // The bare qubit q3 is the max.
  const op = u("Z", [{ qubit: 3 }], [{ qubit: 0, result: 0 }]);
  assert.deepEqual(getWireRange(op), [{ qubit: 0, result: 0 }, { qubit: 3 }]);
});

test("getWireRange: classically-controlled gate with high classical ref \u2014 ref is the MAX endpoint", () => {
  // Z @ q0 cref q3r0 \u2014 the classical row sits below q3, so it's the highest visual position.
  const op = u("Z", [{ qubit: 0 }], [{ qubit: 3, result: 0 }]);
  assert.deepEqual(getWireRange(op), [{ qubit: 0 }, { qubit: 3, result: 0 }]);
});

test("getWireRange: classical refs on the SAME qubit \u2014 lowest-numbered result is the topmost", () => {
  // Multiple classical refs to q0's result rows. r0 sits above r1 (lower-numbered results are drawn
  // topmost). So between q0.r0 and q0.r1, q0.r1 is geometrically lower.
  const op = u(
    "Z",
    [{ qubit: 5 }],
    [
      { qubit: 0, result: 0 },
      { qubit: 0, result: 1 },
    ],
  );
  // Max is q5 (a qubit row well below q0's classical rows). Min among the classical refs is r0 (it
  // sits above r1).
  assert.deepEqual(getWireRange(op), [{ qubit: 0, result: 0 }, { qubit: 5 }]);
});

test("getWireRange: bare qubit row sorts ABOVE its own classical-result rows", () => {
  // Measurement on q0 producing r0: endpoints are the bare q0 (top) and q0.r0 (immediately below
  // it).
  const op = m([{ qubit: 0 }], [{ qubit: 0, result: 0 }]);
  assert.deepEqual(getWireRange(op), [{ qubit: 0 }, { qubit: 0, result: 0 }]);
});

test("getWireRange: quantum control BELOW target \u2014 control is the MAX endpoint", () => {
  // CX with target on q0 and control on q3 \u2014 the control wire is the geometric bottom, the
  // target the top.
  const op = u("X", [{ qubit: 0 }], [{ qubit: 3 }]);
  assert.deepEqual(getWireRange(op), [{ qubit: 0 }, { qubit: 3 }]);
});

test("getWireRange: quantum control ABOVE target \u2014 control is the MIN endpoint", () => {
  // CX with target on q3 and control on q0 \u2014 the control wire is the geometric top, the target
  // the bottom.
  const op = u("X", [{ qubit: 3 }], [{ qubit: 0 }]);
  assert.deepEqual(getWireRange(op), [{ qubit: 0 }, { qubit: 3 }]);
});

test("getWireRange: multiple quantum controls bracketing the target", () => {
  // CCX-like with target on q3 and controls on q0 and q5 \u2014 endpoints are the outermost
  // controls, not the target.
  const op = u("X", [{ qubit: 3 }], [{ qubit: 0 }, { qubit: 5 }]);
  assert.deepEqual(getWireRange(op), [{ qubit: 0 }, { qubit: 5 }]);
});

test("getWireRange: mixed quantum + classical controls \u2014 each contributes its own row", () => {
  // X @ q3 with a quantum control on q1 and a classical ref to q5r0. Geometric stack top-to-bottom:
  // q1 (control), q3 (target), q5 (a quantum wire we cross), q5.r0 (classical row, BELOW q5). So
  // min = q1, max = q5.r0.
  const op = u("X", [{ qubit: 3 }], [{ qubit: 1 }, { qubit: 5, result: 0 }]);
  assert.deepEqual(getWireRange(op), [{ qubit: 1 }, { qubit: 5, result: 0 }]);
});

test("getWireRange: endpoints are fresh objects, not aliases of op's registers", () => {
  // Same hazard as `getChildTargets`: callers shouldn't be able to mutate the op's own register
  // state via the return value.
  const opTargets = [{ qubit: 3 }];
  const op = u("H", opTargets);
  const range = getWireRange(op);
  assert.ok(range);
  assert.notEqual(
    range[0],
    opTargets[0],
    "min endpoint must be a fresh object",
  );
  assert.notEqual(
    range[1],
    opTargets[0],
    "max endpoint must be a fresh object",
  );
  range[0].qubit = 999;
  assert.equal(opTargets[0].qubit, 3, "op's register must be untouched");
});

// ============================================================
// getOuterColumnSiblingWires
// ============================================================
//
// Used by the shift-extend dropzone filter to identify wires that an op cannot directly extend onto
// because an external sibling in the op's outer column already occupies them. The "cross-over" case
// (extending past an in-between sibling) is intentionally NOT covered here — that's a property of
// the action-layer overlap resolver and is tested in the circuit-actions/ suite
// (producerOrdering.test.mjs).

// Helper: build a single-component-grid from a component list.
const grid = (/** @type {any[][]} */ componentLists) =>
  componentLists.map((/** @type {any[]} */ components) => ({ components }));

test("getOuterColumnSiblingWires: null / empty / unresolvable location returns empty set", () => {
  const componentGrid = grid([[u("H", [{ qubit: 0 }])]]);
  assert.equal(getOuterColumnSiblingWires(componentGrid, null).size, 0);
  assert.equal(getOuterColumnSiblingWires(componentGrid, "").size, 0);
  // A location whose ancestor is out of bounds resolves to no parent array; the helper returns
  // empty rather than throwing.
  assert.equal(getOuterColumnSiblingWires(componentGrid, "5,0-0,0").size, 0);
});

test("getOuterColumnSiblingWires: op with no co-resident siblings returns empty set", () => {
  // Top-level op alone in its column — no siblings to enumerate.
  const componentGrid = grid([[u("Foo", [{ qubit: 0 }, { qubit: 1 }])]]);
  const blocked = getOuterColumnSiblingWires(componentGrid, "0,0");
  assert.equal(blocked.size, 0);
});

test("getOuterColumnSiblingWires: returns every wire an external sibling occupies", () => {
  // Column 0 holds Foo @ wires [0,1] alongside Z @ wire 3 and W @ wire 4 — both Z and W are
  // external siblings of Foo. From Foo's perspective, wires 3 and 4 are blocked. (Wires 0 and 1 —
  // Foo's own — are not in the set; this helper is strictly about SIBLINGS, leaving the "in-span"
  // filtering to the caller.)
  const componentGrid = grid([
    [
      u("Foo", [{ qubit: 0 }, { qubit: 1 }]),
      u("Z", [{ qubit: 3 }]),
      u("W", [{ qubit: 4 }]),
    ],
  ]);
  const blocked = getOuterColumnSiblingWires(componentGrid, "0,0");
  assert.deepEqual(
    [...blocked].sort((a, b) => a - b),
    [3, 4],
  );
});

test("getOuterColumnSiblingWires: sibling spans expand into a wire RANGE", () => {
  // A multi-wire sibling (e.g. another group / SWAP) occupies every wire from min to max. Foo @
  // [0,1] + Bar @ [3,5] → wires 3, 4, 5 all blocked from Foo's perspective.
  const componentGrid = grid([
    [
      u("Foo", [{ qubit: 0 }, { qubit: 1 }]),
      u("Bar", [{ qubit: 3 }, { qubit: 5 }]),
    ],
  ]);
  const blocked = getOuterColumnSiblingWires(componentGrid, "0,0");
  assert.deepEqual(
    [...blocked].sort((a, b) => a - b),
    [3, 4, 5],
  );
});

test("getOuterColumnSiblingWires: a sibling's classical-ref endpoint extends its covered range", () => {
  // Z @ q3 with a classical ref to q0r0 visually spans q1..q3 in its column — a box on q3, a
  // connector descending through q1 and q2, ending at the q0r0 classical row (which sits between q0
  // and q1). The connector does NOT cross q0. The helper reports geometric coverage; deciding
  // whether a drop onto a classical-connector wire is acceptable is the caller's call.
  const componentGrid = grid([
    [
      u("Foo", [{ qubit: 1 }]),
      u("Z", [{ qubit: 3 }], [{ qubit: 0, result: 0 }]),
    ],
  ]);
  const blocked = getOuterColumnSiblingWires(componentGrid, "0,0");
  // q0 is ABOVE the classical row's pass-through endpoint, so it is NOT in the set. q1..q3 are.
  assert.equal(
    blocked.has(0),
    false,
    "q0 sits above the classical-ref endpoint and is not covered",
  );
  assert.equal(
    blocked.has(1),
    true,
    "q1 is crossed by the descending connector",
  );
  assert.equal(
    blocked.has(2),
    true,
    "q2 is crossed by the descending connector",
  );
  assert.equal(blocked.has(3), true, "q3 is the gate body's row");
});

test("getOuterColumnSiblingWires: ops in OTHER columns of the parent array do NOT block", () => {
  // The helper is per-column. Foo lives in column 0; X is alone in column 1. From Foo's
  // perspective, wire 3 is free (it's in a different column, not vertically adjacent).
  const componentGrid = grid([
    [u("Foo", [{ qubit: 0 }, { qubit: 1 }])],
    [u("X", [{ qubit: 3 }])],
  ]);
  const blocked = getOuterColumnSiblingWires(componentGrid, "0,0");
  assert.equal(blocked.size, 0);
});

test("getOuterColumnSiblingWires: nested op uses its OWN containing grid, not the top-level grid", () => {
  // Foo (top-level, col 0) contains Inner (a group) at inner col 0; Inner has a sibling InnerSib at
  // inner col 0 too on a different wire. From Inner's perspective, InnerSib's wire is blocked. The
  // top-level X (col 0 of the outer grid, wire 5) is NOT counted — it's not Inner's co-resident
  // sibling.
  const inner = group("Inner", [{ qubit: 0 }], [u("H", [{ qubit: 0 }])]);
  const innerSib = u("InnerSib", [{ qubit: 2 }]);
  const foo = {
    kind: "unitary",
    gate: "Foo",
    targets: [{ qubit: 0 }, { qubit: 2 }],
    children: [{ components: [inner, innerSib] }],
  };
  const componentGrid = grid([[foo, u("X", [{ qubit: 5 }])]]);
  // Inner's location is "0,0-0,0" → outer col 0, inner col 0, opIdx 0.
  const blocked = getOuterColumnSiblingWires(componentGrid, "0,0-0,0");
  assert.equal(
    blocked.has(2),
    true,
    "InnerSib's wire (co-resident in Inner's inner column) must block",
  );
  assert.equal(
    blocked.has(5),
    false,
    "X's wire (top-level, NOT in Inner's containing grid) must not block",
  );
});

// ============================================================
// getAncestorColumnSiblingWires
// ============================================================
//
// Composes `getOuterColumnSiblingWires` across the location's full ancestor chain. Used by the
// shift-extend dropzone filter because the cascade widens every ancestor whose span doesn't already
// enclose the drop wire — collisions can show up at ANY level, not just the immediate parent's.

test("getAncestorColumnSiblingWires: null / empty / unresolvable location returns empty set", () => {
  const componentGrid = grid([[u("H", [{ qubit: 0 }])]]);
  assert.equal(getAncestorColumnSiblingWires(componentGrid, null).size, 0);
  assert.equal(getAncestorColumnSiblingWires(componentGrid, "").size, 0);
  assert.equal(getAncestorColumnSiblingWires(componentGrid, "5,0-0,0").size, 0);
});

test("getAncestorColumnSiblingWires: unions sibling wires from EVERY level of the chain", () => {
  // Deeply-nested op `H` at "0,0-0,0-0,0":
  //   - Its immediate parent `Middle` lives inside `Outer`'s inner column 0 alongside sibling
  //     `MidSib` @ q2 → wire 2 blocked at the Middle level.
  //   - `Outer` lives at top-level column 0 alongside `OuterSib` @ q5 → wire 5 blocked at the Outer
  //     level.
  //   - The chain walk must surface BOTH.
  //
  // This is the regression the immediate-parent-only filter misses: H's own outer-column siblings
  // (none at Inner's level because Middle is the only child here) tell you nothing about wires
  // Outer can extend onto.
  const h = u("H", [{ qubit: 0 }]);
  const middle = {
    kind: "unitary",
    gate: "Middle",
    targets: [{ qubit: 0 }],
    children: [{ components: [h] }],
  };
  const midSib = u("MidSib", [{ qubit: 2 }]);
  const outer = {
    kind: "unitary",
    gate: "Outer",
    targets: [{ qubit: 0 }, { qubit: 2 }],
    children: [{ components: [middle, midSib] }],
  };
  const outerSib = u("OuterSib", [{ qubit: 5 }]);
  const componentGrid = grid([[outer, outerSib]]);

  // H's location: outer "0,0", middle "0,0-0,0", H "0,0-0,0-0,0".
  const blocked = getAncestorColumnSiblingWires(componentGrid, "0,0-0,0-0,0");
  assert.equal(
    blocked.has(2),
    true,
    "MidSib's wire (sibling of Middle inside Outer) must block",
  );
  assert.equal(
    blocked.has(5),
    true,
    "OuterSib's wire (sibling of Outer at top level) must block",
  );
  // H's own ancestor chain has no other co-resident siblings at H's own level — confirm the set is
  // exactly {2, 5}.
  assert.deepEqual(
    [...blocked].sort((a, b) => a - b),
    [2, 5],
  );
});

test("getAncestorColumnSiblingWires: classical-ref endpoints on ancestor-level siblings extend coverage geometrically", () => {
  // Outer-level sibling Z @ q3 with a classical ref to q0r0 spans q1..q3 (connector descends
  // through q1, q2 to the q0r0 classical row sitting between q0 and q1). q0 is above the endpoint
  // and not covered. Same geometry rule as the single-level helper, propagated through the chain
  // walk.
  const h = u("H", [{ qubit: 0 }]);
  const middle = {
    kind: "unitary",
    gate: "Middle",
    targets: [{ qubit: 0 }],
    children: [{ components: [h] }],
  };
  const outer = {
    kind: "unitary",
    gate: "Outer",
    targets: [{ qubit: 0 }],
    children: [{ components: [middle] }],
  };
  const outerSib = u("Z", [{ qubit: 3 }], [{ qubit: 0, result: 0 }]);
  const componentGrid = grid([[outer, outerSib]]);

  const blocked = getAncestorColumnSiblingWires(componentGrid, "0,0-0,0-0,0");
  assert.equal(
    blocked.has(0),
    false,
    "q0 sits above the classical-ref endpoint and is not covered",
  );
  assert.equal(
    blocked.has(1),
    true,
    "q1 is crossed by the descending connector",
  );
  assert.equal(
    blocked.has(2),
    true,
    "q2 is crossed by the descending connector",
  );
  assert.equal(blocked.has(3), true, "q3 is the gate body's row");
});
