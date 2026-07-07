// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// contextMenu tests — direct coverage for `addContextMenuToHostElem`
// in `editor/contextMenu.ts`. Each test wires a small SVG fixture
// (a `<g data-location="...">` with a host shape inside) to a
// minimal `CircuitEvents` stub, dispatches a `contextmenu`
// MouseEvent, and asserts on the rendered `.context-menu` items.
//
// The stub only implements the five members the menu reads:
// `componentGrid`, `model`, `renderFn`, `_startAddingControl`,
// `_startRemovingControl`. The real class delegates `componentGrid`
// to `model.componentGrid`; tests do the same on the stub.

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { addContextMenuToHostElem } from "../../dist/ux/circuit-vis/editor/contextMenu.js";
import { build, circuit, gate, meas, qubits } from "./_helpers.mjs";
/** @typedef {import("../../dist/ux/circuit-vis/data/circuitModel.js").CircuitModel} CircuitModel */

/** @type {JSDOM | null} */
let jsdom = null;

beforeEach(() => {
  jsdom = new JSDOM(`<!doctype html><html><body></body></html>`);
  globalThis.window = jsdom.window;
  globalThis.document = jsdom.window.document;
  globalThis.HTMLElement = jsdom.window.HTMLElement;
  globalThis.SVGElement = jsdom.window.SVGElement;
  globalThis.MouseEvent = jsdom.window.MouseEvent;
});

afterEach(() => {
  jsdom?.window.close();
  jsdom = null;
});

const SVG_NS = "http://www.w3.org/2000/svg";

/**
 * Build a stub `CircuitEvents` carrying just the five members
 * `addContextMenuToHostElem` consults, plus spies on the two
 * delegating helpers so tests can assert they were invoked.
 *
 * @param {CircuitModel} model
 */
function makeStubEvents(model) {
  const startAddingCalls = /** @type {any[]} */ ([]);
  const startRemovingCalls = /** @type {any[]} */ ([]);
  const renderCalls = { count: 0 };
  const stub = {
    model,
    // Mirror `CircuitEvents.componentGrid`'s delegation to
    // `model.componentGrid` so `findOperation` resolves correctly.
    get componentGrid() {
      return model.componentGrid;
    },
    renderFn: () => {
      renderCalls.count++;
    },
    _startAddingControl: (/** @type {any} */ op) => {
      startAddingCalls.push(op);
    },
    _startRemovingControl: (/** @type {any} */ op) => {
      startRemovingCalls.push(op);
    },
  };
  return { stub, startAddingCalls, startRemovingCalls, renderCalls };
}

/**
 * Build a `<g data-location="...">` wrapper containing a host shape
 * (rect for a gate body, circle for a control dot). The wrapper's
 * `data-location` is what `findGateElem` resolves via `closest()`.
 *
 * @param {string} location  - "0,0" etc.
 * @param {"body" | "control-dot"} hostKind
 * @param {number} [wireIdx] - only used for control-dot
 */
function buildGateFixture(location, hostKind, wireIdx) {
  const svg = document.createElementNS(SVG_NS, "svg");
  document.body.appendChild(svg);
  const wrapper = document.createElementNS(SVG_NS, "g");
  wrapper.setAttribute("data-location", location);
  svg.appendChild(wrapper);

  /** @type {SVGGraphicsElement} */
  let host;
  if (hostKind === "control-dot") {
    host = /** @type {any} */ (document.createElementNS(SVG_NS, "circle"));
    host.classList.add("control-dot");
    if (wireIdx != null) host.setAttribute("data-wire", String(wireIdx));
  } else {
    host = /** @type {any} */ (document.createElementNS(SVG_NS, "rect"));
    host.classList.add("gate-h");
  }
  wrapper.appendChild(host);
  return { svg, wrapper, host };
}

/**
 * Dispatch a `contextmenu` event on the host element. The builder
 * reads `ev.clientX` / `ev.clientY` for positioning.
 *
 * @param {SVGGraphicsElement} host
 */
function rightClick(host) {
  host.dispatchEvent(
    new MouseEvent("contextmenu", {
      bubbles: true,
      cancelable: true,
      clientX: 50,
      clientY: 50,
    }),
  );
}

/** Read the rendered menu's option labels in order, or [] if no menu. */
function getMenuLabels() {
  const menu = document.querySelector(".context-menu");
  if (!menu) return null;
  return Array.from(menu.querySelectorAll(".context-menu-option")).map(
    (el) => el.textContent ?? "",
  );
}

// ---------------------------------------------------------------------------
// kind-driven branches
// ---------------------------------------------------------------------------

test("addContextMenuToHostElem: measurement gate shows ONLY Delete", () => {
  // Measurements have no adjoint, controls, or params. The kind ===
  // "measurement" branch offers only delete.
  const model = build(circuit(qubits(1, { 0: 1 }), [[meas(0)]]));
  const { stub } = makeStubEvents(model);
  const { host } = buildGateFixture("0,0", "body");
  addContextMenuToHostElem(/** @type {any} */ (stub), host);

  rightClick(host);

  assert.deepEqual(getMenuLabels(), ["Delete"]);
});

test("addContextMenuToHostElem: ket gate shows ONLY Delete", () => {
  // Kets are treated like measurements for menu purposes — no
  // controls, params, or adjoint.
  const model = build(
    circuit(1, [[{ kind: "ket", gate: "|0〉", targets: [{ qubit: 0 }] }]]),
  );
  const { stub } = makeStubEvents(model);
  const { host } = buildGateFixture("0,0", "body");
  addContextMenuToHostElem(/** @type {any} */ (stub), host);

  rightClick(host);

  assert.deepEqual(getMenuLabels(), ["Delete"]);
});

// ---------------------------------------------------------------------------
// control-dot host
// ---------------------------------------------------------------------------

test("addContextMenuToHostElem: control-dot on a SIMPLE unitary shows ONLY Remove control", () => {
  // Right-clicking the control dot of an ordinary CNOT offers only
  // "Remove control"; gestures affecting the whole gate are reached
  // from the gate body.
  const model = build(circuit(2, [[gate("X", 1, { ctrls: [0] })]]));
  const { stub } = makeStubEvents(model);
  // Host is the control dot on wire 0. The wrapper's
  // `data-location` points at the gate's grid coords ("0,0"); the
  // dot itself carries `data-wire`.
  const { host } = buildGateFixture("0,0", "control-dot", 0);
  addContextMenuToHostElem(/** @type {any} */ (stub), host);

  rightClick(host);

  assert.deepEqual(getMenuLabels(), ["Remove control"]);
});

test("addContextMenuToHostElem: control-dot on a MULTI-TARGET unitary shows NO menu items", () => {
  // Control-dot menu on a body that satisfies
  // `_isMultiTargetOrGroup` mirrors the action layer's refusal —
  // no items appended, no fallback to body-style items. The menu
  // element is created but empty.
  const model = build(
    circuit(3, [
      [
        {
          kind: "unitary",
          gate: "MyMultiTarget",
          targets: [{ qubit: 1 }, { qubit: 2 }],
          controls: [{ qubit: 0 }],
        },
      ],
    ]),
  );
  const { stub } = makeStubEvents(model);
  const { host } = buildGateFixture("0,0", "control-dot", 0);
  addContextMenuToHostElem(/** @type {any} */ (stub), host);

  rightClick(host);

  // Empty menu — no `.context-menu-option` children.
  assert.deepEqual(
    getMenuLabels(),
    [],
    "multi-target body + control-dot host yields an empty menu",
  );
});

// ---------------------------------------------------------------------------
// X-gate special-case ordering
// ---------------------------------------------------------------------------

test("addContextMenuToHostElem: X gate WITHOUT controls shows [Add Control, Delete]", () => {
  // X is special-cased: no Toggle Adjoint (X† == X) and no Edit
  // Argument (no params). Order is "Add Control, [Remove Control,]
  // Delete".
  const model = build(circuit(1, [[gate("X", 0)]]));
  const { stub } = makeStubEvents(model);
  const { host } = buildGateFixture("0,0", "body");
  addContextMenuToHostElem(/** @type {any} */ (stub), host);

  rightClick(host);

  assert.deepEqual(getMenuLabels(), ["Add Control", "Delete"]);
});

test("addContextMenuToHostElem: X gate WITH controls shows [Add Control, Remove Control, Delete]", () => {
  // Same X branch with an existing control — Remove Control is
  // inserted between Add Control and Delete.
  const model = build(circuit(2, [[gate("X", 1, { ctrls: [0] })]]));
  const { stub } = makeStubEvents(model);
  const { host } = buildGateFixture("0,0", "body");
  addContextMenuToHostElem(/** @type {any} */ (stub), host);

  rightClick(host);

  assert.deepEqual(getMenuLabels(), [
    "Add Control",
    "Remove Control",
    "Delete",
  ]);
});

// ---------------------------------------------------------------------------
// Multi-target unitaries, parameterized unitaries, general unitary
// ---------------------------------------------------------------------------

test("addContextMenuToHostElem: multi-target unitary drops Add/Remove Control", () => {
  // Any non-X unitary with `targets.length > 1` must not surface
  // control authoring. The body still gets Toggle Adjoint (not a
  // group) and Delete.
  const model = build(
    circuit(2, [
      [
        {
          kind: "unitary",
          gate: "SWAP",
          targets: [{ qubit: 0 }, { qubit: 1 }],
        },
      ],
    ]),
  );
  const { stub } = makeStubEvents(model);
  const { host } = buildGateFixture("0,0", "body");
  addContextMenuToHostElem(/** @type {any} */ (stub), host);

  rightClick(host);

  assert.deepEqual(
    getMenuLabels(),
    ["Toggle Adjoint", "Delete"],
    "multi-target unitary has no Add Control / Remove Control",
  );
});

test("addContextMenuToHostElem: group drops Toggle Adjoint", () => {
  // An op with `children != null` must not surface Toggle Adjoint.
  // Groups also satisfy `_isMultiTargetOrGroup`, so control
  // authoring is suppressed too. Net menu for a param-less group:
  // just Delete.
  const model = build(
    circuit(1, [
      [
        {
          kind: "unitary",
          gate: "MyGroup",
          targets: [{ qubit: 0 }],
          children: [
            {
              components: [
                { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] },
              ],
            },
          ],
        },
      ],
    ]),
  );
  const { stub } = makeStubEvents(model);
  const { host } = buildGateFixture("0,0", "body");
  addContextMenuToHostElem(/** @type {any} */ (stub), host);

  rightClick(host);

  assert.deepEqual(
    getMenuLabels(),
    ["Delete"],
    "a param-less group has no Toggle Adjoint / Add / Remove Control",
  );
});

test("addContextMenuToHostElem: ordinary unitary with params shows [Toggle Adjoint, Add Control, Edit Argument, Delete]", () => {
  // General-case body menu without controls. Edit Argument only
  // appears when `params.length > 0`.
  const model = build(
    circuit(1, [
      [
        {
          kind: "unitary",
          gate: "Rx",
          targets: [{ qubit: 0 }],
          params: [{ name: "theta", type: "Double" }],
          args: ["0.0"],
        },
      ],
    ]),
  );
  const { stub } = makeStubEvents(model);
  const { host } = buildGateFixture("0,0", "body");
  addContextMenuToHostElem(/** @type {any} */ (stub), host);

  rightClick(host);

  assert.deepEqual(getMenuLabels(), [
    "Toggle Adjoint",
    "Add Control",
    "Edit Argument",
    "Delete",
  ]);
});

test("addContextMenuToHostElem: ordinary unitary with controls + params shows the full menu including Remove Control", () => {
  // Adds an existing control to the prior fixture: Remove Control
  // appears between Add Control and Edit Argument.
  const model = build(
    circuit(2, [
      [
        {
          kind: "unitary",
          gate: "Ry",
          targets: [{ qubit: 1 }],
          controls: [{ qubit: 0 }],
          params: [{ name: "theta", type: "Double" }],
          args: ["0.0"],
        },
      ],
    ]),
  );
  const { stub } = makeStubEvents(model);
  const { host } = buildGateFixture("0,0", "body");
  addContextMenuToHostElem(/** @type {any} */ (stub), host);

  rightClick(host);

  assert.deepEqual(getMenuLabels(), [
    "Toggle Adjoint",
    "Add Control",
    "Remove Control",
    "Edit Argument",
    "Delete",
  ]);
});

// ---------------------------------------------------------------------------
// Menu lifecycle
// ---------------------------------------------------------------------------

test("addContextMenuToHostElem: opening a second time replaces the first menu (no DOM duplication)", () => {
  // The builder removes any existing `.context-menu` before
  // appending the new one, so a double right-click doesn't stack
  // menus.
  const model = build(circuit(1, [[gate("H", 0)]]));
  const { stub } = makeStubEvents(model);
  const { host } = buildGateFixture("0,0", "body");
  addContextMenuToHostElem(/** @type {any} */ (stub), host);

  rightClick(host);
  rightClick(host);

  assert.equal(
    document.querySelectorAll(".context-menu").length,
    1,
    "only one menu should be present after two right-clicks",
  );
});

test("addContextMenuToHostElem: outside-click closes the menu", () => {
  // The document-level `click` listener is registered with
  // `{ once: true }` and removes the menu on the next click anywhere
  // in the page — the same path that closes the menu after the user
  // picks an item.
  const model = build(circuit(1, [[gate("H", 0)]]));
  const { stub } = makeStubEvents(model);
  const { host } = buildGateFixture("0,0", "body");
  addContextMenuToHostElem(/** @type {any} */ (stub), host);

  rightClick(host);
  assert.ok(document.querySelector(".context-menu"), "menu should be open");

  // Simulate an outside click — anywhere in the document.
  document.dispatchEvent(new MouseEvent("click", { bubbles: true }));

  assert.equal(
    document.querySelector(".context-menu"),
    null,
    "menu should be removed by the outside-click handler",
  );
});

test("addContextMenuToHostElem: clicking Add Control invokes _startAddingControl with the selected op", () => {
  // The Add Control item's click handler calls
  // `circuitEvents._startAddingControl(selectedOperation)`.
  const op = { kind: "unitary", gate: "H", targets: [{ qubit: 0 }] };
  const model = build(circuit(1, [[/** @type {any} */ (op)]]));
  const { stub, startAddingCalls } = makeStubEvents(model);
  const { host } = buildGateFixture("0,0", "body");
  addContextMenuToHostElem(/** @type {any} */ (stub), host);

  rightClick(host);
  const menu = document.querySelector(".context-menu");
  assert.ok(menu);
  const items = Array.from(menu.querySelectorAll(".context-menu-option"));
  const addCtrl = items.find((el) => el.textContent === "Add Control");
  assert.ok(addCtrl, "Add Control item should be present");
  /** @type {HTMLElement} */ (addCtrl).click();

  assert.equal(startAddingCalls.length, 1);
  // findOperation returns the live reference, not a clone, so
  // identity matches the op on the grid.
  assert.equal(startAddingCalls[0], model.componentGrid[0].components[0]);
});
