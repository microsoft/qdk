// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * Types for the Quantum Katas webview panel engine.
 *
 * These mirror the shapes in `learning/server/types.ts` but are
 * re-declared so this module has no dependency on `src/learning/`
 * (which is bundled as a separate ESM CLI).
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

export interface KatasState {
  position: Position;
  actions: ActionGroup[];
  progress: OverallProgress;
}

export interface NavigationResult {
  moved: boolean;
  state: KatasState;
}

// ─── Solution check result (from compiler worker) ───

export interface SolutionCheckResult {
  passed: boolean;
  events: { type: string; message?: string }[];
  error?: string;
}
