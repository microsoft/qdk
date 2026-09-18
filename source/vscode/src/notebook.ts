// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";
import { WorkspaceTreeProvider } from "./azure/treeView.js";
import { getPythonCodeForWorkspace } from "./azure/workspaceActions.js";
import {
  qsharpExtensionId,
  openqasmLanguageId,
  qsharpLanguageId,
} from "./common.js";
import { notebookTemplate } from "./notebookTemplate.js";

const qsharpCellMagic = "%%qsharp";
const openqasmCellMagic = "%%openqasm";
export const jupyterNotebookType = "jupyter-notebook";
let defaultLanguageId: string | undefined;

/**
 * Sets up handlers to detect Q# code cells in Jupyter notebooks and set the language to Q#.
 */
export function registerQSharpNotebookHandlers() {
  vscode.workspace.notebookDocuments.forEach((notebookDocument) => {
    if (notebookDocument.notebookType === jupyterNotebookType) {
      updateQdkCellLanguages(notebookDocument.getCells());
    }
  });

  const subscriptions = [];
  subscriptions.push(
    vscode.workspace.onDidOpenNotebookDocument((notebookDocument) => {
      if (notebookDocument.notebookType === jupyterNotebookType) {
        updateQdkCellLanguages(notebookDocument.getCells());
      }
    }),
  );

  subscriptions.push(
    vscode.workspace.onDidChangeNotebookDocument((event) => {
      if (event.notebook.notebookType === jupyterNotebookType) {
        // change.document will be undefined if the cell contents did not change -- filter those out.
        const changedCells = event.cellChanges
          .filter((change) => change.document)
          .map((change) => change.cell);
        const addedCells = event.contentChanges
          .map((change) => change.addedCells)
          .flat();

        updateQdkCellLanguages(changedCells.concat(addedCells));
      }
    }),
  );

  function updateQdkCellLanguages(cells: vscode.NotebookCell[]) {
    for (const cell of cells) {
      if (cell.kind === vscode.NotebookCellKind.Code) {
        const document = cell.document;
        const currentLanguageId = document.languageId;
        // If this is a code cell that starts with %%qsharp, and language wasn't already set to Q#, set it.
        if (findQSharpCellMagic(document)) {
          if (currentLanguageId !== qsharpLanguageId) {
            // Remember the "default" language of the notebook (this will normally be Python)
            defaultLanguageId = currentLanguageId;
            vscode.languages.setTextDocumentLanguage(
              cell.document,
              qsharpLanguageId,
            );
            log.trace(
              `setting cell ${cell.index} language to ${qsharpLanguageId}`,
            );
          }
        } else {
          // This is not a %%qsharp/%%openqasm cell. If the language was set to Q#/OpenQASM,
          // change it back to the default language.
          //
          // If the cell language was not set to a supported language, it's out of our purview and we don't
          // want to automatically change the language settings. For example, this could
          // be a %%bash cell magic and the user may have intentionally set the language
          // to "shell".
          if (
            (currentLanguageId === qsharpLanguageId ||
              currentLanguageId === openqasmLanguageId) &&
            defaultLanguageId
          ) {
            vscode.languages.setTextDocumentLanguage(
              cell.document,
              defaultLanguageId,
            );
            log.trace(
              `setting cell ${cell.index} language to ${defaultLanguageId}`,
            );
          }
        }
      }
    }
  }

  return subscriptions;
}

/**
 * Returns the range of the `%%qsharp` cell magic, or `undefined`
 * if it does not exist.
 */
export function findQSharpCellMagic(document: vscode.TextDocument) {
  return findCellMagic(document, qsharpCellMagic);
}

/**
 * Returns the range of the `%%openqasm` cell magic, or `undefined`
 * if it does not exist.
 */
export function findOpenQasmCellMagic(document: vscode.TextDocument) {
  return findCellMagic(document, openqasmCellMagic);
}

function findCellMagic(document: vscode.TextDocument, magic: string) {
  // Ignore whitespace before the cell magic
  for (let i = 0; i < document.lineCount; i++) {
    const line = document.lineAt(i);
    if (line.isEmptyOrWhitespace) {
      continue;
    }
    return line.text.startsWith(magic, line.firstNonWhitespaceCharacterIndex)
      ? new vscode.Range(
          new vscode.Position(i, line.firstNonWhitespaceCharacterIndex),
          new vscode.Position(
            i,
            line.firstNonWhitespaceCharacterIndex + magic.length,
          ),
        )
      : undefined;
  }
  return undefined;
}

// Yes, this function is long, but mostly to deal with multi-folder VS Code workspace or multi
// Azure Quantum workspace connection scenarios. The actual notebook creation is pretty simple.
export function registerCreateNotebookCommand(
  context: vscode.ExtensionContext,
) {
  context.subscriptions.push(
    vscode.commands.registerCommand(
      `${qsharpExtensionId}.createNotebook`,
      async () => {
        // Update the workspace connection info in the notebook if workspaces are already connected to
        const tree = WorkspaceTreeProvider.instance;
        let choice: string | undefined = undefined;
        if (tree) {
          const workspaces = tree.getWorkspaceIds();
          // Default to the first (and maybe only) workspace, else prompt the user to select one
          choice = workspaces[0] || undefined;
          if (workspaces.length > 1) {
            choice = (
              await vscode.window.showQuickPick(
                workspaces.map((workspace) => ({
                  label: tree.getWorkspace(workspace)?.name || workspace,
                  id: workspace,
                })),
                {
                  title: "Select a workspace to use in the notebook",
                },
              )
            )?.id;
          }
        }

        function getCodeForWorkspace(choice: string | undefined) {
          if (choice) {
            const workspace =
              WorkspaceTreeProvider.instance?.getWorkspace(choice);
            if (workspace) {
              return getPythonCodeForWorkspace(
                workspace.id,
                workspace.endpointUri,
                workspace.name,
              );
            }
          }
          // Else use dummy values
          return getPythonCodeForWorkspace("", "", "");
        }

        // Simplest way to replace the connection is just to stringify and then convert back
        let content = JSON.stringify(notebookTemplate);
        content = content.replace(
          `"# WORKSPACE_CONNECTION_CODE"`,
          JSON.stringify(
            "# Connect to the Azure Quantum workspace\n\n" +
              getCodeForWorkspace(choice),
          ),
        );

        const document = await vscode.workspace.openNotebookDocument(
          "jupyter-notebook",
          JSON.parse(content),
        );
        await vscode.window.showNotebookDocument(document);
      },
    ),
  );
}
