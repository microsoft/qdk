// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";

/** Well-known workspace folder name for katas files. */
const KATAS_WS_FOLDER = "qdk-learning-ws";

/**
 * Document selector that matches exercise files inside the well-known
 * katas workspace folder.
 */
export const exerciseDocumentSelector: vscode.DocumentSelector = {
  language: "qsharp",
  pattern: `**/${KATAS_WS_FOLDER}/exercises/**/*.qs`,
};

/**
 * CodeLens provider for katas exercise files. Shows a "Check Solution"
 * action and a link to open the corresponding section in the Katas panel.
 */
export function createLearningCodeLensProvider(): vscode.CodeLensProvider {
  return {
    provideCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
      // Place all lenses on line 0 — they appear as a row of links above the code.
      const range = new vscode.Range(0, 0, 0, 0);

      return [
        new vscode.CodeLens(range, {
          title: "$(pass) Check Solution",
          command: "qsharp-vscode.learningCheckSolution",
          tooltip: "Check your solution against the expected answer",
        }),
        new vscode.CodeLens(range, {
          title: "$(mortar-board) Open in Katas Panel",
          command: "qsharp-vscode.learningOpenPanel",
          tooltip: "Open the Quantum Katas panel for this exercise",
        }),
      ];
    },
  };
}
