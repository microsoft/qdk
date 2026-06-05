// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Tests for the pure-data helpers in `utils.ts`. Kept narrow: only
// the helpers that don't need a DOM or `CircuitModel` to exercise.
// Heavier paths (host-element lookup, wire-data extraction) go in
// the controller-level suites where a JSDOM fixture exists.

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import {
  getChildTargets,
  getAncestorColumnSiblingWires,
  getOuterColumnSiblingWires,
  parseWireYs,
  pickClosestWireIndex,
} from "../../dist/ux/circuit-vis/utils.js";

// ============================================================
// pickClosestWireIndex
// ============================================================

test("pickClosestWireIndex: empty wireYs returns -1", () => {
  assert.equal(pickClosestWireIndex(0, [], [40, 100, 160]), -1);
});

test("pickClosestWireIndex: single-wire span ignores clickY", () => {
  // Single-wire host elements (control dots, target circles, ket
  // boxes) trivially resolve to their one wire-Y regardless of
  // where the click landed.
  assert.equal(
    pickClosestWireIndex(999, [100], [40, 100, 160]),
    1,
    "wire-Y 100 is at index 1 in wireData",
  );
  assert.equal(
    pickClosestWireIndex(-999, [40], [40, 100, 160]),
    0,
    "wire-Y 40 is at index 0 in wireData",
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
  // Span listed in non-sorted order — algorithm is comparison-based,
  // not order-dependent.
  assert.equal(pickClosestWireIndex(45, [160, 40, 100], wireData), 0);
  assert.equal(pickClosestWireIndex(155, [100, 40, 160], wireData), 2);
});

test("pickClosestWireIndex: returns -1 if the closest wireY is not in wireData", () => {
  // Belt-and-suspenders: the renderer/editor wire tables should
  // always agree, but if they don't, the helper signals "no match"
  // rather than silently returning a wrong index.
  assert.equal(pickClosestWireIndex(50, [40, 100], [200, 300]), -1);
});

test("pickClosestWireIndex: wireData with a duplicate Y returns the FIRST index", () => {
  // `Array.prototype.indexOf` finds the first occurrence; we lock
  // that behavior in so callers can rely on it. In practice the
  // wire table is unique-per-wire, so this is mostly a defensive
  // contract for malformed input.
  assert.equal(pickClosestWireIndex(45, [40], [40, 40, 40]), 0);
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

const makeElem = (attr) => {
  const el = document.createElementNS("http://www.w3.org/2000/svg", "rect");
  if (attr != null) el.setAttribute("data-wire-ys", attr);
  return el;
};

test("parseWireYs: missing attribute returns []", () => {
  assert.deepEqual(parseWireYs(makeElem(null)), []);
});

test("parseWireYs: valid number-array round-trips", () => {
  assert.deepEqual(parseWireYs(makeElem("[40, 100, 160]")), [40, 100, 160]);
  assert.deepEqual(parseWireYs(makeElem("[40]")), [40]);
});

test("parseWireYs: malformed JSON returns []", () => {
  // The renderer always writes well-formed JSON; this is the
  // contract for "data was hand-edited or corrupted".
  assert.deepEqual(parseWireYs(makeElem("not json")), []);
});

test("parseWireYs: non-number entries cause the whole array to be rejected", () => {
  // Better to refuse the whole payload than to silently coerce a
  // string into NaN and propagate it through the closest-wire
  // arithmetic.
  assert.deepEqual(parseWireYs(makeElem('[40, "100", 160]')), []);
});

test("parseWireYs: non-array JSON returns []", () => {
  assert.deepEqual(parseWireYs(makeElem("42")), []);
  assert.deepEqual(parseWireYs(makeElem('"40"')), []);
  assert.deepEqual(parseWireYs(makeElem("{}")), []);
});

// ============================================================
// getChildTargets
// ============================================================

// Helper: wrap a list of children into the single-column shape
// `Operation.children` expects.
const group = (gate, targets, children) => ({
  kind: "unitary",
  gate,
  targets,
  children: [{ components: children }],
});
const u = (gate, targets, controls) => {
  /** @type {any} */
  const op = { kind: "unitary", gate, targets };
  if (controls != null) op.controls = controls;
  return op;
};
const m = (qubits, results) => ({
  kind: "measurement",
  gate: "Measure",
  qubits,
  results,
});

test("getChildTargets: returns [] when op has no children", () => {
  // Leaf ops aren't groups; the action-layer cascade only calls
  // `getChildTargets` on ops that have a `children` grid. The
  // `[]` return models that contract.
  const leaf = u("H", [{ qubit: 0 }]);
  assert.deepEqual(getChildTargets(leaf), []);
});

test("getChildTargets: dedupes overlapping bare-qubit refs", () => {
  // The classical case from the original doc comment: Foo
  // contains H on wire 1 and RX on wires 1, 2. The union is
  // {1, 2} — wire 1 must appear exactly once, not twice.
  const foo = group(
    "Foo",
    [{ qubit: 1 }, { qubit: 2 }],
    [u("H", [{ qubit: 1 }]), u("RX", [{ qubit: 1 }, { qubit: 2 }])],
  );
  assert.deepEqual(getChildTargets(foo), [{ qubit: 1 }, { qubit: 2 }]);
});

test("getChildTargets: walks into nested groups", () => {
  // Wire union must cross group boundaries — the cascade refresh
  // assigns `getChildTargets(outer)` straight into
  // `outer.targets`, and the outer span has to enclose every
  // descendant wire no matter how deep.
  const inner = group("Inner", [{ qubit: 2 }], [u("H", [{ qubit: 2 }])]);
  const outer = group(
    "Outer",
    [{ qubit: 0 }, { qubit: 1 }, { qubit: 2 }],
    [u("H", [{ qubit: 0 }]), inner],
  );
  assert.deepEqual(getChildTargets(outer), [{ qubit: 0 }, { qubit: 2 }]);
});

test("getChildTargets: preserves measurement result registers as distinct entries", () => {
  // A child measurement on wire 0 produces classical result 0; the
  // measurement contributes BOTH `{qubit:0}` (the quantum input,
  // pushed from `operation.qubits`) AND `{qubit:0, result:0}`
  // (the classical output, pushed from `operation.results`).
  // The dedup pass keys on `(qubit, result)` so the two distinct
  // registers survive as separate entries.
  const foo = group(
    "Foo",
    [{ qubit: 0 }],
    [m([{ qubit: 0 }], [{ qubit: 0, result: 0 }])],
  );
  const out = getChildTargets(foo);
  // Order: measurement.qubits comes before measurement.results
  // in the recursion's push order, so the bare-qubit entry comes
  // first.
  assert.deepEqual(out, [{ qubit: 0 }, { qubit: 0, result: 0 }]);
});

test("getChildTargets: preserves classical-control refs from classically-conditional unitaries", () => {
  // Classically-conditional unitaries record their classical
  // dependency in BOTH `controls` and `targets` (the `targets`
  // entries are visual-extent claims that draw the line down to
  // the classical register box — see `_shiftAllRegisters` in
  // circuitActions.ts). If a group contains such a unitary, the
  // group's refreshed `.targets` MUST carry the classical ref
  // through, or the renderer drops the line.
  const cond = u(
    "X",
    [{ qubit: 1 }, { qubit: 0, result: 0 }],
    [{ qubit: 0, result: 0 }],
  );
  const foo = group("Foo", [{ qubit: 0 }, { qubit: 1 }], [cond]);
  const out = getChildTargets(foo);
  // The bare-qubit target `{qubit:1}` and the classical ref
  // `{qubit:0, result:0}` are both present. The classical ref
  // appears once even though it was pushed twice (from `targets`
  // and from `controls`).
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

test("getChildTargets: keeps bare-qubit and classical-ref on the same qubit as distinct entries", () => {
  // The key invariant: `{qubit:0}` and `{qubit:0, result:0}` are
  // two different register identities (a wire vs. a classical
  // bit), and both must survive into the output. Dedup must key
  // on `(qubit, result)`, not on `qubit`.
  const wireOp = u("H", [{ qubit: 0 }]);
  const condOp = u("Z", [{ qubit: 1 }], [{ qubit: 0, result: 0 }]);
  const foo = group("Foo", [{ qubit: 0 }, { qubit: 1 }], [wireOp, condOp]);
  const out = getChildTargets(foo);
  assert.ok(
    out.some((r) => r.qubit === 0 && r.result === undefined),
    `expected bare-qubit {qubit:0}, got ${JSON.stringify(out)}`,
  );
  assert.ok(
    out.some((r) => r.qubit === 0 && r.result === 0),
    `expected classical-ref {qubit:0, result:0}, got ${JSON.stringify(out)}`,
  );
});

test("getChildTargets: returns fresh register objects, not aliases of child registers", () => {
  // Callers assign the returned array straight into
  // `parent.targets` / `parent.results`. If the entries aliased
  // the child's own register objects, a later in-place edit on
  // the child's register (e.g. `_shiftAllRegisters` bumping
  // `qubit`) would silently mutate the parent's cached extent
  // too.
  const childTargets = [{ qubit: 0 }];
  const foo = group("Foo", [{ qubit: 0 }], [u("H", childTargets)]);
  const out = getChildTargets(foo);
  assert.notEqual(
    out[0],
    childTargets[0],
    "returned register must be a fresh object, not a reference to the child's register",
  );
  // Belt-and-suspenders: mutate the returned entry and confirm the
  // child's register is unchanged.
  out[0].qubit = 999;
  assert.equal(childTargets[0].qubit, 0, "child register must be untouched");
});

// ============================================================
// getOuterColumnSiblingWires
// ============================================================
//
// Used by the shift-extend dropzone filter to identify wires that
// an op cannot directly extend onto because an external sibling
// in the op's outer column already occupies them. The
// "cross-over" case (extending past an in-between sibling) is
// intentionally NOT covered here — that's a property of the
// action-layer overlap resolver and is tested in
// circuitActions.test.mjs.

// Helper: build a single-component-grid from a component list.
const grid = (componentLists) =>
  componentLists.map((components) => ({ components }));

test("getOuterColumnSiblingWires: null / empty location returns empty set", () => {
  const componentGrid = grid([[u("H", [{ qubit: 0 }])]]);
  assert.equal(getOuterColumnSiblingWires(componentGrid, null).size, 0);
  assert.equal(getOuterColumnSiblingWires(componentGrid, "").size, 0);
});

test("getOuterColumnSiblingWires: op with no co-resident siblings returns empty set", () => {
  // Top-level op alone in its column — no siblings to enumerate.
  const componentGrid = grid([[u("Foo", [{ qubit: 0 }, { qubit: 1 }])]]);
  const blocked = getOuterColumnSiblingWires(componentGrid, "0,0");
  assert.equal(blocked.size, 0);
});

test("getOuterColumnSiblingWires: returns every wire an external sibling occupies", () => {
  // Column 0 holds Foo @ wires [0,1] alongside Z @ wire 3 and W @
  // wire 4 — both Z and W are external siblings of Foo. From Foo's
  // perspective, wires 3 and 4 are blocked. (Wires 0 and 1 — Foo's
  // own — are not in the set; this helper is strictly about
  // SIBLINGS, leaving the "in-span" filtering to the caller.)
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
  // A multi-wire sibling (e.g. another group / SWAP) occupies
  // every wire from min to max. Foo @ [0,1] + Bar @ [3,5] → wires
  // 3, 4, 5 all blocked from Foo's perspective.
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

test("getOuterColumnSiblingWires: excludes classical-ref entries on siblings", () => {
  // A classically-controlled sibling at q3 with a classical ref to
  // a measurement on q0 contributes ONLY wire 3 — the classical
  // ref paints as a thin indicator on q0, not a real visual wire
  // occupant, so it doesn't represent a "drop here would overlap"
  // situation. Pure-quantum filtering matches `result === undefined`.
  const componentGrid = grid([
    [
      u("Foo", [{ qubit: 1 }]),
      u("Z", [{ qubit: 3 }], [{ qubit: 0, result: 0 }]),
    ],
  ]);
  const blocked = getOuterColumnSiblingWires(componentGrid, "0,0");
  // q0 is the classical-ref's wire — it must NOT be in the set
  // (otherwise the shift-extend filter would over-block).
  assert.equal(blocked.has(0), false, "classical-ref wire q0 must not block");
  assert.equal(blocked.has(3), true, "sibling's quantum wire q3 must block");
});

test("getOuterColumnSiblingWires: ops in OTHER columns of the parent array do NOT block", () => {
  // The helper is per-column. Foo lives in column 0; X is alone in
  // column 1. From Foo's perspective, wire 3 is free (it's in a
  // different column, not vertically adjacent).
  const componentGrid = grid([
    [u("Foo", [{ qubit: 0 }, { qubit: 1 }])],
    [u("X", [{ qubit: 3 }])],
  ]);
  const blocked = getOuterColumnSiblingWires(componentGrid, "0,0");
  assert.equal(blocked.size, 0);
});

test("getOuterColumnSiblingWires: nested op uses its OWN containing grid, not the top-level grid", () => {
  // Foo (top-level, col 0) contains Inner (a group) at inner col
  // 0; Inner has a sibling InnerSib at inner col 0 too on a
  // different wire. From Inner's perspective, InnerSib's wire is
  // blocked. The top-level X (col 0 of the outer grid, wire 5)
  // is NOT counted — it's not Inner's co-resident sibling.
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

test("getOuterColumnSiblingWires: location resolving to no parent array returns empty set", () => {
  // Defensive guard — a location addressing an op nested below a
  // missing ancestor (e.g., "5,0-0,0" on a single-column grid)
  // resolves to no parent array; helper returns empty rather than
  // throwing.
  const componentGrid = grid([[u("H", [{ qubit: 0 }])]]);
  assert.equal(getOuterColumnSiblingWires(componentGrid, "5,0-0,0").size, 0);
});

// ============================================================
// getAncestorColumnSiblingWires
// ============================================================
//
// Composes `getOuterColumnSiblingWires` across the location's full
// ancestor chain. Used by the shift-extend dropzone filter because
// the cascade widens every ancestor whose span doesn't already
// enclose the drop wire — collisions can show up at ANY level, not
// just the immediate parent's.

test("getAncestorColumnSiblingWires: null / empty location returns empty set", () => {
  const componentGrid = grid([[u("H", [{ qubit: 0 }])]]);
  assert.equal(getAncestorColumnSiblingWires(componentGrid, null).size, 0);
  assert.equal(getAncestorColumnSiblingWires(componentGrid, "").size, 0);
});

test("getAncestorColumnSiblingWires: top-level op matches getOuterColumnSiblingWires (chain of length 1)", () => {
  // A top-level location has no ancestors — the chain walk reduces
  // to a single call. Result must match the single-level helper.
  const componentGrid = grid([
    [
      u("Foo", [{ qubit: 0 }, { qubit: 1 }]),
      u("Z", [{ qubit: 3 }]),
      u("W", [{ qubit: 4 }]),
    ],
  ]);
  const single = getOuterColumnSiblingWires(componentGrid, "0,0");
  const chained = getAncestorColumnSiblingWires(componentGrid, "0,0");
  assert.deepEqual(
    [...chained].sort((a, b) => a - b),
    [...single].sort((a, b) => a - b),
  );
});

test("getAncestorColumnSiblingWires: unions sibling wires from EVERY level of the chain", () => {
  // Deeply-nested op `H` at "0,0-0,0-0,0":
  //   - Its immediate parent `Middle` lives inside `Outer`'s
  //     inner column 0 alongside sibling `MidSib` @ q2 → wire 2
  //     blocked at the Middle level.
  //   - `Outer` lives at top-level column 0 alongside `OuterSib`
  //     @ q5 → wire 5 blocked at the Outer level.
  //   - The chain walk must surface BOTH.
  //
  // This is the regression the immediate-parent-only filter
  // misses: H's own outer-column siblings (none at Inner's level
  // because Middle is the only child here) tell you nothing about
  // wires Outer can extend onto.
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
  // H's own ancestor chain has no other co-resident siblings at
  // H's own level — confirm the set is exactly {2, 5}.
  assert.deepEqual(
    [...blocked].sort((a, b) => a - b),
    [2, 5],
  );
});

test("getAncestorColumnSiblingWires: classical-ref entries on ancestor-level siblings are still excluded", () => {
  // Same exclusion the single-level helper applies — propagated
  // through the chain walk because each level just delegates.
  // Outer-level sibling Z @ q3 with a classical ref to q0
  // contributes ONLY wire 3, not wire 0.
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
    "classical-ref wire q0 must not block at any ancestor level",
  );
  assert.equal(
    blocked.has(3),
    true,
    "ancestor sibling's quantum wire q3 must block",
  );
});

test("getAncestorColumnSiblingWires: location resolving to no parent array returns empty set", () => {
  // Defensive guard, mirroring the single-level helper.
  const componentGrid = grid([[u("H", [{ qubit: 0 }])]]);
  assert.equal(getAncestorColumnSiblingWires(componentGrid, "5,0-0,0").size, 0);
});
