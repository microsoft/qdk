// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import type { LearningService } from "./service.js";
import {
  LEARNING_WORKSPACE_FOLDER,
  LEARNING_CONTENT_FOLDER,
} from "./constants.js";

// Context keys set on the VS Code context for use in `when` clauses.
const CTX_IS_EXERCISE = "qsharp-vscode.activeEditorIsExercise";
const CTX_IS_EXAMPLE = "qsharp-vscode.activeEditorIsExample";
const CTX_EXERCISE_PASSED = "qsharp-vscode.activeExercisePassed";

/**
 * Tracks whether the active text editor is a katas exercise or example file
 * and sets VS Code context keys accordingly. These context keys drive the
 * visibility of editor toolbar buttons, context menu items, and status bar.
 */
export function registerEditorContext(
  context: vscode.ExtensionContext,
  service: LearningService,
): vscode.Disposable {
  const disposables: vscode.Disposable[] = [];

  function update(editor: vscode.TextEditor | undefined): void {
    const fsPath = editor?.document.uri.fsPath ?? "";
    const normalized = fsPath.replace(/\\/g, "/");
    const isExercise = normalized.includes(
      `/${LEARNING_WORKSPACE_FOLDER}/exercises/`,
    );
    const isExample =
      normalized.includes(`/${LEARNING_WORKSPACE_FOLDER}/examples/`) ||
      normalized.includes(`/${LEARNING_CONTENT_FOLDER}/`);

    let exercisePassed = false;
    if (isExercise && service.initialized) {
      try {
        const pos = service.getPosition();
        exercisePassed =
          pos.content.type === "exercise" && pos.content.isComplete;
      } catch {
        // Service not initialized or position mismatch — default to false.
      }
    }

    void vscode.commands.executeCommand(
      "setContext",
      CTX_IS_EXERCISE,
      isExercise,
    );
    void vscode.commands.executeCommand(
      "setContext",
      CTX_IS_EXAMPLE,
      isExample,
    );
    void vscode.commands.executeCommand(
      "setContext",
      CTX_EXERCISE_PASSED,
      exercisePassed,
    );
  }

  // Set initial state.
  update(vscode.window.activeTextEditor);

  disposables.push(
    vscode.window.onDidChangeActiveTextEditor(update),
    // Re-evaluate when learning state changes (e.g. exercise just passed).
    service.onDidChangeState(() => update(vscode.window.activeTextEditor)),
  );

  const disposable = vscode.Disposable.from(...disposables);
  context.subscriptions.push(disposable);
  return disposable;
}
