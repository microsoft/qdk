// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// QubitController tests — exercises the qubit-line interaction surface against a hand-built SVG
// fixture. Covers:
//
//   - Direct invocation of `removeQubitLineWithConfirmation` on a wire with zero operations (the
//     no-prompt fast path).
//   - Mousedown on a qubit label, which spawns the swap and insert-between dropzones, sets
//     `selectedWire` / `dragging`, and creates the drag-ghost element.
//   - Mouseup on a swap dropzone dispatches `moveQubit` and the render callback.

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { InteractionState } from "../../dist/ux/circuit-vis/actions/interactionState.js";
import { QubitController } from "../../dist/ux/circuit-vis/editor/controllers/qubitController.js";
import { build, circuit, expectGrid, gate } from "./_helpers.mjs";

/** @type {JSDOM | null} */
let jsdom = null;

beforeEach(() => {
  jsdom = new JSDOM(`<!doctype html><html><body></body></html>`);
  globalThis.window = jsdom.window;
  globalThis.document = jsdom.window.document;
  globalThis.HTMLElement = jsdom.window.HTMLElement;
  globalThis.SVGElement = jsdom.window.SVGElement;
  globalThis.MouseEvent = jsdom.window.MouseEvent;
  globalThis.Node = jsdom.window.Node;
});

afterEach(() => {
  jsdom?.window.close();
  jsdom = null;
});

const SVG_NS = "http://www.w3.org/2000/svg";

/**
 * Build a fixture with an `svg.qviz` containing `n` qubit-label `<text>` elements (data-wire
 * 0..n-1) inside a `g.qubit-input-states` group, plus the editor overlay layer that QubitController
 * appends dropzones into.
 */
function buildFixture(/** @type {number} */ n) {
  const container = document.createElement("div");
  document.body.appendChild(container);

  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("class", "qviz");
  svg.setAttribute("width", "200");
  container.appendChild(svg);

  const labelGroup = document.createElementNS(SVG_NS, "g");
  labelGroup.setAttribute("class", "qubit-input-states");
  svg.appendChild(labelGroup);

  /** @type {SVGTextElement[]} */
  const labels = [];
  for (let i = 0; i < n; i++) {
    const text = /** @type {SVGTextElement} */ (
      document.createElementNS(SVG_NS, "text")
    );
    text.setAttribute("data-wire", String(i));
    text.textContent = `q${i}`;
    labelGroup.appendChild(text);
    labels.push(text);
  }

  const overlay = document.createElementNS(SVG_NS, "g");
  overlay.setAttribute("class", "editor-overlay");
  svg.appendChild(overlay);

  return { container, svg, labelGroup, labels, overlay };
}

/**
 * Construct a QubitController against the given fixture and a fresh model. `wireData[i]` is set to
 * a stable y-coordinate so the dropzone layout math has stable inputs.
 */
function makeController(
  /** @type {any} */ container,
  /** @type {any} */ model,
  /** @type {{ renderFn?: () => void }} */ options = {},
) {
  const interaction = new InteractionState();
  let renderCalls = 0;
  const userRender = options.renderFn;
  const renderFn = () => {
    renderCalls++;
    userRender?.();
  };
  const wireData = Array.from(
    { length: model.qubits.length + 1 },
    (_, i) => 40 + 60 * i,
  );
  const ctx = {
    model,
    interaction,
    layoutMap: /** @type {any} */ ({}),
    container,
    circuitSvg: container.querySelector("svg.qviz"),
    overlayLayer: container.querySelector("g.editor-overlay"),
    dropzoneLayer: /** @type {any} */ ({}),
    ghostQubitLayer: /** @type {any} */ ({}),
    wireData,
    renderFn,
  };
  const controller = new QubitController(/** @type {any} */ (ctx));
  return { controller, ctx, interaction, renderCalls: () => renderCalls };
}

/**
 * One-call setup: build a model from a DSL circuit literal, a fixture with one qubit label per
 * wire, and a QubitController wired to both. Returns the fixture pieces plus the controller handles
 * (including a `renderCalls()` accessor).
 *
 * @param {any} circuitObj
 * @param {{ renderFn?: () => void }} [options]
 */
function setup(circuitObj, options = {}) {
  const model = build(circuitObj);
  const fixture = buildFixture(model.qubits.length);
  const controller = makeController(fixture.container, model, options);
  return { model, ...fixture, ...controller };
}

const dispatchMouseDown = (/** @type {EventTarget} */ target) =>
  target.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));

test("removeQubitLineWithConfirmation removes an empty wire without prompting", () => {
  // Pre-populate wire 0 with an op so `removeTrailingUnusedQubits` doesn't trim every wire after
  // the target removal.
  const { model, controller, ctx, renderCalls } = setup(
    circuit(3, [[gate("H", 0)]]),
  );

  // Wire 1 has zero use count → no prompt.
  controller.removeQubitLineWithConfirmation(1);

  // Wire 1 dropped; wire 2 (also zero-use) drops via the trailing trim that fires after each
  // removal. Wire 0 (use count 1) stays.
  assert.equal(model.qubits.length, 1);
  assert.equal(model.qubits[0].id, 0);
  assert.equal(renderCalls(), 1);
  // wireData was spliced in step with the model.
  assert.equal(ctx.wireData.length, 3);
  // No prompt was added to the document.
  assert.equal(document.querySelectorAll(".prompt-overlay").length, 0);
});

test("removeQubitLineWithConfirmation prompts when the wire has operations", () => {
  const { model, controller, renderCalls } = setup(
    circuit(2, [[gate("H", 1)]]),
  );

  controller.removeQubitLineWithConfirmation(1);

  // The prompt is attached to the document — the actual remove waits for user confirmation, so the
  // model is unchanged at this point.
  assert.equal(document.querySelectorAll(".prompt-overlay").length, 1);
  assert.equal(model.qubits.length, 2);
  assert.equal(renderCalls(), 0);
});

/**
 * Find a prompt button by its visible text. The prompt renders exactly two buttons ("OK" and
 * "Cancel") both with class `prompt-button`, so text is the disambiguator.
 */
const findPromptButton = (/** @type {string} */ label) =>
  /** @type {HTMLButtonElement | undefined} */ (
    Array.from(document.querySelectorAll("button.prompt-button")).find(
      (b) => b.textContent === label,
    )
  );

test("removeQubitLineWithConfirmation prompt message reflects operation count (singular vs plural)", () => {
  const { controller } = setup(circuit(2, [[gate("H", 1)]]));

  controller.removeQubitLineWithConfirmation(1);

  // Singular wording for exactly one associated operation.
  const singularMsg = document.querySelector(".prompt-message")?.textContent;
  assert.match(singularMsg ?? "", /1 operation associated/);
  assert.doesNotMatch(singularMsg ?? "", /operations associated/);

  // Cancel to dismiss before the plural-case fixture.
  findPromptButton("Cancel")?.click();

  const { controller: controller2 } = setup(
    circuit(2, [[gate("H", 1), gate("X", 1)]]),
  );

  controller2.removeQubitLineWithConfirmation(1);

  // Plural wording for >1 associated operation.
  const pluralMsg = document.querySelector(".prompt-message")?.textContent;
  assert.match(pluralMsg ?? "", /2 operations associated/);
});

test("removeQubitLineWithConfirmation OK click cascades removeQubitWithDependents + render", () => {
  const { model, controller, ctx, renderCalls } = setup(
    circuit(3, [[gate("H", 1), gate("X", 0)]]),
  );

  controller.removeQubitLineWithConfirmation(1);
  // Pre-click: model unchanged.
  assert.equal(model.qubits.length, 3);
  assert.equal(renderCalls(), 0);

  // Simulate the user clicking OK.
  const okButton = findPromptButton("OK");
  assert.ok(okButton, "expected OK button on prompt");
  okButton.click();

  // The H on wire 1 was removed via removeQubitWithDependents; only the X on wire 0 survives. Wire
  // 1 itself was removed (trailing wire 2 was also unused so it was trimmed by
  // removeTrailingUnusedQubits, leaving just wire 0).
  assert.equal(model.qubits.length, 1);
  assert.equal(model.qubits[0].id, 0);
  // Surviving op is the X; renumbering may or may not shift its qubit index depending on
  // removeQubit's behavior — assert only that the H is gone and the X remains.
  expectGrid(model, [["X"]]);

  // wireData was spliced in step with the model removal.
  assert.equal(ctx.wireData.length, 3);

  // One render call from doRemove.
  assert.equal(renderCalls(), 1);

  // Prompt was torn down after the click.
  assert.equal(document.querySelectorAll(".prompt-overlay").length, 0);
});

test("removeQubitLineWithConfirmation Cancel click leaves the model untouched and does not render", () => {
  const { model, controller, ctx, renderCalls } = setup(
    circuit(2, [[gate("H", 1)]]),
  );

  controller.removeQubitLineWithConfirmation(1);

  const cancelButton = findPromptButton("Cancel");
  assert.ok(cancelButton, "expected Cancel button on prompt");
  cancelButton.click();

  // Cancel must NOT mutate the model, NOT splice wireData, and NOT trigger a re-render.
  assert.equal(model.qubits.length, 2);
  assert.equal(ctx.wireData.length, 3);
  assert.equal(renderCalls(), 0);
  // The op on wire 1 is still in the grid.
  expectGrid(model, [[{ H: 1 }]]);

  // Prompt was torn down after the click.
  assert.equal(document.querySelectorAll(".prompt-overlay").length, 0);
});

// ---------------------------------------------------------------------------
// Pointer interactions: label mousedown spawns dropzones; mouseup on a swap dropzone dispatches
// moveQubit
// ---------------------------------------------------------------------------

test("mousedown on a qubit label sets selectedWire and dragging", () => {
  const { labels, interaction } = setup(circuit(3, []));

  dispatchMouseDown(labels[1]);

  assert.equal(interaction.selectedWire, 1);
  assert.equal(interaction.dragging, true);
  // Pre-existing selection is cleared at the start of a label drag.
  assert.equal(interaction.selectedOperation, null);
});

test("mousedown on a label spawns swap and insert-between dropzones along OTHER wires", () => {
  const { labels, overlay } = setup(circuit(3, []));

  dispatchMouseDown(labels[1]);

  // wireData has length n+1 (= 4) to account for the trailing ghost wire.
  //
  // Swap loop: targetWire = 0..wireData.length-2 (= 0, 1, 2), skip sourceWire (1) -> 2 dropzones at
  // wires 0 and 2.
  //
  // Between loop: i = 0..wireData.length-1 (= 0..3), skip sourceWire (1) and sourceWire+1 (2) -> 2
  // dropzones at i=0 and i=3.
  //
  // Total = 4.
  const dropzones = overlay.querySelectorAll("[data-dropzone-wire]");
  assert.equal(dropzones.length, 4);

  // Drop targets: wires 0, 2 (swap) plus 0, 3 (between).
  const wires = Array.from(dropzones)
    .map((d) => Number(d.getAttribute("data-dropzone-wire")))
    .sort((a, b) => a - b);
  assert.deepEqual(wires, [0, 0, 2, 3]);
});

test("mouseup on a spawned swap dropzone calls moveQubit and renderFn", () => {
  const { model, labels, overlay, renderCalls } = setup(
    circuit(3, [[gate("X", 0), gate("H", 2)]]),
  );

  dispatchMouseDown(labels[0]);

  // Pick the swap dropzone targeting wire 2 (i.e. swap wires 0 and 2).
  const dropzone = Array.from(
    overlay.querySelectorAll("[data-dropzone-wire]"),
  ).find((d) => d.getAttribute("data-dropzone-wire") === "2");
  assert.ok(dropzone, "expected a swap dropzone for wire 2");

  /** @type {Element} */ (dropzone).dispatchEvent(
    new MouseEvent("mouseup", { bubbles: true }),
  );

  // After the swap the H originally on wire 2 now lives on wire 0, and the X originally on wire 0
  // now lives on wire 2.
  expectGrid(model, [[{ H: 0 }, { X: 2 }]]);
  assert.equal(renderCalls(), 1);
});
