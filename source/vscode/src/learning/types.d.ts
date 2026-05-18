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
 */

// ─── Telemetry ───

export type TelemetrySource = "panel" | "chat" | "tree";

// ─── Location ───

export interface ActivityLocation {
  courseId: string;
  unitId: string;
  activityId: string;
}

// ─── Navigation ───

export interface CurrentActivity {
  location: ActivityLocation;
  unitTitle: string;
  activityTitle: string;
  content: ActivityContent;
}

export type ActivityContent =
  | LessonTextContent
  | LessonExampleContent
  | ExerciseContent;

export interface LessonTextContent {
  type: "lesson-text";
  content: string;
}

export interface LessonExampleContent {
  type: "lesson-example";
  id: string;
  code: string;
  /** URI string for the standalone .qs file */
  filePath: string;
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
}

// ─── Actions ───

export type PrimaryAction = "next" | "run" | "check";

export type Action =
  | "next"
  | "back"
  | "run"
  | "check"
  | "hint-chat"
  | "explain-chat"
  | "progress"
  | "menu"
  | "quit";

export interface ActionBinding {
  /** Keyboard shortcut key (single character like "b", or "space"). */
  key: string;
  label: string;
  action: Action;
  primary?: boolean;
  /** Codicon name to display as an icon prefix on the button. */
  codicon?: string;
}

export type ActionGroup = ActionBinding[];

// ─── Progress ───

export type ActivityKind = "lesson" | "exercise";

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
  currentPosition: ActivityLocation;
  stats: { totalActivities: number; completedActivities: number };
}

// ─── Progress file (qdk-learning.json) ───

export interface ProgressFileData {
  version: 1;
  position: ActivityLocation;
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
}

// ─── Code execution results ───

export interface SolutionCheckResult {
  passed: boolean;
  messages: string[];
  error?: string;
}

export interface RunResult {
  success: boolean;
  messages: string[];
  result?: string;
  error?: string;
}

// ─── Webview messages ───

/** Messages sent from the extension host to the webview panel. */
export type HostToWebviewMessage =
  /** Full state push. */
  | { command: "state"; state: LearningState }
  /** Forward navigation completed. `result.moved` is false when the user is already at the last activity. */
  | {
      command: "result";
      action: "next";
      result: NavigationResult;
      state: LearningState;
    }
  /** Backward navigation completed. `result.moved` is false when the user is already at the first activity. */
  | {
      command: "result";
      action: "back";
      result: NavigationResult;
      state: LearningState;
    }
  /** Exercise check completed. Contains pass/fail, captured output events, and any compiler/runtime error. */
  | {
      command: "result";
      action: "check";
      result: SolutionCheckResult;
      state: LearningState;
    }
  /** Run action completed. Result is empty — output is shown in the VS Code editor/terminal, not the webview. */
  | {
      command: "result";
      action: "run";
      result: Record<string, never>;
      state: LearningState;
    }
  /** An action failed with an unrecoverable error. */
  | { command: "error"; message: string };

type ResultMessage = Extract<HostToWebviewMessage, { command: "result" }>;
export type ResultAction = ResultMessage["action"];
export type ResultPayload<Action extends ResultAction> = Extract<
  ResultMessage,
  { action: Action }
>["result"];

/** Messages sent from the webview panel to the extension host. */
export type WebviewToHostMessage =
  /** Initialization handshake. Tells the host the webview is ready; triggers initial state push and file open. */
  | { command: "ready" }
  /** User triggered an action (next, back, run, check, etc.). */
  | { command: "action"; action: Action }
  /** Open a file in the editor (e.g. exercise or example .qs file). */
  | { command: "openFile"; uri: string }
  /** Open Copilot Chat with a learning-context query. */
  | { command: "openChat"; text: string }
  /** Focus the learning progress tree view in the sidebar. */
  | { command: "focusProgress" };

// ─── Catalog ───
//
// Flattened representation of the katas content. Derived from the raw
// qsharp-lang Kata types in `catalog.ts`.

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

export type CatalogActivity = CatalogExercise | CatalogLesson;

export interface CatalogUnit {
  id: string;
  title: string;
  activities: CatalogActivity[];
}

export interface CatalogCourse {
  id: string;
  title: string;
  units: CatalogUnit[];
}

export interface UnitSummary {
  id: string;
  title: string;
  activityCount: number;
  completedCount: number;
  /** True if this is the first unit that hasn't been fully completed. */
  firstIncomplete: boolean;
}
