// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  MajoranaValidationError,
  validateMajoranaInput,
  validateMajoranaOptions,
  validateMajoranaPresentationOptions,
} from "../../dist/ux/majorana/index.js";

const exampleInput = [
  [1, 1],
  [
    [
      ["Mz-lw", [0], null],
      ["Mz-up", [3], null],
    ],
    [
      ["Mz-up", [2], null],
      ["Mz-lw", [1], null],
    ],
    [
      ["Mxx", [0, 1], null],
      ["Mxx", [2, 3], null],
    ],
    [
      ["Mz-lw", [0], null],
      ["Mz-up", [3], null],
    ],
    [
      ["Mz-up", [2], null],
      ["Mz-lw", [1], null],
    ],
  ],
];

function expectValidationError(input, code, path, options) {
  assert.throws(
    () => validateMajoranaInput(input, options),
    (error) => {
      assert.ok(error instanceof MajoranaValidationError);
      assert.equal(error.code, code);
      assert.equal(error.path, path);
      assert.match(error.message, /received/);
      return true;
    },
  );
}

test("validates and normalizes the documented example without mutation", () => {
  const original = structuredClone(exampleInput);
  const validated = validateMajoranaInput(exampleInput);

  assert.deepEqual(exampleInput, original);
  assert.deepEqual(validated.device, {
    cellRows: 1,
    cellColumns: 1,
    tetronRows: 2,
    tetronColumns: 2,
    qubitCount: 4,
  });
  assert.equal(validated.trace.length, 5);
  assert.deepEqual(validated.trace[0][0], {
    kind: "single",
    name: "Mz-lw",
    target: 0,
    pauli: "Z",
    islandRoute: "lower",
    operationType: "measurement",
    virtualOperation: undefined,
  });
  assert.deepEqual(validated.trace[2][0], {
    kind: "joint",
    name: "Mxx",
    firstTarget: 0,
    secondTarget: 1,
    orientation: "horizontal",
    firstPauli: "X",
    secondPauli: "X",
    virtualOperation: undefined,
  });
});

test("requires three-member physical event tuples in every mode", () => {
  expectValidationError(
    [[1, 1], [[["Mx", [0]]]]],
    "invalid-step",
    "trace[0][0]",
  );
  expectValidationError(
    [[1, 1], [[["Mx", [0], null, "extra"]]]],
    "invalid-step",
    "trace[0][0]",
  );
});

test("ignores third-member contents when virtual validation is disabled", () => {
  const metadata = { malformed: true };
  const input = [[1, 1], [[["Mx", [0], metadata]]]];
  const original = structuredClone(input);
  const validated = validateMajoranaInput(input);

  assert.deepEqual(input, original);
  assert.equal(validated.trace[0][0].virtualOperation, undefined);
});

test("normalizes every supported virtual operation without mutation", () => {
  const input = [
    [2, 2],
    [
      [["Mx", [0], ["T", [0], 0]]],
      [["Mx", [0], ["H", [0], 0]]],
      [["Mx", [0], ["S", [0], 0]]],
      [["Mx", [0], ["Mx", [0], 0]]],
      [["Mx", [0], ["My", [0], 0]]],
      [["Mx", [0], ["Mz", [0], 0]]],
      [["Mx", [0], ["CX", [1, 4], 0]]],
      [["Mx", [0], ["CZ", [2, 0], 0]]],
    ],
  ];
  const original = structuredClone(input);
  const validated = validateMajoranaInput(input, { enableVirtualView: true });

  assert.deepEqual(input, original);
  assert.deepEqual(
    validated.trace
      .slice(0, 6)
      .map(([operation]) => operation.virtualOperation),
    [
      { kind: "single", name: "T", target: 0, precedence: 0 },
      { kind: "single", name: "H", target: 0, precedence: 0 },
      { kind: "single", name: "S", target: 0, precedence: 0 },
      { kind: "single", name: "Mx", target: 0, precedence: 0 },
      { kind: "single", name: "My", target: 0, precedence: 0 },
      { kind: "single", name: "Mz", target: 0, precedence: 0 },
    ],
  );
  assert.deepEqual(validated.trace[6][0].virtualOperation, {
    kind: "joint",
    name: "CX",
    control: 1,
    target: 4,
    precedence: 0,
    orientation: "horizontal",
    adjacencyEdgeId: "horizontal-v1-v4",
  });
  assert.deepEqual(validated.trace[7][0].virtualOperation, {
    kind: "joint",
    name: "CZ",
    firstTarget: 2,
    secondTarget: 0,
    precedence: 0,
    orientation: "vertical",
    adjacencyEdgeId: "vertical-v0-v2",
  });
});

test("rejects invalid enabled virtual metadata with stable codes and paths", () => {
  const enabled = { enableVirtualView: true };
  expectValidationError(
    [[1, 1], [[["Mx", [0], null]]]],
    "invalid-virtual-metadata",
    "trace[0][0].virtualOperation",
    enabled,
  );
  expectValidationError(
    [[1, 1], [[["Mx", [0], ["T", [0]]]]]],
    "invalid-virtual-metadata",
    "trace[0][0].virtualOperation",
    enabled,
  );
  expectValidationError(
    [[1, 1], [[["Mx", [0], ["X", [0], 0]]]]],
    "unknown-virtual-operation",
    "trace[0][0].virtualOperation.operation",
    enabled,
  );
  expectValidationError(
    [[1, 1], [[["Mx", [0], ["T", [0, 1], 0]]]]],
    "invalid-virtual-target-count",
    "trace[0][0].virtualOperation.targets",
    enabled,
  );
  expectValidationError(
    [[1, 1], [[["Mx", [0], ["CZ", [0], 0]]]]],
    "invalid-virtual-target-count",
    "trace[0][0].virtualOperation.targets",
    enabled,
  );
  expectValidationError(
    [[1, 1], [[["Mx", [0], ["T", [2], 0]]]]],
    "invalid-virtual-target",
    "trace[0][0].virtualOperation.targets[0]",
    enabled,
  );
  expectValidationError(
    [[1, 1], [[["Mx", [0], ["CX", [0, 0], 0]]]]],
    "duplicate-virtual-target",
    "trace[0][0].virtualOperation.targets",
    enabled,
  );
  expectValidationError(
    [[2, 1], [[["Mx", [0], ["CX", [0, 2], 0]]]]],
    "invalid-virtual-adjacency",
    "trace[0][0].virtualOperation.targets",
    enabled,
  );
  expectValidationError(
    [[1, 1], [[["Mx", [0], ["CZ", [0, 1], 0]]]]],
    "invalid-virtual-adjacency",
    "trace[0][0].virtualOperation.targets",
    enabled,
  );
  expectValidationError(
    [[2, 2], [[["Mx", [0], ["CX", [0, 5], 0]]]]],
    "invalid-virtual-adjacency",
    "trace[0][0].virtualOperation.targets",
    enabled,
  );
  for (const precedence of [-1, 0.5, "0"]) {
    expectValidationError(
      [[1, 1], [[["Mx", [0], ["T", [0], precedence]]]]],
      "invalid-virtual-precedence",
      "trace[0][0].virtualOperation.precedence",
      enabled,
    );
  }
});

test("normalizes every supported operation", () => {
  const validated = validateMajoranaInput([
    [2, 2],
    [
      [["Mx", [0], null]],
      [["My-up", [2], null]],
      [["My-lw", [0], null]],
      [["Mz-up", [2], null]],
      [["Mz-lw", [0], null]],
      [["T", [0], null]],
      [["Mzz", [0, 2], null]],
      [["Mzy", [0, 2], null]],
      [["Myy", [0, 2], null]],
      [["Myz", [0, 2], null]],
      [["Mxx", [0, 1], null]],
    ],
  ]);

  assert.deepEqual(
    validated.trace.map(([operation]) => operation.name),
    [
      "Mx",
      "My-up",
      "My-lw",
      "Mz-up",
      "Mz-lw",
      "T",
      "Mzz",
      "Mzy",
      "Myy",
      "Myz",
      "Mxx",
    ],
  );
  assert.equal(validated.trace[5][0].operationType, "pulse");
  assert.equal(validated.trace[7][0].secondPauli, "Y");
});

test("accepts adjacency across cell boundaries", () => {
  const validated = validateMajoranaInput([
    [2, 2],
    [[["Mzz", [2, 4], null]], [["Mxx", [1, 8], null]]],
  ]);

  assert.equal(validated.trace[0][0].orientation, "vertical");
  assert.equal(validated.trace[1][0].orientation, "horizontal");
});

test("rejects malformed outer input", () => {
  expectValidationError({}, "invalid-input-shape", "input");
  expectValidationError([[1, 1]], "invalid-input-shape", "input");
});

test("rejects invalid device dimensions", () => {
  expectValidationError(
    [[1], [[["Mx", [0], null]]]],
    "invalid-device-size",
    "deviceSize",
  );
  expectValidationError(
    [[0, 1], [[["Mx", [0], null]]]],
    "invalid-device-size",
    "deviceSize[0]",
  );
  expectValidationError(
    [[1, 4.5], [[["Mx", [0], null]]]],
    "invalid-device-size",
    "deviceSize[1]",
  );
});

test("rejects empty and malformed traces and steps", () => {
  expectValidationError([[1, 1], []], "invalid-trace", "trace");
  expectValidationError([[1, 1], [[]]], "invalid-step", "trace[0]");
  expectValidationError([[1, 1], [["Mx"]]], "invalid-step", "trace[0][0]");
});

test("rejects sparse traces and steps", () => {
  expectValidationError([[1, 1], new Array(1)], "invalid-step", "trace[0]");
  expectValidationError(
    [[1, 1], [new Array(1)]],
    "invalid-step",
    "trace[0][0]",
  );
});

test("rejects unknown operation names", () => {
  expectValidationError(
    [[1, 1], [[["mx", [0], null]]]],
    "unknown-operation",
    "trace[0][0].operation",
  );
});

test("rejects invalid target counts and values", () => {
  expectValidationError(
    [[1, 1], [[["Mx", [0, 1], null]]]],
    "invalid-target-count",
    "trace[0][0].targets",
  );
  expectValidationError(
    [[1, 1], [[["Mxx", [0], null]]]],
    "invalid-target-count",
    "trace[0][0].targets",
  );
  expectValidationError(
    [[1, 1], [[["Mx", [4], null]]]],
    "invalid-target",
    "trace[0][0].targets[0]",
  );
  expectValidationError(
    [[1, 1], [[["Mx", [0.5], null]]]],
    "invalid-target",
    "trace[0][0].targets[0]",
  );
});

test("rejects duplicate joint targets", () => {
  expectValidationError(
    [[1, 1], [[["Mxx", [0, 0], null]]]],
    "duplicate-target",
    "trace[0][0].targets",
  );
});

test("distinguishes invalid adjacency from invalid ordering", () => {
  expectValidationError(
    [[1, 1], [[["Mxx", [0, 3], null]]]],
    "invalid-adjacency",
    "trace[0][0].targets",
  );
  expectValidationError(
    [[1, 1], [[["Mxx", [1, 0], null]]]],
    "invalid-target-order",
    "trace[0][0].targets",
  );
  expectValidationError(
    [[1, 1], [[["Mzz", [0, 3], null]]]],
    "invalid-adjacency",
    "trace[0][0].targets",
  );
  expectValidationError(
    [[1, 1], [[["Mzz", [2, 0], null]]]],
    "invalid-target-order",
    "trace[0][0].targets",
  );
});

test("rejects routes requiring perimeter islands", () => {
  expectValidationError(
    [[1, 1], [[["My-up", [0], null]]]],
    "unavailable-island",
    "trace[0][0].targets[0]",
  );
  expectValidationError(
    [[1, 1], [[["Mz-lw", [2], null]]]],
    "unavailable-island",
    "trace[0][0].targets[0]",
  );
});

test("validates options with documented defaults", () => {
  assert.deepEqual(validateMajoranaOptions(), {
    enableVirtualView: false,
    showQubitLabels: true,
    showMzmLabels: false,
    initialView: "Tetrons",
  });
  assert.deepEqual(
    validateMajoranaOptions({
      showQubitLabels: false,
      showMzmLabels: true,
      initialView: "Qubits",
    }),
    {
      enableVirtualView: false,
      showQubitLabels: false,
      showMzmLabels: true,
      initialView: "Qubits",
    },
  );
  assert.deepEqual(validateMajoranaOptions({ enableVirtualView: true }), {
    enableVirtualView: true,
    showQubitLabels: true,
    showMzmLabels: false,
    initialView: "Virtual",
  });
  assert.equal(
    validateMajoranaOptions({
      enableVirtualView: true,
      initialView: "Tetrons",
    }).initialView,
    "Tetrons",
  );
});

test("rejects invalid and unknown options", () => {
  assert.throws(
    () => validateMajoranaOptions({ initialView: "loops" }),
    (error) =>
      error instanceof MajoranaValidationError &&
      error.code === "invalid-options" &&
      error.path === "options.initialView",
  );
  assert.throws(
    () => validateMajoranaOptions({ initialView: "Virtual" }),
    (error) =>
      error instanceof MajoranaValidationError &&
      error.code === "invalid-options" &&
      error.path === "options.initialView",
  );
  assert.throws(
    () => validateMajoranaOptions({ enableVirtualView: "yes" }),
    (error) =>
      error instanceof MajoranaValidationError &&
      error.code === "invalid-options" &&
      error.path === "options.enableVirtualView",
  );
  assert.throws(
    () => validateMajoranaOptions({ showMZMLabels: true }),
    (error) =>
      error instanceof MajoranaValidationError &&
      error.code === "invalid-options" &&
      error.path === "options.showMZMLabels",
  );
});

test("presentation option updates preserve omitted fields", () => {
  assert.deepEqual(validateMajoranaPresentationOptions({}), {});
  assert.deepEqual(
    validateMajoranaPresentationOptions({ showMzmLabels: true }),
    { showMzmLabels: true },
  );
  assert.throws(
    () => validateMajoranaPresentationOptions({ initialView: "Qubits" }),
    (error) =>
      error instanceof MajoranaValidationError &&
      error.code === "invalid-options",
  );
});
