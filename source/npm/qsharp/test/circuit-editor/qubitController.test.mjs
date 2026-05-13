// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// QubitController tests — exercises the qubit-line interaction
// surface against a hand-built SVG fixture. Covers:
//
//   - Direct invocation of `removeQubitLineWithConfirmation` on a
//     wire with zero operations (the no-prompt fast path).
//   - Mousedown on a qubit label, which spawns the swap and
//     insert-between dropzones, sets `selectedWire` / `dragging`,
//     and creates the drag-ghost element.
//   - Mouseup on a swap dropzone dispatches `moveQubit` and the
//     render callback.

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { CircuitModel } from "../../dist/ux/circuit-vis/data/circuitModel.js";
import { InteractionState } from "../../dist/ux/circuit-vis/actions/interactionState.js";
import { QubitController } from "../../dist/ux/circuit-vis/editor/qubitController.js";

/** @type {JSDOM | null} */
let jsdom = null;

beforeEach(() => {
  jsdom = new JSDOM(`<!doctype html><html><body></body></html>`);
  // @ts-expect-error - jsdom typings vs DOM lib mismatch
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
 * Build a fixture with an `svg.qviz` containing `n` qubit-label
 * `<text>` elements (data-wire 0..n-1) inside a `g.qubit-input-states`
 * group, plus the editor overlay layer that QubitController appends
 * dropzones into.
 */
function buildFixture(n) {
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
 * Construct a QubitController against the given fixture and a
 * fresh model. `wireData[i]` is set to a stable y-coordinate so
 * the dropzone layout math has stable inputs.
 */
function makeController(container, model, options = {}) {
  const interaction = new InteractionState();
  const renderFn = options.renderFn ?? (() => {});
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
  // eslint-disable-next-line no-new
  const controller = new QubitController(/** @type {any} */ (ctx));
  return { controller, ctx, interaction };
}

const emptyCircuit = (n) => ({
  qubits: Array.from({ length: n }, (_, id) => ({ id })),
  componentGrid: [],
});

const dispatchMouseDown = (target) =>
  target.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));

test("constructor on a fixture with no labels is a safe no-op", () => {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("class", "qviz");
  container.appendChild(svg);

  // No g.qubit-input-states → getQubitLabelElems returns [] → no listeners.
  assert.doesNotThrow(() => {
    const model = new CircuitModel(emptyCircuit(0));
    const ctx = {
      model,
      interaction: new InteractionState(),
      layoutMap: /** @type {any} */ ({}),
      container,
      circuitSvg: svg,
      overlayLayer: /** @type {any} */ ({}),
      dropzoneLayer: /** @type {any} */ ({}),
      ghostQubitLayer: /** @type {any} */ ({}),
      wireData: [],
      renderFn: () => {},
    };
    new QubitController(/** @type {any} */ (ctx));
  });
});

test("removeQubitLineWithConfirmation removes an empty wire without prompting", () => {
  // Pre-populate wire 0 with an op so `removeTrailingUnusedQubits`
  // doesn't trim every wire after the target removal.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 0 }] }],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const { container } = buildFixture(3);
  let renderCalls = 0;
  const { controller, ctx } = makeController(container, model, {
    renderFn: () => {
      renderCalls++;
    },
  });

  // Wire 1 has zero use count → no prompt.
  controller.removeQubitLineWithConfirmation(1);

  // Wire 1 dropped; wire 2 (also zero-use) drops via the trailing
  // trim that fires after each removal. Wire 0 (use count 1) stays.
  assert.equal(model.qubits.length, 1);
  assert.equal(model.qubits[0].id, 0);
  assert.equal(renderCalls, 1);
  // wireData was spliced in step with the model.
  assert.equal(ctx.wireData.length, 3);
  // No prompt was added to the document.
  assert.equal(document.querySelectorAll(".prompt-overlay").length, 0);
});

test("removeQubitLineWithConfirmation prompts when the wire has operations", () => {
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 1 }] }],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const { container } = buildFixture(2);
  let renderCalls = 0;
  const { controller } = makeController(container, model, {
    renderFn: () => {
      renderCalls++;
    },
  });

  controller.removeQubitLineWithConfirmation(1);

  // The prompt is attached to the document — the actual remove waits
  // for user confirmation, so the model is unchanged at this point.
  assert.equal(document.querySelectorAll(".prompt-overlay").length, 1);
  assert.equal(model.qubits.length, 2);
  assert.equal(renderCalls, 0);
});

test("mousedown on a qubit label sets selectedWire and dragging", () => {
  const { container, labels } = buildFixture(3);
  const model = new CircuitModel(emptyCircuit(3));
  const { interaction } = makeController(container, model);

  dispatchMouseDown(labels[1]);

  assert.equal(interaction.selectedWire, 1);
  assert.equal(interaction.dragging, true);
  // Pre-existing selection is cleared at the start of a label drag.
  assert.equal(interaction.selectedOperation, null);
});

test("mousedown on a label spawns swap and insert-between dropzones along OTHER wires", () => {
  const { container, labels, overlay } = buildFixture(3);
  const model = new CircuitModel(emptyCircuit(3));
  makeController(container, model);

  dispatchMouseDown(labels[1]);

  // wireData has length n+1 (= 4) to account for the trailing ghost
  // wire.
  //
  // Swap loop: targetWire = 0..wireData.length-2 (= 0, 1, 2), skip
  // sourceWire (1) -> 2 dropzones at wires 0 and 2.
  //
  // Between loop: i = 0..wireData.length-1 (= 0..3), skip sourceWire
  // (1) and sourceWire+1 (2) -> 2 dropzones at i=0 and i=3.
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
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          { kind: "unitary", gate: "X", targets: [{ qubit: 0 }] },
          { kind: "unitary", gate: "H", targets: [{ qubit: 2 }] },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const { container, labels, overlay } = buildFixture(3);
  let renderCalls = 0;
  makeController(container, model, {
    renderFn: () => {
      renderCalls++;
    },
  });

  dispatchMouseDown(labels[0]);

  // Pick the swap dropzone targeting wire 2 (i.e. swap wires 0 and 2).
  const dropzone = Array.from(
    overlay.querySelectorAll("[data-dropzone-wire]"),
  ).find((d) => d.getAttribute("data-dropzone-wire") === "2");
  assert.ok(dropzone, "expected a swap dropzone for wire 2");

  /** @type {Element} */ (dropzone).dispatchEvent(
    new MouseEvent("mouseup", { bubbles: true }),
  );

  // After the swap the H originally on wire 2 now lives on wire 0,
  // and the X originally on wire 0 now lives on wire 2.
  const ops = model.componentGrid[0].components;
  // Column is sorted by lowest reg → H (wire 0) first.
  assert.equal(/** @type {any} */ (ops[0]).gate, "H");
  assert.equal(/** @type {any} */ (ops[0]).targets[0].qubit, 0);
  assert.equal(/** @type {any} */ (ops[1]).gate, "X");
  assert.equal(/** @type {any} */ (ops[1]).targets[0].qubit, 2);
  assert.equal(renderCalls, 1);
});
