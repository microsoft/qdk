// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { assert } from "chai";
import { activateExtension, waitForCondition } from "../extensionUtils";
import { DebugProtocol } from "@vscode/debugprotocol";
import { qsharpExtensionId } from "../../../src/common";
import { Tracker } from "./tracker";

suite("OpenQASM Debugger Tests", function suite() {
  const workspaceFolder =
    vscode.workspace.workspaceFolders && vscode.workspace.workspaceFolders[0];
  assert(workspaceFolder, "Expecting an open folder");

  const selfContainedName = "self-contained.qasm";
  const multifileName = "multifile.qasm";
  const multifileIncludeName = "imports.inc";

  const selfContainedUri = vscode.Uri.joinPath(
    workspaceFolder.uri,
    selfContainedName,
  );
  const multifileIncludeUri = vscode.Uri.joinPath(
    workspaceFolder.uri,
    multifileIncludeName,
  );
  let tracker: Tracker | undefined;
  let disposable;

  this.beforeAll(async () => {
    await activateExtension();
  });

  this.beforeEach(async () => {
    tracker = new Tracker("openqasm");
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
    await vscode.window.showTextDocument(selfContainedUri);

    // launch debugger
    await vscode.commands.executeCommand(`${qsharpExtensionId}.debugProgram`);

    await waitUntilPausedAndAssertStackTrace([
      {
        id: 0,
        source: {
          name: selfContainedName,
          path: `vscode-test-web://mount/${selfContainedName}`,
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 5,
        column: 1,
        name: "program ",
        endLine: 5,
        endColumn: 9,
      },
      { id: 0, line: 0, column: 0, name: "entry", source: undefined },
    ]);
  });

  test("Launch with launch.json configuration - workspaceFolder substitution", async () => {
    // The DebugConfiguration object is what would go in launch.json,
    // pass it in directly here
    await vscode.debug.startDebugging(workspaceFolder, {
      name: `Launch ${selfContainedName}`,
      type: "qsharp",
      request: "launch",
      program: "${workspaceFolder}" + `${selfContainedName}`,
    });

    await waitUntilPausedAndAssertStackTrace([
      {
        id: 0,
        source: {
          name: selfContainedName,
          path: `vscode-test-web://mount/${selfContainedName}`,
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 5,
        column: 1,
        name: "program ",
        endLine: 5,
        endColumn: 9,
      },
      { id: 0, line: 0, column: 0, name: "entry", source: undefined },
    ]);
  });

  test("Launch with launch.json configuration - file substitution", async () => {
    await vscode.window.showTextDocument(selfContainedUri);

    // ${file} will expand to the filesystem path of the currently opened file
    await vscode.debug.startDebugging(workspaceFolder, {
      name: `Launch ${selfContainedName}`,
      type: "qsharp",
      request: "launch",
      program: "${file}",
    });

    await waitUntilPausedAndAssertStackTrace([
      {
        id: 0,
        source: {
          name: selfContainedName,
          path: `vscode-test-web://mount/${selfContainedName}`,
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 5,
        column: 1,
        name: "program ",
        endLine: 5,
        endColumn: 9,
      },
      { id: 0, line: 0, column: 0, name: "entry", source: undefined },
    ]);
  });

  test("Run until completion", async () => {
    // launch debugger
    await vscode.debug.startDebugging(workspaceFolder, {
      name: `Launch ${selfContainedName}`,
      type: "qsharp",
      request: "launch",
      program: "${workspaceFolder}" + `${selfContainedName}`,
      stopOnEntry: true,
    });

    // should hit the breakpoint we set above
    await waitUntilPausedAndAssertStackTrace([
      {
        id: 0,
        source: {
          name: selfContainedName,
          path: `vscode-test-web://mount/${selfContainedName}`,
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 5,
        column: 1,
        name: "program ",
        endLine: 5,
        endColumn: 9,
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

  test("Set breakpoint in single file program", async () => {
    // Set a breakpoint on line 6 of foo.qs (5 when 0-indexed)
    await vscode.debug.addBreakpoints([
      new vscode.SourceBreakpoint(
        new vscode.Location(selfContainedUri, new vscode.Position(5, 0)),
      ),
    ]);

    // launch debugger
    await vscode.debug.startDebugging(workspaceFolder, {
      name: `Launch ${selfContainedName}`,
      type: "qsharp",
      request: "launch",
      program: "${workspaceFolder}" + `${selfContainedName}`,
      stopOnEntry: false,
    });

    // should hit the breakpoint we set above
    await waitUntilPausedAndAssertStackTrace([
      {
        id: 0,
        source: {
          name: selfContainedName,
          path: `vscode-test-web://mount/${selfContainedName}`,
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 6,
        column: 7,
        name: "program ",
        endLine: 6,
        endColumn: 8,
      },
      { id: 0, line: 0, column: 0, name: "entry", source: undefined },
    ]);
  });

  test("Set breakpoint in multi file program", async () => {
    // Set a breakpoint on line 3 of imports.inc (2 when 0-indexed)
    await vscode.debug.addBreakpoints([
      new vscode.SourceBreakpoint(
        new vscode.Location(multifileIncludeUri, new vscode.Position(2, 0)),
      ),
    ]);

    // launch debugger
    await vscode.debug.startDebugging(workspaceFolder, {
      name: `Launch ${multifileName}`,
      type: "qsharp",
      request: "launch",
      program: "${workspaceFolder}" + `${multifileName}`,
      stopOnEntry: false,
    });

    // should hit the breakpoint we set above
    await waitUntilPausedAndAssertStackTrace([
      {
        id: 1,
        source: {
          name: multifileIncludeName,
          path: `vscode-test-web://mount/${multifileIncludeName}`,
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 3,
        column: 5,
        name: "Bar ",
        endLine: 3,
        endColumn: 16,
      },
      {
        id: 0,
        source: {
          name: multifileName,
          path: `vscode-test-web://mount/${multifileName}`,
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 5,
        column: 1,
        name: "program ",
        endLine: 5,
        endColumn: 6,
      },
      { id: 0, line: 0, column: 0, name: "entry", source: undefined },
    ]);
  });

  test("Step into standard lib", async () => {
    // Set a breakpoint on line 8 of foo.qs (7 when 0-indexed)
    // This will be a call into stdlib
    await vscode.debug.addBreakpoints([
      new vscode.SourceBreakpoint(
        new vscode.Location(selfContainedUri, new vscode.Position(7, 0)),
      ),
    ]);

    // launch debugger
    await vscode.debug.startDebugging(workspaceFolder, {
      name: `Launch ${selfContainedName}`,
      type: "qsharp",
      request: "launch",
      program: "${workspaceFolder}" + `${selfContainedName}`,
      stopOnEntry: false,
    });

    // should hit the breakpoint we set above
    await waitUntilPausedAndAssertStackTrace([
      {
        id: 0,
        source: {
          name: selfContainedName,
          path: `vscode-test-web://mount/${selfContainedName}`,
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 8,
        column: 1,
        name: "program ",
        endLine: 8,
        endColumn: 5,
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
          path: "qsharp-library-source:Std/OpenQASM/Intrinsic.qs",
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 130,
        column: 5,
        name: "h ",
        endLine: 130,
        endColumn: 28,
      },
      {
        id: 0,
        source: {
          name: selfContainedName,
          path: `vscode-test-web://mount/${selfContainedName}`,
          sourceReference: 0,
          adapterData: "qsharp-adapter-data",
        },
        line: 8,
        column: 1,
        name: "program ",
        endLine: 8,
        endColumn: 5,
      },
      { id: 0, line: 0, column: 0, name: "entry", source: undefined },
    ]);

    // text editor should now be open on intrinsic.qs
    await waitForTextEditorOn(
      vscode.Uri.parse("qsharp-library-source:Std/OpenQASM/Intrinsic.qs"),
    );
  });

  test("Show local variables of selected frame", async () => {
    // Set a breakpoint on line 13 of self-contained.qasm (12 when 0-indexed)
    // This will be in the function `f` after `b` has been defined.
    vscode.debug.addBreakpoints([
      new vscode.SourceBreakpoint(
        new vscode.Location(selfContainedUri, new vscode.Position(12, 0)),
      ),
    ]);

    // Start a debug session.
    await vscode.debug.startDebugging(workspaceFolder, {
      name: `Launch ${selfContainedName}`,
      type: "qsharp",
      request: "launch",
      program: "${workspaceFolder}" + `${selfContainedName}`,
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

    // Request scopes for the frame with frameId 1 (g's frame).
    await vscode.debug.activeDebugSession?.customRequest("scopes", {
      frameId: 1,
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
