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
