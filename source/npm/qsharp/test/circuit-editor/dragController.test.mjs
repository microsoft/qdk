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
import { Location } from "../../dist/ux/circuit-vis/data/location.js";

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

// ---------------------------------------------------------------
// B11a regression — onGateMouseDown on an expanded group's
// control dot.
//
// An expanded group renders as `<g class="gate" data-expanded="true">`
// with control dots as direct children. The pre-B11 early return
// on `data-expanded === "true"` left `selectedOperation` null even
// when the click was on a control dot, blocking the drag entirely.
// The fix carves out a `movingControl` exception so the control-
// drag flow can start. See B11 in CIRCUIT_EDITOR_TODO.md.
// ---------------------------------------------------------------

test("onGateMouseDown on an expanded group's control dot sets selectedOperation when movingControl is true", () => {
  const fixture = buildFixture();
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 1 }, { qubit: 2 }],
            controls: [{ qubit: 0 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 1 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  // Build the gate elem BEFORE constructing the controller — gate
  // listeners are wired once, at construction, by `getGateElems`.
  const gateElem = document.createElementNS(SVG_NS, "g");
  gateElem.setAttribute("class", "gate");
  gateElem.setAttribute("data-expanded", "true");
  gateElem.setAttribute("data-location", "0,0");
  // The control dot lives directly under the expanded group's `<g>`,
  // not inside a nested `.gate` wrapper. (Nested child gates have
  // their own `.gate` wrappers; only top-level controls of the group
  // bubble up to this one.)
  const controlDot = document.createElementNS(SVG_NS, "circle");
  controlDot.setAttribute("class", "control-dot");
  controlDot.setAttribute("data-wire", "0");
  gateElem.appendChild(controlDot);
  fixture.svg.appendChild(gateElem);

  const { interaction, dragController } = makeController(fixture, model);

  // Simulate the selectionController's effect (it runs first on the
  // control-dot host element and sets these before the gate handler
  // sees the bubbled event).
  interaction.movingControl = true;
  interaction.selectedWire = 0;

  dispatchMouseDown(gateElem);

  assert.ok(
    interaction.selectedOperation,
    "selectedOperation must be set so the drag flow can proceed",
  );
  assert.equal(
    /** @type {any} */ (interaction.selectedOperation).gate,
    "Foo",
    "selectedOperation resolves to the expanded group itself",
  );

  dragController.dispose();
});

test("onGateMouseDown on an expanded group WITHOUT movingControl still no-ops (no regression)", () => {
  // The carve-out is `movingControl`-gated; ordinary clicks on the
  // expanded group's dashed box / label area must still leave
  // `selectedOperation` untouched so the user can't grab the group
  // as a whole when it's expanded.
  const fixture = buildFixture();
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 0 }, { qubit: 1 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  const gateElem = document.createElementNS(SVG_NS, "g");
  gateElem.setAttribute("class", "gate");
  gateElem.setAttribute("data-expanded", "true");
  gateElem.setAttribute("data-location", "0,0");
  const dashedBox = document.createElementNS(SVG_NS, "rect");
  dashedBox.setAttribute("class", "gate-unitary");
  gateElem.appendChild(dashedBox);
  fixture.svg.appendChild(gateElem);

  const { interaction, dragController } = makeController(fixture, model);

  // `movingControl` is the default-false; no selectionController
  // emulation here.
  interaction.selectedWire = 0;

  dispatchMouseDown(gateElem);

  assert.equal(
    interaction.selectedOperation,
    null,
    "expanded-group mousedown without movingControl must NOT set selectedOperation",
  );

  dragController.dispose();
});

// ---------------------------------------------------------------
// Regression: commitAddControl must NOT duplicate the source op
// when the new control's wire crosses a same-column sibling.
//
// Earlier versions of `commitAddControl` ran their OWN split-and-
// shift block after calling `addControl(...)`. Once the action
// layer's `_resolveSpanChange` centralized the cascade-aware
// split (so `addControl` itself splits the column when widening
// would collide with a sibling), the legacy block ran a second
// time over the just-split layout, spliced the source op into yet
// another fresh column, and left the source op visible twice.
//
// This test goes through the full UI commit path (no calling
// `addControl` directly) so the regression is owned by the
// dragController layer, not the action layer — the action-layer
// tests already prove `addControl` splits correctly on its own.
// ---------------------------------------------------------------

test("commitAddControl does not duplicate the source op when widening collides with a sibling", () => {
  const fixture = buildFixture();
  /** @type {any} */
  const circuit = {
    // 4 qubits. Column 0: [H on q0, Z on q3]. Adding a control on
    // q2 to H widens H to span q0..q2 — no overlap with Z (q3), so
    // the column should NOT split.
    //
    // Adding a control on q3 instead WOULD widen H to span q0..q3,
    // overlapping Z. That's the case we test below for the
    // duplicate. Use q3 to force the collision.
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "H",
            targets: [{ qubit: 0 }],
            dataAttributes: { location: "0,0" },
          },
          {
            kind: "unitary",
            gate: "Z",
            targets: [{ qubit: 3 }],
            dataAttributes: { location: "0,1" },
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  let renderCalls = 0;
  const { dragController } = makeController(fixture, model, {
    renderFn: () => {
      renderCalls++;
    },
  });

  const hOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  dragController.startAddingControl(hOp);

  // The wire-pick dropzone for q3 is the one that, on click, will
  // widen H to span q0..q3 — collision territory.
  const dropzone = fixture.overlay.querySelector(
    '[data-dropzone-wire="3"]:not(.dropzone)',
  );
  assert.ok(dropzone, "wire-pick dropzone for q3 must have been spawned");

  dropzone.dispatchEvent(new MouseEvent("click", { button: 0, bubbles: true }));

  // Action layer's centralized cascade must have split the column:
  // H alone in col 0, Z alone in col 1.
  assert.equal(
    model.componentGrid.length,
    2,
    `expected col 0 to split; got ${JSON.stringify(
      model.componentGrid.map((c) =>
        c.components.map((/** @type {any} */ op) => op.gate),
      ),
    )}`,
  );
  assert.deepEqual(
    model.componentGrid[0].components.map((/** @type {any} */ op) => op.gate),
    ["H"],
    "H must occupy col 0 alone",
  );
  assert.deepEqual(
    model.componentGrid[1].components.map((/** @type {any} */ op) => op.gate),
    ["Z"],
    "Z must occupy col 1 alone",
  );

  // The duplication bug's smoking gun: H appears exactly once in
  // the grid. Count by gate name to catch any phantom duplicate
  // wherever it ended up (different column, same column, etc.).
  let hCount = 0;
  for (const col of model.componentGrid) {
    for (const op of col.components) {
      if (/** @type {any} */ (op).gate === "H") hCount++;
    }
  }
  assert.equal(
    hCount,
    1,
    `H must appear exactly once in the grid after commitAddControl; got ${hCount}`,
  );

  // The new control landed on q3 as expected.
  assert.deepEqual(
    hOp.controls.map((/** @type {any} */ c) => c.qubit),
    [3],
    "H must have exactly one control on q3",
  );

  // Exactly one renderFn call from commitAddControl.
  assert.equal(renderCalls, 1, "expected exactly one render after commit");

  dragController.dispose();
});

test("commitAddControl on a nested op does not duplicate when widening cascades to split the outer column", () => {
  // The nested cousin of the previous test. Adding a control to a
  // child of Foo widens Foo's `.targets` to enclose the new wire;
  // if that widened span overlaps a top-level sibling of Foo, the
  // top-level column splits. The legacy duplicate-split block in
  // commitAddControl looked at the IMMEDIATE column of the
  // selected op (the inner column inside Foo), not the top-level
  // column where the real collision lived — so it could have
  // duplicated the H inside Foo even though the visible collision
  // was at the top level. Pin both invariants: no duplicate at
  // either level.
  const fixture = buildFixture();
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 0 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "H",
                    targets: [{ qubit: 0 }],
                    dataAttributes: { location: "0,0-0,0" },
                  },
                ],
              },
            ],
            dataAttributes: { location: "0,0" },
          },
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 3 }],
            dataAttributes: { location: "0,1" },
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const { dragController } = makeController(fixture, model);

  const fooOp = /** @type {any} */ (model.componentGrid[0].components[0]);
  const hOp = /** @type {any} */ (fooOp.children[0].components[0]);
  dragController.startAddingControl(hOp);

  // q3 is the X's wire — adding a control there widens H to
  // q0..q3 → cascades up to widen Foo to q0..q3 → overlaps X
  // → top-level col 0 must split.
  const dropzone = fixture.overlay.querySelector(
    '[data-dropzone-wire="3"]:not(.dropzone)',
  );
  assert.ok(dropzone, "wire-pick dropzone for q3 must have been spawned");
  dropzone.dispatchEvent(new MouseEvent("click", { button: 0, bubbles: true }));

  // Top-level grid: [Foo] in col 0, [X] in col 1.
  assert.equal(
    model.componentGrid.length,
    2,
    `expected top-level column to split; got ${JSON.stringify(
      model.componentGrid.map((c) =>
        c.components.map((/** @type {any} */ op) => op.gate),
      ),
    )}`,
  );

  // H appears exactly once across the entire tree (no duplicate
  // at the inner OR outer level).
  let hCount = 0;
  let fooCount = 0;
  let xCount = 0;
  const walk = (/** @type {any} */ grid) => {
    for (const col of grid) {
      for (const op of col.components) {
        if (op.gate === "H") hCount++;
        if (op.gate === "Foo") fooCount++;
        if (op.gate === "X") xCount++;
        if (op.children) walk(op.children);
      }
    }
  };
  walk(model.componentGrid);
  assert.equal(hCount, 1, `H must appear exactly once; got ${hCount}`);
  assert.equal(fooCount, 1, `Foo must appear exactly once; got ${fooCount}`);
  assert.equal(xCount, 1, `X must appear exactly once; got ${xCount}`);

  dragController.dispose();
});

// ---------------------------------------------------------------
// hideInvalidDropzones / showAllDropzones — the producer-before-
// consumer dropzone filter and its reset cycle.
//
// `hideInvalidDropzones(selectedLocation)` is the user-facing
// surface that prevents the user from dropping a classically-
// conditional op into a column at-or-before its producing
// measurement (which would invert the producer→consumer ordering
// the renderer assumes). `showAllDropzones` is the reset half:
// shared by `hideInvalidDropzones` itself (so each drag starts
// from a clean slate) AND by the layer-mouseup teardown (so a
// canceled / non-rendering drag doesn't leave stale `display:none`
// marks behind for the next drag — including a toolbox drag,
// which never runs the filter at all).
//
// We exercise the methods directly via `/** @type {any} */` casts
// rather than driving them through `onGateMouseDown` so the tests
// can focus on the filter contract without a full gate-elem +
// LayoutMap fixture. The end-to-end mouse path is covered by the
// existing gate-mousedown tests.
// ---------------------------------------------------------------

/** Append a `.dropzone` rect with display:none preset (stale-mark fixture). */
function appendHiddenDropzone(dropzoneLayer, location, wire) {
  const dz = appendDropzone(dropzoneLayer, location, wire);
  /** @type {any} */ (dz).style.display = "none";
  return dz;
}

test("showAllDropzones clears stale display:none marks on every dropzone in the layer", () => {
  const fixture = buildFixture();
  const dzA = appendHiddenDropzone(fixture.dropzoneLayer, "0,0", 0);
  const dzB = appendHiddenDropzone(fixture.dropzoneLayer, "1,0", 0);
  // Pre-condition: both dropzones have display:none.
  assert.equal(/** @type {any} */ (dzA).style.display, "none");
  assert.equal(/** @type {any} */ (dzB).style.display, "none");

  const model = new CircuitModel(emptyCircuit(1));
  const { dragController } = makeController(fixture, model);

  /** @type {any} */ (dragController).showAllDropzones();

  // Empty string → inherit from CSS (i.e. visible). The reset
  // ditches the inline mark rather than swapping it for "block";
  // dropzone-layer-level `display:none` toggles via
  // `dropzoneLayer.style.display` still hide everything during
  // non-drag states.
  assert.equal(/** @type {any} */ (dzA).style.display, "");
  assert.equal(/** @type {any} */ (dzB).style.display, "");

  dragController.dispose();
});

test("hideInvalidDropzones hides dropzones whose location is not strictly after the external producer", () => {
  // Circuit: top-level col 0 has measurement M on q0 producing
  // result 0; col 1 has consumer Z on q1 with classical control
  // (q0, result 0). Drag the Z (selectedLocation "1,0"); the only
  // external producer is M at "0,0".
  //
  // Dropzones we lay down in the layer:
  //   "0,0" → producer.col(0) NOT < target.col(0) → HIDE
  //   "1,0" → producer.col(0) <   target.col(1) → keep
  //   "2,0" → producer.col(0) <   target.col(2) → keep
  const fixture = buildFixture();
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "measurement",
            gate: "Measure",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
            dataAttributes: { location: "0,0" },
          },
        ],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "Z",
            targets: [{ qubit: 1 }],
            controls: [{ qubit: 0, result: 0 }],
            dataAttributes: { location: "1,0" },
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const { dragController } = makeController(fixture, model);

  const dzAtZero = appendDropzone(fixture.dropzoneLayer, "0,0", 1);
  const dzAtOne = appendDropzone(fixture.dropzoneLayer, "1,0", 1);
  const dzAtTwo = appendDropzone(fixture.dropzoneLayer, "2,0", 1);

  /** @type {any} */ (dragController).hideInvalidDropzones("1,0");

  // The producer at col 0 means anything targeting col 0 is invalid.
  assert.equal(
    /** @type {any} */ (dzAtZero).style.display,
    "none",
    "drop at col 0 must be hidden — producer is at col 0",
  );
  assert.equal(
    /** @type {any} */ (dzAtOne).style.display,
    "",
    "drop at col 1 must stay visible — producer col 0 < target col 1",
  );
  assert.equal(
    /** @type {any} */ (dzAtTwo).style.display,
    "",
    "drop at col 2 must stay visible — producer col 0 < target col 2",
  );

  dragController.dispose();
});

test("hideInvalidDropzones with no external producers leaves every dropzone visible AND clears stale marks", () => {
  // Drag an op with no classical-control dependencies. The filter
  // pass must (a) leave every dropzone visible, AND (b) clear any
  // stale display:none marks left over from a prior drag — that's
  // the belt-and-suspenders reset at the top of the method.
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
  const { dragController } = makeController(fixture, model);

  // Drop two dropzones with stale display:none marks (simulating
  // leftover state from a prior drag that DID have external
  // producers).
  const dzStale1 = appendHiddenDropzone(fixture.dropzoneLayer, "0,0", 0);
  const dzStale2 = appendHiddenDropzone(fixture.dropzoneLayer, "1,0", 0);

  /** @type {any} */ (dragController).hideInvalidDropzones("0,0");

  assert.equal(
    /** @type {any} */ (dzStale1).style.display,
    "",
    "stale display:none from prior drag must be cleared",
  );
  assert.equal(/** @type {any} */ (dzStale2).style.display, "");

  dragController.dispose();
});

test("hideInvalidDropzones skips dropzones missing data-dropzone-location (defensive)", () => {
  // The pass reads `data-dropzone-location` off each `.dropzone`
  // and skips entries where the attribute is missing. Defensive
  // — every real dropzone gets the attr from `makeDropzoneBox` —
  // but the filter shouldn't crash if a stray `.dropzone` snuck
  // into the layer some other way (e.g. a future overlay element
  // that shares the class).
  const fixture = buildFixture();
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "measurement",
            gate: "Measure",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
            dataAttributes: { location: "0,0" },
          },
        ],
      },
      {
        components: [
          {
            kind: "unitary",
            gate: "Z",
            targets: [{ qubit: 1 }],
            controls: [{ qubit: 0, result: 0 }],
            dataAttributes: { location: "1,0" },
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const { dragController } = makeController(fixture, model);

  // A stray .dropzone with no location attr. Should be skipped
  // (untouched) by the filter loop.
  const stray = document.createElementNS(SVG_NS, "rect");
  stray.setAttribute("class", "dropzone");
  fixture.dropzoneLayer.appendChild(stray);

  assert.doesNotThrow(() =>
    /** @type {any} */ (dragController).hideInvalidDropzones("1,0"),
  );
  // Stray dropzone untouched (no display mark).
  assert.equal(/** @type {any} */ (stray).style.display, "");

  dragController.dispose();
});

test("container mouseup teardown clears stale per-dropzone display marks", () => {
  // Pairs with `showAllDropzones` and `hideInvalidDropzones`:
  // when a drag is canceled or its commit doesn't re-render
  // (e.g. drop landed in the same spot, deepEqual short-circuit),
  // the layer-level mouseup must wipe any `display:none` marks
  // the filter applied — otherwise the next drag (including a
  // toolbox drag, which never runs the filter) inherits them.
  const fixture = buildFixture();
  const dzHidden = appendHiddenDropzone(fixture.dropzoneLayer, "0,0", 0);
  const dzAlsoHidden = appendHiddenDropzone(fixture.dropzoneLayer, "1,0", 0);

  const model = new CircuitModel(emptyCircuit(1));
  const { dragController } = makeController(fixture, model);

  dispatchMouseUp(fixture.container);

  assert.equal(
    /** @type {any} */ (dzHidden).style.display,
    "",
    "container mouseup must clear stale display:none from a previous drag",
  );
  assert.equal(/** @type {any} */ (dzAlsoHidden).style.display, "");

  dragController.dispose();
});

// ---------------------------------------------------------------
// D4 Stage B — shift-extend lifecycle. Pins the contracts of the
// six private methods that own the shift-extend pathway:
//
//   - `setupShiftExtend`: top-level no-op vs internal-source arm.
//   - `spawnShiftExtendDropzones`: emits dropzones only for wires
//     OUTSIDE the parent group's current span, skips wires blocked
//     by ancestor-column siblings (B6), tags each dropzone with
//     `data-shift-extend="true"`, and is re-spawn-safe (subsequent
//     calls clear the prior spawn first).
//   - `clearShiftExtendDropzones`: removes shift-extend dropzones
//     from the DOM, leaves regular dropzones alone.
//   - `paintGhostBorder` / `clearGhostBorder`: append/replace a
//     single `.shift-extend-ghost` rect in the overlay layer.
//   - `tearDownShiftExtend`: clears dropzones, ghost border,
//     `_shiftExtendCtx`, and the document shift-key listeners.
//
// We invoke the methods directly via `/** @type {any} */` casts and
// stage `layoutMap.scopes` manually so each test can hold the geometry
// inputs constant. The end-to-end "press shift mid-drag" pathway
// would require a full keyboard-event harness on top of the existing
// gate-mousedown fixture; the unit-level coverage here is the
// contract surface other code actually depends on.
// ---------------------------------------------------------------

/**
 * Install a `LayoutScope` for `parentLoc` into the controller's
 * `ctx.layoutMap.scopes`. `columnXOffsets` defaults to a single
 * column so `spawnShiftExtendDropzones`' `totalCols = real + 1`
 * computes to 2 (one real + one trailing-append).
 */
function setScope(ctx, parentLoc, columnXOffsets = [100], columnWidths = [60]) {
  ctx.layoutMap.scopes.set(parentLoc, { columnXOffsets, columnWidths });
}

test("setupShiftExtend no-ops for a top-level source (depth < 2)", () => {
  // Top-level ops have no ancestor group to extend. Calling
  // setupShiftExtend with their `Location` must leave the controller
  // disarmed — no `_shiftExtendCtx`, no installed shift listeners.
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
  const { dragController } = makeController(fixture, model);

  /** @type {any} */ (dragController).setupShiftExtend(Location.parse("0,0"));

  assert.equal(/** @type {any} */ (dragController)._shiftExtendCtx, null);
  assert.equal(/** @type {any} */ (dragController)._onShiftDown, null);
  assert.equal(/** @type {any} */ (dragController)._onShiftUp, null);

  dragController.dispose();
});

test("setupShiftExtend arms _shiftExtendCtx and installs shift listeners for an internal-source drag", () => {
  // A child of an expanded group (depth=2). The controller must
  // capture the parent group's wire span + scope and install
  // document keydown/keyup listeners so the user can toggle the
  // shift-extend UI mid-drag.
  const fixture = buildFixture();
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            // Parent group spans wires 0..1.
            targets: [{ qubit: 0 }, { qubit: 1 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "H",
                    targets: [{ qubit: 0 }],
                    dataAttributes: { location: "0,0-0,0" },
                  },
                ],
              },
            ],
            dataAttributes: { location: "0,0" },
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const { dragController, ctx } = makeController(fixture, model);
  // setupShiftExtend looks up the IMMEDIATE parent's scope.
  setScope(ctx, "0,0");

  /** @type {any} */ (dragController).setupShiftExtend(
    Location.parse("0,0-0,0"),
  );

  const armed = /** @type {any} */ (dragController)._shiftExtendCtx;
  assert.ok(armed, "_shiftExtendCtx must be populated");
  assert.equal(armed.parentLoc, "0,0");
  assert.equal(armed.parentMinWire, 0);
  assert.equal(armed.parentMaxWire, 1);
  assert.ok(
    armed.parentScope.columnXOffsets,
    "parentScope must carry layout geometry",
  );

  // Shift-key listeners installed (the toggle pathway).
  assert.notEqual(
    /** @type {any} */ (dragController)._onShiftDown,
    null,
    "keydown listener must be installed",
  );
  assert.notEqual(
    /** @type {any} */ (dragController)._onShiftUp,
    null,
    "keyup listener must be installed",
  );

  dragController.dispose();
});

test("setupShiftExtend no-ops when the parent scope isn't in the LayoutMap (defensive)", () => {
  // Defensive — every expanded group's scope should be in the
  // LayoutMap, but the method must skip silently rather than
  // throw if it isn't.
  const fixture = buildFixture();
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 0 }, { qubit: 1 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
                ],
              },
            ],
            dataAttributes: { location: "0,0" },
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const { dragController } = makeController(fixture, model);
  // Intentionally do NOT register a scope for "0,0".

  /** @type {any} */ (dragController).setupShiftExtend(
    Location.parse("0,0-0,0"),
  );

  assert.equal(/** @type {any} */ (dragController)._shiftExtendCtx, null);
  assert.equal(/** @type {any} */ (dragController)._onShiftDown, null);

  dragController.dispose();
});

test("spawnShiftExtendDropzones emits dropzones only for wires outside the parent group's span", () => {
  // Parent group spans wires 0..1. `wireData` covers wires 0..4
  // (4 qubits + trailing ghost). Spawn should emit dropzones for
  // wires {2, 3, 4} only — wires {0, 1} are inside the span and
  // already covered by regular inner dropzones.
  //
  // Per-column count: 3 wires × 2 columns (1 real + 1 trailing) = 6.
  const fixture = buildFixture();
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 0 }, { qubit: 1 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "H",
                    targets: [{ qubit: 0 }],
                    dataAttributes: { location: "0,0-0,0" },
                  },
                ],
              },
            ],
            dataAttributes: { location: "0,0" },
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const { dragController, ctx } = makeController(fixture, model);
  setScope(ctx, "0,0");

  /** @type {any} */ (dragController).setupShiftExtend(
    Location.parse("0,0-0,0"),
  );
  /** @type {any} */ (dragController).spawnShiftExtendDropzones();

  const spawned = fixture.dropzoneLayer.querySelectorAll("[data-shift-extend]");
  // Wires {2, 3, 4} × 2 columns = 6.
  assert.equal(spawned.length, 6);

  const wires = new Set(
    Array.from(spawned).map((d) =>
      Number(d.getAttribute("data-dropzone-wire")),
    ),
  );
  assert.deepEqual(
    [...wires].sort((a, b) => a - b),
    [2, 3, 4],
  );

  dragController.dispose();
});

test("spawnShiftExtendDropzones skips wires blocked by ancestor-column siblings (B6)", () => {
  // Top-level col 0 contains both the parent group (wires 0..1) AND
  // a sibling X at wire 3. The B6 filter must mark wire 3 as
  // blocked because dropping a child of the parent group onto wire
  // 3 in any column would have nowhere to go in the top-level
  // column without colliding with X.
  //
  // Eligible outside-span wires: {2, 3, 4}. Blocked: {3}. Emitted: {2, 4}.
  // Per-column count: 2 wires × 2 columns = 4.
  const fixture = buildFixture();
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 0 }, { qubit: 1 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "H",
                    targets: [{ qubit: 0 }],
                    dataAttributes: { location: "0,0-0,0" },
                  },
                ],
              },
            ],
            dataAttributes: { location: "0,0" },
          },
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 3 }],
            dataAttributes: { location: "0,1" },
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const { dragController, ctx } = makeController(fixture, model);
  setScope(ctx, "0,0");

  /** @type {any} */ (dragController).setupShiftExtend(
    Location.parse("0,0-0,0"),
  );
  /** @type {any} */ (dragController).spawnShiftExtendDropzones();

  const spawned = fixture.dropzoneLayer.querySelectorAll("[data-shift-extend]");
  // 2 unblocked wires × 2 columns = 4.
  assert.equal(spawned.length, 4);

  const wires = new Set(
    Array.from(spawned).map((d) =>
      Number(d.getAttribute("data-dropzone-wire")),
    ),
  );
  assert.deepEqual(
    [...wires].sort((a, b) => a - b),
    [2, 4],
  );
  assert.ok(
    !wires.has(3),
    "wire 3 must be excluded — sibling X blocks it at the ancestor column",
  );

  dragController.dispose();
});

test("spawnShiftExtendDropzones tags every dropzone and is re-spawn-safe", () => {
  // Two contracts in one test (cheap to combine, hard to separate
  // meaningfully):
  //   1. Every spawned dropzone carries `data-shift-extend="true"`
  //      AND `data-dropzone-inter-column="false"` (so the mouseup
  //      handler doesn't insert a new column).
  //   2. Calling spawn twice in a row leaves the layer with one
  //      copy, not two — the method clears its prior spawn first.
  const fixture = buildFixture();
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 0 }, { qubit: 1 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "H",
                    targets: [{ qubit: 0 }],
                    dataAttributes: { location: "0,0-0,0" },
                  },
                ],
              },
            ],
            dataAttributes: { location: "0,0" },
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const { dragController, ctx } = makeController(fixture, model);
  setScope(ctx, "0,0");

  /** @type {any} */ (dragController).setupShiftExtend(
    Location.parse("0,0-0,0"),
  );

  /** @type {any} */ (dragController).spawnShiftExtendDropzones();
  const firstSpawn = fixture.dropzoneLayer.querySelectorAll(
    "[data-shift-extend]",
  );
  assert.ok(firstSpawn.length > 0, "first spawn must emit some dropzones");
  // Every dropzone is tagged correctly.
  for (const dz of firstSpawn) {
    assert.equal(dz.getAttribute("data-shift-extend"), "true");
    assert.equal(dz.getAttribute("data-dropzone-inter-column"), "false");
  }

  // Re-spawn: count must NOT double. (Idempotency / re-arm safety.)
  /** @type {any} */ (dragController).spawnShiftExtendDropzones();
  const secondSpawn = fixture.dropzoneLayer.querySelectorAll(
    "[data-shift-extend]",
  );
  assert.equal(
    secondSpawn.length,
    firstSpawn.length,
    "second spawn must replace, not append",
  );

  dragController.dispose();
});

test("paintGhostBorder appends a .shift-extend-ghost rect and replaces a prior one", () => {
  // Each `paintGhostBorder` call clears the existing ghost before
  // appending a new one, so the overlay never carries two ghost
  // rects at once (would be visible as a doubled halo).
  const fixture = buildFixture();
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 0 }, { qubit: 1 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "H",
                    targets: [{ qubit: 0 }],
                    dataAttributes: { location: "0,0-0,0" },
                  },
                ],
              },
            ],
            dataAttributes: { location: "0,0" },
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const { dragController, ctx } = makeController(fixture, model);
  setScope(ctx, "0,0");
  /** @type {any} */ (dragController).setupShiftExtend(
    Location.parse("0,0-0,0"),
  );

  // First paint — one ghost.
  /** @type {any} */ (dragController).paintGhostBorder(2, 0);
  let ghosts = fixture.overlay.querySelectorAll(".shift-extend-ghost");
  assert.equal(ghosts.length, 1);
  const firstGhost = ghosts[0];

  // Second paint at a different wire — old ghost replaced, not appended.
  /** @type {any} */ (dragController).paintGhostBorder(0, 0);
  ghosts = fixture.overlay.querySelectorAll(".shift-extend-ghost");
  assert.equal(ghosts.length, 1, "second paint must replace, not append");
  assert.notEqual(
    ghosts[0],
    firstGhost,
    "new ghost element should be a fresh node",
  );

  // clearGhostBorder wipes it.
  /** @type {any} */ (dragController).clearGhostBorder();
  ghosts = fixture.overlay.querySelectorAll(".shift-extend-ghost");
  assert.equal(ghosts.length, 0);

  dragController.dispose();
});

test("tearDownShiftExtend clears dropzones, ghost border, _shiftExtendCtx, and shift listeners", () => {
  // Full teardown chain. After teardown the controller is back to
  // its initial unarmed state — no dropzones in the DOM, no ghost
  // border, no listener refs.
  const fixture = buildFixture();
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 0 }, { qubit: 1 }],
            children: [
              {
                components: [
                  {
                    kind: "unitary",
                    gate: "H",
                    targets: [{ qubit: 0 }],
                    dataAttributes: { location: "0,0-0,0" },
                  },
                ],
              },
            ],
            dataAttributes: { location: "0,0" },
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const { dragController, ctx } = makeController(fixture, model);
  setScope(ctx, "0,0");
  /** @type {any} */ (dragController).setupShiftExtend(
    Location.parse("0,0-0,0"),
  );
  /** @type {any} */ (dragController).spawnShiftExtendDropzones();
  /** @type {any} */ (dragController).paintGhostBorder(2, 0);

  // Sanity: state was actually armed.
  assert.notEqual(/** @type {any} */ (dragController)._shiftExtendCtx, null);
  assert.ok(
    fixture.dropzoneLayer.querySelectorAll("[data-shift-extend]").length > 0,
  );
  assert.equal(
    fixture.overlay.querySelectorAll(".shift-extend-ghost").length,
    1,
  );

  /** @type {any} */ (dragController).tearDownShiftExtend();

  // Everything cleared.
  assert.equal(/** @type {any} */ (dragController)._shiftExtendCtx, null);
  assert.equal(/** @type {any} */ (dragController)._onShiftDown, null);
  assert.equal(/** @type {any} */ (dragController)._onShiftUp, null);
  assert.deepEqual(
    /** @type {any} */ (dragController)._shiftExtendDropzones,
    [],
  );
  assert.equal(
    fixture.dropzoneLayer.querySelectorAll("[data-shift-extend]").length,
    0,
    "shift-extend dropzones must be gone from the DOM",
  );
  assert.equal(
    fixture.overlay.querySelectorAll(".shift-extend-ghost").length,
    0,
    "ghost border must be gone from the overlay",
  );

  // Idempotent — calling teardown a second time must not throw.
  assert.doesNotThrow(() =>
    /** @type {any} */ (dragController).tearDownShiftExtend(),
  );

  dragController.dispose();
});

// ---------------------------------------------------------------
// Wave 4 — remaining dragController paths. Each test pins a flow
// that has its own model-side contract distinct from the drop /
// drag-out-delete paths already covered above.
//
//   - Ctrl+drag clone: source op stays put, a copy lands at the
//     target. The `selectedWire` is passed through as the source
//     wire so multi-target/group clones shift every register by
//     the same delta.
//   - Document mouseup with `!dragging` is a no-op — protects
//     mouseup events from unrelated UI from accidentally mutating
//     the model.
//   - Qubit-drag-off (no `selectedOperation`, only `selectedWire`):
//     delegates to `qubitController.removeQubitLineWithConfirmation`.
//   - movingControl drag-out: removes ONLY the dragged control via
//     `removeControl`, not the whole op via `_deleteOperationWithConfirmation`.
//   - Document mousedown clears any wire dropzones in the SVG —
//     the cleanup hook the add-control / qubit-label flows lean on
//     so a click elsewhere dismisses their wire-pick UI.
// ---------------------------------------------------------------

test("Ctrl+drag clone of a regular op: source stays, copy lands at the target", () => {
  // Source: H on q0 in col 0. Target: an inter-column dropzone at
  // "0,0" — i.e. "insert a new column before column 0" — on wire 0.
  // Copying (Ctrl) → `addOperation` is called instead of
  // `_moveOperationWithConfirmation`; source op must remain in place.
  // We target an inter-column slot so the new clone lands in a
  // fresh column without colliding with (or replacing) the source.
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

  // IMPORTANT: append the target dropzone BEFORE constructing the
  // controller. `installDropzoneListeners` wires `mouseup` listeners
  // at construction time only — dropzones added later are inert.
  const targetDz = appendDropzone(
    fixture.dropzoneLayer,
    "0,0",
    0,
    /* interColumn */ true,
  );

  let renderCalls = 0;
  const { interaction, dragController } = makeController(fixture, model, {
    renderFn: () => {
      renderCalls++;
    },
  });

  // Simulate the in-progress drag of the H op.
  interaction.selectedOperation = /** @type {any} */ (
    model.componentGrid[0].components[0]
  );
  interaction.selectedWire = 0;
  interaction.dragging = true;

  // With ctrlKey set on the mouseup, `onDropzoneMouseUp` takes the
  // copying branch: `addOperation` is called with the source's
  // `selectedWire` as the source wire, so the original H stays put
  // and a deep-copied clone lands in the newly-inserted col 0.
  targetDz.dispatchEvent(
    new MouseEvent("mouseup", { ctrlKey: true, bubbles: true }),
  );

  return Promise.resolve().then(() => {
    // Two gates now: the original H and the clone.
    const allGates = [];
    for (const col of model.componentGrid) {
      for (const op of col.components) {
        allGates.push(/** @type {any} */ (op).gate);
      }
    }
    const hCount = allGates.filter((g) => g === "H").length;
    assert.equal(
      hCount,
      2,
      `expected 2 H gates after Ctrl+drag clone, got ${hCount} (${allGates})`,
    );
    // renderFn fires from the deepEqual block (grid changed).
    assert.equal(renderCalls, 1);
    // Transient cleared.
    assert.equal(interaction.selectedOperation, null);

    dragController.dispose();
  });
});

test("document mouseup with !dragging is a no-op (no model change, no render)", () => {
  // Rogue mouseup events from unrelated UI must not trigger the
  // drag-out-delete branch. The guard is `interaction.dragging`,
  // which the in-progress-drag tests above all set to `true`.
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

  // Default state: dragging is false; selectedOperation is null.
  assert.equal(interaction.dragging, false);

  dispatchMouseUp(document);

  // Model is untouched; renderFn was never called.
  assert.equal(model.componentGrid.length, 1);
  assert.equal(model.componentGrid[0].components.length, 1);
  assert.equal(renderCalls, 0);

  dragController.dispose();
});

test("qubit-drag-off (only selectedWire, no selectedOperation) removes the qubit line", () => {
  // The drag-controller's document-mouseup handler delegates to
  // `qubitController.removeQubitLineWithConfirmation` when a drag
  // ends off-circuit with `selectedOperation == null` but
  // `selectedWire != null` — i.e. a qubit label was dragged off.
  //
  // The qubit controller skips the confirmation prompt when the
  // qubit has zero ops attached (per `removeQubitLineWithConfirmation`),
  // so we test against an unused qubit and assert the model shrinks.
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

  // Simulate an in-progress qubit-label drag.
  interaction.selectedOperation = null;
  interaction.selectedWire = 1; // q1 — no ops attached
  interaction.dragging = true;
  interaction.mouseUpOnCircuit = false;

  dispatchMouseUp(document);

  // q1 removed → only q0 remains.
  assert.equal(model.qubits.length, 1);
  assert.equal(model.qubits[0].id, 0);
  // Render fired (from the doRemove fast-path inside the qubit controller).
  assert.equal(renderCalls, 1);

  dragController.dispose();
});

test("drag-off with movingControl removes just the dragged control via removeControl (not the whole op)", () => {
  // The movingControl branch of the document-mouseup drag-out path
  // routes through `removeControl(selectedOperation, selectedWire)`,
  // NOT `_deleteOperationWithConfirmation`. The op must remain in
  // the grid with its `.controls` array shortened by the one we
  // dragged off.
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
            // The control on q1 is the one being dragged off.
            controls: [{ qubit: 1 }],
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

  interaction.selectedOperation = /** @type {any} */ (
    model.componentGrid[0].components[0]
  );
  interaction.selectedWire = 1; // the control's wire
  interaction.movingControl = true;
  interaction.dragging = true;
  interaction.mouseUpOnCircuit = false;

  dispatchMouseUp(document);

  // The H op is still there — only the control was removed.
  assert.equal(model.componentGrid.length, 1);
  const op = /** @type {any} */ (model.componentGrid[0].components[0]);
  assert.equal(op.gate, "H");
  // `removeControl` empties the controls array when the last
  // control is removed (some paths null it; either is a "no
  // controls" state).
  const remainingControls = op.controls ?? [];
  assert.equal(
    remainingControls.length,
    0,
    `expected zero remaining controls, got ${JSON.stringify(remainingControls)}`,
  );
  // One render fired from the removeControl branch.
  assert.equal(renderCalls, 1);

  dragController.dispose();
});

test("document mousedown clears wire dropzones in the SVG", () => {
  // The wire-pick UIs (`startAddingControl`, qubit-label drag) drop
  // `.dropzone-full-wire` rects into the SVG. The document-mousedown
  // handler clears them so clicking anywhere outside the wire-pick
  // dropzones dismisses the flow.
  const fixture = buildFixture();
  const model = new CircuitModel(emptyCircuit(1));
  const { dragController } = makeController(fixture, model);

  // Inject two wire dropzones directly — what `createWireDropzone`
  // produces, minus the wiring.
  const wireDz1 = document.createElementNS(SVG_NS, "rect");
  wireDz1.setAttribute("class", "dropzone-full-wire");
  fixture.svg.appendChild(wireDz1);
  const wireDz2 = document.createElementNS(SVG_NS, "rect");
  wireDz2.setAttribute("class", "dropzone-full-wire");
  fixture.svg.appendChild(wireDz2);
  // A regular `.dropzone` for contrast — must NOT be removed.
  const regularDz = appendDropzone(fixture.dropzoneLayer, "0,0", 0);

  dispatchMouseDown(document);

  assert.equal(
    fixture.svg.querySelectorAll(".dropzone-full-wire").length,
    0,
    "wire dropzones must be cleared on document mousedown",
  );
  // Regular dropzone left alone.
  assert.ok(regularDz.parentNode, "regular .dropzone must remain attached");

  dragController.dispose();
});
