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
import { SelectionController } from "../../dist/ux/circuit-vis/editor/selectionController.js";

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
 * `InteractionContext` and a stub `CircuitEvents`.
 */
function makeController(container, interaction = new InteractionState()) {
  const ctx = {
    model: /** @type {any} */ ({}),
    interaction,
    layoutMap: /** @type {any} */ ({}),
    container,
    circuitSvg: container.querySelector("svg.qviz"),
    overlayLayer: /** @type {any} */ ({}),
    dropzoneLayer: /** @type {any} */ ({}),
    ghostQubitLayer: /** @type {any} */ ({}),
    wireData: [],
    renderFn: () => {},
  };
  // Stub CircuitEvents — only used as a closure capture by
  // addContextMenuToHostElem (never invoked in these tests).
  const stubEvents = /** @type {any} */ ({
    componentGrid: [],
    model: ctx.model,
    renderFn: ctx.renderFn,
  });
  // eslint-disable-next-line no-new
  new SelectionController(/** @type {any} */ (ctx), stubEvents);
  return { ctx, interaction };
}

const dispatchMouseDown = (target, button = 0) =>
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
  makeController(container, interaction);

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
  makeController(container, interaction);

  dispatchMouseDown(gateH);

  // Wire updated, but movingControl is left alone (still true).
  assert.equal(interaction.selectedWire, 0);
  assert.equal(interaction.movingControl, true);
});
