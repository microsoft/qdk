// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Toolbox panel rendering tests — covers the structural contract
// `createToolboxElement` exposes to the rest of the editor:
//
//   - Panel skeleton: `<div class="toolbox-panel">` with an
//     `<h2 class="title">Toolbox</h2>` and a
//     `<svg class="toolbox-panel-svg">`.
//   - One `[toolbox-item]` SVG node per `toolboxGateDictionary`
//     entry, with `data-type` matching the dictionary key.
//   - Two-column grid layout: every other gate starts a new row.
//   - SVG `height` attribute grows when the Run button is present.
//
// Run button callback wiring lives in toolboxRunButton.test.mjs.

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

// Layout constants inlined from `renderer/constants.ts`. Importing
// them would make the test a tautology against the same module that
// supplies them to `toolbox.ts`; pinning literals here flags any
// constant change as a toolbox layout regression.
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
  // Title comes before the svg in DOM order.
  const children = Array.from(toolbox.children);
  assert.equal(children[0], title);
  assert.equal(children[1], svg);
});

test("renders one [toolbox-item] per toolboxGateDictionary entry", () => {
  const toolbox = createToolboxElement();
  const items = toolbox.querySelectorAll("[toolbox-item]");

  // Pin the count via the dictionary itself so new toolbox gates
  // don't require a lockstep test update.
  const dictKeys = Object.keys(toolboxGateDictionary);
  assert.equal(
    items.length,
    dictKeys.length,
    `expected one [toolbox-item] per dictionary key (${dictKeys.length})`,
  );

  // Defense-in-depth: also pin the literal count so a dictionary
  // shrink fails here instead of silently passing.
  assert.equal(dictKeys.length, 12);

  // `dragController.onToolboxMouseDown` checks for attribute
  // presence; locking down "true" catches an accidental falsy swap.
  for (const item of items) {
    assert.equal(item.getAttribute("toolbox-item"), "true");
  }
});

test("each toolbox item carries a data-type matching its dictionary key", () => {
  // `dragController.onToolboxMouseDown` looks up the prototype op
  // by reading `data-type` off the toolbox item:
  //
  //   const gateType = elem.getAttribute("data-type")!;
  //   const proto = toolboxGateDictionary[gateType];
  //
  // A wrong/missing `data-type` either drags the wrong op or returns
  // a no-op proto. Pin every dictionary key against the rendered
  // items.
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
  // The toolbox lays gates out in a 2-column grid via:
  //
  //   if (index % 2 === 0 && index !== 0) {
  //     prefixX = 0;
  //     prefixY += gateHeight + verticalGap;
  //   }
  //
  // Expected positions for the dictionary's first few unitary gates:
  //   index 0 (RX): (x=0, y=0)
  //   index 2 (RY): (x=0, y=GATE_HEIGHT + VERTICAL_GAP)
  //   index 3 (Y):  (x>0, y=GATE_HEIGHT + VERTICAL_GAP)
  //
  // Index 1 (X) renders as an `oplus` glyph with no `<rect>` and is
  // skipped. `_unitary` centers the body rect around the target wire
  // y with height `gateHeight`, so `rect.y = targetY - gateHeight/2`
  // and row deltas match `gateHeight + verticalGap`.
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
  // The toolbox sizes its SVG to its content so the surrounding
  // `<div class="toolbox-panel">` has a known height. Computed:
  //
  //   no button:   prefixY + gateHeight + 16
  //   with button: prefixY + 2 * gateHeight + 32
  //
  // Difference must be `gateHeight + 16`.
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
