// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

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
  HintResult,
  OverallProgress,
  KataProgress,
  SectionProgress,
  ProgressFileData,
  SolutionCheckResult,
  RunResult,
  KataSummary,
} from "./types.js";
