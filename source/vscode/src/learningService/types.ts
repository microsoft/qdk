// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * Shared types for the QDK Learning feature.
 *
 * These types are used by both the LearningService (in-proc singleton) and
 * the Katas Panel webview. They mirror the shapes in `learning/server/types.ts`
 * but are self-contained so this module has no dependency on `src/learning/`
 * (which is a separate ESM CLI bundle — now dead code).
 */

// ─── Navigation ───

export interface Position {
  kataId: string;
  sectionId: string;
  itemIndex: number;
  item: NavigationItem;
}

export type NavigationItem =
  | LessonTextItem
  | LessonExampleItem
  | LessonQuestionItem
  | ExerciseItem;

export interface LessonTextItem {
  type: "lesson-text";
  content: string;
  sectionTitle: string;
}

export interface LessonExampleItem {
  type: "lesson-example";
  id: string;
  code: string;
  /** URI string for the standalone .qs file */
  filePath: string;
  sectionTitle: string;
  contentBefore?: string;
  contentAfter?: string;
}

export interface LessonQuestionItem {
  type: "lesson-question";
  description: string;
  answer: string;
  sectionTitle: string;
}

export interface ExerciseItem {
  type: "exercise";
  id: string;
  title: string;
  description: string;
  /** URI string for the user's .qs solution file */
  filePath: string;
  isComplete: boolean;
  hintCount: number;
}

// ─── Actions ───

export type PrimaryAction = "next" | "run" | "check" | "reveal-answer";

export type Action =
  | "next"
  | "back"
  | "run"
  | "circuit"
  | "check"
  | "hint"
  | "solution"
  | "reveal-answer"
  | "progress"
  | "menu"
  | "quit";

export interface ActionBinding {
  key: string;
  label: string;
  action: Action;
  primary?: boolean;
}

export type ActionGroup = ActionBinding[];

// ─── Progress ───

export interface SectionProgress {
  id: string;
  title: string;
  type: "lesson" | "exercise";
  isComplete: boolean;
  completedAt?: string;
}

export interface KataProgress {
  total: number;
  completed: number;
  sections: SectionProgress[];
}

export interface OverallProgress {
  katas: Map<string, KataProgress>;
  currentPosition: { kataId: string; sectionId: string; itemIndex: number };
  stats: { totalSections: number; completedSections: number };
}

// ─── Progress file (qdk-learning.json) ───

export interface ProgressFileData {
  version: 1;
  katasRoot: string;
  position: { kataId: string; sectionId: string; itemIndex: number };
  completions: Record<string, { completedAt: string }>;
  startedAt: string;
}

// ─── Bundled state ───

export interface HintResult {
  hint: string;
  current: number;
  total: number;
}

export interface LearningState {
  position: Position;
  actions: ActionGroup[];
  progress: OverallProgress;
}

export interface NavigationResult {
  moved: boolean;
  state: LearningState;
}

// ─── Code execution results ───

export interface SolutionCheckResult {
  passed: boolean;
  events: { type: string; message?: string }[];
  error?: string;
}

export interface RunResult {
  success: boolean;
  events: { type: string; message?: string }[];
  result?: string;
  error?: string;
}

// ─── Catalog ───

export interface KataSummary {
  id: string;
  title: string;
  sectionCount: number;
  completedCount: number;
  /** True if this kata is the next recommended one in learning order */
  recommended: boolean;
}
