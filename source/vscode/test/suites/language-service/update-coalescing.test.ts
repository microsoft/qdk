// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { assert } from "chai";
import * as vscode from "vscode";
import {
  activateExtension,
  openDocumentAndWaitForProcessing,
  waitForCondition,
  TEST_TIMEOUT_MS,
} from "../extensionUtils";

/**
 * Verifies that edits arriving while a compilation is in flight are coalesced into far
 * fewer compilations than there were edits.
 *
 * Compilation blocks the extension host, so the edits have to originate on the other
 * side of that boundary to pile up the way real keystrokes do. They are therefore issued
 * without awaiting, letting the editor apply them and deliver the change notifications
 * while the host is busy.
 *
 * Which version was compiled is read from the dev status diagnostic, whose message
 * carries `version=N`. That diagnostic is the only place a document version is
 * observable from outside the language service.
 */
suite("Update coalescing", function suite() {
  const workspaceFolder =
    vscode.workspace.workspaceFolders && vscode.workspace.workspaceFolders[0];
  assert(workspaceFolder, "Expecting an open folder");

  const noErrorsQs = vscode.Uri.joinPath(workspaceFolder.uri, "no-errors.qs");

  // Long enough that every edit lands while a compilation is blocking the host.
  const simulatedCompileDelayMs = 100;
  const editCount = 20;

  this.beforeAll(async () => {
    await activateExtension();
    await vscode.workspace
      .getConfiguration("Q#")
      .update(
        "dev.simulatedCompileDelayMs",
        simulatedCompileDelayMs,
        vscode.ConfigurationTarget.Global,
      );
  });

  this.afterAll(async () => {
    await vscode.workspace
      .getConfiguration("Q#")
      .update(
        "dev.simulatedCompileDelayMs",
        undefined,
        vscode.ConfigurationTarget.Global,
      );
    await vscode.commands.executeCommand(
      "workbench.action.revertAndCloseActiveEditor",
    );
  });

  test("many rapid edits produce far fewer compilations", async () => {
    const doc = await openDocumentAndWaitForProcessing(noErrorsQs);
    const editor = await vscode.window.showTextDocument(doc);

    const compiledVersions: number[] = [];
    const recorder = vscode.languages.onDidChangeDiagnostics((event) => {
      if (!event.uris.some((u) => u.toString() === doc.uri.toString())) {
        return;
      }
      for (const diagnostic of vscode.languages.getDiagnostics(doc.uri)) {
        const match = /version=(\d+)/.exec(diagnostic.message);
        if (match) {
          const version = Number(match[1]);
          if (compiledVersions.at(-1) !== version) {
            compiledVersions.push(version);
          }
        }
      }
    });

    try {
      const insertAt = new vscode.Position(3, 26);
      editor.selection = new vscode.Selection(insertAt, insertAt);

      // Deliberately not awaited individually, so they queue up in the editor rather
      // than being serialized behind each compilation.
      const typed: Thenable<unknown>[] = [];
      for (let i = 0; i < editCount; i++) {
        typed.push(vscode.commands.executeCommand("type", { text: "a" }));
      }
      await Promise.all(typed);

      const finalVersion = doc.version;

      await waitForCondition(
        () => compiledVersions.includes(finalVersion),
        vscode.languages.onDidChangeDiagnostics,
        TEST_TIMEOUT_MS,
        `Final document version ${finalVersion} was never compiled. ` +
          `Compiled versions: ${compiledVersions.join(", ")}`,
      );

      console.log(
        `qsharp-tests: ${editCount} edits produced document version ${finalVersion}; ` +
          `compiled versions: ${compiledVersions.join(", ")}`,
      );

      // The exact count depends on machine speed, so this only asserts that coalescing
      // happened at all. Without it there would be one compilation per edit.
      assert.isBelow(
        compiledVersions.length,
        editCount,
        "expected fewer compilations than edits",
      );
    } finally {
      recorder.dispose();
    }
  });
});
