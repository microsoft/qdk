// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// KeyboardController tests — exercises the smallest controller in
// isolation against a stub `InteractionContext`. Demonstrates the
// controller-layer testability: a controller can be instantiated
// against a hand-rolled context and asserted directly, without
// spinning up the full editor (no Sqore, no LayoutMap, no actual
// circuit).
//
// JSDOM is required because the controller installs document-level
// keydown/keyup listeners. Other dependencies (`model`, `layoutMap`,
// `circuitSvg`, etc.) are unused by this controller and stubbed
// minimally.
//

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { InteractionState } from "../../dist/ux/circuit-vis/actions/interactionState.js";
import { KeyboardController } from "../../dist/ux/circuit-vis/editor/controllers/keyboardController.js";

/** @type {JSDOM | null} */
let jsdom = null;
/** @type {KeyboardController | null} */
let controller = null;

beforeEach(() => {
  jsdom = new JSDOM(`<!doctype html><html><body></body></html>`);
  // @ts-expect-error - the `jsdom` typings and DOM typings don't match
  globalThis.window = jsdom.window;
  globalThis.document = jsdom.window.document;
  globalThis.HTMLElement = jsdom.window.HTMLElement;
  globalThis.KeyboardEvent = jsdom.window.KeyboardEvent;
});

afterEach(() => {
  controller?.dispose();
  controller = null;
  jsdom = null;
});

/** Build a KeyboardController with a minimal stub context. */
const makeController = (interaction = new InteractionState()) => {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const ctx = {
    model: /** @type {any} */ ({}),
    interaction,
    layoutMap: /** @type {any} */ ({}),
    container,
    circuitSvg: /** @type {any} */ ({}),
    dropzoneLayer: /** @type {any} */ ({}),
    ghostQubitLayer: /** @type {any} */ ({}),
    wireData: [],
    renderFn: () => {},
  };
  controller = new KeyboardController(ctx);
  return { ctx, container };
};

const dispatchCtrlKey = (type) => {
  document.dispatchEvent(
    new KeyboardEvent(type, { ctrlKey: true, bubbles: true }),
  );
};

const dispatchPlainKey = (type) => {
  document.dispatchEvent(
    new KeyboardEvent(type, { ctrlKey: false, bubbles: true }),
  );
};

test("Ctrl-down with no selection is a no-op", () => {
  const { container } = makeController();
  // No selectedOperation set → no class change.
  dispatchCtrlKey("keydown");
  assert.equal(container.classList.contains("copying"), false);
  assert.equal(container.classList.contains("moving"), false);
});

test("Ctrl-down on a placed gate switches moving → copying", () => {
  const interaction = new InteractionState();
  // Stand in for a placed gate: dataAttributes.location is what
  // getGateLocationString reads.
  interaction.selectedOperation = /** @type {any} */ ({
    kind: "unitary",
    dataAttributes: { location: "0,1" },
  });
  const { container } = makeController(interaction);
  container.classList.add("moving");

  dispatchCtrlKey("keydown");

  assert.equal(container.classList.contains("moving"), false);
  assert.equal(container.classList.contains("copying"), true);
});

test("Ctrl-up flips copying → moving", () => {
  const interaction = new InteractionState();
  interaction.selectedOperation = /** @type {any} */ ({
    kind: "unitary",
    dataAttributes: { location: "0,1" },
  });
  const { container } = makeController(interaction);
  container.classList.add("copying");

  dispatchCtrlKey("keyup");

  assert.equal(container.classList.contains("copying"), false);
  assert.equal(container.classList.contains("moving"), true);
});

test("Non-Ctrl keys are ignored", () => {
  const interaction = new InteractionState();
  interaction.selectedOperation = /** @type {any} */ ({
    kind: "unitary",
    dataAttributes: { location: "0,1" },
  });
  const { container } = makeController(interaction);
  container.classList.add("moving");

  dispatchPlainKey("keydown");
  dispatchPlainKey("keyup");

  // Neither class was added/removed.
  assert.equal(container.classList.contains("moving"), true);
  assert.equal(container.classList.contains("copying"), false);
});

test("Toolbox-drag (op without location) is treated as no-selection", () => {
  const interaction = new InteractionState();
  // A toolbox prototype: selectedOperation is set but has no
  // dataAttributes.location yet (it gets one once dropped).
  interaction.selectedOperation = /** @type {any} */ ({
    kind: "unitary",
  });
  const { container } = makeController(interaction);

  dispatchCtrlKey("keydown");

  assert.equal(container.classList.contains("copying"), false);
});

test("dispose() removes document listeners", () => {
  const interaction = new InteractionState();
  interaction.selectedOperation = /** @type {any} */ ({
    kind: "unitary",
    dataAttributes: { location: "0,1" },
  });
  const { container } = makeController(interaction);
  container.classList.add("moving");

  controller?.dispose();
  controller = null;

  dispatchCtrlKey("keydown");
  // No state change after dispose.
  assert.equal(container.classList.contains("copying"), false);
  assert.equal(container.classList.contains("moving"), true);
});
