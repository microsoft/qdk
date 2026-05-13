// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// DragController tests — exercises the gate drag-and-drop surface
// against a hand-built SVG fixture. The tests focus on the
// controller's contracts that don't require the full `LayoutMap` /
// `process` rendering pipeline:
//
//   - `dispose()` removes the document-level listeners.
//   - Toolbox mousedown sets the toolbox prototype as
//     `selectedOperation` and flags `dragging`.
//   - Mouseup on a dropzone after a toolbox drag commits an
//     `addOperation` and triggers the render callback.
//   - `startAddingControl` spawns one dropzone per wire that is
//     neither a target nor an existing control.
//   - `startRemovingControl` spawns one dropzone per existing
//     control.
//   - Document mouseup off-circuit during a drag (drag-out-delete)
//     removes the source operation.

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { CircuitModel } from "../../dist/ux/circuit-vis/data/circuitModel.js";
import { InteractionState } from "../../dist/ux/circuit-vis/actions/interactionState.js";
import { DragController } from "../../dist/ux/circuit-vis/editor/controllers/dragController.js";
import { QubitController } from "../../dist/ux/circuit-vis/editor/controllers/qubitController.js";

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
 * Build a fixture with:
 *   - container > svg.qviz[width=200]
 *   - svg.qviz > g.editor-overlay > g.dropzone-layer + g.ghost-qubit-layer
 *   - container > a single toolbox item with `[toolbox-item]` and `data-type="H"`
 *
 * Returns the elements callers most often need to fire events on.
 */
function buildFixture() {
  const container = document.createElement("div");
  document.body.appendChild(container);

  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("class", "qviz");
  svg.setAttribute("width", "200");
  container.appendChild(svg);

  const overlay = document.createElementNS(SVG_NS, "g");
  overlay.setAttribute("class", "editor-overlay");
  svg.appendChild(overlay);

  const dropzoneLayer = document.createElementNS(SVG_NS, "g");
  dropzoneLayer.setAttribute("class", "dropzone-layer");
  overlay.appendChild(dropzoneLayer);

  const ghostQubitLayer = document.createElementNS(SVG_NS, "g");
  ghostQubitLayer.setAttribute("class", "ghost-qubit-layer");
  overlay.appendChild(ghostQubitLayer);

  // Toolbox item for the H gate. `getToolboxElems` selects on
  // `[toolbox-item]`; the controller reads `data-type`.
  const toolboxItem = document.createElement("div");
  toolboxItem.setAttribute("toolbox-item", "");
  toolboxItem.setAttribute("data-type", "H");
  container.appendChild(toolboxItem);

  return {
    container,
    svg,
    overlay,
    dropzoneLayer,
    ghostQubitLayer,
    toolboxItem,
  };
}

/**
 * Append a dropzone rect to the dropzone-layer at `(location, wire)`.
 * Returns the element so the caller can dispatch mouseup on it.
 */
function appendDropzone(dropzoneLayer, location, wire, interColumn = false) {
  const dropzone = document.createElementNS(SVG_NS, "rect");
  dropzone.setAttribute("class", "dropzone");
  dropzone.setAttribute("data-dropzone-location", location);
  dropzone.setAttribute("data-dropzone-wire", String(wire));
  if (interColumn) {
    dropzone.setAttribute("data-dropzone-inter-column", "true");
  }
  dropzoneLayer.appendChild(dropzone);
  return dropzone;
}

/**
 * Construct a DragController + its `QubitController` dependency
 * against the fixture. `wireData` is filled with stable y-coords so
 * any spawned wire dropzones have somewhere to anchor.
 */
function makeController(fixture, model, options = {}) {
  const interaction = new InteractionState();
  const renderFn = options.renderFn ?? (() => {});
  const wireData = Array.from(
    { length: model.qubits.length + 1 },
    (_, i) => 40 + 60 * i,
  );
  const ctx = {
    model,
    interaction,
    layoutMap: /** @type {any} */ ({ scopes: new Map() }),
    container: fixture.container,
    circuitSvg: fixture.svg,
    overlayLayer: fixture.overlay,
    dropzoneLayer: fixture.dropzoneLayer,
    ghostQubitLayer: fixture.ghostQubitLayer,
    wireData,
    renderFn,
  };
  // The drag controller uses the qubit controller for the
  // qubit-label drag-out-delete path. We also need a qubit-input-states
  // group so QubitController's constructor doesn't crash.
  const labelGroup = document.createElementNS(SVG_NS, "g");
  labelGroup.setAttribute("class", "qubit-input-states");
  fixture.svg.insertBefore(labelGroup, fixture.overlay);
  const qubitController = new QubitController(/** @type {any} */ (ctx));
  const dragController = new DragController(
    /** @type {any} */ (ctx),
    qubitController,
  );
  return { dragController, qubitController, ctx, interaction };
}

const emptyCircuit = (n) => ({
  qubits: Array.from({ length: n }, (_, id) => ({ id })),
  componentGrid: [],
});

const dispatchMouseDown = (target, init = {}) =>
  target.dispatchEvent(
    new MouseEvent("mousedown", { button: 0, bubbles: true, ...init }),
  );

const dispatchMouseUp = (target, init = {}) =>
  target.dispatchEvent(new MouseEvent("mouseup", { bubbles: true, ...init }));

test("toolbox mousedown sets selectedOperation to the toolbox prototype", () => {
  const fixture = buildFixture();
  const model = new CircuitModel(emptyCircuit(2));
  const { interaction, dragController } = makeController(fixture, model);

  dispatchMouseDown(fixture.toolboxItem);

  assert.ok(interaction.selectedOperation, "selectedOperation should be set");
  assert.equal(/** @type {any} */ (interaction.selectedOperation).gate, "H");
  assert.equal(interaction.dragging, true);

  // Cleanup so test isolation isn't broken by leftover document listeners.
  dragController.dispose();
});

test("dropzone mouseup after a toolbox drag adds the operation to the model", () => {
  const fixture = buildFixture();
  const dropzone = appendDropzone(fixture.dropzoneLayer, "0,0", 0);
  const model = new CircuitModel(emptyCircuit(2));
  let renderCalls = 0;
  const { interaction, dragController } = makeController(fixture, model, {
    renderFn: () => {
      renderCalls++;
    },
  });

  dispatchMouseDown(fixture.toolboxItem);
  // Verify drag actually started before we test the drop.
  assert.equal(/** @type {any} */ (interaction.selectedOperation).gate, "H");

  dispatchMouseUp(dropzone);

  // Promise microtasks need to flush — onDropzoneMouseUp is async
  // even when there are no params to prompt for.
  return Promise.resolve().then(() => {
    assert.equal(model.componentGrid.length, 1);
    assert.equal(model.componentGrid[0].components[0].gate, "H");
    assert.equal(
      /** @type {any} */ (model.componentGrid[0].components[0]).targets[0]
        .qubit,
      0,
    );
    assert.equal(renderCalls, 1);
    // Transient state cleared after commit.
    assert.equal(interaction.selectedOperation, null);
    assert.equal(interaction.dragging, false);

    dragController.dispose();
  });
});

test("startAddingControl spawns one dropzone per non-target / non-control wire", () => {
  const fixture = buildFixture();
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 1 }],
            controls: [{ qubit: 0 }],
            dataAttributes: { location: "0,0" },
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const { dragController } = makeController(fixture, model);

  const op = /** @type {any} */ (model.componentGrid[0].components[0]);
  dragController.startAddingControl(op, "0,0");

  // wireData has length n+1 (= 5) including the trailing ghost wire.
  // The controller iterates the full wireData length and excludes
  // only target / existing-control wires — wires 0 (control) and 1
  // (target) are skipped; wires 2, 3, and the ghost wire 4 each get
  // a dropzone (3 total). Including the ghost wire is intentional:
  // adding a control to it grows the circuit by one qubit.
  const dropzones = fixture.overlay.querySelectorAll(
    "[data-dropzone-wire]:not(.dropzone)",
  );
  assert.equal(dropzones.length, 3);
  const wires = Array.from(dropzones)
    .map((d) => Number(d.getAttribute("data-dropzone-wire")))
    .sort((a, b) => a - b);
  assert.deepEqual(wires, [2, 3, 4]);

  dragController.dispose();
});

test("startRemovingControl spawns one dropzone per existing control", () => {
  const fixture = buildFixture();
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 3 }],
            controls: [{ qubit: 0 }, { qubit: 2 }],
            dataAttributes: { location: "0,0" },
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const { dragController } = makeController(fixture, model);

  const op = /** @type {any} */ (model.componentGrid[0].components[0]);
  dragController.startRemovingControl(op);

  // One dropzone per control → 2 dropzones.
  const dropzones = fixture.overlay.querySelectorAll(
    "[data-dropzone-wire]:not(.dropzone)",
  );
  assert.equal(dropzones.length, 2);
  const wires = Array.from(dropzones)
    .map((d) => Number(d.getAttribute("data-dropzone-wire")))
    .sort((a, b) => a - b);
  assert.deepEqual(wires, [0, 2]);

  dragController.dispose();
});

test("document mouseup off-circuit during a drag removes the source operation", () => {
  const fixture = buildFixture();
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "H",
            targets: [{ qubit: 0 }],
            dataAttributes: { location: "0,0" },
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  let renderCalls = 0;
  const { interaction, dragController } = makeController(fixture, model, {
    renderFn: () => {
      renderCalls++;
    },
  });

  // Simulate a drag in progress: source op is selected and ghost is
  // out, but the mouseup never landed on the circuit surface.
  interaction.selectedOperation = /** @type {any} */ (
    model.componentGrid[0].components[0]
  );
  interaction.dragging = true;
  interaction.mouseUpOnCircuit = false;

  dispatchMouseUp(document);

  // Source op was removed via removeOperation → grid is empty.
  assert.equal(model.componentGrid.length, 0);
  assert.equal(renderCalls, 1);
  // Transient state cleared.
  assert.equal(interaction.dragging, false);

  dragController.dispose();
});

test("dispose() removes document listeners so subsequent mouseup is a no-op", () => {
  const fixture = buildFixture();
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "H",
            targets: [{ qubit: 0 }],
            dataAttributes: { location: "0,0" },
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  let renderCalls = 0;
  const { interaction, dragController } = makeController(fixture, model, {
    renderFn: () => {
      renderCalls++;
    },
  });

  dragController.dispose();

  // After dispose, the document mouseup handler is unregistered, so
  // a drag-out-delete style dispatch should NOT mutate the model.
  interaction.selectedOperation = /** @type {any} */ (
    model.componentGrid[0].components[0]
  );
  interaction.dragging = true;
  interaction.mouseUpOnCircuit = false;

  dispatchMouseUp(document);

  // Model unchanged; no render fired.
  assert.equal(model.componentGrid.length, 1);
  assert.equal(renderCalls, 0);
});
