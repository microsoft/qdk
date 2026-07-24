// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// SelectionController tests — exercises the host-element mousedown
// path against a hand-built SVG fixture and a stub
// `CircuitEvents`. Verifies the controller captures
// `selectedWire` from `data-wire`, sets `movingControl` only when
// the host is a control dot, and ignores non-primary-button events.
//
// The context-menu attachment side effect installs a `contextmenu`
// listener on each host but does not run any code until the menu
// is opened — these tests do not dispatch `contextmenu`, so the
// stub `CircuitEvents` is never read.

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { InteractionState } from "../../dist/ux/circuit-vis/actions/interactionState.js";
import { SelectionController } from "../../dist/ux/circuit-vis/editor/controllers/selectionController.js";

/** @type {JSDOM | null} */
let jsdom = null;

beforeEach(() => {
  jsdom = new JSDOM(`<!doctype html><html><body></body></html>`);
  globalThis.window = jsdom.window;
  globalThis.document = jsdom.window.document;
  globalThis.HTMLElement = jsdom.window.HTMLElement;
  globalThis.SVGElement = jsdom.window.SVGElement;
  globalThis.MouseEvent = jsdom.window.MouseEvent;
  // JSDOM doesn't implement DOMPoint or DOMMatrix at all. The
  // `pickSelectedWire` closest-wire path uses both:
  //   new DOMPoint(clientX, clientY).matrixTransform(ctm.inverse())
  // We stub a minimal *identity* DOMPoint so clientY flows through
  // to SVG-space Y unchanged — the CTM stub in `buildMultiWireFixture`
  // is wired to match (it returns an identity matrix whose
  // `.inverse()` is also identity, and `matrixTransform` on our
  // stub just returns the point itself). This is enough surface
  // area to exercise the controller's coord-projection wiring
  // without dragging an SVG-layout shim into the test deps.
  // Cast the assignment because our stub doesn't implement the
  // full DOMPoint static surface (e.g. `fromPoint`).
  globalThis.DOMPoint = /** @type {any} */ (
    class {
      constructor(x = 0, y = 0) {
        this.x = x;
        this.y = y;
      }
      // Identity transform: the fixture's CTM stub is also identity,
      // so this is correct for all our tests. A real layout-aware
      // run would apply the matrix; we don't need that fidelity.
      matrixTransform() {
        return { x: this.x, y: this.y };
      }
    }
  );
});

afterEach(() => {
  jsdom?.window.close();
  jsdom = null;
});

const SVG_NS = "http://www.w3.org/2000/svg";

/**
 * Build an `svg.qviz` host with:
 *   - one rect.gate-h on wire 0
 *   - one circle.control-dot on wire 1
 *   - one rect with no `data-wire` attribute on wire ?
 * Returns the container plus a map of the host elements for
 * direct event dispatch.
 */
function buildFixture() {
  const container = document.createElement("div");
  document.body.appendChild(container);

  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("class", "qviz");
  container.appendChild(svg);

  const gateH = document.createElementNS(SVG_NS, "rect");
  gateH.setAttribute("class", "gate-h");
  gateH.setAttribute("data-wire", "0");
  svg.appendChild(gateH);

  const controlDot = document.createElementNS(SVG_NS, "circle");
  controlDot.setAttribute("class", "control-dot");
  controlDot.setAttribute("data-wire", "1");
  svg.appendChild(controlDot);

  const orphan = document.createElementNS(SVG_NS, "rect");
  // Picked up by `[class^="gate-"]` but has no data-wire.
  orphan.setAttribute("class", "gate-x");
  svg.appendChild(orphan);

  return { container, svg, gateH, controlDot, orphan };
}

/**
 * Construct a SelectionController with the minimum viable
 * `InteractionContext` and a stub `CircuitEvents`. Pass `wireData`
 * (matching a multi-wire fixture) to exercise the closest-wire path.
 *
 * @param {HTMLElement} container
 * @param {{ interaction?: InteractionState, wireData?: number[] }} [options]
 */
function makeController(
  container,
  { interaction = new InteractionState(), wireData = [] } = {},
) {
  const ctx = {
    model: /** @type {any} */ ({}),
    interaction,
    layoutMap: /** @type {any} */ ({}),
    container,
    circuitSvg: container.querySelector("svg.qviz"),
    overlayLayer: /** @type {any} */ ({}),
    dropzoneLayer: /** @type {any} */ ({}),
    ghostQubitLayer: /** @type {any} */ ({}),
    wireData,
    renderFn: () => {},
  };
  // Stub CircuitEvents — only used as a closure capture by
  // addContextMenuToHostElem (never invoked in these tests).
  const stubEvents = /** @type {any} */ ({
    componentGrid: [],
    model: ctx.model,
    renderFn: ctx.renderFn,
  });
  new SelectionController(/** @type {any} */ (ctx), stubEvents);
  return { ctx, interaction };
}

const dispatchMouseDown = (/** @type {EventTarget} */ target, button = 0) =>
  target.dispatchEvent(new MouseEvent("mousedown", { button, bubbles: true }));

test("mousedown on a gate host sets selectedWire from data-wire", () => {
  const { container, gateH } = buildFixture();
  const { interaction } = makeController(container);

  dispatchMouseDown(gateH);

  assert.equal(interaction.selectedWire, 0);
  assert.equal(interaction.movingControl, false);
});

test("mousedown on a control-dot sets selectedWire AND movingControl", () => {
  const { container, controlDot } = buildFixture();
  const { interaction } = makeController(container);

  dispatchMouseDown(controlDot);

  assert.equal(interaction.selectedWire, 1);
  assert.equal(interaction.movingControl, true);
});

test("mousedown with non-primary button is ignored", () => {
  const { container, gateH } = buildFixture();
  const { interaction } = makeController(container);

  // Right-click (button 2) — controller bails before reading data.
  dispatchMouseDown(gateH, 2);

  assert.equal(interaction.selectedWire, null);
  assert.equal(interaction.movingControl, false);
});

test("mousedown on a host without data-wire sets selectedWire to null", () => {
  const { container, orphan } = buildFixture();
  const interaction = new InteractionState();
  // Pre-seed with a non-null value so we can see the explicit clear.
  interaction.selectedWire = 7;
  makeController(container, { interaction });

  dispatchMouseDown(orphan);

  assert.equal(interaction.selectedWire, null);
});

test("mousedown on a non-control gate does not set movingControl", () => {
  const { container, gateH } = buildFixture();
  const interaction = new InteractionState();
  // Pre-seed to confirm the controller does not flip a true to false
  // — it only sets the flag, never clears it. (Clearing happens via
  // `resetTransient` in interactionActions.)
  interaction.movingControl = true;
  makeController(container, { interaction });

  dispatchMouseDown(gateH);

  // Wire updated, but movingControl is left alone (still true).
  assert.equal(interaction.selectedWire, 0);
  assert.equal(interaction.movingControl, true);
});

// ============================================================
// Closest-wire-to-click resolution for multi-wire host elems
// ============================================================

/**
 * Build a multi-wire fixture: a group body that spans wires 0..2,
 * with `data-wire-ys = [40, 100, 160]` and a (stale) static
 * `data-wire = 0`. The closest-wire path should override the
 * static attribute and pick the wire closest to the click's
 * SVG-space Y.
 *
 * Stubs `circuitSvg.getScreenCTM()` to the identity transform so
 * `clientY` flows through to SVG-space Y unchanged. JSDOM doesn't
 * implement layout, so a real CTM call would return null and the
 * controller would fall back to the static `data-wire` — masking
 * the closest-wire behavior we're trying to test.
 */
function buildMultiWireFixture(wireYs = [40, 100, 160]) {
  const container = document.createElement("div");
  document.body.appendChild(container);

  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("class", "qviz");
  container.appendChild(svg);

  // Identity CTM hand-stub. JSDOM doesn't ship a DOMMatrix at
  // all, and our DOMPoint stub's `matrixTransform()` already
  // returns the point unchanged — so all this needs is something
  // non-null with an `.inverse()` method the controller can call
  // before passing it to `matrixTransform`. The returned value
  // never has to be a real matrix.
  const identityCtm = /** @type {any} */ ({ inverse: () => identityCtm });
  svg.getScreenCTM = () => identityCtm;

  const groupBody = document.createElementNS(SVG_NS, "rect");
  groupBody.setAttribute("class", "gate-group");
  groupBody.setAttribute("data-wire-ys", JSON.stringify(wireYs));
  // Static attr is the topmost-wire fallback, which the
  // closest-wire path must override.
  groupBody.setAttribute("data-wire", "0");
  svg.appendChild(groupBody);

  return { container, svg, groupBody, wireYs };
}

/**
 * Dispatch a primary-button mousedown at a given client Y so the
 * closest-wire resolution has a coordinate to project.
 */
const dispatchMouseDownAt = (
  /** @type {EventTarget} */ target,
  /** @type {number} */ clientY,
  button = 0,
) =>
  target.dispatchEvent(
    new MouseEvent("mousedown", {
      button,
      clientX: 0,
      clientY,
      bubbles: true,
    }),
  );

test("multi-wire host: click near top wire picks top wire (overrides data-wire shortcut)", () => {
  const { container, groupBody } = buildMultiWireFixture();
  const { interaction } = makeController(container, {
    wireData: [40, 100, 160],
  });

  dispatchMouseDownAt(groupBody, 42); // closest to 40

  assert.equal(interaction.selectedWire, 0);
});

test("multi-wire host: click near middle wire picks middle wire", () => {
  const { container, groupBody } = buildMultiWireFixture();
  const { interaction } = makeController(container, {
    wireData: [40, 100, 160],
  });

  dispatchMouseDownAt(groupBody, 95);

  assert.equal(
    interaction.selectedWire,
    1,
    "static data-wire was 0 (topmost) — closest-wire path must override",
  );
});

test("multi-wire host: click near bottom wire picks bottom wire", () => {
  const { container, groupBody } = buildMultiWireFixture();
  const { interaction } = makeController(container, {
    wireData: [40, 100, 160],
  });

  dispatchMouseDownAt(groupBody, 158);

  assert.equal(interaction.selectedWire, 2);
});

test("multi-wire host: click far above the span clamps to topmost wire", () => {
  const { container, groupBody } = buildMultiWireFixture();
  const { interaction } = makeController(container, {
    wireData: [40, 100, 160],
  });

  dispatchMouseDownAt(groupBody, -200);

  assert.equal(interaction.selectedWire, 0);
});

test("multi-wire host: click far below the span clamps to bottommost wire", () => {
  const { container, groupBody } = buildMultiWireFixture();
  const { interaction } = makeController(container, {
    wireData: [40, 100, 160],
  });

  dispatchMouseDownAt(groupBody, 1000);

  assert.equal(interaction.selectedWire, 2);
});

test("multi-wire host: falls back to data-wire when getScreenCTM returns null", () => {
  // Detached SVG / pre-mount edge case. The controller must not
  // throw; it should fall back to the static `data-wire` attribute
  // so the click still resolves *some* wire.
  const { container, groupBody } = buildMultiWireFixture();
  const svg = /** @type {any} */ (container.querySelector("svg.qviz"));
  svg.getScreenCTM = () => null;
  const { interaction } = makeController(container, {
    wireData: [40, 100, 160],
  });

  dispatchMouseDownAt(groupBody, 95);

  assert.equal(
    interaction.selectedWire,
    0,
    "fallback path returns the static data-wire value (0)",
  );
});

test("multi-wire host: falls back to data-wire if closest wireY is not in wireData", () => {
  // wireYs claims [40, 100, 160] but the editor's wireData is
  // [200, 300] — table mismatch. pickClosestWireIndex returns -1,
  // the controller falls back to the static data-wire.
  const { container, groupBody } = buildMultiWireFixture();
  const { interaction } = makeController(container, { wireData: [200, 300] });

  dispatchMouseDownAt(groupBody, 95);

  assert.equal(interaction.selectedWire, 0, "fallback to static data-wire");
});

test("single-wire host: closest-wire path is skipped, data-wire still wins", () => {
  // Smoke-check: the multi-wire branch must not engage for
  // single-wire spans (control dots, target circles, etc.). Use
  // a host with data-wire-ys=[100] and data-wire=1 — controller
  // must resolve to wire 1 regardless of clickY.
  const container = document.createElement("div");
  document.body.appendChild(container);
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("class", "qviz");
  container.appendChild(svg);
  // No CTM stub — single-wire path doesn't call getScreenCTM.

  const dot = document.createElementNS(SVG_NS, "circle");
  dot.setAttribute("class", "control-dot");
  dot.setAttribute("data-wire-ys", "[100]");
  dot.setAttribute("data-wire", "1");
  svg.appendChild(dot);

  const { interaction } = makeController(container, { wireData: [40, 100] });

  dispatchMouseDownAt(dot, -999);

  assert.equal(interaction.selectedWire, 1);
});
