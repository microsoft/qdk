// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// DEAD CODE: The standalone KatasServer has been replaced by the in-proc
// LearningService (see learningService/service.ts). Will be deleted.

export { KatasServer } from "./server.js";
export { CompilerService } from "./compiler.js";
export { WorkspaceManager } from "./workspace.js";
export { ProgressManager } from "./progress.js";
export { NoOpAIProvider, LLMAIProvider } from "./ai.js";
export type { LLMProviderConfig } from "./ai.js";
export type {
  IKatasServer,
  IAIProvider,
  InitConfig,
  KataSummary,
  KataDetail,
  SectionSummary,
  Position,
  NavigationItem,
  LessonTextItem,
  LessonExampleItem,
  LessonQuestionItem,
  ExerciseItem,
  RunResult,
  RunEvent,
  SolutionCheckResult,
  CircuitResult,
  EstimateResult,
  NoiseConfig,
  OverallProgress,
  KataProgress,
  SectionProgress,
  DumpInfo,
  MatrixInfo,
  AIHintContext,
  AIErrorContext,
  AIReviewContext,
  AIQuestionContext,
  PrimaryAction,
  HintResult,
  Action,
  ActionBinding,
  ActionGroup,
  ServerState,
  StatefulResult,
  NavigationResult,
} from "./types.js";
