// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { JSDOM } from "jsdom";
import {
  Majorana,
  MajoranaValidationError,
} from "../../dist/ux/majorana/index.js";

const input = [
  [2, 2],
  [[["Mx", [0], null]], [["T", [0], null]], [["Mzz", [0, 2], null]]],
];

let jsdom;

before(() => {
  jsdom = new JSDOM("<!doctype html><html><body></body></html>");
  globalThis.window = jsdom.window;
  globalThis.document = jsdom.window.document;
  jsdom.window.matchMedia = () => ({
    matches: true,
    addEventListener() {},
    removeEventListener() {},
  });
});

after(() => {
  jsdom.window.close();
  delete globalThis.window;
  delete globalThis.document;
});

test("renders complete physical topology with semantic IDs and labels", () => {
  const { container, controller } = mount();
  const scene = requiredElement(container, ".qs-majorana-scene");

  assert.equal(scene.getAttribute("viewBox"), "0 0 1540 756");
  assert.equal(scene.getAttribute("data-step"), "0");
  assert.equal(scene.style.width, "1155px");
  assert.equal(
    requiredElement(container, ".qs-majorana").style.width,
    "1187px",
  );
  assert.equal(container.querySelectorAll(".qs-majorana-tetron").length, 16);
  assert.equal(
    container.querySelectorAll(".qs-majorana-tetron-rail").length,
    32,
  );
  assert.equal(
    container.querySelectorAll(".qs-majorana-tetron-bridge").length,
    16,
  );
  assert.equal(container.querySelectorAll(".qs-majorana-mzm").length, 64);
  assert.equal(container.querySelectorAll(".qs-majorana-island").length, 12);
  assert.equal(
    container.querySelectorAll(".qs-majorana-island-terminal").length,
    24,
  );
  assert.equal(
    container.querySelectorAll(".qs-majorana-routing-obstacle").length,
    280,
  );
  assert.equal(
    container.querySelectorAll(".qs-majorana-qubit-label").length,
    16,
  );
  assert.equal(container.querySelectorAll(".qs-majorana-mzm-label").length, 0);
  assert.ok(container.querySelector('[data-majorana-id="q0-m1"]'));
  assert.ok(container.querySelector('[data-majorana-id="island-r0-c0"]'));
  for (const id of ["q0-m1-block", "q0-m1-connector"]) {
    const obstacle = requiredElement(container, `[data-majorana-id="${id}"]`);
    assert.equal(obstacle.getAttribute("rx"), "5");
    assert.equal(obstacle.getAttribute("ry"), "5");
  }
  assert.ok(
    container.querySelector(
      '[data-majorana-id="horizontal-q0-q1-upper-connector"]',
    ),
  );
  assert.equal(
    requiredElement(container, ".qs-majorana-qubit-label").getAttribute("x"),
    "168",
  );
  assert.match(scene.getAttribute("aria-label"), /16 qubits/);
  assert.equal(container.querySelector(".qs-majorana-virtual"), null);
  assert.equal(
    container.querySelector('button[aria-label="Virtual Qubits"]'),
    null,
  );

  controller.updatePresentation({
    showQubitLabels: false,
    showMzmLabels: true,
  });
  assert.equal(
    container.querySelectorAll(".qs-majorana-qubit-label").length,
    0,
  );
  assert.equal(container.querySelectorAll(".qs-majorana-mzm-label").length, 64);
  controller.dispose();
});

test("renders enabled virtual topology, controls, and active operations", () => {
  const container = document.createElement("div");
  const controller = Majorana(
    container,
    [
      [2, 2],
      [
        [
          ["Mx", [0], ["T", [3], 0]],
          ["Mxx", [0, 1], ["CX", [1, 4], 0]],
          ["Mzz", [0, 2], ["CZ", [0, 2], 0]],
        ],
      ],
    ],
    { enableVirtualView: true },
  );
  const scene = requiredElement(container, ".qs-majorana-scene");
  const virtualButton = requiredElement(
    container,
    'button[aria-label="Virtual Qubits"]',
  );

  assert.equal(scene.getAttribute("data-view"), "Virtual");
  assert.match(scene.getAttribute("aria-label"), /8 virtual qubits/);
  assert.equal(
    requiredElement(virtualButton, ".qs-majorana-view-label-full").textContent,
    "Virtual",
  );
  assert.equal(virtualButton.getAttribute("aria-pressed"), "true");
  assert.match(
    requiredElement(container, ".qs-majorana-keyboard-help-panel").textContent,
    /Cycle Tetrons \/ Qubits \/ Virtual/,
  );
  assert.equal(
    container.querySelectorAll(".qs-majorana-virtual-qubit").length,
    8,
  );
  assert.equal(
    container.querySelectorAll(".qs-majorana-virtual-adjacency").length,
    10,
  );
  assert.equal(
    container.querySelectorAll(".qs-majorana-virtual-adjacency-horizontal")
      .length,
    6,
  );
  assert.equal(
    container.querySelectorAll(".qs-majorana-virtual-adjacency-vertical")
      .length,
    4,
  );
  assert.equal(
    container.querySelectorAll(".qs-majorana-virtual-qubit-label").length,
    8,
  );
  assert.equal(container.querySelector(".qs-majorana-tetron"), null);
  assert.equal(container.querySelector(".qs-majorana-qubit-box"), null);
  assert.ok(container.querySelector('input[aria-label="Qubit labels"]'));
  assert.equal(container.querySelector('input[aria-label="MZM labels"]'), null);
  assert.match(
    requiredElement(container, '[data-majorana-id="v0"]').getAttribute(
      "aria-label",
    ),
    /associated physical qubits Q0 and Q2/,
  );
  const vertical = requiredElement(
    container,
    '[data-majorana-id="vertical-v0-v2"]',
  );
  assert.equal(vertical.getAttribute("data-orientation"), "vertical");
  assert.equal(vertical.getAttribute("data-scope"), "inter-cell");
  assert.equal(vertical.getAttribute("data-supported-operation"), "CZ");
  assert.match(vertical.getAttribute("aria-label"), /supports CZ/);

  controller.setStep(1);
  assert.equal(
    container.querySelectorAll(".qs-majorana-virtual-operation").length,
    3,
  );
  assert.equal(
    container.querySelectorAll(".qs-majorana-virtual-operation-glyph").length,
    5,
  );
  assert.equal(
    container.querySelectorAll(".qs-majorana-virtual-qubit-active").length,
    5,
  );
  assert.equal(
    container.querySelectorAll(".qs-majorana-virtual-adjacency-active").length,
    2,
  );
  assert.equal(
    container.querySelectorAll(
      ".qs-majorana-virtual-adjacency-active.qs-majorana-virtual-adjacency-horizontal",
    ).length,
    1,
  );
  assert.equal(
    container.querySelectorAll(
      ".qs-majorana-virtual-adjacency-active.qs-majorana-virtual-adjacency-vertical",
    ).length,
    1,
  );
  assert.equal(
    requiredElement(
      container,
      ".qs-majorana-virtual-adjacency-active.qs-majorana-virtual-adjacency-horizontal",
    ).getAttribute("data-orientation"),
    "horizontal",
  );
  assert.equal(
    requiredElement(
      container,
      ".qs-majorana-virtual-adjacency-active.qs-majorana-virtual-adjacency-vertical",
    ).getAttribute("data-orientation"),
    "vertical",
  );
  assert.ok(
    requiredElement(
      container,
      '[data-majorana-id="horizontal-v1-v4"]',
    ).classList.contains("qs-majorana-virtual-adjacency-horizontal"),
  );
  assert.ok(
    requiredElement(
      container,
      '[data-majorana-id="vertical-v0-v2"]',
    ).classList.contains("qs-majorana-virtual-adjacency-vertical"),
  );
  assert.match(
    requiredElement(container, '[data-operation="CX"]').getAttribute(
      "aria-label",
    ),
    /CX on V1, V4, via Mxx on Q0, Q1/,
  );
  assert.ok(container.querySelector('[aria-label="CX control on V1: ●"]'));
  assert.ok(container.querySelector('[aria-label="CX target on V4: ⨁"]'));
  const virtualView = requiredElement(container, ".qs-majorana-virtual");
  const operationLayer = requiredElement(
    virtualView,
    ".qs-majorana-virtual-operation-layer",
  );
  const labelLayer = requiredElement(
    virtualView,
    ".qs-majorana-virtual-qubit-label-layer",
  );
  assert.equal(labelLayer.previousElementSibling, operationLayer);
  assert.equal(virtualView.lastElementChild, labelLayer);
  assert.equal(
    requiredElement(labelLayer, ".qs-majorana-virtual-qubit-label").textContent,
    "V0",
  );

  controller.updatePresentation({ showQubitLabels: false });
  assert.equal(
    container.querySelectorAll(".qs-majorana-virtual-qubit-label").length,
    0,
  );
  assert.equal(
    container.querySelector(".qs-majorana-virtual-qubit-label-layer"),
    null,
  );
  requiredElement(container, 'button[aria-label="Qubits"]').click();
  controller.updatePresentation({ showMzmLabels: true });
  requiredElement(container, 'button[aria-label="Virtual Qubits"]').click();
  assert.equal(container.querySelector('input[aria-label="MZM labels"]'), null);
  requiredElement(container, 'button[aria-label="Qubits"]').click();
  assert.equal(
    requiredElement(container, 'input[aria-label="MZM labels"]').checked,
    true,
  );
  controller.dispose();
});

test("renders only the lowest-precedence overlapping virtual operations", () => {
  const container = document.createElement("div");
  const controller = Majorana(
    container,
    [
      [2, 2],
      [
        [
          ["Mx", [0], ["H", [0], 2]],
          ["Mxx", [0, 1], ["CX", [0, 1], 1]],
          ["Mzz", [0, 2], ["CZ", [0, 2], 0]],
          ["T", [1], ["S", [1], 2]],
          ["Mx", [4], ["T", [4], 0]],
        ],
      ],
    ],
    { enableVirtualView: true },
  );

  controller.setStep(1);
  assert.equal(container.querySelector('[data-operation="H"]'), null);
  assert.equal(container.querySelector('[data-operation="S"]'), null);
  assert.equal(container.querySelector('[data-operation="CX"]'), null);
  assert.ok(container.querySelector('[data-operation="CZ"]'));
  assert.ok(container.querySelector('[data-operation="T"]'));
  assert.equal(
    container.querySelectorAll(".qs-majorana-virtual-operation-glyph").length,
    3,
  );
  assert.equal(
    container.querySelectorAll(".qs-majorana-virtual-adjacency-active").length,
    1,
  );
  assert.equal(
    requiredElement(
      container,
      ".qs-majorana-virtual-adjacency-active",
    ).getAttribute("data-orientation"),
    "vertical",
  );
  controller.dispose();
});

test("renders every virtual operation glyph at the circle center", () => {
  const entries = [
    ["T", [0], 0],
    ["H", [0], 0],
    ["S", [0], 0],
    ["Mx", [0], 0],
    ["My", [0], 0],
    ["Mz", [0], 0],
    ["CX", [0, 1], 0],
    ["CZ", [0, 2], 0],
  ];
  const expectedText = ["T", "H", "S", "⟨X❘", "⟨Y❘", "⟨Z❘", "●⨁", "●●"];
  const container = document.createElement("div");
  const controller = Majorana(
    container,
    [
      [2, 2],
      entries.map((virtualOperation) => [["Mx", [0], virtualOperation]]),
    ],
    { enableVirtualView: true },
  );
  const firstLabel = requiredElement(
    container,
    ".qs-majorana-virtual-qubit-label",
  );

  entries.forEach(([name], index) => {
    controller.setStep(index + 1);
    const operation = requiredElement(container, `[data-operation="${name}"]`);
    assert.equal(operation.textContent, expectedText[index]);
    const glyph = requiredElement(
      operation,
      ".qs-majorana-virtual-operation-glyph",
    );
    assert.ok(
      Number(glyph.getAttribute("y")) > Number(firstLabel.getAttribute("y")),
    );
  });
  controller.dispose();
});

test("coalesces duplicate virtual DOM and lists every contributor", () => {
  const container = document.createElement("div");
  const controller = Majorana(
    container,
    [
      [2, 2],
      [
        [
          ["Mx", [0], ["T", [3], 0]],
          ["T", [1], ["T", [3], 0]],
          ["Mzz", [0, 2], ["CZ", [0, 2], 0]],
          ["Myy", [0, 2], ["CZ", [2, 0], 0]],
        ],
      ],
    ],
    { enableVirtualView: true },
  );

  controller.setStep(1);
  assert.equal(
    container.querySelectorAll(".qs-majorana-virtual-operation").length,
    2,
  );
  assert.match(
    requiredElement(container, '[data-operation="T"]').getAttribute(
      "aria-label",
    ),
    /via Mx on Q0; T on Q1/,
  );
  assert.match(
    requiredElement(container, '[data-operation="CZ"]').getAttribute(
      "aria-label",
    ),
    /via Mzz on Q0, Q2; Myy on Q0, Q2/,
  );
  controller.dispose();
});

test("keeps the maximum scene above the measured readable width", () => {
  const container = document.createElement("div");
  const controller = Majorana(container, [[4, 4], [[["Mx", [0], null]]]]);

  assert.equal(
    requiredElement(container, ".qs-majorana-scene").style.width,
    "2295px",
  );
  controller.dispose();
});

test("matches a small initial scene to the bordered component content width", () => {
  const container = document.createElement("div");
  const controller = Majorana(container, [[1, 1], [[["Mx", [0], null]]]]);

  assert.equal(
    requiredElement(container, ".qs-majorana").style.width,
    "1000px",
  );
  assert.equal(
    requiredElement(container, ".qs-majorana-scene").style.width,
    "968px",
  );
  controller.dispose();
});

test("renders solid measurement and dotted pulse loops directly by step", () => {
  const { container, controller } = mount();

  controller.setStep(1);
  const measurement = requiredElement(container, '[data-operation="Mx"]');
  assert.equal(measurement.getAttribute("data-stroke"), "solid");
  assert.ok(!measurement.classList.contains("qs-majorana-loop-dotted"));
  assert.equal(
    measurement.getAttribute("d"),
    "M 300 24 L 200 24 L 200 96 L 300 96 L 355 96 L 355 24 L 300 24 Z",
  );
  assert.match(
    requiredElement(container, ".qs-majorana-operation").getAttribute(
      "aria-label",
    ),
    /Mx on Q0/,
  );

  controller.setStep(2);
  const pulse = requiredElement(container, '[data-operation="T"]');
  assert.equal(pulse.getAttribute("data-stroke"), "dotted");
  assert.ok(pulse.classList.contains("qs-majorana-loop-dotted"));
  assert.match(
    requiredElement(container, ".qs-majorana-operation").getAttribute(
      "aria-label",
    ),
    /dotted T-pulse loop/,
  );
  controller.dispose();
});

test("renders every operation projection in both views", () => {
  const allOperations = [
    ["Mx", [0], null],
    ["My-up", [2], null],
    ["My-lw", [0], null],
    ["Mz-up", [2], null],
    ["Mz-lw", [0], null],
    ["T", [0], null],
    ["Mzz", [0, 2], null],
    ["Mzy", [0, 2], null],
    ["Myy", [0, 2], null],
    ["Myz", [0, 2], null],
    ["Mxx", [0, 1], null],
  ];
  const container = document.createElement("div");
  const controller = Majorana(container, [
    [2, 2],
    allOperations.map((operation) => [operation]),
  ]);

  allOperations.forEach(([name, targets], index) => {
    controller.setView("Tetrons");
    controller.setStep(index + 1);
    assert.ok(container.querySelector(`[data-operation="${name}"]`));

    controller.setView("Qubits");
    assert.equal(
      container.querySelectorAll(`[data-operation="${name}"]`).length,
      targets.length,
    );
  });
  controller.dispose();
});

test("derives virtual projections without changing physical rendering", () => {
  const container = document.createElement("div");
  const controller = Majorana(
    container,
    [[1, 1], [[["Mxx", [0, 1], ["CX", [1, 0], 0]]]]],
    {
      enableVirtualView: true,
      initialView: "Tetrons",
    },
  );

  controller.setStep(1);
  assert.ok(container.querySelector('[data-operation="Mxx"]'));
  controller.setView("Qubits");
  assert.equal(container.querySelectorAll('[data-operation="Mxx"]').length, 2);
  controller.dispose();
});

test("routes loops through tetron bridges, islands, and obstacle channels", () => {
  const container = document.createElement("div");
  const controller = Majorana(container, [
    [1, 1],
    [[["My-lw", [0], null]], [["Mzz", [0, 2], null]], [["Mxx", [0, 1], null]]],
  ]);

  controller.setStep(1);
  assert.equal(
    requiredElement(container, '[data-operation="My-lw"]').getAttribute("d"),
    "M 100 24 L 200 24 L 200 96 L 300 96 L 355 96 L 355 166 L 300 166 L 100 166 L 45 166 L 45 24 L 100 24 Z",
  );

  controller.setStep(2);
  assert.equal(
    requiredElement(container, '[data-operation="Mzz"]').getAttribute("d"),
    "M 100 96 L 300 96 L 355 96 L 355 236 L 300 236 L 100 236 L 45 236 L 45 96 L 100 96 Z",
  );

  controller.setStep(3);
  assert.equal(
    requiredElement(container, '[data-operation="Mxx"]').getAttribute("d"),
    "M 300 24 L 200 24 L 200 96 L 300 96 L 390 96 L 480 96 L 580 96 L 580 24 L 480 24 L 390 24 L 300 24 Z",
  );
  controller.dispose();
});

test("uses aligned centers and active semantics in the qubit view", () => {
  const { container, controller } = mount();
  const physicalCenters = [
    ...container.querySelectorAll(".qs-majorana-tetron"),
  ].map((tetron) => [
    Number(tetron.getAttribute("data-center-x")),
    Number(tetron.getAttribute("data-center-y")),
  ]);

  controller.setStep(3);
  controller.setView("Qubits");

  assert.equal(container.querySelectorAll(".qs-majorana-qubit-box").length, 16);
  assert.equal(container.querySelectorAll(".qs-majorana-adjacency").length, 24);
  assert.equal(
    container.querySelectorAll(".qs-majorana-qubit-box-active").length,
    2,
  );
  assert.equal(
    container.querySelectorAll(".qs-majorana-adjacency-active").length,
    1,
  );
  assert.equal(
    container.querySelectorAll(".qs-majorana-operation-overlay").length,
    2,
  );
  assert.deepEqual(
    [...container.querySelectorAll(".qs-majorana-qubit-box")].map(rectCenter),
    physicalCenters,
  );
  assert.equal(
    requiredElement(
      container,
      '[data-majorana-id="vertical-q0-q2"]',
    ).getAttribute("data-active"),
    "true",
  );
  assert.match(
    requiredElement(
      container,
      '[data-majorana-id="vertical-q0-q2"]',
    ).getAttribute("aria-label"),
    /Active adjacency between Q0 and Q2/,
  );

  controller.setStep(2);
  assert.ok(
    requiredElement(
      container,
      ".qs-majorana-operation-overlay",
    ).classList.contains("qs-majorana-operation-overlay-pulse"),
  );
  controller.dispose();
});

test("preserves the last scene on invalid input and recovers", () => {
  const { container, controller } = mount();
  controller.setStep(2);
  controller.updateInput([[1, 1], []]);

  assert.equal(
    requiredElement(container, ".qs-majorana-scene").getAttribute("data-step"),
    "2",
  );
  assert.ok(container.querySelector('[role="alert"]'));

  controller.updateInput([[1, 1], [[["Mx", [0], null]]]]);
  assert.equal(container.querySelector('[role="alert"]'), null);
  assert.equal(
    requiredElement(container, ".qs-majorana-scene").getAttribute("data-step"),
    "1",
  );
  controller.dispose();
});

test("returns a recoverable controller for invalid initial input", () => {
  const container = document.createElement("div");
  const controller = Majorana(container, [[1, 1], []]);

  assert.equal(container.querySelector(".qs-majorana-scene"), null);
  assert.ok(container.querySelector('[role="alert"]'));
  controller.updateInput(input);
  assert.ok(container.querySelector(".qs-majorana-scene"));

  assert.throws(() => controller.setView("invalid"), MajoranaValidationError);
  controller.dispose();
  assert.equal(container.childElementCount, 0);
  assert.throws(
    () => controller.setStep(0),
    /Majorana controller has been disposed/,
  );
});

function mount() {
  const container = document.createElement("div");
  const controller = Majorana(container, input);
  return { container, controller };
}

function requiredElement(container, selector) {
  const element = container.querySelector(selector);
  assert.ok(element, `Expected element matching ${selector}`);
  return element;
}

function rectCenter(rect) {
  return [
    Number(rect.getAttribute("x")) + Number(rect.getAttribute("width")) / 2,
    Number(rect.getAttribute("y")) + Number(rect.getAttribute("height")) / 2,
  ];
}
