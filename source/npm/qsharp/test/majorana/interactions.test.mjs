// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import assert from "node:assert/strict";
import { after, before, beforeEach, test } from "node:test";
import { JSDOM } from "jsdom";
import { Majorana } from "../../dist/ux/majorana/index.js";

const input = [
  [2, 2],
  [[["Mx", [0], null]], [["T", [0], null]], [["Mzz", [0, 2], null]]],
];

let jsdom;
let motionQuery;
let resizeObservers;

before(() => {
  jsdom = new JSDOM("<!doctype html><html><body></body></html>");
  globalThis.window = jsdom.window;
  globalThis.document = jsdom.window.document;
  globalThis.ResizeObserver = class {
    constructor(callback) {
      this.callback = callback;
      this.disconnected = false;
      resizeObservers.push(this);
    }
    observe() {}
    disconnect() {
      this.disconnected = true;
    }
    resize(width) {
      this.callback([{ contentRect: { width } }]);
    }
  };
  jsdom.window.matchMedia = () => motionQuery;
});

beforeEach(() => {
  document.body.replaceChildren();
  motionQuery = createMotionQuery(false);
  resizeObservers = [];
});

after(() => {
  jsdom.window.close();
  delete globalThis.window;
  delete globalThis.document;
  delete globalThis.ResizeObserver;
});

test("provides accessible controls with shared view and label state", () => {
  const { container, controller } = mount();
  const previous = button(container, "Previous visualization step");
  const next = button(container, "Next visualization step");
  const tetrons = button(container, "Tetrons");
  const qubits = button(container, "Qubits");

  assert.equal(previous.disabled, true);
  assert.equal(next.disabled, false);
  assert.equal(tetrons.getAttribute("aria-pressed"), "true");
  assert.equal(qubits.getAttribute("aria-pressed"), "false");
  assert.equal(
    checkbox(container, "Qubit labels").getAttribute("aria-label"),
    "Qubit labels",
  );
  assert.equal(
    checkbox(container, "MZM labels").getAttribute("aria-label"),
    "MZM labels",
  );
  assert.equal(
    requiredElement(container, ".qs-majorana-scrubber").getAttribute("max"),
    "3",
  );
  assert.equal(
    requiredElement(container, ".qs-majorana-toolstrip").firstElementChild,
    requiredElement(container, ".qs-majorana-zoom-controls"),
  );
  assert.equal(
    requiredElement(container, ".qs-majorana-zoom-controls").firstElementChild,
    requiredElement(container, ".qs-majorana-keyboard-help"),
  );
  assert.equal(
    requiredElement(container, ".qs-majorana-zoom-controls").lastElementChild,
    button(container, "Zoom out"),
  );
  assert.match(
    requiredElement(container, ".qs-majorana-keyboard-help-panel").textContent,
    /Toggle Tetrons \/ Qubits/,
  );
  assert.equal(
    requiredElement(container, ".qs-majorana-info-line").getAttribute("d"),
    "M12 10.875v6.75",
  );
  assert.equal(
    requiredElement(container, ".qs-majorana-toolstrip").lastElementChild,
    requiredElement(container, ".qs-majorana-playback-controls"),
  );
  assert.equal(
    requiredElement(container, ".qs-majorana-controls").childElementCount,
    2,
  );
  assert.equal(container.querySelector(".qs-majorana-options"), null);
  assert.equal(
    container.querySelectorAll(".qs-majorana-icon-button").length,
    6,
  );
  assert.equal(container.querySelector('[aria-label="Playback speed"]'), null);

  qubits.focus();
  assert.equal(document.activeElement, qubits);
  qubits.click();
  assert.equal(qubits.getAttribute("aria-pressed"), "true");
  assert.equal(scene(container).getAttribute("data-view"), "Qubits");
  assert.throws(
    () => controller.setView("Virtual"),
    (error) =>
      error instanceof Error &&
      error.message.includes("virtual view is disabled"),
  );
  assert.equal(scene(container).getAttribute("data-view"), "Qubits");

  checkbox(container, "Qubit labels").click();
  checkbox(container, "MZM labels").click();
  tetrons.click();
  assert.equal(
    container.querySelectorAll(".qs-majorana-qubit-label").length,
    0,
  );
  assert.equal(container.querySelectorAll(".qs-majorana-mzm-label").length, 64);
  assert.equal(scene(container).getAttribute("data-step"), "0");
  controller.dispose();
});

test("supports Atoms-style keyboard shortcuts from focused child controls", (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  motionQuery.setMatches(true);
  const { container, controller } = mount();
  const root = requiredElement(container, ".qs-majorana-root");

  assert.equal(root.tabIndex, 0);
  pressKey(container, "ArrowLeft");
  assert.equal(scene(container).getAttribute("data-step"), "0");
  pressKey(container, "ArrowRight");
  assert.equal(scene(container).getAttribute("data-step"), "1");
  pressKey(container, "ArrowLeft");
  assert.equal(scene(container).getAttribute("data-step"), "0");

  pressKey(container, "ArrowUp");
  assert.equal(scene(container).getAttribute("data-zoom"), "1.15");
  pressKey(container, "ArrowDown");
  assert.equal(
    scene(container).getAttribute("data-zoom"),
    "0.9774999999999999",
  );

  pressKey(container, "f");
  pressKey(container, "s");

  pressKey(container, "t");
  assert.equal(scene(container).getAttribute("data-view"), "Qubits");
  pressKey(container, "T");
  assert.equal(scene(container).getAttribute("data-view"), "Tetrons");

  pressKey(container, "p");
  assert.ok(button(container, "Pause playback"));
  pressKey(container, "p");
  assert.ok(button(container, "Play visualization"));

  controller.setStep(3);
  pressKey(container, "ArrowRight");
  assert.equal(scene(container).getAttribute("data-step"), "3");
  controller.dispose();
});

test("cycles all enabled views while preserving the selected physical step", () => {
  motionQuery.setMatches(true);
  const { container, controller } = mountVirtual([
    [["Mx", [0], ["T", [0], 0]]],
  ]);

  controller.setStep(1);
  assertSceneState(container, 1, 1, "stable");
  pressKey(container, "t");
  assert.equal(scene(container).getAttribute("data-view"), "Tetrons");
  assertSceneState(container, 1, 1, "stable");
  pressKey(container, "t");
  assert.equal(scene(container).getAttribute("data-view"), "Qubits");
  assertSceneState(container, 1, 1, "stable");
  pressKey(container, "t");
  assert.equal(scene(container).getAttribute("data-view"), "Virtual");
  assertSceneState(container, 1, 1, "stable");
  assert.equal(
    requiredElement(container, ".qs-majorana-scrubber").getAttribute("max"),
    "1",
  );
  controller.dispose();
});

test("keeps repeated virtual identity stable while physical steps fade", (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  const { container, controller } = mountVirtual([
    [["Mx", [0], ["T", [0], 0]]],
    [["T", [1], ["T", [0], 0]]],
    [["Mx", [0], ["H", [0], 0]]],
  ]);

  controller.setStep(1);
  assert.equal(virtualOperations(container).length, 0);
  context.mock.timers.tick(75);
  assertVirtualOperation(container, "T", "in");
  context.mock.timers.tick(75);
  assertVirtualOperation(container, "T", "stable");

  controller.setStep(2);
  assertSceneState(container, 1, 2, "out");
  assertVirtualOperation(container, "T", "stable");
  assert.match(
    virtualOperation(container, "T").getAttribute("aria-label"),
    /via T on Q1/,
  );
  context.mock.timers.tick(75);
  assertSceneState(container, 2, 2, "in");
  assertVirtualOperation(container, "T", "stable");
  context.mock.timers.tick(75);
  assertVirtualOperation(container, "T", "stable");

  controller.setStep(3);
  assertVirtualOperation(container, "T", "out");
  assert.equal(container.querySelector('[data-operation="H"]'), null);
  context.mock.timers.tick(75);
  assert.equal(container.querySelector('[data-operation="T"]'), null);
  assertVirtualOperation(container, "H", "in");
  context.mock.timers.tick(75);
  assertVirtualOperation(container, "H", "stable");
  controller.dispose();
});

test("diffs concurrent virtual identities independently", (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  const { container, controller } = mountVirtual([
    [
      ["Mx", [0], ["T", [0], 0]],
      ["T", [1], ["S", [1], 0]],
    ],
    [
      ["T", [0], ["T", [0], 0]],
      ["Mx", [1], ["H", [1], 0]],
    ],
  ]);

  controller.setStep(1);
  context.mock.timers.tick(150);
  controller.setStep(2);
  assertVirtualOperation(container, "T", "stable");
  assertVirtualOperation(container, "S", "out");
  assert.equal(container.querySelector('[data-operation="H"]'), null);
  context.mock.timers.tick(75);
  assertVirtualOperation(container, "T", "stable");
  assert.equal(container.querySelector('[data-operation="S"]'), null);
  assertVirtualOperation(container, "H", "in");
  context.mock.timers.tick(75);
  assertVirtualOperation(container, "T", "stable");
  assertVirtualOperation(container, "H", "stable");
  controller.dispose();
});

test("keeps reversed CZ stable and transitions reversed CX roles", (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  const { container, controller } = mountVirtual([
    [["Mzz", [0, 2], ["CZ", [0, 2], 0]]],
    [["Mzz", [0, 2], ["CZ", [2, 0], 0]]],
    [["Mxx", [0, 1], ["CX", [0, 1], 0]]],
    [["Mxx", [0, 1], ["CX", [1, 0], 0]]],
  ]);

  controller.setStep(1);
  context.mock.timers.tick(150);
  controller.setStep(2);
  assertVirtualOperation(container, "CZ", "stable");
  context.mock.timers.tick(150);
  controller.setStep(3);
  assertVirtualOperation(container, "CZ", "out");
  context.mock.timers.tick(75);
  context.mock.timers.tick(75);
  assertVirtualOperation(container, "CX", "stable");
  controller.setStep(4);
  assert.equal(virtualOperations(container).length, 1);
  assertVirtualOperation(container, "CX", "out");
  context.mock.timers.tick(75);
  assertVirtualOperation(container, "CX", "in");
  assert.equal(
    requiredElement(
      container,
      '[aria-label="CX control on V1: ●"]',
    ).getAttribute("data-role"),
    "control",
  );
  controller.dispose();
});

test("rapid virtual navigation converges and view changes do not replay transitions", (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  const { container, controller } = mountVirtual([
    [["Mx", [0], ["T", [0], 0]]],
    [["Mx", [0], ["H", [0], 0]]],
    [["Mx", [0], ["S", [0], 0]]],
  ]);

  controller.setStep(1);
  context.mock.timers.tick(75);
  assertVirtualOperation(container, "T", "in");
  controller.setStep(3);
  assertVirtualOperation(container, "T", "out");
  controller.setView("Qubits");
  assertSceneState(container, 1, 3, "out");
  controller.setView("Virtual");
  assertVirtualOperation(container, "T", "out");
  context.mock.timers.tick(75);
  assertVirtualOperation(container, "S", "in");
  context.mock.timers.tick(75);
  assertSceneState(container, 3, 3, "stable");
  assertVirtualOperation(container, "S", "stable");
  controller.dispose();
});

test("skips virtual fades for reduced motion and fast playback", (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  const trace = [[["Mx", [0], ["T", [0], 0]]], [["Mx", [0], ["H", [0], 0]]]];
  motionQuery.setMatches(true);
  const reduced = mountVirtual(trace);

  reduced.controller.setStep(1);
  assertSceneState(reduced.container, 1, 1, "stable");
  assertVirtualOperation(reduced.container, "T", "stable");
  reduced.controller.dispose();

  motionQuery.setMatches(false);
  const fast = mountVirtual(trace);
  pressKey(fast.container, "f");
  button(fast.container, "Play visualization").click();
  context.mock.timers.tick(500);
  assertSceneState(fast.container, 1, 1, "stable");
  assertVirtualOperation(fast.container, "T", "stable");
  fast.controller.dispose();
});

test("settles virtual transitions when input is replaced", (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  const { container, controller } = mountVirtual([
    [["Mx", [0], ["T", [0], 0]]],
  ]);

  controller.setStep(1);
  context.mock.timers.tick(75);
  assertVirtualOperation(container, "T", "in");
  controller.updateInput([[2, 2], [[["Mx", [0], ["H", [0], 0]]]]]);
  assertSceneState(container, 1, 1, "stable");
  assert.equal(container.querySelector('[data-operation="T"]'), null);
  assertVirtualOperation(container, "H", "stable");
  context.mock.timers.tick(150);
  assertVirtualOperation(container, "H", "stable");
  controller.dispose();
});

test("disposal cancels active virtual transition timers", (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  const { container, controller } = mountVirtual([
    [["Mx", [0], ["T", [0], 0]]],
  ]);

  controller.setStep(1);
  context.mock.timers.tick(75);
  assertVirtualOperation(container, "T", "in");
  controller.dispose();
  assert.equal(container.childElementCount, 0);
  assert.equal(motionQuery.listenerCount(), 0);
  context.mock.timers.tick(75);
  assert.equal(container.childElementCount, 0);
});

test("zooms the scene independently and preserves zoom across updates", () => {
  const { container, controller } = mount();
  const zoomIn = button(container, "Zoom in");
  const zoomOut = button(container, "Zoom out");
  const toolstrip = requiredElement(container, ".qs-majorana-toolstrip");

  assert.equal(scene(container).style.width, "1155px");
  assert.equal(visualization(container).style.width, "1187px");
  assert.equal(toolstrip.style.width, "");

  zoomIn.click();
  assert.equal(scene(container).style.width, "1328.25px");
  assert.equal(visualization(container).style.width, "1360.25px");
  assert.equal(toolstrip.style.width, "");
  assert.equal(scene(container).getAttribute("data-zoom"), "1.15");

  controller.setView("Qubits");
  controller.updateInput([[1, 1], [[["Mx", [0], null]]]]);
  assert.equal(scene(container).style.width, "1113.1999999999998px");
  assert.equal(visualization(container).style.width, "1145.1999999999998px");

  for (let index = 0; index < 10; index += 1) {
    zoomOut.click();
  }
  assert.equal(button(container, "Zoom out").disabled, false);
  assert.ok(Number.parseFloat(scene(container).style.width) < 250);
  assert.equal(visualization(container).style.width, "1000px");

  for (let index = 0; index < 20; index += 1) {
    button(container, "Zoom in").click();
  }
  assert.equal(button(container, "Zoom in").disabled, false);
  assert.ok(Number.parseFloat(scene(container).style.width) > 2000);
  assert.ok(Number.parseFloat(visualization(container).style.width) > 2000);
  controller.dispose();
});

test("fits the bordered component and scene to the available host width", () => {
  const { container, controller } = mount();

  resizeObservers[0].resize(800);
  assert.equal(visualization(container).style.width, "800px");
  assert.equal(scene(container).style.width, "768px");

  button(container, "Zoom in").click();
  assert.equal(visualization(container).style.width, "800px");
  assert.equal(scene(container).style.width, "883.1999999999999px");

  resizeObservers[0].resize(1200);
  assert.equal(visualization(container).style.width, "1200px");
  assert.equal(scene(container).style.width, "1328.25px");

  resizeObservers[0].resize(500);
  assert.equal(visualization(container).style.width, "500px");
  assert.equal(scene(container).style.width, "538.1999999999999px");
  controller.dispose();
  assert.equal(resizeObservers[0].disconnected, true);
});

test("cancels obsolete fades and converges on the latest step", (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  const { container, controller } = mount();

  controller.setStep(1);
  assertSceneState(container, 0, 1, "out");
  context.mock.timers.tick(75);
  assertSceneState(container, 1, 1, "in");

  controller.setStep(2);
  assertSceneState(container, 1, 2, "out");
  context.mock.timers.tick(75);
  assertSceneState(container, 2, 2, "in");
  context.mock.timers.tick(75);
  assertSceneState(container, 2, 2, "stable");

  motionQuery.setMatches(true);
  controller.setStep(3);
  assertSceneState(container, 3, 3, "stable");
  controller.dispose();
});

test("plays, pauses, reschedules speed changes, and restarts from the end", (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  const { container, controller } = mount();

  button(container, "Play visualization").click();
  assert.equal(
    button(container, "Pause playback").getAttribute("aria-pressed"),
    "true",
  );
  context.mock.timers.tick(999);
  assert.equal(scene(container).getAttribute("data-selected-step"), "0");
  context.mock.timers.tick(1);
  assertSceneState(container, 0, 1, "out");
  context.mock.timers.tick(75);
  context.mock.timers.tick(75);
  assertSceneState(container, 1, 1, "stable");

  pressKey(container, "f");
  pressKey(container, "f");
  pressKey(container, "f");
  context.mock.timers.tick(199);
  assert.equal(scene(container).getAttribute("data-selected-step"), "1");
  context.mock.timers.tick(1);
  assertSceneState(container, 2, 2, "stable");

  button(container, "Pause playback").click();
  context.mock.timers.tick(1000);
  assert.equal(scene(container).getAttribute("data-step"), "2");

  button(container, "Play visualization").click();
  context.mock.timers.tick(200);
  assertSceneState(container, 3, 3, "stable");
  assert.ok(button(container, "Play visualization"));

  button(container, "Play visualization").click();
  assertSceneState(container, 0, 0, "stable");
  assert.ok(button(container, "Pause playback"));
  controller.dispose();
});

test("manual navigation pauses playback and enforces boundaries", (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  motionQuery.setMatches(true);
  const { container, controller } = mount();

  button(container, "Play visualization").click();
  button(container, "Next visualization step").click();
  assert.equal(scene(container).getAttribute("data-step"), "1");
  assert.ok(button(container, "Play visualization"));
  context.mock.timers.tick(2000);
  assert.equal(scene(container).getAttribute("data-step"), "1");

  const scrubber = requiredElement(container, ".qs-majorana-scrubber");
  scrubber.value = "3";
  scrubber.dispatchEvent(new window.Event("input", { bubbles: true }));
  assert.equal(button(container, "Next visualization step").disabled, true);
  button(container, "Previous visualization step").click();
  assert.equal(scene(container).getAttribute("data-step"), "2");
  assert.throws(() => controller.setStep(4), RangeError);
  controller.dispose();
});

test("stops timers on updates, preserves invalid scenes, and recovers in place", (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  motionQuery.setMatches(true);
  const { container, controller } = mount();

  controller.setStep(2);
  button(container, "Play visualization").click();
  controller.updateInput([[1, 1], []]);
  assert.equal(scene(container).getAttribute("data-step"), "2");
  assert.ok(container.querySelector('[role="alert"]'));
  assert.ok(button(container, "Play visualization"));
  context.mock.timers.tick(2000);
  assert.equal(scene(container).getAttribute("data-step"), "2");

  controller.updateInput([[1, 1], [[["Mx", [0], null]]]]);
  assert.equal(container.querySelector('[role="alert"]'), null);
  assert.equal(scene(container).getAttribute("data-step"), "1");
  assert.equal(container.querySelectorAll(".qs-majorana-root").length, 1);
  assert.equal(container.querySelectorAll(".qs-majorana-controls").length, 1);
  controller.dispose();
});

test("disposal cancels playback, transitions, and motion listeners", (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  const { container, controller } = mount();

  button(container, "Play visualization").click();
  controller.setStep(1);
  controller.dispose();
  assert.equal(container.childElementCount, 0);
  assert.equal(motionQuery.listenerCount(), 0);
  context.mock.timers.tick(2000);
  assert.equal(container.childElementCount, 0);
});

function mount() {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const controller = Majorana(container, input);
  return { container, controller };
}

function mountVirtual(trace) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const controller = Majorana(container, [[2, 2], trace], {
    enableVirtualView: true,
  });
  return { container, controller };
}

function createMotionQuery(initialMatches) {
  let matches = initialMatches;
  const listeners = new Set();
  return {
    get matches() {
      return matches;
    },
    addEventListener(type, listener) {
      if (type === "change") listeners.add(listener);
    },
    removeEventListener(type, listener) {
      if (type === "change") listeners.delete(listener);
    },
    setMatches(nextMatches) {
      matches = nextMatches;
      for (const listener of listeners) {
        listener({ matches });
      }
    },
    listenerCount() {
      return listeners.size;
    },
  };
}

function scene(container) {
  return requiredElement(container, ".qs-majorana-scene");
}

function visualization(container) {
  return requiredElement(container, ".qs-majorana");
}

function virtualOperations(container) {
  return [...container.querySelectorAll(".qs-majorana-virtual-operation")];
}

function virtualOperation(container, name) {
  return requiredElement(
    container,
    `.qs-majorana-virtual-operation[data-operation="${name}"]`,
  );
}

function assertVirtualOperation(container, name, transition) {
  assert.equal(
    virtualOperation(container, name).getAttribute("data-transition"),
    transition,
  );
}

function assertSceneState(container, step, selectedStep, transition) {
  assert.equal(scene(container).getAttribute("data-step"), String(step));
  assert.equal(
    scene(container).getAttribute("data-selected-step"),
    String(selectedStep),
  );
  assert.equal(scene(container).getAttribute("data-transition"), transition);
}

function button(container, name) {
  const element = [...container.querySelectorAll("button")].find(
    (candidate) =>
      candidate.getAttribute("aria-label") === name ||
      candidate.textContent === name,
  );
  assert.ok(element, `Expected button named ${name}`);
  return element;
}

function checkbox(container, label) {
  const element = [...container.querySelectorAll("label")]
    .find((candidate) => candidate.textContent.includes(label))
    ?.querySelector('input[type="checkbox"]');
  assert.ok(element, `Expected checkbox labeled ${label}`);
  return element;
}

function pressKey(container, key) {
  const target = checkbox(container, "Qubit labels");
  target.focus();
  const event = new window.KeyboardEvent("keydown", {
    key,
    bubbles: true,
    cancelable: true,
  });
  assert.equal(target.dispatchEvent(event), false);
}

function requiredElement(container, selector) {
  const element = container.querySelector(selector);
  assert.ok(element, `Expected element matching ${selector}`);
  return element;
}
