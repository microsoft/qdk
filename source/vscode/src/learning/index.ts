// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import {
  createLearningCodeLensProvider,
  exerciseDocumentSelector,
} from "./codeLens.js";
import { registerLearningCommands } from "./commands.js";
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
          void learningService.goToExerciseByCellId(cellId, "panel");
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
  return learningService;
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
