// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import {
  exerciseDocumentSelector,
  createLearningCodeLensProvider,
} from "./codeLens.js";
import { registerLearningCommands } from "./commands.js";
import { registerLearningDecorations } from "./decorations.js";
import { registerEditorContext } from "./editorContext.js";
import { registerKatasPanelSerializer } from "./panel.js";
import { registerLearningProgressView } from "./progressTreeView.js";
import { registerLearningWelcomeView } from "./welcomeView.js";
import { LearningService } from "./service.js";

export {
  LEARNING_WORKSPACE_FOLDER as KATAS_WS_FOLDER,
  LEARNING_WORKSPACE_RELATIVE_PATH as KATAS_WS_FOLDER_REL,
  LEARNING_FILE,
  LEARNING_CONTENT_FOLDER,
  KATAS_COURSE_ID,
} from "./constants.js";

export { LearningService } from "./service.js";
export { detectLearningWorkspace } from "./service.js";
export type { KatasWorkspaceInfo } from "./service.js";
export { registerEditorContext } from "./editorContext.js";
export { registerLearningCommands } from "./commands.js";
export {
  createLearningCodeLensProvider,
  exerciseDocumentSelector,
} from "./codeLens.js";
export { registerLearningDecorations } from "./decorations.js";
export { scanForCourses } from "./courseScanner.js";

export function initLearning(
  context: vscode.ExtensionContext,
): LearningService {
  const learningService = new LearningService(context.extensionUri);
  context.subscriptions.push({ dispose: () => learningService.dispose() });
  registerEditorContext(context, learningService);
  registerLearningDecorations(context, learningService);
  context.subscriptions.push(
    vscode.languages.registerCodeLensProvider(
      exerciseDocumentSelector,
      createLearningCodeLensProvider(),
    ),
  );
  registerLearningProgressView(context, learningService);
  registerLearningWelcomeView(context);
  registerLearningCommands(context, learningService);
  registerKatasPanelSerializer(context, learningService);
  return learningService;
}

export type {
  ActivityLocation,
  CurrentActivity,
  ActivityContent,
  LessonTextContent,
  LessonExampleContent,
  ExampleContent,
  ExerciseContent,
  PrimaryAction,
  ActionGroup,
  LearningState,
  NavigationResult,
  HintContext,
  OverallProgress,
  UnitProgress,
  ActivityProgress,
  ProgressFileData,
  SolutionCheckResult,
  RunResult,
  UnitSummary,
  CatalogCourse,
  CatalogUnit,
  CatalogExample,
} from "./types.js";
