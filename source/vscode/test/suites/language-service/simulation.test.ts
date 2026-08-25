// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { assert } from "chai";
import * as vscode from "vscode";
import { getSimulationConfig } from "../../../src/config";

suite("Q# Simulation Configuration Tests", function suite() {
  const simulation = vscode.workspace.getConfiguration("Q#.simulation");

  this.afterEach(async () => {
    await simulation.update("type", undefined);
    await simulation.update("clifford.maxQubits", undefined);
  });

  test("defaults to sparse simulation", () => {
    assert.deepEqual(getSimulationConfig(), { type: "sparse" });
  });

  test("reads Clifford simulation settings", async () => {
    await simulation.update("type", "clifford");
    await simulation.update("clifford.maxQubits", 2048);

    assert.deepEqual(getSimulationConfig(), {
      type: "clifford",
      maxQubits: 2048,
    });
  });

  test("rejects an unknown simulator type", async () => {
    await simulation.update("type", "invalid");

    assert.throws(
      () => getSimulationConfig(),
      /Expected "sparse" or "clifford"/,
    );
  });

  test("rejects an invalid Clifford qubit capacity", async () => {
    await simulation.update("type", "clifford");
    await simulation.update("clifford.maxQubits", 0);

    assert.throws(
      () => getSimulationConfig(),
      /must be an integer between 1 and 10000/,
    );

    await simulation.update("clifford.maxQubits", 10001);
    assert.throws(
      getSimulationConfig,
      /must be an integer between 1 and 10000/,
    );
  });
});
