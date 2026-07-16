// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Pure-helper unit tests for the editor's draggable module (`ux/circuit-vis/editor/draggable.ts`).
// Locks down the geometry and DOM-attribute contracts of the three exported helpers that
// `dragController` and the rendering pipeline lean on:
//
//   - `makeDropzoneBox`: inter-column vs on-column geometry, the trailing-append column past the
//     rightmost real column, and the `data-dropzone-*` attribute set used by `findParentArray`.
//   - `createWireDropzone`: full-width wire-spanning dropzone Y math, the `isBetween` cases that
//     target the gaps before the first / after the last wire.
//   - `removeAllWireDropzones`: targets `.dropzone-full-wire` only and leaves other overlay
//     children alone.
//
// End-to-end behaviour through `draw()` is covered by `dropzones.test.mjs`. Helpers run in
// isolation against a hand-built `LayoutScope` / `wireData` so geometry assertions hold without
// pulling in the layout pass.

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import {
  createWireDropzone,
  makeDropzoneBox,
  removeAllWireDropzones,
} from "../../dist/ux/circuit-vis/editor/draggable.js";

const documentTemplate = `<!doctype html><html>
  <head></head>
  <body></body>
</html>`;

/** @type {JSDOM | null} */
let jsdom = null;

beforeEach(() => {
  jsdom = new JSDOM(documentTemplate);
  globalThis.window = jsdom.window;
  globalThis.document = jsdom.window.document;
  globalThis.Node = jsdom.window.Node;
  globalThis.HTMLElement = jsdom.window.HTMLElement;
  globalThis.SVGElement = jsdom.window.SVGElement;
});

afterEach(() => {
  jsdom?.window.close();
  jsdom = null;
});

// Geometry constants — hand-mirrored from the product source so the assertions are
// self-documenting and catch the "someone tweaked a padding constant and didn't realize the editor
// math depended on it" regression. These are duplicated, not imported, on purpose: the test is the
// canary that fires when the source drifts.
//
// If a value below stops matching the source, do NOT just edit the number to make the test pass —
// that defeats the guard. Instead:
//   1. Find the source of truth for the constant:
//      - GATE_PADDING / GATE_HEIGHT / MIN_GATE_WIDTH mirror the `gatePadding` / `gateHeight` /
//        `minGateWidth` exports in `ux/circuit-vis/renderer/constants.ts`.
//      - DROPZONE_PADDING_Y mirrors the private `DROPZONE_PADDING_Y` in
//        `ux/circuit-vis/editor/draggable.ts`.
//      - INTER_COLUMN_HALF_WIDTH / INTER_COLUMN_FULL_WIDTH / REGISTER_HEIGHT are DERIVED (see the
//        formulas below); they mirror the same derivations in `draggable.ts`. Update the formula,
//        not the literal, if the derivation itself changed.
//   2. Confirm the change to the source constant was intentional (and, for the base constants, that
//      the CSS custom properties in `sqore.ts` were updated to match — the renderer reads several
//      of these through CSS).
//   3. Update the mirrored value (or formula) here to match, and eyeball the dependent assertions
//      in this file that hard-code the resulting pixel offsets.
const GATE_PADDING = 6;
const GATE_HEIGHT = 40;
const MIN_GATE_WIDTH = 40;
const INTER_COLUMN_HALF_WIDTH = GATE_PADDING * 2; // 12
const INTER_COLUMN_FULL_WIDTH = INTER_COLUMN_HALF_WIDTH * 2; // 24
const DROPZONE_PADDING_Y = 20;
const REGISTER_HEIGHT = GATE_HEIGHT + GATE_PADDING * 2; // 52

/**
 * Build a `LayoutScope` with the given column starts/widths. Mirrors the shape
 * `LayoutMap.scopes.get(prefix)` returns.
 *
 * @param {number[]} columnXOffsets
 * @param {number[]} columnWidths
 */
function makeScope(columnXOffsets, columnWidths) {
  return { columnXOffsets, columnWidths };
}

/**
 * Read a numeric SVG attribute. Fails the test loudly if the attribute is missing — every helper
 * here is expected to set the geometry attrs.
 *
 * @param {SVGElement} elem
 * @param {string} name
 */
function attrNum(elem, name) {
  const raw = elem.getAttribute(name);
  assert.notEqual(raw, null, `expected attribute "${name}" to be set`);
  return Number(raw);
}

// ─── makeDropzoneBox ────────────────────────────────────────────────

test("makeDropzoneBox: inter-column band sits centered on the column's left edge", () => {
  // Single column at x=100, width=60, single wire at y=200. Inter-column band straddles the gap to
  // the *left* of this column, so its center is at colStartX - gatePadding (the renderer's
  // between-columns midpoint), with half-width INTER_COLUMN_HALF_WIDTH.
  const scope = makeScope([100], [60]);
  const wireData = [200];

  const dz = makeDropzoneBox(
    { scope, wireData },
    { colIndex: 0, opIndex: 0, wireIndex: 0, interColumn: true },
  );

  assert.equal(dz.getAttribute("class"), "dropzone");
  // Left edge = colStartX - INTER_COLUMN_HALF_WIDTH - gatePadding
  //           = 100 - 12 - 6 = 82
  assert.equal(attrNum(dz, "x"), 100 - INTER_COLUMN_HALF_WIDTH - GATE_PADDING);
  assert.equal(attrNum(dz, "width"), INTER_COLUMN_FULL_WIDTH);
  // Vertically padded around the wire Y.
  assert.equal(attrNum(dz, "y"), 200 - DROPZONE_PADDING_Y);
  assert.equal(attrNum(dz, "height"), DROPZONE_PADDING_Y * 2);
});

test("makeDropzoneBox: on-column box spans exactly the column's width", () => {
  // Distinct columnWidths value so we can tell a column-width lookup apart from a fallback to
  // `minGateWidth`.
  const scope = makeScope([100, 200], [60, 90]);
  const wireData = [200];

  const dz = makeDropzoneBox(
    { scope, wireData },
    { colIndex: 1, opIndex: 0, wireIndex: 0, interColumn: false },
  );

  assert.equal(dz.getAttribute("class"), "dropzone");
  assert.equal(attrNum(dz, "x"), 200);
  assert.equal(attrNum(dz, "width"), 90);
  assert.equal(attrNum(dz, "y"), 200 - DROPZONE_PADDING_Y);
  assert.equal(attrNum(dz, "height"), DROPZONE_PADDING_Y * 2);
});

test("makeDropzoneBox: trailing-append column synthesizes position past the rightmost real column", () => {
  // Two real columns; ask for colIndex 2 (the trailing-append slot). Spacing rule: lastStart +
  // lastWidth + gatePadding*2, width = minGateWidth.
  const scope = makeScope([100, 200], [60, 90]);
  const wireData = [200];

  const dz = makeDropzoneBox(
    { scope, wireData },
    { colIndex: 2, opIndex: 0, wireIndex: 0, interColumn: false },
  );

  // Synthesized start: 200 + 90 + 12 = 302
  assert.equal(attrNum(dz, "x"), 200 + 90 + GATE_PADDING * 2);
  assert.equal(attrNum(dz, "width"), MIN_GATE_WIDTH);
});

test("makeDropzoneBox: stamps data-dropzone-location, -wire, and -inter-column attrs", () => {
  const scope = makeScope([100, 200], [60, 90]);
  const wireData = [100, 200, 300];

  const dz = makeDropzoneBox(
    { scope, wireData },
    { colIndex: 1, opIndex: 2, wireIndex: 1, interColumn: true },
  );

  // Top-level location → no prefix, format "col,op".
  assert.equal(dz.getAttribute("data-dropzone-location"), "1,2");
  assert.equal(dz.getAttribute("data-dropzone-wire"), "1");
  assert.equal(dz.getAttribute("data-dropzone-inter-column"), "true");
});

test("makeDropzoneBox: nested pathPrefix produces hierarchical location string", () => {
  // pathPrefix `"0,0"` (children of the top-level op at column 0 / opIndex 0). The location's
  // wire-format is `<prefix>-<col>,<op>`, which `findParentArray` then walks back into the right
  // `children` grid.
  const scope = makeScope([100], [60]);
  const wireData = [200];

  const dz = makeDropzoneBox(
    { scope, wireData, pathPrefix: "0,0" },
    { colIndex: 1, opIndex: 2, wireIndex: 0, interColumn: false },
  );

  assert.equal(dz.getAttribute("data-dropzone-location"), "0,0-1,2");
  assert.equal(dz.getAttribute("data-dropzone-inter-column"), "false");
});

// ─── createWireDropzone ─────────────────────────────────────────────

/** Make an SVG element with a `width` attribute that mimics `svg.qviz`. */
function makeSvg(width = 600) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("width", String(width));
  return /** @type {SVGElement} */ (svg);
}

test("createWireDropzone: on-wire dropzone is centered on the wire Y and spans the full SVG width", () => {
  const svg = makeSvg(800);
  const wireData = [100, 200, 300];

  const dz = createWireDropzone(svg, wireData, 1, /* isBetween */ false);

  assert.equal(dz.getAttribute("class"), "dropzone-full-wire");
  assert.equal(attrNum(dz, "x"), 0);
  assert.equal(attrNum(dz, "width"), 800);
  assert.equal(attrNum(dz, "y"), 200 - DROPZONE_PADDING_Y);
  assert.equal(attrNum(dz, "height"), DROPZONE_PADDING_Y * 2);
  assert.equal(dz.getAttribute("data-dropzone-wire"), "1");
});

test("createWireDropzone: between-wires dropzone before the first wire offsets by half a register height", () => {
  // isBetween + wireIndex=0 → Y centered at wireData[0] - registerHeight/2, i.e. midway between the
  // (nonexistent) wire -1 and wire 0.
  const svg = makeSvg(600);
  const wireData = [100, 200];

  const dz = createWireDropzone(svg, wireData, 0, /* isBetween */ true);

  const centerY = 100 - REGISTER_HEIGHT / 2;
  assert.equal(attrNum(dz, "y"), centerY - DROPZONE_PADDING_Y);
  assert.equal(dz.getAttribute("data-dropzone-wire"), "0");
});

test("createWireDropzone: between-wires dropzone after the last wire is offset past the bottom", () => {
  // isBetween + wireIndex == wireData.length → Y centered past the last wire (the "add a qubit
  // below" affordance).
  const svg = makeSvg(600);
  const wireData = [100, 200];

  const dz = createWireDropzone(svg, wireData, wireData.length, true);

  const centerY = 200 + REGISTER_HEIGHT / 2;
  assert.equal(attrNum(dz, "y"), centerY - DROPZONE_PADDING_Y);
  assert.equal(dz.getAttribute("data-dropzone-wire"), "2");
});

// ─── removeAllWireDropzones ─────────────────────────────────────────

test("removeAllWireDropzones: strips every .dropzone-full-wire and leaves other overlay children intact", () => {
  // Mixed children: two wire dropzones and a regular `.dropzone` box (the kind `makeDropzoneBox`
  // produces). Only the wire dropzones should be cleared.
  const svg = makeSvg(600);
  const wireData = [100, 200];

  const wireDz1 = createWireDropzone(svg, wireData, 0, false);
  const wireDz2 = createWireDropzone(svg, wireData, 1, false);
  const onColumnDz = makeDropzoneBox(
    { scope: makeScope([50], [40]), wireData },
    { colIndex: 0, opIndex: 0, wireIndex: 0, interColumn: false },
  );

  svg.appendChild(wireDz1);
  svg.appendChild(onColumnDz);
  svg.appendChild(wireDz2);

  removeAllWireDropzones(svg);

  assert.equal(svg.querySelectorAll(".dropzone-full-wire").length, 0);
  // The non-wire dropzone is untouched.
  const remaining = svg.querySelectorAll(".dropzone");
  assert.equal(remaining.length, 1);
  assert.equal(remaining[0], onColumnDz);
});
