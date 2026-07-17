// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";
import type { LearningService } from "./service.js";

/**
 * Pattern that identifies exercise/verification cells in python-notebook
 * courses. These cells import check functions from the per-unit `_unit`
 * module (e.g. `from _unit import check_value`).
 */
// const exerciseCellPattern = /from\s+_unit\s+import\s+check/;

/**
 * Registers a {@link vscode.NotebookCellStatusBarItemProvider} that adds a
 * "Ask for a Hint" button to exercise code cells in python-notebook courses.
 */
export function createNotebookCellStatusBarProvider(
  service: LearningService,
): vscode.NotebookCellStatusBarItemProvider {
  log.debug("createNotebookCellStatusBarProvider");
  return {
    provideCellStatusBarItems(
      cell: vscode.NotebookCell,
    ): vscode.NotebookCellStatusBarItem[] {
      log.debug("provideCellStatusBarItems called for cell %d", cell.index);

      if (!service.initialized) {
        log.debug("Skipping status bar: service not initialized");
        return [];
      }

      const courseInfo = service.getActiveCourseInfo();
      if (courseInfo.kind !== "python-notebook") {
        log.debug(
          "Skipping status bar: course kind is '%s', not 'python-notebook'",
          courseInfo.kind,
        );
        return [];
      }

      // Only annotate code cells whose text contains a check import.
      if (cell.kind !== vscode.NotebookCellKind.Code) {
        log.debug(
          "Skipping status bar: cell %d is not a code cell",
          cell.index,
        );
        return [];
      }

      // TODO (acasey): populate from _exercises.json
      // const text = cell.document.getText();
      // if (!exerciseCellPattern.test(text)) {
      //   log.debug(
      //     "Skipping status bar: cell %d does not match exercise pattern",
      //     cell.index,
      //   );
      //   return [];
      // }

      // Use the cell's stable ID from notebook metadata.
      const cellId = cell.metadata?.id;
      if (typeof cellId !== "string") {
        log.debug(
          "Skipping status bar: cell %d has no metadata.id",
          cell.index,
        );
        return [];
      }

      log.debug("Adding 'Ask for a Hint' status bar item for cell %s", cellId);

      const item = new vscode.NotebookCellStatusBarItem(
        "$(comment-discussion-sparkle) Ask for a Hint",
        vscode.NotebookCellStatusBarAlignment.Right,
      );
      item.command = {
        title: "Ask for a Hint",
        command: "qsharp-vscode.learningNotebookHint",
        arguments: [cellId],
      };
      item.tooltip = "Open Copilot Chat for a hint on this exercise";
      return [item];
    },
  };
}
