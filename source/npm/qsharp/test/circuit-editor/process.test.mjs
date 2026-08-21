// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// @ts-check

import assert from "node:assert/strict";
import { test } from "node:test";
import { gate, group } from "./_helpers.mjs";
import { processOperations } from "../../dist/ux/circuit-vis/renderer/process.js";

const registers = {
  0: { type: 0, y: 40 },
  1: { type: 0, y: 80 },
  2: { type: 0, y: 120 },
  3: { type: 0, y: 150, children: [{ type: 1, y: 160 }] },
};

const processConditional = (label, children) => {
  const operation = group(label, children, {
    ctrls: [{ q: 3, r: 0 }],
    conditional: true,
  });

  return processOperations([{ components: [operation] }], 40, 160, registers)
    .renderDataArray[0][0];
};

test("processOperations: detects compact classical controls", () => {
  const cases = [
    { name: "X", label: "if: c_0 = |1〉", children: [[gate("X", 0)]] },
    { name: "H", label: "if: c_0 = |1〉", children: [[gate("H", 0)]] },
    {
      name: "anti-controlled X",
      label: "if: c_0 = |0〉",
      children: [[gate("X", 0)]],
      isAntiControlled: true,
    },
    {
      name: "Rxx",
      label: "if: c_0 = |1〉",
      children: [[gate("Rxx", [0, 1])]],
    },
  ];

  for (const { name, label, children, isAntiControlled = false } of cases) {
    const renderData = processConditional(label, children);
    assert.equal(renderData.displayAsClassicallyControlledGate, true, name);
    assert.equal(renderData.isAntiControlled, isAntiControlled, name);
  }
});

test("processOperations: rejects non-compact classical controls", () => {
  const cases = [
    {
      name: "complex condition",
      label: "if: f(c_0)",
      children: [[gate("X", 0)]],
    },
    {
      name: "two gates",
      label: "if: c_0 = |1〉",
      children: [[gate("X", 0), gate("H", 1)]],
    },
    {
      name: "CNOT",
      label: "if: c_0 = |1〉",
      children: [[gate("X", 1, { ctrls: [0] })]],
    },
  ];

  for (const { name, label, children } of cases) {
    const renderData = processConditional(label, children);
    assert.equal(renderData.displayAsClassicallyControlledGate, false, name);
    assert.equal(renderData.isAntiControlled, false, name);
  }
});
