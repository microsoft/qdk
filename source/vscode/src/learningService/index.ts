// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/** Well-known workspace folder name for katas exercise/example files. */
export const KATAS_WS_FOLDER = "qdk-learning-ws";

/** Relative path form of {@link KATAS_WS_FOLDER}, for use in URI joins. */
export const KATAS_WS_FOLDER_REL = `./${KATAS_WS_FOLDER}`;

export { LearningService } from "./service.js";
export { registerEditorContext } from "./editorContext.js";
export { registerLearningCommands } from "./commands.js";
export {
  createLearningCodeLensProvider,
  exerciseDocumentSelector,
} from "./codeLens.js";
export { registerLearningDecorations } from "./decorations.js";
export type {
  Position,
  NavigationItem,
  LessonTextItem,
  LessonExampleItem,
  LessonQuestionItem,
  ExerciseItem,
  PrimaryAction,
  ActionGroup,
  LearningState,
  NavigationResult,
  AllHintsResult,
  OverallProgress,
  KataProgress,
  SectionProgress,
  ProgressFileData,
  SolutionCheckResult,
  RunResult,
  KataSummary,
} from "./types.js";
