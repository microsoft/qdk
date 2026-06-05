// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// contextMenu tests — direct coverage for `addContextMenuToHostElem`
// in `editor/contextMenu.ts`. This wires a small SVG fixture (a
// `<g data-location="...">` with a host shape inside) to a
// minimal `CircuitEvents`-shaped stub, dispatches a `contextmenu`
// MouseEvent, and asserts on the rendered `.context-menu` items.
//
// Pinned branches:
//
//   - **Measurement / ket kinds** → only "Delete" is offered. No
//     adjoint, no control authoring, no Edit Argument.
//   - **Control-dot host on a non-multi-target parent** → only
//     "Remove control". (Authoring a NEW control from a
//     control-dot context menu wouldn't make sense; toggling
//     adjoint, editing args, or deleting the whole op are
//     reached via the target's gate body, not the dot.)
//   - **Control-dot host on a multi-target / group parent**
//     (the **B5** gate) → no items appended at all. Authoring +
//     removing controls on those bodies is gated at the action
//     layer; the menu must not expose a no-op affordance.
//   - **X-gate ordering** — special-cased to "Add Control",
//     "Remove Control" (if any), "Delete". No Toggle Adjoint,
//     no Edit Argument (X has no params, and X† == X visually).
//   - **Multi-target unitary** (the **M5** gate) → "Toggle
//     Adjoint" + "Delete" but no Add/Remove Control.
//   - **Group / `children != null`** (the **M7** gate) → no
//     Toggle Adjoint; control authoring also suppressed because
//     groups satisfy `_isMultiTargetOrGroup`.
//   - **Ordinary unitary with controls + params** → Toggle
//     Adjoint, Add Control, Remove Control, Edit Argument,
//     Delete — full menu.
//   - **Re-open replaces the prior menu** — opening a second
//     time removes the first `.context-menu` so two never
//     coexist in the DOM.
//   - **Outside-click closes the menu** — the document-level
//     `click` listener (registered `{ once: true }`) removes
//     the menu the next time anything in the page is clicked.
//
// The stub `CircuitEvents` only needs the five members
// `addContextMenuToHostElem` actually reads: `componentGrid`,
// `model`, `renderFn`, `_startAddingControl`,
// `_startRemovingControl`. The real class delegates
// `componentGrid` to `model.componentGrid`; here we just set both
// from the same circuit instance.

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { CircuitModel } from "../../dist/ux/circuit-vis/data/circuitModel.js";
import { addContextMenuToHostElem } from "../../dist/ux/circuit-vis/editor/contextMenu.js";

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
    // The real `CircuitEvents.componentGrid` getter delegates to
    // `model.componentGrid`; replicate that here so `findOperation`
    // resolves correctly when the menu opens.
    get componentGrid() {
      return model.componentGrid;
    },
    renderFn: () => {
      renderCalls.count++;
    },
    _startAddingControl: (op) => {
      startAddingCalls.push(op);
    },
    _startRemovingControl: (op) => {
      startRemovingCalls.push(op);
    },
  };
  return { stub, startAddingCalls, startRemovingCalls, renderCalls };
}

/**
 * Build a `<g data-location="...">` wrapper containing a host
 * shape (rect for a gate body, circle for a control dot). Returns
 * the wrapper plus the host element to install the listener on.
 *
 * The wrapper carries `data-location` so `findGateElem` (a
 * `closest("[data-location]")` walk from the host) resolves to
 * the right enclosing `<g>` and the menu reads the right
 * operation from the grid.
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
 * Dispatch a `contextmenu` event on the host element, mirroring
 * what the browser does on right-click. The menu builder reads
 * `ev.clientX` / `ev.clientY` for positioning so we provide both.
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
  // Measurements don't have adjoint, controls, or params — the
  // only meaningful action is delete. The menu branch is keyed
  // off `selectedOperation.kind === "measurement"`.
  const model = new CircuitModel({
    qubits: [{ id: 0, numResults: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "measurement",
            gate: "Measure",
            qubits: [{ qubit: 0 }],
            results: [{ qubit: 0, result: 0 }],
          },
        ],
      },
    ],
  });
  const { stub } = makeStubEvents(model);
  const { host } = buildGateFixture("0,0", "body");
  addContextMenuToHostElem(/** @type {any} */ (stub), host);

  rightClick(host);

  assert.deepEqual(getMenuLabels(), ["Delete"]);
});

test("addContextMenuToHostElem: ket gate shows ONLY Delete", () => {
  // Ket initializers are treated like measurements for menu
  // purposes — no controls, no params, no adjoint. Pin the kind
  // === "ket" branch.
  const model = new CircuitModel({
    qubits: [{ id: 0 }],
    componentGrid: [
      {
        components: [
          {
            kind: "ket",
            gate: "|0〉",
            targets: [{ qubit: 0 }],
          },
        ],
      },
    ],
  });
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
  // Right-clicking the control dot of an ordinary CNOT (single
  // target on q1, single control on q0): the menu offers only
  // "Remove control" — gestures that affect the whole gate are
  // reached from the gate body, not the dot.
  const model = new CircuitModel({
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 1 }],
            controls: [{ qubit: 0 }],
          },
        ],
      },
    ],
  });
  const { stub } = makeStubEvents(model);
  // Host is the control dot on wire 0 (the control's wire). The
  // wrapper's `data-location` still points at the gate's grid
  // coords ("0,0"); the dot itself just carries `data-wire`.
  const { host } = buildGateFixture("0,0", "control-dot", 0);
  addContextMenuToHostElem(/** @type {any} */ (stub), host);

  rightClick(host);

  assert.deepEqual(getMenuLabels(), ["Remove control"]);
});

test("addContextMenuToHostElem: control-dot on a MULTI-TARGET unitary shows NO menu items (B5)", () => {
  // B5: control-dot context menu on an op that satisfies
  // `_isMultiTargetOrGroup` mirrors the action layer's refusal —
  // no "Remove control", and no fallback to body-style items
  // either. The menu element itself is created but empty.
  const model = new CircuitModel({
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "MyMultiTarget",
            targets: [{ qubit: 1 }, { qubit: 2 }],
            controls: [{ qubit: 0 }],
          },
        ],
      },
    ],
  });
  const { stub } = makeStubEvents(model);
  const { host } = buildGateFixture("0,0", "control-dot", 0);
  addContextMenuToHostElem(/** @type {any} */ (stub), host);

  rightClick(host);

  // Empty menu — no `.context-menu-option` children.
  assert.deepEqual(
    getMenuLabels(),
    [],
    "B5: multi-target body + control-dot host yields an empty menu",
  );
});

// ---------------------------------------------------------------------------
// X-gate special-case ordering
// ---------------------------------------------------------------------------

test("addContextMenuToHostElem: X gate WITHOUT controls shows [Add Control, Delete]", () => {
  // X is special-cased: no Toggle Adjoint (X† == X), no Edit
  // Argument (no params). Ordering is "Add Control, [Remove
  // Control,] Delete" — distinct from the general unitary order
  // which has Toggle Adjoint first.
  const model = new CircuitModel({
    qubits: [{ id: 0 }],
    componentGrid: [
      {
        components: [{ kind: "unitary", gate: "X", targets: [{ qubit: 0 }] }],
      },
    ],
  });
  const { stub } = makeStubEvents(model);
  const { host } = buildGateFixture("0,0", "body");
  addContextMenuToHostElem(/** @type {any} */ (stub), host);

  rightClick(host);

  assert.deepEqual(getMenuLabels(), ["Add Control", "Delete"]);
});

test("addContextMenuToHostElem: X gate WITH controls shows [Add Control, Remove Control, Delete]", () => {
  // Same X branch with an existing control — Remove Control is
  // inserted between Add Control and Delete. Still no Toggle
  // Adjoint / Edit Argument.
  const model = new CircuitModel({
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 1 }],
            controls: [{ qubit: 0 }],
          },
        ],
      },
    ],
  });
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
// M5 / M7 / general unitary
// ---------------------------------------------------------------------------

test("addContextMenuToHostElem: multi-target unitary (M5) drops Add/Remove Control", () => {
  // M5: any non-X unitary whose `targets.length > 1` MUST NOT
  // surface control authoring. The body still gets Toggle Adjoint
  // (it's not a group), no Edit Argument (no params), and Delete.
  const model = new CircuitModel({
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "SWAP",
            targets: [{ qubit: 0 }, { qubit: 1 }],
          },
        ],
      },
    ],
  });
  const { stub } = makeStubEvents(model);
  const { host } = buildGateFixture("0,0", "body");
  addContextMenuToHostElem(/** @type {any} */ (stub), host);

  rightClick(host);

  assert.deepEqual(
    getMenuLabels(),
    ["Toggle Adjoint", "Delete"],
    "M5: multi-target unitary has no Add Control / Remove Control",
  );
});

test("addContextMenuToHostElem: group (M7) drops Toggle Adjoint", () => {
  // M7: any op with `children != null` MUST NOT surface Toggle
  // Adjoint. Groups also satisfy `_isMultiTargetOrGroup`, so
  // control authoring is also suppressed (M5 + M7 are the same
  // body-shape predicate for that purpose). Net menu for a
  // param-less group: just Delete.
  const model = new CircuitModel({
    qubits: [{ id: 0 }],
    componentGrid: [
      {
        components: [
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
      },
    ],
  });
  const { stub } = makeStubEvents(model);
  const { host } = buildGateFixture("0,0", "body");
  addContextMenuToHostElem(/** @type {any} */ (stub), host);

  rightClick(host);

  assert.deepEqual(
    getMenuLabels(),
    ["Delete"],
    "M7: a param-less group has no Toggle Adjoint / Add / Remove Control",
  );
});

test("addContextMenuToHostElem: ordinary unitary with params shows [Toggle Adjoint, Add Control, Edit Argument, Delete]", () => {
  // The full general-case body menu without controls. Edit
  // Argument only appears when `params.length > 0`.
  const model = new CircuitModel({
    qubits: [{ id: 0 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Rx",
            targets: [{ qubit: 0 }],
            params: [{ name: "theta", type: "Double" }],
            args: ["0.0"],
          },
        ],
      },
    ],
  });
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
  // Adding an existing control to the previous fixture surfaces
  // Remove Control between Add Control and Edit Argument —
  // verifying the four-item conditional block all renders.
  const model = new CircuitModel({
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Ry",
            targets: [{ qubit: 1 }],
            controls: [{ qubit: 0 }],
            params: [{ name: "theta", type: "Double" }],
            args: ["0.0"],
          },
        ],
      },
    ],
  });
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
  // appending the new one — so a user who right-clicks twice
  // doesn't end up with stacked menus.
  const model = new CircuitModel({
    qubits: [{ id: 0 }],
    componentGrid: [
      {
        components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 0 }] }],
      },
    ],
  });
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
  // `{ once: true }` and removes the menu on the next click
  // anywhere in the page. This is what closes the menu after the
  // user clicks an item OR clicks somewhere else entirely.
  const model = new CircuitModel({
    qubits: [{ id: 0 }],
    componentGrid: [
      {
        components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 0 }] }],
      },
    ],
  });
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
  // Verify the delegation contract for the Add Control item:
  // the menu item's click handler calls
  // `circuitEvents._startAddingControl(selectedOperation)`.
  const op = {
    kind: "unitary",
    gate: "H",
    targets: [{ qubit: 0 }],
  };
  const model = new CircuitModel({
    qubits: [{ id: 0 }],
    componentGrid: [{ components: [/** @type {any} */ (op)] }],
  });
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
  // Same identity as the op stored on the grid (findOperation
  // returns the live reference, not a clone).
  assert.equal(startAddingCalls[0], model.componentGrid[0].components[0]);
});
