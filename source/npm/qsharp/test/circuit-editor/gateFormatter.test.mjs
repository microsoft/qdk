// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// gateFormatter unit tests — covers the pure-logic islands inside
// `renderer/formatters/gateFormatter.ts` that the snapshot suite
// catches only indirectly:
//
//   - `_getQuantumControlYs`: routing for mixed classical+quantum
//     control arrays.
//   - `_zoomButton`: the expand/collapse decision tree and the
//     classical-control x-offset alignment.
//   - `_classicalControls`: marker emission for classical controls
//     on groups.
//   - `_createGate`: the `classically-controlled-group` CSS-class
//     hook the editor relies on.
//
// The bulk of the formatter (SVG primitives, `_unitary`, `_swap`,
// `_oplus`) is covered by the snapshot suite in `test/circuits.js`;
// duplicating that here would just re-spell the implementation.

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import {
  _createGate,
  _zoomButton,
  _classicalControls,
  _getQuantumControlYs,
} from "../../dist/ux/circuit-vis/renderer/formatters/gateFormatter.js";
import { GateType } from "../../dist/ux/circuit-vis/renderer/gateRenderData.js";
import { controlCircleOffset } from "../../dist/ux/circuit-vis/renderer/constants.js";

/** @type {JSDOM | null} */
let jsdom = null;

beforeEach(() => {
  jsdom = new JSDOM(`<!doctype html><html><body></body></html>`);
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
    // classicalControlIds omitted — every entry is quantum.
  });

  assert.deepEqual(_getQuantumControlYs(data), [40, 80, 120]);
});

test("_getQuantumControlYs: keeps every entry whose classicalControlIds slot is undefined", () => {
  // Every control is quantum but the classicalControlIds array
  // exists (defensively created upstream) with all slots undefined.
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
  // Mixed shape from an add-quantum-control-on-classical-op: index
  // 0 is a classical ref (numeric id), index 1 is the freshly-added
  // quantum control.
  const data = makeRenderData({
    controlsY: [40, 120],
    classicalControlIds: [0, undefined],
  });

  assert.deepEqual(_getQuantumControlYs(data), [120]);
});

test("_getQuantumControlYs: filters out null entries (unresolved classical id)", () => {
  // `null` marks a classical ref whose id couldn't be resolved.
  // It must still route through the classical render path; the
  // quantum path would draw a stray dot on the qubit wire.
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
  // Expand = plus sign = path with vertical and horizontal strokes.
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
  // Collapse = minus sign = horizontal stroke only.
  const path = btn?.querySelector("path")?.getAttribute("d") ?? "";
  assert.match(path, /h14/);
  assert.doesNotMatch(path, /v14/);
});

test("_zoomButton: expanded non-group (Unitary) returns a collapse button", () => {
  // The `expanded` branch fires for any op type, not just groups
  // — expanded ControlledUnitary / extracted-gate bodies also
  // render a collapse chevron.
  const btn = _zoomButton(
    makeRenderData({ type: GateType.Unitary, isExpanded: true }),
  );

  assert.notEqual(btn, null);
  assert.equal(btn?.getAttribute("class"), "gate-control gate-collapse");
});

test("_zoomButton: collapsed non-group returns null", () => {
  // A plain non-group leaf has nothing to expand into, so no
  // chevron is offered.
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
  // When an op carries classical controls, the bounding box extends
  // LEFT to make room for the dashed control circles. The chevron
  // must align with the gate body's left edge (where the dashed box
  // draws), not the bounding box's left edge.
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
  // both cases; the offset case adds `controlCircleOffset` to nudge
  // the chevron into the body's column.
  assert.equal(offsetCx - baselineCx, controlCircleOffset);
});

// ---------------------------------------------------------------------------
// _classicalControls — emission count + filter + unresolved-id fallback
// ---------------------------------------------------------------------------

test("_classicalControls: emits one circle + connector per classical entry", () => {
  // Each classical entry emits a dashed circle, a vertical dashed
  // line, and a horizontal dashed line — three elements.
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
  // Quantum entries (`undefined`) must NOT be drawn here —
  // otherwise the qubit wire gets a stray dashed circle.
  const elems = _classicalControls(
    50,
    makeRenderData({
      controlsY: [120, 200, 280],
      classicalControlIds: [0, undefined, 2],
    }),
  );

  // Two classical entries → 6 elements; the undefined slot adds
  // nothing.
  assert.equal(elems.length, 6);
  const btns = elems.filter(
    (e) => e.getAttribute("class") === "classically-controlled-btn",
  );
  assert.equal(btns.length, 2);
});

test("_classicalControls: renders null id (unresolved) without crashing", () => {
  // `null` marks a classical ref whose global id couldn't be
  // resolved (e.g. a `.qsc` file missing `controlResultIds`
  // metadata). The render path still draws the dashed circle with
  // a literal "null" subscript label — the user needs to see
  // something on the control wire.
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
  // The tspan child carries the id-or-"null" subscript inside the
  // `c<sub>…</sub>` label.
  const tspan = btn?.querySelector("tspan");
  assert.equal(tspan?.textContent, "null");
});

// ---------------------------------------------------------------------------
// _createGate — CSS-class hook for classically-controlled wrappers
// ---------------------------------------------------------------------------

test("_createGate: adds classically-controlled-group CSS class when classical controls are present", () => {
  // The editor relies on this class to scope CSS and to select
  // classically-controlled wrappers via `querySelectorAll`.
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
  // Negative side of the contract — pure-quantum groups (or any
  // op without classical refs) must not carry the class.
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
