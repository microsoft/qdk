// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Toolbox panel rendering tests — exercises the structural contract
// `createToolboxElement` exposes to the rest of the editor. The Run
// button half (callback wiring + presence/absence) lives in
// [toolboxRunButton.test.mjs](toolboxRunButton.test.mjs); this file
// covers what's around it:
//
//   - Panel skeleton: a `<div class="toolbox-panel">` holding a
//     `<h2 class="title">Toolbox</h2>` and a
//     `<svg class="toolbox-panel-svg">`.
//   - One `[toolbox-item]` SVG node per `toolboxGateDictionary`
//     entry, with `data-type` matching the dictionary key. The drag
//     controller's [`onToolboxMouseDown`](../../ux/circuit-vis/editor/controllers/dragController.ts)
//     keys on these two attributes to look the prototype op up; the
//     two halves silently break if the toolbox renders the wrong
//     count or omits `data-type`.
//   - Two-column grid layout: every other gate starts a new row,
//     and each new row sits exactly `gateHeight + verticalGap`
//     below the prior one. Verified by reading `y` attributes
//     directly from the rendered unitary `<rect>` elements (no
//     layout engine needed — JSDOM doesn't compute one).
//   - SVG `height` attribute grows with content + accounts for the
//     optional Run button. Hosts that shrink the toolbox in
//     CSS depend on this attribute for the scroll-when-too-short
//     fallback to fire at the right threshold.

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { createToolboxElement } from "../../dist/ux/circuit-vis/editor/toolbox.js";
import { toolboxGateDictionary } from "../../dist/ux/circuit-vis/editor/toolboxGates.js";

const documentTemplate = `<!doctype html><html>
  <head></head>
  <body></body>
</html>`;

/** @type {JSDOM | null} */
let jsdom = null;

beforeEach(() => {
  jsdom = new JSDOM(documentTemplate);
  // @ts-expect-error - the `jsdom` typings and DOM typings don't match
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

// Constants are inlined from `renderer/constants.ts` — using
// production values would either require importing them (the test
// becomes a tautology against the production module that supplies
// them to `toolbox.ts` in the first place) or chase a refactor in
// two places. Pin the literal values here; if the constants change
// the test will flag the toolbox layout regression.
const GATE_HEIGHT = 40;
const VERTICAL_GAP = 10;

test("panel structure: toolbox-panel div with title and toolbox-panel-svg child", () => {
  const toolbox = createToolboxElement();

  // Outer element is exactly `<div class="toolbox-panel">`.
  assert.equal(toolbox.tagName, "DIV");
  assert.ok(toolbox.classList.contains("toolbox-panel"));

  // Two direct children: title (h2.title) + svg.toolbox-panel-svg.
  const title = toolbox.querySelector("h2.title");
  assert.ok(title, "expected an h2.title header");
  assert.equal(title?.textContent, "Toolbox");

  const svg = toolbox.querySelector("svg.toolbox-panel-svg");
  assert.ok(svg, "expected an svg.toolbox-panel-svg container");
  // The title comes before the svg in DOM order — the panel's CSS
  // grid stacks them in that order.
  const children = Array.from(toolbox.children);
  assert.equal(children[0], title);
  assert.equal(children[1], svg);
});

test("renders one [toolbox-item] per toolboxGateDictionary entry", () => {
  const toolbox = createToolboxElement();
  const items = toolbox.querySelectorAll("[toolbox-item]");

  // Today the dictionary has 12 entries — but pin the count via the
  // dictionary itself so adding a new toolbox gate doesn't require
  // updating this test in lockstep.
  const dictKeys = Object.keys(toolboxGateDictionary);
  assert.equal(
    items.length,
    dictKeys.length,
    `expected one [toolbox-item] per dictionary key (${dictKeys.length})`,
  );

  // Defense-in-depth: also pin the literal count so if the dictionary
  // shrinks unexpectedly, the failure points at the dictionary, not
  // just the rendering loop.
  assert.equal(dictKeys.length, 12);

  // The `toolbox-item` attribute is always the literal string
  // "true" — `dragController.onToolboxMouseDown` checks for the
  // attribute's presence, not its value, but locking down "true"
  // catches an accidental swap to a boolean false-y value.
  for (const item of items) {
    assert.equal(item.getAttribute("toolbox-item"), "true");
  }
});

test("each toolbox item carries a data-type matching its dictionary key", () => {
  // The `dragController`'s drag-start handler looks up the prototype
  // operation by reading `data-type` off the toolbox item:
  //
  //   const gateType = elem.getAttribute("data-type")!;
  //   const proto = toolboxGateDictionary[gateType];
  //
  // If the toolbox renders the wrong key (or omits `data-type`), the
  // wrong op gets dragged onto the circuit (or a no-op proto is
  // returned). Pin every dictionary key against the rendered items.
  const toolbox = createToolboxElement();
  const items = toolbox.querySelectorAll("[toolbox-item]");

  const renderedTypes = Array.from(items)
    .map((item) => item.getAttribute("data-type"))
    .filter(/** @returns {x is string} */ (x) => x != null);

  // Sort both for set-equality without dragging in an extra dep.
  const expectedTypes = Object.keys(toolboxGateDictionary).slice().sort();
  const actualTypes = renderedTypes.slice().sort();

  assert.deepEqual(actualTypes, expectedTypes);
});

test("two-column grid layout: column 1 sits beside column 0 on the same row; row 2 sits below row 1", () => {
  // The toolbox lays gates out in a 2-column grid. The layout math
  // (in `createToolboxElement`):
  //
  //   if (index % 2 === 0 && index !== 0) {
  //     prefixX = 0;
  //     prefixY += gateHeight + verticalGap;
  //   }
  //
  // gives the following positions for the dictionary's first few
  // unitary gates:
  //   index 0 (RX): (x=0, y=0)
  //   index 2 (RY): (x=0, y=GATE_HEIGHT + VERTICAL_GAP)
  //   index 3 (Y):  (x>0, y=GATE_HEIGHT + VERTICAL_GAP)
  //
  // (Index 1 is the X gate, which renders as an `oplus` glyph with
  // no `<rect>`; skip it for this layout check.)
  //
  // The toolbox-item `<g>` doesn't carry an absolute position
  // attribute — coordinates are baked into its descendants. Reading
  // a `<rect>`'s `y` attribute gives the rect's top edge. `_unitary`
  // renders the body rect centered around the target wire `y` with
  // height `gateHeight`, so:
  //
  //   rect.y = targetY - gateHeight / 2
  //
  // ...meaning the row delta between rect.y[2] and rect.y[0] is
  // exactly `targetY[2] - targetY[0]` = `gateHeight + verticalGap`.
  const toolbox = createToolboxElement();
  const items = toolbox.querySelectorAll("[toolbox-item]");

  /** @param {Element} item */
  const rectY = (item) => Number(item.querySelector("rect")?.getAttribute("y"));
  /** @param {Element} item */
  const rectX = (item) => Number(item.querySelector("rect")?.getAttribute("x"));

  // Indices 0 (RX), 2 (RY), 3 (Y) are all unitary gates that render
  // a labeled rect — pick those for layout comparisons.
  const y0 = rectY(items[0]); // RX, row 0, col 0
  const y2 = rectY(items[2]); // RY, row 1, col 0
  const y3 = rectY(items[3]); // Y,  row 1, col 1
  const x0 = rectX(items[0]); // RX, row 0, col 0
  const x2 = rectX(items[2]); // RY, row 1, col 0
  const x3 = rectX(items[3]); // Y,  row 1, col 1

  // Same row (row 1): col 0 (RY) and col 1 (Y) share y.
  assert.equal(y2, y3, "row 1 items must share the same y");

  // Different rows: row 2 sits exactly `gateHeight + verticalGap`
  // below row 1.
  assert.equal(
    y2 - y0,
    GATE_HEIGHT + VERTICAL_GAP,
    "row 2 must sit exactly gateHeight + verticalGap below row 1",
  );

  // Same column (col 0): row 0 (RX) and row 1 (RY) share x.
  assert.equal(x0, x2, "column 0 x-coordinate must reset to row 0's column 0");

  // Different columns (same row): col 1 (Y) sits right of col 0 (RY).
  assert.ok(x3 > x2, `column 1 (x=${x3}) must sit right of column 0 (x=${x2})`);
});

test("SVG height grows when a Run button is added", () => {
  // The toolbox sizes its SVG to its content (gates + optional
  // button + padding) so the surrounding `<div class="toolbox-panel">`
  // can rely on a known height for its scroll-when-window-too-short
  // fallback. The two computed heights are:
  //
  //   no button:   prefixY + gateHeight + 16
  //   with button: prefixY + 2 * gateHeight + 32
  //
  // Difference must therefore be `gateHeight + 16`.
  const withoutBtn = createToolboxElement();
  const withBtn = createToolboxElement(() => {});

  const hNo = Number(
    withoutBtn.querySelector(".toolbox-panel-svg")?.getAttribute("height"),
  );
  const hYes = Number(
    withBtn.querySelector(".toolbox-panel-svg")?.getAttribute("height"),
  );

  assert.ok(
    Number.isFinite(hNo) && hNo > 0,
    `no-button height must be a positive number, got ${hNo}`,
  );
  assert.ok(
    Number.isFinite(hYes) && hYes > 0,
    `with-button height must be a positive number, got ${hYes}`,
  );

  assert.equal(
    hYes - hNo,
    GATE_HEIGHT + 16,
    "Run button must contribute exactly gateHeight + 16 of vertical space",
  );
});
