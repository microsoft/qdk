// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import type { LearningService } from "./service.js";
import { LEARNING_WORKSPACE_FOLDER } from "./constants.js";

const FLASH_DURATION_MS = 2000;

/**
 * Provides visual decorations for katas exercise files:
 * - Placeholder highlighting on the `// ...` placeholder region
 * - Green/red flash after a check succeeds/fails
 */
export function registerLearningDecorations(
  context: vscode.ExtensionContext,
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  _service: LearningService,
): vscode.Disposable {
  const disposables: vscode.Disposable[] = [];

  // ── Placeholder decoration ──
  const placeholderDecorationType =
    vscode.window.createTextEditorDecorationType({
      backgroundColor: new vscode.ThemeColor(
        "editor.findMatchHighlightBackground",
      ),
      border: "1px dashed",
      borderColor: new vscode.ThemeColor("editorInfo.foreground"),
      isWholeLine: true,
    });

  // ── Pass/fail flash decoration types ──
  const passFlashType = vscode.window.createTextEditorDecorationType({
    backgroundColor: "rgba(40, 167, 69, 0.15)",
    isWholeLine: true,
    overviewRulerColor: "rgba(40, 167, 69, 0.6)",
  });
  const failFlashType = vscode.window.createTextEditorDecorationType({
    backgroundColor: "rgba(220, 53, 69, 0.15)",
    isWholeLine: true,
    overviewRulerColor: "rgba(220, 53, 69, 0.6)",
  });

  disposables.push(placeholderDecorationType, passFlashType, failFlashType);

  let flashTimeout: ReturnType<typeof setTimeout> | undefined;

  function clearFlash(editor: vscode.TextEditor): void {
    if (flashTimeout) {
      clearTimeout(flashTimeout);
      flashTimeout = undefined;
    }
    editor.setDecorations(passFlashType, []);
    editor.setDecorations(failFlashType, []);
  }

  function applyPlaceholderDecorations(
    editor: vscode.TextEditor | undefined,
  ): void {
    if (!editor) {
      return;
    }

    const fsPath = editor.document.uri.fsPath.replace(/\\/g, "/");
    if (!fsPath.includes(`/${LEARNING_WORKSPACE_FOLDER}/exercises/`)) {
      editor.setDecorations(placeholderDecorationType, []);
      return;
    }

    // Find the placeholder comment line(s). The standard pattern is
    // `// ...` on its own line — the exercise placeholder code from the corpus.
    const ranges: vscode.Range[] = [];
    for (let i = 0; i < editor.document.lineCount; i++) {
      const text = editor.document.lineAt(i).text.trim();
      if (text === "// ...") {
        ranges.push(
          new vscode.Range(i, 0, i, editor.document.lineAt(i).text.length),
        );
      }
    }
    editor.setDecorations(placeholderDecorationType, ranges);
  }

  applyPlaceholderDecorations(vscode.window.activeTextEditor);

  disposables.push(
    vscode.window.onDidChangeActiveTextEditor(applyPlaceholderDecorations),
    vscode.workspace.onDidChangeTextDocument((e) => {
      const editor = vscode.window.activeTextEditor;
      if (editor && e.document === editor.document) {
        applyPlaceholderDecorations(editor);
      }
    }),
  );

  // ── Flash on check results ──
  // Exposed as a command so the check handler can trigger it.
  disposables.push(
    vscode.commands.registerCommand(
      "qsharp-vscode._learningFlash",
      (passed: boolean) => {
        const editor = vscode.window.activeTextEditor;
        if (!editor) {
          return;
        }

        clearFlash(editor);

        const flashType = passed ? passFlashType : failFlashType;
        const fullRange = new vscode.Range(
          0,
          0,
          editor.document.lineCount - 1,
          editor.document.lineAt(editor.document.lineCount - 1).text.length,
        );
        editor.setDecorations(flashType, [fullRange]);

        flashTimeout = setTimeout(() => {
          clearFlash(editor);
        }, FLASH_DURATION_MS);
      },
    ),
  );

  const disposable = vscode.Disposable.from(...disposables);
  context.subscriptions.push(disposable);
  return disposable;
}
