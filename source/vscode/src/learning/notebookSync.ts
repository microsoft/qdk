// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import {
  LEARNING_COURSES_SUBDIR,
  LEARNING_NOTEBOOK_ACTIVE_CONTEXT,
  LEARNING_WORKSPACE_FOLDER,
  WORKBOOK_SUFFIX,
} from "./constants.js";
import type { LearningService } from "./service.js";

/**
 * Keep the learning service and {@link LEARNING_NOTEBOOK_ACTIVE_CONTEXT} in
 * sync with the active notebook editor.
 *
 * VS Code activates this extension for any Jupyter notebook, so a session
 * can be restored with a course workbook in the editor and the learning
 * views never shown. Nothing else would initialize the service in that
 * case, leaving the notebook without its toolbar actions, hint buttons, or
 * exercise completion tracking.
 */
export function registerNotebookSync(
  context: vscode.ExtensionContext,
  service: LearningService,
): void {
  const sync = (editor: vscode.NotebookEditor | undefined) =>
    void syncActiveNotebook(service, editor);

  context.subscriptions.push(
    vscode.window.onDidChangeActiveNotebookEditor(sync),
  );
  sync(vscode.window.activeNotebookEditor);
}

async function syncActiveNotebook(
  service: LearningService,
  editor: vscode.NotebookEditor | undefined,
): Promise<void> {
  if (editor && isCandidateWorkbookUri(editor.notebook.uri)) {
    // Detect-only — never `createIfMissing`. A `*.workbook.ipynb` is
    // generated during initialization, so its presence normally implies a
    // learning workspace already exists. When it doesn't, the learner
    // hasn't started yet and merely opening a notebook must not materialize
    // one behind their back.
    if (await service.tryInitialize()) {
      await service.syncToWorkbook(editor.notebook.uri);
    }
  }

  // The awaits above can outlive the editor that triggered them.
  if (vscode.window.activeNotebookEditor !== editor) {
    return;
  }

  void vscode.commands.executeCommand(
    "setContext",
    LEARNING_NOTEBOOK_ACTIVE_CONTEXT,
    editor !== undefined && isCourseWorkbook(service, editor.notebook.uri),
  );
}

/**
 * True when a URI *looks like* a course workbook, judged purely from its
 * path: `<workspace folder>/qdk-learning/courses/**\/*.workbook.ipynb`.
 *
 * Does no I/O and doesn't consult the service, so it is safe to call before
 * the learning workspace has been loaded. Notebook courses are only ever
 * discovered under that folder pair, and only python-notebook courses
 * produce `*.workbook.ipynb` files, so a match can never be a Q# artifact.
 */
function isCandidateWorkbookUri(uri: vscode.Uri): boolean {
  if (!uri.path.endsWith(WORKBOOK_SUFFIX)) {
    return false;
  }
  const target = uri.toString();
  for (const folder of vscode.workspace.workspaceFolders ?? []) {
    const coursesRoot = vscode.Uri.joinPath(
      folder.uri,
      LEARNING_WORKSPACE_FOLDER,
      LEARNING_COURSES_SUBDIR,
    ).toString();
    if (target.startsWith(`${coursesRoot}/`)) {
      return true;
    }
  }
  return false;
}

/**
 * True when the URI is a course workbook belonging to the loaded learning
 * workspace. Scopes notebook toolbar actions to learning content rather
 * than every Jupyter notebook the user has open.
 */
function isCourseWorkbook(service: LearningService, uri: vscode.Uri): boolean {
  if (!service.initialized) {
    return false;
  }
  const target = uri.toString();
  return (
    target.startsWith(service.learningContentRoot.toString()) &&
    target.endsWith(WORKBOOK_SUFFIX)
  );
}
