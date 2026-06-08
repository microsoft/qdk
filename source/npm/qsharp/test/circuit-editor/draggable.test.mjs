// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Pure-helper unit tests for the editor's draggable module
// (`ux/circuit-vis/editor/draggable.ts`). Locks down the geometry
// and DOM-attribute contracts of the four exported helpers that
// `dragController` and the rendering pipeline lean on:
//
//   - `makeDropzoneBox`: inter-column vs on-column geometry, the
//     trailing-append column past the rightmost real column, and
//     the `data-dropzone-*` attribute set used by `findParentArray`.
//   - `makeShiftExtendGhost`: vertical span extension above/below
//     the group, horizontal extension onto the trailing-append
//     column, and the `shift-extend-ghost` CSS hook.
//   - `createWireDropzone`: full-width wire-spanning dropzone Y math,
//     the `isBetween` cases that target the gaps before the first /
//     after the last wire.
//   - `removeAllWireDropzones`: targets `.dropzone-full-wire` only
//     and leaves other overlay children alone.
//
// End-to-end behaviour through `draw()` is covered by
// `dropzones.test.mjs`. Helpers run in isolation against a hand-built
// `LayoutScope` / `wireData` so geometry assertions hold without
// pulling in the layout pass.

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import {
  createWireDropzone,
  makeDropzoneBox,
  makeShiftExtendGhost,
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

// Geometry constants — kept in sync with `renderer/constants.ts` and
// the private constants in `draggable.ts`. Locking these in test
// fixtures makes the assertions self-documenting and catches the
// "someone tweaked a padding constant and didn't realize the editor
// math depended on it" regression.
const GATE_PADDING = 6;
const GATE_HEIGHT = 40;
const MIN_GATE_WIDTH = 40;
const INTER_COLUMN_HALF_WIDTH = GATE_PADDING * 2; // 12
const INTER_COLUMN_FULL_WIDTH = INTER_COLUMN_HALF_WIDTH * 2; // 24
const DROPZONE_PADDING_Y = 20;
const REGISTER_HEIGHT = GATE_HEIGHT + GATE_PADDING * 2; // 52

/**
 * Build a `LayoutScope` with the given column starts/widths. Mirrors
 * the shape `LayoutMap.scopes.get(prefix)` returns.
 *
 * @param {number[]} columnXOffsets
 * @param {number[]} columnWidths
 */
function makeScope(columnXOffsets, columnWidths) {
  return { columnXOffsets, columnWidths };
}

/**
 * Read a numeric SVG attribute. Fails the test loudly if the attribute
 * is missing — every helper here is expected to set the geometry attrs.
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
  // Single column at x=100, width=60, single wire at y=200.
  // Inter-column band straddles the gap to the *left* of this column,
  // so its center is at colStartX - gatePadding (the renderer's
  // between-columns midpoint), with half-width INTER_COLUMN_HALF_WIDTH.
  const scope = makeScope([100], [60]);
  const wireData = [200];

  const dz = makeDropzoneBox(0, 0, scope, wireData, 0, true);

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
  // Distinct columnWidths value so we can tell a column-width lookup
  // apart from a fallback to `minGateWidth`.
  const scope = makeScope([100, 200], [60, 90]);
  const wireData = [200];

  const dz = makeDropzoneBox(1, 0, scope, wireData, 0, false);

  assert.equal(dz.getAttribute("class"), "dropzone");
  assert.equal(attrNum(dz, "x"), 200);
  assert.equal(attrNum(dz, "width"), 90);
  assert.equal(attrNum(dz, "y"), 200 - DROPZONE_PADDING_Y);
  assert.equal(attrNum(dz, "height"), DROPZONE_PADDING_Y * 2);
});

test("makeDropzoneBox: trailing-append column synthesizes position past the rightmost real column", () => {
  // Two real columns; ask for colIndex 2 (the trailing-append slot).
  // Spacing rule: lastStart + lastWidth + gatePadding*2, width = minGateWidth.
  const scope = makeScope([100, 200], [60, 90]);
  const wireData = [200];

  const dz = makeDropzoneBox(2, 0, scope, wireData, 0, false);

  // Synthesized start: 200 + 90 + 12 = 302
  assert.equal(attrNum(dz, "x"), 200 + 90 + GATE_PADDING * 2);
  assert.equal(attrNum(dz, "width"), MIN_GATE_WIDTH);
});

test("makeDropzoneBox: stamps data-dropzone-location, -wire, and -inter-column attrs", () => {
  const scope = makeScope([100, 200], [60, 90]);
  const wireData = [100, 200, 300];

  const dz = makeDropzoneBox(1, 2, scope, wireData, 1, true);

  // Top-level location → no prefix, format "col,op".
  assert.equal(dz.getAttribute("data-dropzone-location"), "1,2");
  assert.equal(dz.getAttribute("data-dropzone-wire"), "1");
  assert.equal(dz.getAttribute("data-dropzone-inter-column"), "true");
});

test("makeDropzoneBox: nested pathPrefix produces hierarchical location string", () => {
  // pathPrefix `"0,0"` (children of the top-level op at column 0 /
  // opIndex 0). The location's wire-format is `<prefix>-<col>,<op>`,
  // which `findParentArray` then walks back into the right `children` grid.
  const scope = makeScope([100], [60]);
  const wireData = [200];

  const dz = makeDropzoneBox(1, 2, scope, wireData, 0, false, "0,0");

  assert.equal(dz.getAttribute("data-dropzone-location"), "0,0-1,2");
  assert.equal(dz.getAttribute("data-dropzone-inter-column"), "false");
});

// ─── makeShiftExtendGhost ───────────────────────────────────────────

test("makeShiftExtendGhost: hover above the group's span extends the rect upward", () => {
  // Group spans wires [1, 2]; hover wire 0 (above the group).
  // Vertical bounds: min(top wire Y, hover Y) - padding ... max(bottom wire Y, hover Y) + padding.
  const scope = makeScope([100], [60]);
  const wireData = [50, 150, 250, 350];

  const ghost = makeShiftExtendGhost(
    scope,
    wireData,
    /* groupMinWire */ 1,
    /* groupMaxWire */ 2,
    /* hoverWireIndex */ 0,
    /* hoverColIndex */ 0,
  );

  assert.equal(ghost.getAttribute("class"), "shift-extend-ghost");
  // Top = min(150, 50) - 20 = 30
  assert.equal(attrNum(ghost, "y"), 50 - DROPZONE_PADDING_Y);
  // Bottom = max(250, 50) + 20 = 270; height = 270 - 30 = 240
  assert.equal(
    attrNum(ghost, "height"),
    250 + DROPZONE_PADDING_Y - (50 - DROPZONE_PADDING_Y),
  );
});

test("makeShiftExtendGhost: hover below the group's span extends the rect downward", () => {
  // Group spans wires [0, 1]; hover wire 3 (below).
  const scope = makeScope([100], [60]);
  const wireData = [50, 150, 250, 350];

  const ghost = makeShiftExtendGhost(
    scope,
    wireData,
    /* groupMinWire */ 0,
    /* groupMaxWire */ 1,
    /* hoverWireIndex */ 3,
    /* hoverColIndex */ 0,
  );

  // Top = min(50, 350) - 20 = 30
  assert.equal(attrNum(ghost, "y"), 50 - DROPZONE_PADDING_Y);
  // Bottom = max(150, 350) + 20 = 370; height = 370 - 30 = 340
  assert.equal(
    attrNum(ghost, "height"),
    350 + DROPZONE_PADDING_Y - (50 - DROPZONE_PADDING_Y),
  );
});

test("makeShiftExtendGhost: hover on the trailing-append column extends horizontally to include it", () => {
  // Two real columns; hover on colIndex 2 (the trailing slot). The
  // ghost rect should extend right to cover the synthesized column,
  // not just the rightmost real column.
  const scope = makeScope([100, 200], [60, 90]);
  const wireData = [50, 150];

  const ghostOnReal = makeShiftExtendGhost(
    scope,
    wireData,
    0,
    1,
    0,
    /* hoverColIndex */ 1,
  );
  const ghostOnTrailing = makeShiftExtendGhost(
    scope,
    wireData,
    0,
    1,
    0,
    /* hoverColIndex */ 2,
  );

  // Hover on real rightmost: rightEdge = 200 + 90 = 290
  // Hover on trailing: rightEdge = (200 + 90 + 12) + 40 = 342
  // Left edge for both = colStartX(0) - gatePadding = 100 - 6 = 94
  // Width = rightEdge - colStartX(0) + 2*gatePadding
  //       = real:     290 - 100 + 12 = 202
  //       = trailing: 342 - 100 + 12 = 254
  assert.equal(attrNum(ghostOnReal, "x"), 100 - GATE_PADDING);
  assert.equal(attrNum(ghostOnReal, "width"), 290 - 100 + GATE_PADDING * 2);
  assert.equal(attrNum(ghostOnTrailing, "x"), 100 - GATE_PADDING);
  assert.equal(
    attrNum(ghostOnTrailing, "width"),
    200 + 90 + GATE_PADDING * 2 + MIN_GATE_WIDTH - 100 + GATE_PADDING * 2,
  );
  // Sanity: trailing footprint is strictly wider than the real one.
  assert.ok(
    attrNum(ghostOnTrailing, "width") > attrNum(ghostOnReal, "width"),
    "trailing-column ghost should be wider than the real-column ghost",
  );
});

test("makeShiftExtendGhost: hover within the group span leaves vertical bounds at the group's wires", () => {
  // Hover wire is inside the group's existing wire span — vertical
  // bounds should land exactly on the group's wires (the min/max
  // doesn't pull them anywhere new), only padded.
  const scope = makeScope([100], [60]);
  const wireData = [50, 150, 250, 350];

  const ghost = makeShiftExtendGhost(scope, wireData, 1, 2, /* hover */ 2, 0);

  // Top = min(150, 250) - 20 = 130
  assert.equal(attrNum(ghost, "y"), 150 - DROPZONE_PADDING_Y);
  // Bottom = max(250, 250) + 20 = 270; height = 140
  assert.equal(attrNum(ghost, "height"), 250 - 150 + DROPZONE_PADDING_Y * 2);
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
  // isBetween + wireIndex=0 → Y centered at wireData[0] - registerHeight/2,
  // i.e. midway between the (nonexistent) wire -1 and wire 0.
  const svg = makeSvg(600);
  const wireData = [100, 200];

  const dz = createWireDropzone(svg, wireData, 0, /* isBetween */ true);

  const centerY = 100 - REGISTER_HEIGHT / 2;
  assert.equal(attrNum(dz, "y"), centerY - DROPZONE_PADDING_Y);
  assert.equal(dz.getAttribute("data-dropzone-wire"), "0");
});

test("createWireDropzone: between-wires dropzone after the last wire is offset past the bottom", () => {
  // isBetween + wireIndex == wireData.length → Y centered past the
  // last wire (the "add a qubit below" affordance).
  const svg = makeSvg(600);
  const wireData = [100, 200];

  const dz = createWireDropzone(svg, wireData, wireData.length, true);

  const centerY = 200 + REGISTER_HEIGHT / 2;
  assert.equal(attrNum(dz, "y"), centerY - DROPZONE_PADDING_Y);
  assert.equal(dz.getAttribute("data-dropzone-wire"), "2");
});

// ─── removeAllWireDropzones ─────────────────────────────────────────

test("removeAllWireDropzones: strips every .dropzone-full-wire and leaves other overlay children intact", () => {
  // Mixed children: two wire dropzones and a regular `.dropzone` box
  // (the kind `makeDropzoneBox` produces). Only the wire dropzones
  // should be cleared.
  const svg = makeSvg(600);
  const wireData = [100, 200];

  const wireDz1 = createWireDropzone(svg, wireData, 0, false);
  const wireDz2 = createWireDropzone(svg, wireData, 1, false);
  const onColumnDz = makeDropzoneBox(
    0,
    0,
    makeScope([50], [40]),
    wireData,
    0,
    false,
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

test("removeAllWireDropzones: no-op when nothing matches", () => {
  // Defensive: calling on an SVG with no wire dropzones shouldn't throw
  // and shouldn't disturb other children.
  const svg = makeSvg(600);
  const wireData = [100];
  const onColumnDz = makeDropzoneBox(
    0,
    0,
    makeScope([50], [40]),
    wireData,
    0,
    false,
  );
  svg.appendChild(onColumnDz);

  removeAllWireDropzones(svg);

  assert.equal(svg.querySelectorAll(".dropzone").length, 1);
});
