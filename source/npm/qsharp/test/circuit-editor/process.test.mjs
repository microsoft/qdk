// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// @ts-check

import assert from "node:assert/strict";
import { test } from "node:test";
import { processOperations } from "../../dist/ux/circuit-vis/renderer/process.js";

const registers = {
  0: { type: 0, y: 40 },
  1: { type: 0, y: 80, children: [{ type: 1, y: 100 }] },
};

test("processOperations: maps direct classical controls to render data", () => {
  const operation = {
    kind: "unitary",
    gate: "H",
    targets: [{ qubit: 0 }],
    classicalControls: [
      { register: { qubit: 1, result: 0 } },
      { register: { qubit: 1, result: 0 }, inverted: true },
    ],
  };

  const renderData = processOperations(
    [{ components: [operation] }],
    40,
    100,
    registers,
  ).renderDataArray[0][0];

  assert.deepEqual(renderData.classicalControls, [
    { y: 100, inverted: false },
    { y: 100, inverted: true },
  ]);
  assert.deepEqual(renderData.classicalControlRegs, [{ qubit: 1, result: 0 }]);
});
