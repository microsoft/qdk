// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// gateFormatter unit tests — covers the small islands of pure logic
// inside [gateFormatter.ts](../../ux/circuit-vis/renderer/formatters/gateFormatter.ts)
// that the snapshot suite catches only indirectly:
//
//   - `_getQuantumControlYs`: the predicate that routes mixed
//     classical+quantum controls (post-B5) to the right renderer.
//     Pure data, no JSDOM.
//   - `_zoomButton`: the expand/collapse decision tree plus the
//     classical-control x-offset alignment.
//   - `_gateBoundingBox` + `_classicalControls`: the geometry +
//     marker emission for the "classical control on a group"
//     surface (the load-bearing part of M2/B9 we DO support, vs.
//     quantum-controls-on-groups which the audit explicitly
//     declined for direct geometry tests).
//   - `_createGate` CSS-class contract: classically-controlled
//     wrapper element gets the `classically-controlled-group`
//     class hook the editor relies on.
//
// The bulk of the formatter (SVG primitives, `_unitary`, `_swap`,
// `_oplus`, etc.) is intentionally NOT covered here — those are
// well-served by the snapshot suite in
// [test/circuits.js](../circuits.js) and a direct unit test would
// just re-spell the implementation.

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import {
  _createGate,
  _gateBoundingBox,
  _zoomButton,
  _classicalControls,
  _getQuantumControlYs,
} from "../../dist/ux/circuit-vis/renderer/formatters/gateFormatter.js";
import { GateType } from "../../dist/ux/circuit-vis/renderer/gateRenderData.js";
import {
  gateHeight,
  controlCircleOffset,
} from "../../dist/ux/circuit-vis/renderer/constants.js";

/** @type {JSDOM | null} */
let jsdom = null;

beforeEach(() => {
  jsdom = new JSDOM(`<!doctype html><html><body></body></html>`);
  // @ts-expect-error - jsdom typings vs DOM lib mismatch
  globalThis.window = jsdom.window;
  globalThis.document = jsdom.window.document;
  globalThis.HTMLElement = jsdom.window.HTMLElement;
  globalThis.SVGElement = jsdom.window.SVGElement;
});

afterEach(() => {
  jsdom?.window.close();
  jsdom = null;
});

// ---------------------------------------------------------------------------
// Test fixture helper
// ---------------------------------------------------------------------------

/**
 * Build a minimal `GateRenderData` for tests. Defaults cover the
 * fields every code path reads; overrides on top.
 *
 * @param {Partial<import("../../dist/ux/circuit-vis/renderer/gateRenderData.js").GateRenderData>} overrides
 */
function makeRenderData(overrides = {}) {
  return {
    type: GateType.Unitary,
    isExpanded: false,
    x: 100,
    controlsY: [],
    targetsY: [[40]],
    label: "H",
    width: 40,
    topPadding: 0,
    bottomPadding: 0,
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// _getQuantumControlYs — pure-data filter (no JSDOM needed, but the
// `beforeEach` setup is harmless)
// ---------------------------------------------------------------------------

test("_getQuantumControlYs: returns every control when classicalControlIds is undefined", () => {
  const data = makeRenderData({
    controlsY: [40, 80, 120],
    // classicalControlIds omitted — the op has no classical refs at
    // all, every entry is quantum.
  });

  assert.deepEqual(_getQuantumControlYs(data), [40, 80, 120]);
});

test("_getQuantumControlYs: keeps every entry whose classicalControlIds slot is undefined", () => {
  // Post-B5 shape: every control is quantum, but the
  // classicalControlIds array exists (e.g. created defensively
  // upstream) with all slots undefined.
  const data = makeRenderData({
    controlsY: [40, 80],
    classicalControlIds: [undefined, undefined],
  });

  assert.deepEqual(_getQuantumControlYs(data), [40, 80]);
});

test("_getQuantumControlYs: filters out every classical entry (numeric id)", () => {
  const data = makeRenderData({
    controlsY: [40, 80, 120],
    classicalControlIds: [0, 1, 2],
  });

  assert.deepEqual(_getQuantumControlYs(data), []);
});

test("_getQuantumControlYs: filters mixed entries — quantum kept, classical (number) dropped", () => {
  // The mixed shape the post-B5 add-quantum-control-on-classical-op
  // path produces. Index 0 is a classical ref (numeric id), index 1
  // is the freshly-added quantum control.
  const data = makeRenderData({
    controlsY: [40, 120],
    classicalControlIds: [0, undefined],
  });

  assert.deepEqual(_getQuantumControlYs(data), [120]);
});

test("_getQuantumControlYs: filters out null entries (B1 unresolved classical id)", () => {
  // `null` marks a classical ref whose id couldn't be resolved (B1).
  // It MUST still route through the classical render path, not the
  // quantum one — otherwise a "C_null" condition would be drawn as
  // a stray control dot on the qubit wire.
  const data = makeRenderData({
    controlsY: [40, 120],
    classicalControlIds: [null, undefined],
  });

  assert.deepEqual(_getQuantumControlYs(data), [120]);
});

// ---------------------------------------------------------------------------
// _zoomButton — expand/collapse decision tree + classical-control offset
// ---------------------------------------------------------------------------

test("_zoomButton: collapsed group returns an expand button", () => {
  const btn = _zoomButton(
    makeRenderData({ type: GateType.Group, isExpanded: false }),
  );

  assert.notEqual(btn, null);
  assert.equal(btn?.getAttribute("class"), "gate-control gate-expand");
  // Expand button = plus sign = path with both vertical and
  // horizontal strokes ("M... v14 M... h14").
  const path = btn?.querySelector("path")?.getAttribute("d") ?? "";
  assert.match(path, /v14/);
  assert.match(path, /h14/);
});

test("_zoomButton: expanded group returns a collapse button", () => {
  const btn = _zoomButton(
    makeRenderData({ type: GateType.Group, isExpanded: true }),
  );

  assert.notEqual(btn, null);
  assert.equal(btn?.getAttribute("class"), "gate-control gate-collapse");
  // Collapse button = minus sign = path with horizontal stroke
  // only ("M... h14"), no vertical stroke.
  const path = btn?.querySelector("path")?.getAttribute("d") ?? "";
  assert.match(path, /h14/);
  assert.doesNotMatch(path, /v14/);
});

test("_zoomButton: expanded non-group (Unitary) returns a collapse button", () => {
  // The `expanded` branch fires for any op type, not just groups —
  // this is what lets expanded ControlledUnitary / extracted-gate
  // bodies still render a collapse chevron.
  const btn = _zoomButton(
    makeRenderData({ type: GateType.Unitary, isExpanded: true }),
  );

  assert.notEqual(btn, null);
  assert.equal(btn?.getAttribute("class"), "gate-control gate-collapse");
});

test("_zoomButton: collapsed non-group returns null", () => {
  // The whole point of the decision tree: a plain non-group leaf
  // op has nothing to expand into, so no chevron is offered.
  assert.equal(
    _zoomButton(makeRenderData({ type: GateType.Unitary, isExpanded: false })),
    null,
  );
  assert.equal(
    _zoomButton(makeRenderData({ type: GateType.X, isExpanded: false })),
    null,
  );
});

test("_zoomButton: classical-control op shifts the button right by controlCircleOffset", () => {
  // When an op carries classical controls, the overall bounding box
  // extends LEFT to make room for the dashed control circles —
  // [`_gateBoundingBox`](../../ux/circuit-vis/renderer/formatters/gateFormatter.ts)
  // honors the wider span. The chevron MUST align with the gate
  // body's left edge (where the dashed box draws), not the bounding
  // box's left edge, or it would sit out in the empty corner above
  // the classical-control circles.
  const baseline = _zoomButton(
    makeRenderData({
      type: GateType.Group,
      isExpanded: true,
    }),
  );

  const withClassicalControls = _zoomButton(
    makeRenderData({
      type: GateType.Group,
      isExpanded: true,
      controlsY: [200],
      classicalControlIds: [0],
    }),
  );

  const baselineCx = Number(
    baseline?.querySelector("circle")?.getAttribute("cx"),
  );
  const offsetCx = Number(
    withClassicalControls?.querySelector("circle")?.getAttribute("cx"),
  );

  // The bounding-box's left edge sits at `centerX - width/2` in
  // both cases, so the absolute baseline x is identical; the
  // offset case adds `controlCircleOffset` to nudge the chevron
  // into the body's column.
  assert.equal(offsetCx - baselineCx, controlCircleOffset);
});

// ---------------------------------------------------------------------------
// _gateBoundingBox — classical controls extend the bounding box, group
// padding is honored
// ---------------------------------------------------------------------------

test("_gateBoundingBox: single-target gate with no controls", () => {
  const bb = _gateBoundingBox(
    makeRenderData({ x: 100, width: 40, targetsY: [[60]] }),
  );

  // Bounding-box x centers around `centerX` with width/2 on each
  // side; height is just `gateHeight` for a single-wire op.
  assert.equal(bb.x, 100 - 20);
  assert.equal(bb.width, 40);
  assert.equal(bb.y, 60 - gateHeight / 2);
  assert.equal(bb.height, gateHeight);
});

test("_gateBoundingBox: includes classical-control wire in the y-range", () => {
  // Classical controls sit on a DIFFERENT wire from the targets
  // (typically below — the M is on a higher-numbered wire than the
  // consumer). The bounding box MUST span both, otherwise the
  // dashed connector emitted by `_classicalControls` would have
  // nothing to terminate against.
  const bbWithControl = _gateBoundingBox(
    makeRenderData({
      x: 100,
      width: 40,
      targetsY: [[60]],
      controlsY: [180],
      classicalControlIds: [0],
    }),
  );

  // Top stays at the target wire; bottom extends to include the
  // classical control wire 120px below.
  assert.equal(bbWithControl.y, 60 - gateHeight / 2);
  assert.equal(bbWithControl.height, 180 - 60 + gateHeight);
});

test("_gateBoundingBox: applies topPadding and bottomPadding for groups", () => {
  // Groups carry non-zero `topPadding` / `bottomPadding` so the
  // dashed box draws beyond the topmost / bottommost child wire.
  // The bounding box must include both paddings.
  const bb = _gateBoundingBox(
    makeRenderData({
      type: GateType.Group,
      x: 100,
      width: 80,
      targetsY: [[60]],
      topPadding: 30,
      bottomPadding: 10,
    }),
  );

  assert.equal(bb.y, 60 - gateHeight / 2 - 30);
  assert.equal(bb.height, gateHeight + 30 + 10);
});

// ---------------------------------------------------------------------------
// _classicalControls — emission count + filter + B1 fallback
// ---------------------------------------------------------------------------

test("_classicalControls: emits one circle + connector per classical entry", () => {
  // For each classical entry: a dashed circle (the control button),
  // a vertical dashed line, and a horizontal dashed line. Three
  // SVG elements per entry.
  const elems = _classicalControls(
    50,
    makeRenderData({
      controlsY: [120, 200],
      classicalControlIds: [0, 1],
    }),
  );

  // _classicalControls pushes [horLine, vertLine, controlCircle]
  // per entry, so 2 classical refs → 6 elements.
  assert.equal(elems.length, 6);

  // Each control circle is a `<g class="classically-controlled-btn">`.
  const btns = elems.filter(
    (e) => e.getAttribute("class") === "classically-controlled-btn",
  );
  assert.equal(btns.length, 2);
});

test("_classicalControls: skips undefined (quantum) entries in a mixed-control op", () => {
  // The other half of the M2/B5 mixed-controls contract — quantum
  // entries (`undefined`) must NOT be drawn here. Otherwise the
  // qubit wire gets a stray dashed circle slapped on it.
  const elems = _classicalControls(
    50,
    makeRenderData({
      controlsY: [120, 200, 280],
      classicalControlIds: [0, undefined, 2],
    }),
  );

  // Two classical entries → 6 elements; the undefined slot
  // contributes nothing.
  assert.equal(elems.length, 6);
  const btns = elems.filter(
    (e) => e.getAttribute("class") === "classically-controlled-btn",
  );
  assert.equal(btns.length, 2);
});

test("_classicalControls: renders null id (B1 unresolved) without crashing", () => {
  // `null` is the B1 fallback for "this classical ref exists but
  // its global id couldn't be resolved" — typically a `.qsc` file
  // missing the `controlResultIds` metadata. The render path must
  // still draw the dashed circle (the user needs to see SOMETHING
  // on the control wire) with a literal "null" subscript label.
  const elems = _classicalControls(
    50,
    makeRenderData({
      controlsY: [120],
      classicalControlIds: [null],
    }),
  );

  assert.equal(elems.length, 3);
  const btn = elems.find(
    (e) => e.getAttribute("class") === "classically-controlled-btn",
  );
  assert.notEqual(btn, undefined);
  // Label inside the circle is `c<sub>null</sub>` — the tspan child
  // carries the id-or-"null" subscript.
  const tspan = btn?.querySelector("tspan");
  assert.equal(tspan?.textContent, "null");
});

// ---------------------------------------------------------------------------
// _createGate — CSS-class hook for classically-controlled wrappers
// ---------------------------------------------------------------------------

test("_createGate: adds classically-controlled-group CSS class when classical controls are present", () => {
  // The editor relies on this class to scope CSS rules and select
  // classically-controlled wrappers via `querySelectorAll`. Without
  // it, the dashed-box styling and any future selection / hover
  // affordances scoped to that class would silently break.
  const gate = _createGate(
    [],
    makeRenderData({
      type: GateType.Group,
      isExpanded: true,
      controlsY: [200],
      classicalControlIds: [0],
      targetsY: [[60]],
      topPadding: 30,
      bottomPadding: 10,
    }),
  );

  assert.equal(gate.classList.contains("classically-controlled-group"), true);
});

test("_createGate: omits classically-controlled-group CSS class when no classical controls", () => {
  // The negative side of the contract — pure-quantum groups (or
  // any op without classical refs) must NOT carry the class, so
  // classical-only styling doesn't bleed onto unrelated wrappers.
  const gate = _createGate(
    [],
    makeRenderData({
      type: GateType.Group,
      isExpanded: true,
      controlsY: [],
      targetsY: [[60]],
      topPadding: 30,
      bottomPadding: 10,
    }),
  );

  assert.equal(gate.classList.contains("classically-controlled-group"), false);
});
