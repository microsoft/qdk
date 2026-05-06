// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * Shared types for the QDK Learning feature.
 *
 * Taxonomy: Course → Unit → Activity
 *
 * - **Course**: a top-level learning experience (e.g. "Quantum Katas").
 * - **Unit**: a thematic group of activities (maps to a kata in the content).
 * - **Activity**: a single lesson or exercise within a unit.
 *
 * These types are used by both the LearningService (in-proc singleton) and
 * the Katas Panel webview.
 */

// ─── Navigation ───

/** Identifies a specific activity within the course hierarchy. */
export interface ActivityLocation {
  courseId: string;
  unitId: string;
  activityId: string;
}

export interface CurrentActivity extends ActivityLocation {
  unitTitle: string;
  activityTitle: string;
  content: ActivityContent;
}

export type ActivityContent =
  | LessonTextContent
  | LessonExampleContent
  | ExerciseContent
  | ExampleContent;

export interface LessonTextContent {
  type: "lesson-text";
  content: string;
  activityTitle: string;
}

export interface LessonExampleContent {
  type: "lesson-example";
  id: string;
  code: string;
  /** URI string for the standalone .qs file */
  filePath: string;
  activityTitle: string;
  contentBefore?: string;
  contentAfter?: string;
}

export interface ExerciseContent {
  type: "exercise";
  id: string;
  title: string;
  description: string;
  /** URI string for the user's .qs solution file */
  filePath: string;
  isComplete: boolean;
  hintCount: number;
}

export interface ExampleContent {
  type: "example";
  /** Absolute file path of the code asset (ipynb, .qs, .py, etc.) */
  filePath: string;
  activityTitle: string;
  /** 0-based cell index to reveal when navigating (notebook anchor). */
  cellIndex?: number;
}

// ─── Actions ───

export type PrimaryAction = "next" | "run" | "check";

export type Action =
  | "next"
  | "back"
  | "run"
  | "circuit"
  | "check"
  | "hint-chat"
  | "explain-chat"
  | "progress"
  | "menu"
  | "quit";

export interface ActionBinding {
  key: string;
  label: string;
  action: Action;
  primary?: boolean;
  /** Codicon name to display as an icon prefix on the button. */
  codicon?: string;
}

export type ActionGroup = ActionBinding[];

// ─── Progress ───

export type ActivityKind = "lesson" | "exercise" | "example";

export interface ActivityProgress {
  id: string;
  title: string;
  type: ActivityKind;
  isComplete: boolean;
  completedAt?: string;
  /** True for lessons that contain at least one code example. */
  hasExample?: boolean;
  /** For lessons with examples, the id of the first example part. */
  exampleId?: string;
}

export interface UnitProgress {
  id: string;
  title: string;
  total: number;
  completed: number;
  activities: ActivityProgress[];
}

export interface OverallProgress {
  units: UnitProgress[];
  currentPosition: ActivityLocation & { unitTitle?: string };
  stats: { totalActivities: number; completedActivities: number };
}

// ─── Progress file (qdk-learning.json) ───

export interface ProgressFileData {
  version: 1;
  position: Partial<Pick<ActivityLocation, "courseId">> &
    Omit<ActivityLocation, "courseId">;
  completions: Record<string, { completedAt: string }>;
  startedAt: string;
}

// ─── Bundled state ───

export interface HintContext {
  hints: string[];
  solutionExplanation: string;
}

export interface LearningState {
  position: CurrentActivity;
  actions: ActionGroup[];
  progress: OverallProgress;
}

export interface NavigationResult {
  moved: boolean;
  state: LearningState;
}

// ─── Webview messages ───

/** Messages sent from the extension host to the webview panel. */
export type HostToWebviewMessage =
  | { command: "state"; state: LearningState }
  | {
      command: "result";
      action: "next" | "back";
      result: NavigationResult;
      state: LearningState;
    }
  | {
      command: "result";
      action: "check";
      result: SolutionCheckResult;
      state: LearningState;
    }
  | {
      command: "result";
      action: "run" | "circuit";
      result: Record<string, never>;
      state: LearningState;
    }
  | { command: "error"; message: string };

/** Messages sent from the webview panel to the extension host. */
export type WebviewToHostMessage =
  | { command: "ready" }
  | { command: "action"; action: Action }
  | { command: "openFile"; uri: string }
  | { command: "openChat"; text: string }
  | { command: "focusProgress" };

// ─── Code execution results ───

export type OutputEvent =
  | { type: "message"; message: string }
  | { type: "dump"; dump: { state: Record<string, [number, number]> } }
  | { type: "matrix"; matrix: { matrix: number[][][] } };

export interface SolutionCheckResult {
  passed: boolean;
  events: OutputEvent[];
  error?: string;
}

export interface RunResult {
  success: boolean;
  events: OutputEvent[];
  result?: string;
  error?: string;
}

// ─── Catalog ───
//
// These types describe the pre-flattened content loaded from the katas content
// module. Only `catalog.ts` touches the raw qsharp-lang Kata types; everything
// else in the learning module works with these types.

export interface CatalogExercise {
  type: "exercise";
  id: string;
  title: string;
  /** Exercise description (markdown). */
  description: string;
  /** Starter code written to the user's exercise file. */
  placeholderCode: string;
  /** IDs of global source files needed for exercise checking. */
  sourceIds: string[];
  /** Author-written pedagogical hints. */
  hints: string[];
  /** Reference solution code. */
  solutionCode: string;
  /** Prose explanation of the solution (markdown). */
  solutionExplanation: string;
}

export interface CatalogLesson {
  type: "lesson";
  id: string;
  title: string;
  /** If the lesson contains a code example, its id and code. */
  example?: { id: string; code: string };
  /** Markdown text before the example (only when `example` is set). */
  contentBefore?: string;
  /** Markdown text after the example (only when `example` is set). */
  contentAfter?: string;
  /** Merged markdown text (only when there is no example). */
  content?: string;
}

export interface CatalogExample {
  type: "example";
  id: string;
  title: string;
  /** Absolute file path of the code asset on disk. */
  filePath: string;
  /** 0-based cell index to reveal when navigating (notebook anchor). */
  cellIndex?: number;
}

export type CatalogSection = CatalogExercise | CatalogLesson | CatalogExample;

export interface CatalogUnit {
  id: string;
  title: string;
  sections: CatalogSection[];
}

export interface CatalogCourse {
  id: string;
  title: string;
  units: CatalogUnit[];
  /** Optional absolute path to an SVG icon for the course. */
  iconPath?: string;
}

export interface UnitSummary {
  id: string;
  title: string;
  courseId: string;
  activityCount: number;
  completedCount: number;
  /** True if this unit is the next recommended one in learning order */
  recommended: boolean;
}
