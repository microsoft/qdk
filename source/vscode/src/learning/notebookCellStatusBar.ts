// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";
import type { LearningService } from "./service.js";

/**
 * Registers a {@link vscode.NotebookCellStatusBarItemProvider} that adds a
 * "Ask for a Hint" button to exercise code cells in python-notebook courses.
 */
export function createNotebookCellStatusBarProvider(
  service: LearningService,
): LearningCellStatusBarProvider {
  // TODO (acasey): clean up logging
  log.debug("createNotebookCellStatusBarProvider");
  return new LearningCellStatusBarProvider(service);
}

class LearningCellStatusBarProvider
  implements vscode.NotebookCellStatusBarItemProvider, vscode.Disposable
{
  private readonly _onDidChangeCellStatusBarItems =
    new vscode.EventEmitter<void>();
  readonly onDidChangeCellStatusBarItems =
    this._onDidChangeCellStatusBarItems.event;

  private readonly subscription: vscode.Disposable;

  constructor(private readonly service: LearningService) {
    // VS Code caches the items it gets from a provider. A workbook is
    // usually opened before the service has any state to answer with, so
    // without this the buttons would never appear.
    this.subscription = service.onDidChangeState(() =>
      this._onDidChangeCellStatusBarItems.fire(),
    );
  }

  dispose(): void {
    this.subscription.dispose();
    this._onDidChangeCellStatusBarItems.dispose();
  }

  provideCellStatusBarItems(
    cell: vscode.NotebookCell,
  ): vscode.NotebookCellStatusBarItem[] {
    const service = this.service;
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

    // Only annotate code cells that are exercises.
    if (cell.kind !== vscode.NotebookCellKind.Code) {
      log.debug("Skipping status bar: cell %d is not a code cell", cell.index);
      return [];
    }

    // Use the cell's stable ID from notebook metadata.
    const cellId = cell.metadata?.id;
    if (typeof cellId !== "string") {
      log.debug("Skipping status bar: cell %d has no metadata.id", cell.index);
      return [];
    }

    // Only show the hint button for cells that are exercises.
    const exerciseCellIds = service.getExerciseCellIds();
    if (!exerciseCellIds.has(cellId)) {
      log.debug("Skipping status bar: cell %s is not an exercise", cellId);
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
  }
}
