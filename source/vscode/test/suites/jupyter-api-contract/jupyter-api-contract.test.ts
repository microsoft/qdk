// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { assert } from "chai";
import * as vscode from "vscode";
import { TEST_TIMEOUT_MS } from "../extensionUtils";

suite("Jupyter API contract", function () {
  this.timeout(TEST_TIMEOUT_MS);

  let api: Record<string, unknown>;
  let version: string;

  suiteSetup(async () => {
    const extension =
      vscode.extensions.getExtension<unknown>("ms-toolsai.jupyter");
    assert.isDefined(
      extension,
      "The Jupyter extension must be installed for this contract test",
    );

    version = String(extension.packageJSON.version ?? "unknown");
    console.info(`Testing Jupyter extension API version ${version}`);

    const exports: unknown = await extension.activate();
    assert.isObject(
      exports,
      `Jupyter ${version} activation must return an API object`,
    );
    api = exports as Record<string, unknown>;
  });

  test("exports openNotebook", () => {
    assert.isFunction(
      api.openNotebook,
      `Jupyter ${version} must export openNotebook`,
    );
  });

  test("exports a Python environment change event", () => {
    const onDidChangePythonEnvironment = getRequiredFunction(
      "onDidChangePythonEnvironment",
    );
    let subscription: unknown;
    try {
      subscription = onDidChangePythonEnvironment(() => undefined);
      assert.isObject(
        subscription,
        `Jupyter ${version} onDidChangePythonEnvironment must return a disposable`,
      );
      assert.isFunction(
        (subscription as Record<string, unknown>).dispose,
        `Jupyter ${version} event subscription must be disposable`,
      );
    } finally {
      const subscriptionRecord = subscription as
        | Record<string, unknown>
        | undefined;
      if (typeof subscriptionRecord?.dispose === "function") {
        subscriptionRecord.dispose();
      }
    }
  });

  test("returns the expected Python environment shape", () => {
    const getPythonEnvironment = getRequiredFunction("getPythonEnvironment");
    const environment = getPythonEnvironment(
      vscode.Uri.parse("untitled:jupyter-api-contract.ipynb"),
    );
    if (environment === undefined) {
      return;
    }

    assert.isObject(
      environment,
      `Jupyter ${version} getPythonEnvironment must return an object or undefined`,
    );
    const environmentRecord = environment as Record<string, unknown>;
    assert.isString(
      environmentRecord.id,
      `Jupyter ${version} Python environment ID must be a string`,
    );
    assert.isString(
      environmentRecord.path,
      `Jupyter ${version} Python environment path must be a string`,
    );
  });

  function getRequiredFunction(name: string): (...args: unknown[]) => unknown {
    const member = api[name];
    assert.isFunction(member, `Jupyter ${version} must export ${name}`);
    return member as (...args: unknown[]) => unknown;
  }
});
