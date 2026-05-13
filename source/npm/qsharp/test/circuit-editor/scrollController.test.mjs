// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// ScrollController tests — exercises the `enableAutoScroll` helper
// against a JSDOM scrollable container. Verifies that mousemove
// near each edge scrolls the appropriate axis, that mouseup
// removes both document listeners, and that the
// `disableLeftAutoScroll` flag is honored and lifted at the right
// threshold.

// @ts-check

import { JSDOM } from "jsdom";
import { afterEach, beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { InteractionState } from "../../dist/ux/circuit-vis/actions/interactionState.js";
import { enableAutoScroll } from "../../dist/ux/circuit-vis/editor/scrollController.js";

/** @type {JSDOM | null} */
let jsdom = null;
/** @type {HTMLElement | null} */
let scrollable = null;
/** @type {SVGElement | null} */
let circuitSvg = null;

beforeEach(() => {
  jsdom = new JSDOM(`<!doctype html><html><body></body></html>`);
  // @ts-expect-error - jsdom typings vs DOM lib mismatch
  globalThis.window = jsdom.window;
  globalThis.document = jsdom.window.document;
  globalThis.HTMLElement = jsdom.window.HTMLElement;
  globalThis.SVGElement = jsdom.window.SVGElement;
  globalThis.MouseEvent = jsdom.window.MouseEvent;

  // Build a scrollable container at a known viewport position so the
  // mousemove handler's edge-distance math has stable inputs.
  scrollable = document.createElement("div");
  // The controller's `getScrollableAncestor` checks computed
  // overflowY/overflowX individually, not the shorthand.
  scrollable.style.overflowY = "auto";
  scrollable.style.overflowX = "auto";
  scrollable.style.width = "200px";
  scrollable.style.height = "200px";
  // Stub getBoundingClientRect so the controller sees a fixed
  // viewport rectangle for the scrollable ancestor. JSDOM's default
  // returns zeros, which would put every cursor "near every edge."
  scrollable.getBoundingClientRect = () =>
    /** @type {DOMRect} */ ({
      top: 100,
      bottom: 300,
      left: 100,
      right: 300,
      width: 200,
      height: 200,
      x: 100,
      y: 100,
      toJSON: () => ({}),
    });
  document.body.appendChild(scrollable);

  // The "circuitSvg" only matters as the starting point for the
  // scrollable-ancestor walk. Append it to the scrollable so the
  // walk terminates at our stub.
  circuitSvg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  scrollable.appendChild(circuitSvg);
});

afterEach(() => {
  jsdom?.window.close();
  jsdom = null;
  scrollable = null;
  circuitSvg = null;
});

const move = (clientX, clientY) =>
  document.dispatchEvent(
    new MouseEvent("mousemove", { clientX, clientY, bubbles: true }),
  );

const releaseMouse = () =>
  document.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));

test("mousemove in the middle of the area does not scroll", () => {
  const interaction = new InteractionState();
  enableAutoScroll(/** @type {SVGElement} */ (circuitSvg), interaction);

  move(200, 200); // center of the 100..300 / 100..300 rect

  assert.equal(/** @type {HTMLElement} */ (scrollable).scrollTop, 0);
  assert.equal(/** @type {HTMLElement} */ (scrollable).scrollLeft, 0);
});

test("mousemove near the top edge scrolls up (negative scrollTop)", () => {
  const interaction = new InteractionState();
  enableAutoScroll(/** @type {SVGElement} */ (circuitSvg), interaction);
  // Pre-seed scrollTop so the negative delta is observable.
  /** @type {HTMLElement} */ (scrollable).scrollTop = 100;

  // edgeThreshold = 50 → anything within 50px of top (i.e. clientY < 150).
  move(200, 120);

  // Scroll moved up by scrollSpeed (10).
  assert.equal(/** @type {HTMLElement} */ (scrollable).scrollTop, 90);
});

test("mousemove near the bottom edge scrolls down", () => {
  const interaction = new InteractionState();
  enableAutoScroll(/** @type {SVGElement} */ (circuitSvg), interaction);

  move(200, 280); // within 50px of bottom (300)

  assert.equal(/** @type {HTMLElement} */ (scrollable).scrollTop, 10);
});

test("mousemove near the left edge scrolls left", () => {
  const interaction = new InteractionState();
  enableAutoScroll(/** @type {SVGElement} */ (circuitSvg), interaction);
  /** @type {HTMLElement} */ (scrollable).scrollLeft = 100;

  move(120, 200); // within 50px of left (100)

  assert.equal(/** @type {HTMLElement} */ (scrollable).scrollLeft, 90);
});

test("mousemove near the right edge scrolls right", () => {
  const interaction = new InteractionState();
  enableAutoScroll(/** @type {SVGElement} */ (circuitSvg), interaction);

  move(280, 200); // within 50px of right (300)

  assert.equal(/** @type {HTMLElement} */ (scrollable).scrollLeft, 10);
});

test("disableLeftAutoScroll suppresses the left-edge scroll trigger", () => {
  const interaction = new InteractionState();
  interaction.disableLeftAutoScroll = true;
  enableAutoScroll(/** @type {SVGElement} */ (circuitSvg), interaction);
  /** @type {HTMLElement} */ (scrollable).scrollLeft = 100;

  move(120, 200); // would normally scroll left

  // Left-scroll suppressed by the flag.
  assert.equal(/** @type {HTMLElement} */ (scrollable).scrollLeft, 100);
});

test("disableLeftAutoScroll is lifted once the cursor moves far enough right", () => {
  const interaction = new InteractionState();
  interaction.disableLeftAutoScroll = true;
  enableAutoScroll(/** @type {SVGElement} */ (circuitSvg), interaction);

  // Threshold for lifting: clientX > leftBoundary + 3 * edgeThreshold
  // = 100 + 150 = 250. A move past that releases the flag.
  move(260, 200);

  assert.equal(interaction.disableLeftAutoScroll, false);
});

test("mouseup removes both document listeners", () => {
  const interaction = new InteractionState();
  enableAutoScroll(/** @type {SVGElement} */ (circuitSvg), interaction);

  // Establish that the controller was active (a move scrolls).
  move(200, 280);
  assert.equal(/** @type {HTMLElement} */ (scrollable).scrollTop, 10);

  releaseMouse();

  // After mouseup the listener is gone; subsequent moves are no-ops.
  /** @type {HTMLElement} */ (scrollable).scrollTop = 0;
  move(200, 280);
  assert.equal(/** @type {HTMLElement} */ (scrollable).scrollTop, 0);
});
