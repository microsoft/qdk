// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { assert } from "chai";
import {
  activateExtension,
  waitForCondition,
  waitForDiagnosticsToAppear,
  waitForDiagnosticsToBeEmpty,
} from "../extensionUtils";

suite("Q# Notebook Tests", function suite() {
  const workspaceFolder =
    vscode.workspace.workspaceFolders && vscode.workspace.workspaceFolders[0];
  assert(workspaceFolder, "Expecting an open folder");

  const workspaceFolderUri = workspaceFolder.uri;

  this.beforeAll(async () => {
    await activateExtension();
  });

  test("Cell language is set to Q#", async () => {
    const notebook = await vscode.workspace.openNotebookDocument(
      vscode.Uri.joinPath(workspaceFolderUri, "test-no-lang-metadata.ipynb"),
    );

    // Test if the cell with the %%qsharp magic has been detected
    // and its language switched to qsharp. We can only expect this to happen
    // after the notebook has been opened, and the document handlers
    // have been invoked, but of course there is no callback for that.
    // So we just verify the notebook has been updated with the qsharp
    // language id within 50ms (If we start exceeding this timeout for some
    // reason, it's enough of a user-perceptible delay that we're probably
    // better off disabling this behavior, rather than suddenly change the
    // cell language from under the user after a delay).
    await waitForCondition(
      () =>
        !!notebook
          .getCells()
          .find((cell) => cell.document.languageId === "qsharp"),
      vscode.workspace.onDidChangeNotebookDocument,
      50,
      "timed out waiting for a Q# code cell",
    );
  });

  test("Cell language is set back to Python", async () => {
    const notebook = await vscode.workspace.openNotebookDocument(
      vscode.Uri.joinPath(workspaceFolderUri, "test.ipynb"),
    );

    await vscode.window.showNotebookDocument(notebook);

    assert.equal(
      vscode.window.activeNotebookEditor?.notebook.uri.toString(),
      notebook.uri.toString(),
    );

    const oldLength = notebook.getCells().length;

    // Add a new cell at the bottom of the notebook
    await vscode.commands.executeCommand("notebook.focusBottom");
    await vscode.commands.executeCommand("notebook.cell.insertCodeCellBelow");

    // There should be an additional cell in the notebook and it should be Python
    await waitForCondition(
      () => {
        const cellsAfter = notebook.getCells();
        return (
          cellsAfter.length === oldLength + 1 &&
          notebook.getCells()[cellsAfter.length - 1].document.languageId ===
            "python"
        );
      },
      vscode.workspace.onDidChangeNotebookDocument,
      50,
      "timed out waiting for a Python code cell",
    );
  });

  test("Diagnostics", async () => {
    const notebook = await vscode.workspace.openNotebookDocument(
      vscode.Uri.joinPath(workspaceFolderUri, "test.ipynb"),
    );

    const thirdQSharpCellUri = notebook.cellAt(3).document.uri;

    // Verify diagnostics in Q# cell
    const diagnostics = await waitForDiagnosticsToAppear(thirdQSharpCellUri);
    assert.lengthOf(diagnostics, 1);

    assert.include(diagnostics[0].message, "syntax error");
    assert.equal(diagnostics[0].range.start.line, 2);
  });

  test("Definition", async () => {
    const notebook = await vscode.workspace.openNotebookDocument(
      vscode.Uri.joinPath(workspaceFolderUri, "test.ipynb"),
    );

    const firstQSharpCellUri = notebook.cellAt(1).document.uri;
    const secondQSharpCellUri = notebook.cellAt(2).document.uri;

    const pos = new vscode.Position(2, 0); // cursor on the usage of Test()

    // Verify go to definition across cells
    const actualDefinition = (await vscode.commands.executeCommand(
      "vscode.executeDefinitionProvider",
      secondQSharpCellUri,
      pos,
    )) as vscode.Location[];

    const location = actualDefinition[0];
    assert.equal(location.uri.toString(), firstQSharpCellUri.toString());
    assert.equal(location.range.start.line, 2);
    assert.equal(location.range.start.character, 10);
  });

  test("Notebook uses Unrestricted profile by default even when workspace is Base", async function () {
    // Open a notebook with quantum operations that require unrestricted profile
    const notebook = await vscode.workspace.openNotebookDocument(
      vscode.Uri.joinPath(workspaceFolderUri, "test-notebook-profile.ipynb"),
    );

    const qsharpCellUri = notebook.cellAt(1).document.uri;

    // The measurement operation M(q) == One should work in notebooks without errors
    // even when workspace is set to base profile, because notebooks default to unrestricted
    // Use the proper helper function to wait for diagnostics to be empty (no errors)
    await waitForDiagnosticsToBeEmpty(qsharpCellUri);
  });
});
