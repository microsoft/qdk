// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import {
  createLearningCodeLensProvider,
  exerciseDocumentSelector,
} from "./codeLens.js";
import { registerLearningCommands } from "./commands.js";
import { LessonPanelManager, registerLessonPanelSerializer } from "./panel.js";
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
  registerLearningProgressView(context, learningService);
  registerLearningWelcomeView(context, learningService);
  registerLearningCommands(context, learningService, panelManager);
  registerLessonPanelSerializer(context, panelManager);
  return learningService;
}

export type {
  CurrentActivity,
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
