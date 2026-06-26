// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import type { LearningService } from "./service.js";

/**
 * Pattern that identifies exercise/verification cells in python-notebook
 * courses. These cells import check functions from the per-unit `_unit`
 * module (e.g. `from _unit import check_value`).
 */
const exerciseCellPattern = /from\s+_unit\s+import\s+check/;

/**
 * Registers a {@link vscode.NotebookCellStatusBarItemProvider} that adds a
 * "Ask for a Hint" button to exercise code cells in python-notebook courses.
 */
export function createNotebookCellStatusBarProvider(
  service: LearningService,
): vscode.NotebookCellStatusBarItemProvider {
  return {
    provideCellStatusBarItems(
      cell: vscode.NotebookCell,
    ): vscode.NotebookCellStatusBarItem[] {
      if (!service.initialized) {
        return [];
      }

      const courseInfo = service.getActiveCourseInfo();
      if (courseInfo.kind !== "python-notebook") {
        return [];
      }

      // Only annotate code cells whose text contains a check import.
      if (cell.kind !== vscode.NotebookCellKind.Code) {
        return [];
      }

      const text = cell.document.getText();
      if (!exerciseCellPattern.test(text)) {
        return [];
      }

      // Use 1-based cell index as a definitive reference.
      const cellNumber = cell.index + 1;

      const item = new vscode.NotebookCellStatusBarItem(
        "$(comment-discussion-sparkle) Ask for a Hint",
        vscode.NotebookCellStatusBarAlignment.Right,
      );
      item.command = {
        title: "Ask for a Hint",
        command: "qsharp-vscode.learningNotebookHint",
        arguments: [cellNumber],
      };
      item.tooltip = "Open Copilot Chat for a hint on this exercise";
      return [item];
    },
  };
}
