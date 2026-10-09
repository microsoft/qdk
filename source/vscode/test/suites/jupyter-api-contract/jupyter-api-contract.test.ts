// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { assert } from "chai";
import * as vscode from "vscode";
import { TEST_TIMEOUT_MS } from "../extensionUtils";

suite("Jupyter API contract", function () {
  this.timeout(TEST_TIMEOUT_MS);

  let jupyterApi: Record<string, unknown>;
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
    jupyterApi = exports as Record<string, unknown>;
  });

  test("opens a notebook with a Python environment", async () => {
    const pythonPath = process.env.JUPYTER_API_TEST_PYTHON_PATH;
    assert.isString(
      pythonPath,
      "The test runner must provide a Python environment path",
    );

    const pythonExtension =
      vscode.extensions.getExtension<unknown>("ms-python.python");
    assert.isDefined(
      pythonExtension,
      "The Python extension must be installed for this contract test",
    );
    const pythonExports: unknown = await pythonExtension.activate();
    assert.isObject(
      pythonExports,
      "Python extension activation must return an API object",
    );
    const pythonApi = pythonExports as Record<string, unknown>;
    assert.isObject(
      pythonApi.environments,
      "The Python extension must export its environments API",
    );
    const environments = pythonApi.environments as Record<string, unknown>;
    const resolveEnvironment = getRequiredFunction(
      environments,
      "resolveEnvironment",
      "Python",
    );
    const resolvedEnvironment = await resolveEnvironment.call(
      environments,
      pythonPath,
    );
    assertEnvironment(resolvedEnvironment, "resolved Python environment");
    const expectedEnvironment = resolvedEnvironment as Record<string, string>;

    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    assert.isDefined(workspaceFolder, "The test workspace must be open");
    const notebookUri = vscode.Uri.joinPath(
      workspaceFolder.uri,
      "jupyter-api-contract.ipynb",
    );

    const openNotebook = getRequiredFunction(
      jupyterApi,
      "openNotebook",
      `Jupyter ${version}`,
    );
    const onDidChangePythonEnvironment = getRequiredFunction(
      jupyterApi,
      "onDidChangePythonEnvironment",
      `Jupyter ${version}`,
    );
    const getPythonEnvironment = getRequiredFunction(
      jupyterApi,
      "getPythonEnvironment",
      `Jupyter ${version}`,
    );

    let resolveEnvironmentChanged: () => void;
    let rejectEnvironmentChanged!: (reason: Error) => void;
    const environmentChanged = new Promise<void>((resolve, reject) => {
      resolveEnvironmentChanged = resolve;
      rejectEnvironmentChanged = reject;
    });
    const environmentChangeTimeoutError = new Error(
      `Jupyter ${version} did not report an environment change for the test notebook`,
    );
    const eventTimeout = setTimeout(
      rejectEnvironmentChanged,
      60_000,
      environmentChangeTimeoutError,
    );

    const subscription = onDidChangePythonEnvironment.call(
      jupyterApi,
      (changedUri: unknown) => {
        if (
          typeof (changedUri as vscode.Uri | undefined)?.toString !== "function"
        ) {
          clearTimeout(eventTimeout);
          rejectEnvironmentChanged(
            new Error(
              `Jupyter ${version} emitted an environment change without a URI`,
            ),
          );
          return;
        }
        if ((changedUri as vscode.Uri).toString() === notebookUri.toString()) {
          clearTimeout(eventTimeout);
          resolveEnvironmentChanged();
        }
      },
    );
    assertDisposable(subscription);

    try {
      await openNotebook.call(jupyterApi, notebookUri, {
        id: expectedEnvironment.id,
        path: expectedEnvironment.path,
      });
      await environmentChanged;

      assert.equal(
        vscode.window.activeNotebookEditor?.notebook.uri.toString(),
        notebookUri.toString(),
        `Jupyter ${version} must open the requested notebook`,
      );

      const actualEnvironment = getPythonEnvironment.call(
        jupyterApi,
        notebookUri,
      );
      assertEnvironment(actualEnvironment, "notebook Python environment");
      const actualEnvironmentRecord = actualEnvironment as Record<
        string,
        string
      >;
      assert.equal(actualEnvironmentRecord.id, expectedEnvironment.id);
      assert.equal(
        normalizePath(actualEnvironmentRecord.path),
        normalizePath(expectedEnvironment.path),
      );
    } finally {
      clearTimeout(eventTimeout);
      (subscription as { dispose(): void }).dispose();
    }
  });

  function assertEnvironment(environment: unknown, description: string): void {
    assert.isObject(environment, `The ${description} must be an object`);
    const environmentRecord = environment as Record<string, unknown>;
    assert.isString(
      environmentRecord.id,
      `The ${description} ID must be a string`,
    );
    assert.isString(
      environmentRecord.path,
      `The ${description} path must be a string`,
    );
  }

  function assertDisposable(disposable: unknown): void {
    assert.isObject(
      disposable,
      `Jupyter ${version} onDidChangePythonEnvironment must return a disposable`,
    );
    assert.isFunction(
      (disposable as Record<string, unknown>).dispose,
      `Jupyter ${version} event subscription must be disposable`,
    );
  }

  function getRequiredFunction(
    target: Record<string, unknown>,
    name: string,
    owner: string,
  ): (...args: unknown[]) => unknown {
    const member = target[name];
    assert.isFunction(member, `${owner} must export ${name}`);
    return member as (...args: unknown[]) => unknown;
  }

  function normalizePath(value: string): string {
    const path = vscode.Uri.file(value).fsPath;
    return process.platform === "win32" ? path.toLowerCase() : path;
  }
});
