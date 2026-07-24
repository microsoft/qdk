// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import {
  createLearningCodeLensProvider,
  exerciseDocumentSelector,
} from "./codeLens.js";
import { registerLearningCommands } from "./commands.js";
import {
  LEARNING_NOTEBOOK_ACTIVE_CONTEXT,
  WORKBOOK_SUFFIX,
} from "./constants.js";
import { LessonPanelManager, registerLessonPanelSerializer } from "./panel.js";
import { createNotebookCellStatusBarProvider } from "./notebookCellStatusBar.js";
import { registerLearningProgressView } from "./progressTreeView.js";
import { LearningService } from "./service.js";
import { registerLearningWelcomeView } from "./welcomeView.js";

export function initLearning(
  context: vscode.ExtensionContext,
): LearningService {
  const learningService = new LearningService(context.extensionUri);
  const panelManager = new LessonPanelManager(
    context.extensionUri,
    learningService,
  );
  context.subscriptions.push(
    { dispose: () => learningService.dispose() },
    panelManager,
  );
  context.subscriptions.push(
    vscode.languages.registerCodeLensProvider(
      exerciseDocumentSelector,
      createLearningCodeLensProvider(),
    ),
  );
  context.subscriptions.push(
    vscode.notebooks.registerNotebookCellStatusBarItemProvider(
      "jupyter-notebook",
      createNotebookCellStatusBarProvider(learningService),
    ),
  );
  context.subscriptions.push(
    vscode.workspace.onDidChangeNotebookDocument((e) => {
      // When a cell finishes executing (executionSummary changes), auto-save
      // the notebook, check if it corresponds to an exercise in the active
      // python-notebook course and update focus. If execution succeeded,
      // mark complete.
      if (
        !learningService.initialized ||
        learningService.getActiveCourseInfo().kind !== "python-notebook"
      ) {
        return;
      }
      const hasExecutionChange = e.cellChanges.some(
        (change) => change.executionSummary !== undefined,
      );
      if (hasExecutionChange) {
        // Moving between notebooks is clumsy when they're unsaved.  Since this
        // is a working copy we created on the user's behalf, we're free to
        // auto-save.
        void e.notebook.save();
      }

      for (const change of e.cellChanges) {
        if (change.executionSummary !== undefined) {
          const cellId = change.cell.metadata?.id;
          if (typeof cellId !== "string") {
            continue;
          }
          void learningService.goToExerciseByCellId(cellId, "notebook");
          if (change.executionSummary.success) {
            void learningService.markExerciseCompleteByCellId(cellId);
          }
        }
      }
    }),
  );
  registerLearningProgressView(context, learningService);
  registerLearningWelcomeView(context, learningService);
  registerLearningCommands(context, learningService, panelManager);
  registerLessonPanelSerializer(context, panelManager);
  registerNotebookContextKey(context, learningService);
  return learningService;
}

/**
 * Keep {@link LEARNING_NOTEBOOK_ACTIVE_CONTEXT} in sync with the active
 * notebook editor so notebook toolbar actions only appear on course
 * workbooks, not on every Jupyter notebook the user has open.
 */
function registerNotebookContextKey(
  context: vscode.ExtensionContext,
  service: LearningService,
): void {
  const sync = (editor: vscode.NotebookEditor | undefined) => {
    let isCourseNotebook = false;
    if (editor && service.initialized) {
      const uri = editor.notebook.uri.toString();
      isCourseNotebook =
        uri.startsWith(service.learningContentRoot.toString()) &&
        uri.endsWith(WORKBOOK_SUFFIX);
    }
    void vscode.commands.executeCommand(
      "setContext",
      LEARNING_NOTEBOOK_ACTIVE_CONTEXT,
      isCourseNotebook,
    );
  };

  context.subscriptions.push(
    vscode.window.onDidChangeActiveNotebookEditor(sync),
  );
  sync(vscode.window.activeNotebookEditor);
}

export type {
  CourseDescriptor,
  CourseKind,
  CurrentActivity,
  EnvironmentCheckReport,
  HintContext,
  OverallProgress,
  RunResult,
  SolutionCheckResult,
  UnitSummary,
} from "./types.js";
export { LEARNING_WORKSPACE_FOLDER } from "./constants.js";
export {
  detectLearningWorkspace,
  LearningService,
  resolveNewWorkspaceRoot,
} from "./service.js";
