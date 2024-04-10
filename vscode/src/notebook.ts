// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { ILanguageService, log } from "qsharp-lang";
import * as vscode from "vscode";
import { isQsharpNotebookCell, qsharpLanguageId } from "./common.js";
import { WorkspaceTreeProvider } from "./azure/treeView.js";
import { getPythonCodeForWorkspace } from "./azure/workspaceActions.js";
import { notebookTemplate } from "./notebookTemplate.js";

const qsharpCellMagic = "%%qsharp";
const jupyterNotebookType = "jupyter-notebook";
const qsharpConfigMimeType = "application/x.qsharp-config";

/**
 * Sets up handlers to detect Q# code cells in Jupyter notebooks and set the language to Q#.
 */
export function registerQSharpNotebookHandlers() {
  vscode.workspace.notebookDocuments.forEach((notebookDocument) => {
    if (notebookDocument.notebookType === jupyterNotebookType) {
      updateQSharpCellLanguages(notebookDocument.getCells());
    }
  });

  vscode.workspace.notebookDocuments.forEach((notebookDocument) => {
    if (notebookDocument.notebookType === jupyterNotebookType) {
      updateQSharpCellLanguages(notebookDocument.getCells());
    }
  });

  const subscriptions = [];
  subscriptions.push(
    vscode.workspace.onDidOpenNotebookDocument((notebookDocument) => {
      if (notebookDocument.notebookType === jupyterNotebookType) {
        updateQSharpCellLanguages(notebookDocument.getCells());
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

        updateQSharpCellLanguages(changedCells.concat(addedCells));
      }
    }),
  );

  let defaultLanguageId: string | undefined;

  function updateQSharpCellLanguages(cells: vscode.NotebookCell[]) {
    for (const cell of cells) {
      // If this is a code cell that starts with %%qsharp, and language wasn't already set to Q#, set it.
      if (cell.kind === vscode.NotebookCellKind.Code) {
        const document = cell.document;
        const currentLanguageId = document.languageId;
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
          // This is not a %%qsharp cell. If the language was set to Q#,
          // change it back to the default language.
          //
          // If the cell language was not set to Q#, it's out of our purview and we don't
          // want to automatically change the language settings. For example, this could
          // be a %%bash cell magic and the user may have intentionally set the language
          // to "shell".
          if (currentLanguageId === qsharpLanguageId && defaultLanguageId) {
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

const openQSharpNotebooks = new Set<string>();

/**
 * Returns the range of the `%%qsharp` cell magic, or `undefined`
 * if it does not exist.
 */
function findQSharpCellMagic(document: vscode.TextDocument) {
  // Ignore whitespace before the cell magic
  for (let i = 0; i < document.lineCount; i++) {
    const line = document.lineAt(i);
    if (line.isEmptyOrWhitespace) {
      continue;
    }
    return line.text.startsWith(
      qsharpCellMagic,
      line.firstNonWhitespaceCharacterIndex,
    )
      ? new vscode.Range(
          new vscode.Position(i, line.firstNonWhitespaceCharacterIndex),
          new vscode.Position(
            i,
            line.firstNonWhitespaceCharacterIndex + qsharpCellMagic.length,
          ),
        )
      : undefined;
  }
  return undefined;
}

/**
 * This one is for syncing with the language service
 */
export function registerQSharpNotebookCellUpdateHandlers(
  languageService: ILanguageService,
) {
  vscode.workspace.notebookDocuments.forEach((notebook) => {
    updateIfQsharpNotebook(notebook);
  });

  const subscriptions = [];
  subscriptions.push(
    vscode.workspace.onDidOpenNotebookDocument((notebook) => {
      updateIfQsharpNotebook(notebook);
    }),
  );

  subscriptions.push(
    vscode.workspace.onDidChangeNotebookDocument((event) => {
      updateIfQsharpNotebook(event.notebook);
    }),
  );

  subscriptions.push(
    vscode.workspace.onDidCloseNotebookDocument((notebook) => {
      closeIfKnownQsharpNotebook(notebook);
    }),
  );

  function updateIfQsharpNotebook(notebook: vscode.NotebookDocument) {
    if (notebook.notebookType === jupyterNotebookType) {
      const qsharpMetadata = getQSharpConfigMetadata(notebook);
      const qsharpCells = getQSharpCells(notebook);
      const notebookUri = notebook.uri.toString();
      if (qsharpCells.length > 0) {
        openQSharpNotebooks.add(notebookUri);
        languageService.updateNotebookDocument(
          notebookUri,
          notebook.version,
          qsharpMetadata,
          qsharpCells.map((cell) => {
            return {
              uri: cell.document.uri.toString(),
              version: cell.document.version,
              code: getQSharpText(cell.document),
            };
          }),
        );
      } else {
        // All Q# cells could have been deleted, check if we know this doc from previous calls
        closeIfKnownQsharpNotebook(notebook);
      }
    }
  }

  function closeIfKnownQsharpNotebook(notebook: vscode.NotebookDocument) {
    const notebookUri = notebook.uri.toString();
    if (openQSharpNotebooks.has(notebookUri)) {
      languageService.closeNotebookDocument(notebookUri);
      openQSharpNotebooks.delete(notebook.uri.toString());
    }
  }

  function getQSharpCells(notebook: vscode.NotebookDocument) {
    return notebook
      .getCells()
      .filter((cell) => isQsharpNotebookCell(cell.document));
  }

  function getQSharpText(document: vscode.TextDocument) {
    const magicRange = findQSharpCellMagic(document);
    if (magicRange) {
      const magicStartOffset = document.offsetAt(magicRange.start);
      const magicEndOffset = document.offsetAt(magicRange.end);
      // Erase the %%qsharp magic line if it's there.
      // Replace it with a comment so that document offsets remain the same.
      // This will save us from having to map offsets later when
      // communicating with the language service.
      const text = document.getText();
      return (
        text.substring(0, magicStartOffset) +
        "//qsharp" +
        text.substring(magicEndOffset)
      );
    } else {
      // No %%qsharp magic. This can happen if the user manually sets the
      // cell language to Q#. Python won't recognize the cell as a Q# cell,
      // so this will fail at runtime, but as the language service we respect
      // the manually set cell language, so we treat this as any other
      // Q# cell. We could consider raising a warning here to help the user.
      log.info(
        "found Q# cell without %%qsharp magic: " + document.uri.toString(),
      );
      return document.getText();
    }
  }

  return subscriptions;
}

/**
 * Finds an output cell that contains an item with the Q# config MIME type,
 * and returns the data from it. This data and is generated by the execution of a
 * `qsharp.init()` call. It's Q# configuration data to be passed
 * to the language service as "notebook metadata".
 */
function getQSharpConfigMetadata(notebook: vscode.NotebookDocument): object {
  const data = notebook
    .getCells()
    .flatMap((cell) => cell.outputs)
    .flatMap((output) => output.items)
    .find((item) => {
      return item.mime === qsharpConfigMimeType;
    })?.data;

  if (data) {
    const dataString = new TextDecoder().decode(data);
    log.trace("found Q# config metadata: " + dataString);
    return JSON.parse(dataString);
  } else {
    return {};
  }
}

// Yes, this function is long, but mostly to deal with multi-folder VS Code workspace or multi
// Azure Quantum workspace connection scenarios. The actual notebook creation is pretty simple.
export function registerCreateNotebookCommand(
  context: vscode.ExtensionContext,
) {
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "qsharp-vscode.createNotebook",
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
