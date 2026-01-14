// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { assert } from "chai";
import { activateExtension, waitForCondition } from "../extensionUtils";
import { DebugProtocol } from "@vscode/debugprotocol";
import { qsharpExtensionId } from "../../../src/common";
import { Tracker } from "./tracker";

suite("Q# Debugger Tests", function suite() {
  const workspaceFolder =
    vscode.workspace.workspaceFolders && vscode.workspace.workspaceFolders[0];
  assert(workspaceFolder, "Expecting an open folder");
  const fooUri = vscode.Uri.joinPath(workspaceFolder.uri, "src", "foo.qs");
  const barUri = vscode.Uri.joinPath(workspaceFolder.uri, "src", "bar.qs");

  let tracker: Tracker | undefined;
  let disposable;

  this.beforeAll(async () => {
    await activateExtension();
  });

  this.beforeEach(async () => {
    tracker = new Tracker();
    disposable = vscode.debug.registerDebugAdapterTrackerFactory("qsharp", {
      createDebugAdapterTracker(): vscode.ProviderResult<vscode.DebugAdapterTracker> {
        return tracker;
      },
    });
  });

  this.afterEach(async () => {
    disposable.dispose();
    tracker = undefined;
    await terminateSession();
    vscode.commands.executeCommand("workbench.action.closeAllEditors");
    vscode.debug.removeBreakpoints(vscode.debug.breakpoints);
  });

  test("Launch with debugProgram command", async () => {
    this.timeout(4000); // This test can be slow on some CI machines, so increase timeout
    await vscode.window.showTextDocument(fooUri);

    // launch debugger
    await vscode.commands.executeCommand(`${qsharpExtensionId}.debugProgram`);

    await waitUntilPausedAndAssertStackTrace([
      {
        id: 0,
        source: {
          name: "foo.qs",
          path: "vscode-test-web://mount/src/foo.qs",
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 5,
        column: 9,
        name: "Foo ",
        endLine: 5,
        endColumn: 15,
      },
      { id: 0, line: 0, column: 0, name: "entry", source: undefined },
    ]);
  });

  test("Launch with launch.json configuration - workspaceFolder substitution", async () => {
    // The DebugConfiguration object is what would go in launch.json,
    // pass it in directly here
    await vscode.debug.startDebugging(workspaceFolder, {
      name: "Launch foo.qs",
      type: "qsharp",
      request: "launch",
      program: "${workspaceFolder}src/foo.qs",
    });

    await waitUntilPausedAndAssertStackTrace([
      {
        id: 0,
        source: {
          name: "foo.qs",
          path: "vscode-test-web://mount/src/foo.qs",
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 5,
        column: 9,
        name: "Foo ",
        endLine: 5,
        endColumn: 15,
      },
      { id: 0, line: 0, column: 0, name: "entry", source: undefined },
    ]);
  });

  test("Launch with launch.json configuration - file substitution", async () => {
    await vscode.window.showTextDocument(fooUri);

    // ${file} will expand to the filesystem path of the currently opened file
    await vscode.debug.startDebugging(workspaceFolder, {
      name: "Launch foo.qs",
      type: "qsharp",
      request: "launch",
      program: "${file}",
    });

    await waitUntilPausedAndAssertStackTrace([
      {
        id: 0,
        source: {
          name: "foo.qs",
          path: "vscode-test-web://mount/src/foo.qs",
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 5,
        column: 9,
        name: "Foo ",
        endLine: 5,
        endColumn: 15,
      },
      { id: 0, line: 0, column: 0, name: "entry", source: undefined },
    ]);
  });

  test("Run until completion", async () => {
    // launch debugger
    await vscode.debug.startDebugging(workspaceFolder, {
      name: "Launch foo.qs",
      type: "qsharp",
      request: "launch",
      program: "${workspaceFolder}src/foo.qs",
      stopOnEntry: true,
    });

    // should hit the breakpoint we set above
    await waitUntilPausedAndAssertStackTrace([
      {
        id: 0,
        source: {
          name: "foo.qs",
          path: "vscode-test-web://mount/src/foo.qs",
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 5,
        column: 9,
        name: "Foo ",
        endLine: 5,
        endColumn: 15,
      },
      { id: 0, line: 0, column: 0, name: "entry", source: undefined },
    ]);

    vscode.commands.executeCommand("workbench.action.debug.continue");

    // wait until there's no longer an active debug session
    await waitForCondition(
      () => !vscode.debug.activeDebugSession,
      vscode.debug.onDidChangeActiveDebugSession,
      2000,
      "timed out waiting for the debugger to be terminated",
    );
  });

  test("Set breakpoint in main file", async () => {
    // Set a breakpoint on line 6 of foo.qs (5 when 0-indexed)
    await vscode.debug.addBreakpoints([
      new vscode.SourceBreakpoint(
        new vscode.Location(fooUri, new vscode.Position(5, 0)),
      ),
    ]);

    // launch debugger
    await vscode.debug.startDebugging(workspaceFolder, {
      name: "Launch foo.qs",
      type: "qsharp",
      request: "launch",
      program: "${workspaceFolder}src/foo.qs",
      stopOnEntry: false,
    });

    // should hit the breakpoint we set above
    await waitUntilPausedAndAssertStackTrace([
      {
        id: 0,
        source: {
          name: "foo.qs",
          path: "vscode-test-web://mount/src/foo.qs",
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 6,
        column: 9,
        name: "Foo ",
        endLine: 6,
        endColumn: 15,
      },
      { id: 0, line: 0, column: 0, name: "entry", source: undefined },
    ]);
  });

  test("Set breakpoint in other file", async () => {
    // Set a breakpoint on line 3 of bar.qs (2 when 0-indexed)
    await vscode.debug.addBreakpoints([
      new vscode.SourceBreakpoint(
        new vscode.Location(barUri, new vscode.Position(2, 0)),
      ),
    ]);

    // launch debugger
    await vscode.debug.startDebugging(workspaceFolder, {
      name: "Launch foo.qs",
      type: "qsharp",
      request: "launch",
      program: "${workspaceFolder}src/foo.qs",
      stopOnEntry: false,
    });

    // should hit the breakpoint we set above
    await waitUntilPausedAndAssertStackTrace([
      {
        id: 1,
        source: {
          name: "bar.qs",
          path: "vscode-test-web://mount/src/bar.qs",
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 3,
        column: 9,
        name: "Bar ",
        endLine: 3,
        endColumn: 26,
      },
      {
        id: 0,
        source: {
          name: "foo.qs",
          path: "vscode-test-web://mount/src/foo.qs",
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 5,
        column: 9,
        name: "Foo ",
        endLine: 5,
        endColumn: 14,
      },
      { id: 0, line: 0, column: 0, name: "entry", source: undefined },
    ]);

    // text editor should now be open on bar.qs
    await waitForTextEditorOn(barUri);
  });

  test("Step into other file", async () => {
    // launch debugger
    await vscode.debug.startDebugging(workspaceFolder, {
      name: "Launch foo.qs",
      type: "qsharp",
      request: "launch",
      program: "${workspaceFolder}src/foo.qs",
      stopOnEntry: true,
    });

    // should break on entry (per debug config above)
    await waitUntilPausedAndAssertStackTrace([
      {
        id: 0,
        source: {
          name: "foo.qs",
          path: "vscode-test-web://mount/src/foo.qs",
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 5,
        column: 9,
        name: "Foo ",
        endLine: 5,
        endColumn: 15,
      },
      { id: 0, line: 0, column: 0, name: "entry", source: undefined },
    ]);

    // step into call (will be a call into bar.qs)
    await vscode.commands.executeCommand("workbench.action.debug.stepInto");

    await waitUntilPausedAndAssertStackTrace([
      {
        id: 1,
        source: {
          name: "bar.qs",
          path: "vscode-test-web://mount/src/bar.qs",
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 3,
        column: 9,
        name: "Bar ",
        endLine: 3,
        endColumn: 26,
      },
      {
        id: 0,
        source: {
          name: "foo.qs",
          path: "vscode-test-web://mount/src/foo.qs",
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 5,
        column: 9,
        name: "Foo ",
        endLine: 5,
        endColumn: 14,
      },
      { id: 0, line: 0, column: 0, name: "entry", source: undefined },
    ]);

    // text editor should now be open on bar.qs
    await waitForTextEditorOn(barUri);
  });

  test("Step into standard lib", async () => {
    // Set a breakpoint on line 8 of foo.qs (7 when 0-indexed)
    // This will be a call into stdlib
    await vscode.debug.addBreakpoints([
      new vscode.SourceBreakpoint(
        new vscode.Location(fooUri, new vscode.Position(7, 0)),
      ),
    ]);

    // launch debugger
    await vscode.debug.startDebugging(workspaceFolder, {
      name: "Launch foo.qs",
      type: "qsharp",
      request: "launch",
      program: "${workspaceFolder}src/foo.qs",
      stopOnEntry: false,
    });

    // should hit the breakpoint we set above
    await waitUntilPausedAndAssertStackTrace([
      {
        id: 0,
        source: {
          name: "foo.qs",
          path: "vscode-test-web://mount/src/foo.qs",
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 8,
        column: 9,
        name: "Foo ",
        endLine: 8,
        endColumn: 14,
      },
      { id: 0, line: 0, column: 0, name: "entry", source: undefined },
    ]);

    // step into call (will be a call into intrinsic.qs)
    await vscode.commands.executeCommand("workbench.action.debug.stepInto");

    await waitUntilPausedAndAssertStackTrace([
      {
        id: 1,
        source: {
          name: "Intrinsic.qs",
          path: "qsharp-library-source:Std/Intrinsic.qs",
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 206,
        column: 9,
        name: "H ",
        endLine: 206,
        endColumn: 40,
      },
      {
        id: 0,
        source: {
          name: "foo.qs",
          path: "vscode-test-web://mount/src/foo.qs",
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 8,
        column: 9,
        name: "Foo ",
        endLine: 8,
        endColumn: 13,
      },
      { id: 0, line: 0, column: 0, name: "entry", source: undefined },
    ]);

    // text editor should now be open on intrinsic.qs
    await waitForTextEditorOn(
      vscode.Uri.parse("qsharp-library-source:Std/Intrinsic.qs"),
    );
  });

  test("Show local variables of selected frame", async () => {
    // Set a breakpoint on line 16 of foo.qs (15 when 0-indexed)
    // This will be in the function `AnotherCallFrame` after `b`
    // has been defined.
    vscode.debug.addBreakpoints([
      new vscode.SourceBreakpoint(
        new vscode.Location(fooUri, new vscode.Position(15, 0)),
      ),
    ]);

    // Start a debug session.
    await vscode.debug.startDebugging(workspaceFolder, {
      name: "Launch foo.qs",
      type: "qsharp",
      request: "launch",
      program: "${workspaceFolder}src/foo.qs",
      stopOnEntry: false,
    });

    // Should hit the breakpoint set above.
    await waitUntilPausedAndAssertVariables([
      { name: "b", type: "Int", value: "3", variablesReference: 0 },
    ]);

    // Step over to prepare the tracker to detect a new variable.
    await vscode.commands.executeCommand("workbench.action.debug.stepOver");

    // Wait until paused. A new variable should be in the locals.
    await waitUntilPausedAndAssertVariables([
      { name: "b", type: "Int", value: "3", variablesReference: 0 },
      { name: "c", type: "Int", value: "4", variablesReference: 0 },
    ]);

    // Request scopes for the frame with frameId 0 (Foo's frame).
    await vscode.debug.activeDebugSession?.customRequest("scopes", {
      frameId: 0,
    });
    const scopes = (await waitUntilResponse(
      "scopes",
    )) as DebugProtocol.ScopesResponse;

    // Request variables for the Locals scope.
    const variablesReference = scopes.body.scopes.find(
      (scope) => scope.name === "Locals",
    )?.variablesReference;

    assert.isNotNull(
      variablesReference,
      "Expected to find a variables reference for the Locals scope",
    );

    await vscode.debug.activeDebugSession?.customRequest("variables", {
      variablesReference,
    });

    const variables = (await waitUntilResponse(
      "variables",
    )) as DebugProtocol.VariablesResponse;

    assert.deepEqual(variables.body.variables, [
      {
        name: "q",
        type: "Qubit",
        value: "Qubit0",
        variablesReference: 0,
      },
      {
        name: "a",
        type: "Int",
        value: "2",
        variablesReference: 0,
      },
    ]);
  });

  /**
   * Wait until the debugger has entered the paused state and then asserts
   * that the stack traces matches the `expectedStackTrace`.
   *
   * @param expectedStackTrace assert that the stack trace matches this value.
   */
  function waitUntilPausedAndAssertStackTrace(
    expectedStackTrace: DebugProtocol.StackFrame[],
  ) {
    return tracker!.waitUntilPaused({ expectedStackTrace });
  }

  /**
   * Wait until the debugger has entered the paused state and then asserts
   * that the local variables match the `expectedVariables`.
   *
   * @param expectedVariables assert that the tracker.variables trace matches this value.
   */
  function waitUntilPausedAndAssertVariables(
    expectedVariables: DebugProtocol.Variable[],
  ) {
    return tracker!.waitUntilPaused({ expectedVariables });
  }

  /**
   * Wait until the debugger has returned a response for a
   * specific command.
   */
  function waitUntilResponse(
    command: string,
  ): Promise<DebugProtocol.Response | undefined> {
    return tracker!.waitUntilResponse(command);
  }
});

/**
 * Terminate the active debug session and wait for it to end.
 */
async function terminateSession() {
  vscode.commands.executeCommand("workbench.action.debug.stop");
  await waitForCondition(
    () => !vscode.debug.activeDebugSession,
    vscode.debug.onDidChangeActiveDebugSession,
    2000,
    "timed out waiting for the debugger to be terminated",
  );
}

/**
 * Wait for the active text editor to be open to the given document URI.
 */
async function waitForTextEditorOn(uri: vscode.Uri) {
  await waitForCondition(
    () =>
      vscode.window.activeTextEditor?.document.uri.toString() ===
      uri.toString(),
    vscode.window.onDidChangeActiveTextEditor,
    500,
    `timed out waiting for the text editor to open to ${uri}.\nactive text editor is ${vscode.window.activeTextEditor?.document.uri}`,
  );
}
