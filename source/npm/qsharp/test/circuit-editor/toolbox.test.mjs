// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Toolbox editor-contract tests — cover the things interaction code
// depends on:
//
//   - Each rendered toolbox item exposes a `[toolbox-item]` attribute
//     and a `data-type` the dragController uses to look up the
//     prototype op.
//   - The optional Run button: present (and wired) only when a
//     callback is provided, absent otherwise.
//
// Panel layout, title, and gate positions are visual concerns
// covered by the snapshot suite in `test/circuits.js`.

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

test("each toolbox item exposes a [toolbox-item] flag and a data-type matching its dictionary key", () => {
  // `dragController.onToolboxMouseDown` selects on `[toolbox-item]`
  // and reads `data-type` to look up the prototype op:
  //
  //   const gateType = elem.getAttribute("data-type")!;
  //   const proto = toolboxGateDictionary[gateType];
  //
  // Keying every assertion off `toolboxGateDictionary` means editing
  // the toolbox gates updates this test automatically.
  const toolbox = createToolboxElement();
  const items = toolbox.querySelectorAll("[toolbox-item]");

  // `dragController` checks for attribute presence; locking down
  // "true" catches an accidental falsy swap.
  for (const item of Array.from(items)) {
    assert.equal(item.getAttribute("toolbox-item"), "true");
  }

  const renderedTypes = Array.from(items)
    .map((item) => item.getAttribute("data-type"))
    .filter(/** @returns {x is string} */ (x) => x != null)
    .sort();
  const expectedTypes = Object.keys(toolboxGateDictionary).sort();

  assert.deepEqual(renderedTypes, expectedTypes);
});

// ---------------------------------------------------------------------------
// Run button — the only optional piece of the toolbox. Hosts that
// can't run circuits (e.g. read-only previews) pass no callback and
// should get no button at all, not a hidden one taking up space.
// ---------------------------------------------------------------------------

test("toolbox without runCallback renders no Run button", () => {
  const toolbox = createToolboxElement();

  assert.equal(
    toolbox.querySelectorAll(".svg-run-button").length,
    0,
    "Run button must not be rendered when no callback is provided",
  );
  // The toolbox itself still renders — only the button is suppressed.
  assert.ok(
    toolbox.querySelector(".toolbox-panel-svg"),
    "toolbox SVG should still be present",
  );
});

test("toolbox with runCallback renders exactly one Run button", () => {
  const toolbox = createToolboxElement(() => {});

  const buttons = toolbox.querySelectorAll(".svg-run-button");
  assert.equal(buttons.length, 1, "exactly one Run button expected");

  const button = buttons[0];
  // Accessibility wiring set by `_createRunButton` — guards against
  // a future refactor silently dropping the role/tabindex.
  assert.equal(button.getAttribute("role"), "button");
  assert.equal(button.getAttribute("tabindex"), "0");
  assert.equal(
    button.querySelector(".svg-run-button-text")?.textContent,
    "Run",
  );
});

test("clicking the Run button invokes the callback exactly once per click", () => {
  let callCount = 0;
  const toolbox = createToolboxElement(() => {
    callCount += 1;
  });

  const button = toolbox.querySelector(".svg-run-button");
  assert.ok(button, "Run button should be present");

  button.dispatchEvent(
    new /** @type {any} */ (jsdom).window.Event("click", { bubbles: true }),
  );
  assert.equal(callCount, 1, "one click → one callback invocation");

  button.dispatchEvent(
    new /** @type {any} */ (jsdom).window.Event("click", { bubbles: true }),
  );
  assert.equal(callCount, 2, "second click → second invocation (no debounce)");
});
