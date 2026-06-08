// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Run-button regression test for the toolbox.
//
// The toolbox always renders. The Run button is the only optional
// piece — hosts that can't run circuits (e.g. read-only previews)
// pass no callback and should get no button at all, not a hidden one
// taking up vertical space.
//
// This test locks down the contract on `createToolboxElement`:
//   - no callback → zero `.svg-run-button` elements in the toolbox
//   - with callback → exactly one `.svg-run-button`, clicking it
//     invokes the callback once (no double-wire)

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { createToolboxElement } from "../../dist/ux/circuit-vis/editor/toolbox.js";

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
