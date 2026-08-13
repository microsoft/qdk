// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { isNotebookCourse } from "./courseLayout.js";
import type { LearningService } from "./service.js";

/**
 * Registers a {@link vscode.NotebookCellStatusBarItemProvider} that adds a
 * "Ask for a Hint" button to exercise code cells in python-notebook courses.
 */
export function createNotebookCellStatusBarProvider(
  service: LearningService,
): LearningCellStatusBarProvider {
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

    if (!service.initialized) {
      return [];
    }

    const courseInfo = service.getActiveCourseInfo();
    if (!isNotebookCourse(courseInfo)) {
      return [];
    }

    // Only annotate code cells that are exercises.
    if (cell.kind !== vscode.NotebookCellKind.Code) {
      return [];
    }

    // Use the cell's stable ID from notebook metadata.
    const cellId = cell.metadata?.id;
    if (typeof cellId !== "string") {
      return [];
    }

    // Only show the hint button for cells that are exercises.
    if (!service.isExerciseCellId(cellId)) {
      return [];
    }

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
