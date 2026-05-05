// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { LEARNING_WORKSPACE_FOLDER } from "./constants.js";

/**
 * Document selector that matches exercise files inside the well-known
 * learning workspace folder.
 */
export const exerciseDocumentSelector: vscode.DocumentSelector = {
  language: "qsharp",
  pattern: `**/${LEARNING_WORKSPACE_FOLDER}/exercises/**/*.qs`,
};

/**
 * CodeLens provider for learning exercise files. Shows a "Check Solution"
 * action and a link to open the corresponding section in the Quantum Katas panel.
 */
export function createLearningCodeLensProvider(): vscode.CodeLensProvider {
  return {
    provideCodeLenses(): vscode.CodeLens[] {
      // Place all lenses on line 0 — they appear as a row of links above the code.
      const range = new vscode.Range(0, 0, 0, 0);

      return [
        new vscode.CodeLens(range, {
          title: "$(pass) Check Solution",
          command: "qsharp-vscode.learningCheckSolution",
          tooltip: "Check your solution against the expected answer",
        }),
        new vscode.CodeLens(range, {
          title: "$(discard) Reset Exercise",
          command: "qsharp-vscode.learningResetExercise",
          tooltip: "Reset the exercise to its original state",
        }),
        new vscode.CodeLens(range, {
          title: "$(mortar-board) Show in Quantum Katas",
          command: "qsharp-vscode.learningOpenPanel",
          tooltip: "Open the Quantum Katas panel for this exercise",
        }),
      ];
    },
  };
}
