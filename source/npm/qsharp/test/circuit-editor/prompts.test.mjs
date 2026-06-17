// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// prompts tests — direct coverage for `_createConfirmPrompt`, the
// confirm-dialog primitive in `editor/prompts.ts`. Pins:
//
//   - DOM shape: `.prompt-overlay > .prompt-container >
//     .prompt-message + .prompt-buttons > [OK, Cancel]`. The
//     widget classes are load-bearing — the host page's CSS styles
//     by them and operationPrompts.test.mjs locates buttons by them.
//   - Click semantics: OK → `callback(true)` + overlay removed;
//     Cancel → `callback(false)` + overlay removed.
//   - Keyboard semantics: Enter → OK, Escape → Cancel, wired through
//     a document-level capture-phase keydown listener so the prompt
//     wins over any descendant input handler.
//   - Listener lifecycle: the keydown listener is removed when the
//     prompt closes (clicking OK or Cancel — including via Enter or
//     Escape), so a subsequent key press doesn't re-invoke the
//     callback.
//
// `_createConfirmPrompt` is exported and self-contained — no
// `CircuitEvents` or controller dependency — so these tests stand
// alone over a bare JSDOM document.

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { _createConfirmPrompt } from "../../dist/ux/circuit-vis/editor/prompts.js";

/** @type {JSDOM | null} */
let jsdom = null;

beforeEach(() => {
  jsdom = new JSDOM(`<!doctype html><html><body></body></html>`);
  globalThis.window = jsdom.window;
  globalThis.document = jsdom.window.document;
  globalThis.HTMLElement = jsdom.window.HTMLElement;
  globalThis.KeyboardEvent = jsdom.window.KeyboardEvent;
});

afterEach(() => {
  jsdom?.window.close();
  jsdom = null;
});

/** Locate the open prompt's structural pieces by class. */
function getPrompt() {
  const overlay = /** @type {HTMLElement | null} */ (
    document.querySelector(".prompt-overlay")
  );
  if (!overlay) return null;
  const container = overlay.querySelector(".prompt-container");
  const message = overlay.querySelector(".prompt-message");
  const buttons = overlay.querySelectorAll(".prompt-button");
  return {
    overlay,
    container,
    message,
    okButton: /** @type {HTMLButtonElement} */ (buttons[0]),
    cancelButton: /** @type {HTMLButtonElement} */ (buttons[1]),
  };
}

/**
 * Open a confirm prompt over the current document and return a handle
 * to the located DOM parts plus two accessors:
 *   - `result()`    — the value the callback last received (null until fired)
 *   - `callCount()` — how many times the callback has fired
 */
function openPrompt(message = "ok?") {
  /** @type {boolean | null} */
  let captured = null;
  let callCount = 0;
  _createConfirmPrompt(message, (c) => {
    captured = c;
    callCount++;
  });
  const parts = getPrompt();
  assert.ok(parts, "prompt overlay should be open after _createConfirmPrompt");
  return { ...parts, result: () => captured, callCount: () => callCount };
}

/** Dispatch a document-level keydown — the path the prompt listens on. */
function pressKey(/** @type {string} */ key) {
  document.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
}

/** Assert the prompt overlay has been removed from the DOM. */
function assertPromptClosed(label = "prompt overlay must be removed") {
  assert.equal(document.querySelector(".prompt-overlay"), null, label);
}

/** Assert the prompt overlay is still present in the DOM. */
function assertPromptOpen(label = "prompt overlay must still be open") {
  assert.ok(document.querySelector(".prompt-overlay"), label);
}

test("_createConfirmPrompt: builds the expected DOM subtree under document.body", () => {
  // Pinning the DOM shape because both the host page's CSS and
  // the operationPrompts tests rely on these specific class names
  // and the button ordering (OK first, Cancel second).
  const p = openPrompt("Confirm something?");

  assert.equal(p.overlay.parentNode, document.body);
  assert.ok(p.container, "container should exist inside overlay");
  assert.equal(
    p.message?.textContent,
    "Confirm something?",
    "message element should carry the caller's text verbatim",
  );
  assert.equal(p.okButton.textContent, "OK");
  assert.equal(p.cancelButton.textContent, "Cancel");
  // Callback shouldn't have fired yet — just the construction.
  assert.equal(p.result(), null);
});

test("_createConfirmPrompt: OK button click fires callback(true) and removes the overlay", () => {
  const p = openPrompt();
  p.okButton.click();

  assert.equal(p.result(), true, "OK click must pass true to callback");
  assertPromptClosed("overlay must be removed from the DOM after OK");
});

test("_createConfirmPrompt: Cancel button click fires callback(false) and removes the overlay", () => {
  const p = openPrompt();
  p.cancelButton.click();

  assert.equal(p.result(), false, "Cancel click must pass false to callback");
  assertPromptClosed("overlay must be removed from the DOM after Cancel");
});

test("_createConfirmPrompt: Enter key commits as if OK was clicked", () => {
  // The document-level keydown listener is registered in capture
  // phase, so dispatching a `keydown` from `document` directly
  // exercises the same path real key events take in the browser.
  const p = openPrompt();
  pressKey("Enter");

  assert.equal(p.result(), true, "Enter must commit (callback(true))");
  assertPromptClosed("Enter must close the prompt");
});

test("_createConfirmPrompt: Escape key cancels as if Cancel was clicked", () => {
  const p = openPrompt();
  pressKey("Escape");

  assert.equal(p.result(), false, "Escape must cancel (callback(false))");
  assertPromptClosed("Escape must close the prompt");
});

test("_createConfirmPrompt: keydown listener is removed after close — subsequent keys do not fire callback again", () => {
  // After OK closes the prompt, the document-level handler MUST
  // be uninstalled — otherwise a stray Enter elsewhere on the
  // page would try to click a now-detached button and (worse)
  // could double-fire the callback if a second prompt has since
  // opened. The implementation uses `removeEventListener` with
  // matching capture flag inside both click handlers; here we
  // pin that contract.
  const p = openPrompt();

  // First Enter → OK → callback fires once, prompt closes.
  pressKey("Enter");
  assert.equal(p.callCount(), 1);
  assertPromptClosed();

  // Subsequent Enter must NOT fire the now-closed prompt's
  // callback again.
  pressKey("Enter");
  assert.equal(
    p.callCount(),
    1,
    "callback must NOT fire after the prompt is closed",
  );
});

test("_createConfirmPrompt: keys other than Enter/Escape are ignored", () => {
  // Defense-in-depth: typing inside the prompt (e.g. someone
  // accidentally hitting a letter key) must not close it. Only
  // Enter and Escape are honored.
  const p = openPrompt();

  pressKey("a");
  pressKey(" ");
  pressKey("Tab");

  assert.equal(
    p.result(),
    null,
    "callback must not fire for non-Enter/Escape keys",
  );
  assertPromptOpen("prompt must still be open after stray keypresses");
});
