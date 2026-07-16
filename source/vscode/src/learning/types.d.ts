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
  /** True when multiple reference solutions exist for this exercise. */
  hasMultipleSolutions: boolean;
}

// ─── Actions ───

export type PrimaryAction = "next" | "run" | "check";

export type Action =
  | "next"
  | "back"
  | "run"
  | "check"
  | "reset"
  | "hint-chat"
  | "explain-chat"
  | "open-notebook";

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
  /** The currently-active course. */
  course: { id: string; title: string; kind: CourseKind };
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
  | { command: "focusProgress" }
  /** Switch to a different course. */
  | { command: "switchCourse"; courseId: string }
  /** Show README/info for a course (defaults to the active course). */
  | { command: "courseInfo"; courseId?: string }
  /** Open the course picker to browse and switch courses. */
  | { command: "browseCourses" };

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
  /** Reference solution code (one per @[solution] block in the content). */
  solutionCodes: string[];
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

/**
 * Exercise metadata loaded from a per-unit `_exercises.json` sidecar
 * (python-notebook courses). Provides hints, solutions, and descriptions
 * for the chat LM tools without requiring cell parsing or execution.
 */
export interface NotebookExerciseInfo {
  id: string;
  title: string;
  description: string;
  hints: string[];
  solution: string;
  solutionExplanation: string;
  /** 1-based cell index in the notebook where this exercise lives. */
  cellIndex?: number;
}

export interface CatalogUnit {
  id: string;
  title: string;
  activities: CatalogActivity[];
  /**
   * Exercise metadata for python-notebook courses, loaded from
   * `_exercises.json`. Used by chat LM tools for hints/solutions.
   */
  notebookExercises?: NotebookExerciseInfo[];
  /**
   * Path (relative to the course source dir) of the notebook for this
   * unit. Set for python-notebook courses.
   */
  notebookRel?: string;
}

/** The execution model for a course's activities. */
export type CourseKind = "qsharp" | "python-notebook";

export interface CatalogCourse {
  id: string;
  title: string;
  /** Execution model for this course. Defaults to `"qsharp"`. */
  kind: CourseKind;
  units: CatalogUnit[];
  /**
   * URI string of the folder the course was loaded from (drop-in courses
   * only). Used to locate notebooks and other assets for materialization.
   */
  sourceDir?: string;
  /** Environment requirements (python-notebook courses). */
  environment?: CourseEnvironment;
}

/**
 * Lightweight metadata describing a course that can be loaded by the
 * {@link CourseRegistry}. Used to populate course pickers and the tree
 * view without forcing a full course load.
 */
export interface CourseDescriptor {
  id: string;
  title: string;
  shortDescription?: string;
  kind: CourseKind;
  /** Optional path (URI string) to a README rendered for "Course info". */
  readmePath?: string;
  /** Optional environment requirements (used by python-notebook courses). */
  environment?: CourseEnvironment;
}

/**
 * Environment requirements for a course (python-notebook courses).
 *
 * Courses that ship a `pyproject.toml` use `uv sync` for environment setup;
 * the `python` and `requirements` fields are only used as a legacy fallback
 * when no `pyproject.toml` is present.
 */
export interface CourseEnvironment {
  /**
   * Python version specifier for the course venv (e.g. `">=3.11"`, `"3.12"`).
   * Legacy: used only when no `pyproject.toml` is present. Prefer declaring
   * `requires-python` in `pyproject.toml` instead.
   */
  python?: string;
  /**
   * Python package requirements (e.g. `["qdk[jupyter]>=1.0", "ipympl"]`).
   * Legacy: used only when no `pyproject.toml` is present. Prefer declaring
   * `dependencies` in `pyproject.toml` instead.
   */
  requirements?: string[];
  /**
   * Module names to probe with `importlib.util.find_spec` in the notebook's
   * environment check cell (e.g. `["qdk", "qdk.widgets"]`). These are
   * importable module names, not pip package names.
   */
  importChecks?: string[];
}

export interface UnitSummary {
  id: string;
  title: string;
  activityCount: number;
  completedCount: number;
  /** True if this is the first unit that hasn't been fully completed. */
  firstIncomplete: boolean;
}

// ─── Environment check (environment diagnostics) ───

/** Severity of a single {@link EnvironmentCheckItem}. */
export type EnvironmentCheckStatus = "ok" | "warn" | "fail" | "skip";

/** A suggested fix attached to a failing {@link EnvironmentCheckItem}. */
export interface EnvironmentCheckFix {
  /** Short label for the action (e.g. "Set up environment"). */
  label: string;
  /**
   * What the fix does when chosen:
   * - `setup`: run the per-course environment setup.
   * - `install-extensions`: prompt to install Python/Jupyter.
   * - `select-kernel`: re-select the course kernel for the notebook.
   * - `docs`: informational only; no action.
   */
  kind: "setup" | "install-extensions" | "select-kernel" | "docs"; // TODO (acasey): select-kernel appears to be unused
}

/** One diagnostic in an {@link EnvironmentCheckReport}. */
export interface EnvironmentCheckItem {
  /** Stable identifier for the check (e.g. `"venv"`). */
  id: string;
  /** Human-readable label. */
  label: string;
  /** Pass/warn/fail/skip. */
  status: EnvironmentCheckStatus;
  /** Extra detail (a path, version, or error message). */
  detail?: string;
  /** Guidance on how to fix a non-ok check. */
  hint?: string;
  /** Optional fixes the UI can offer for this check. */
  fixes?: EnvironmentCheckFix[];
}

/** Overall status for an {@link EnvironmentCheckReport}. */
export type EnvironmentStatus = "ok" | "warning" | "error";

/** Structured result of running environment diagnostics for a course. */
export interface EnvironmentCheckReport {
  courseId: string;
  /** Overall status across all checks. */
  overallStatus: EnvironmentStatus;
  /** One-line human summary of the overall status. */
  summary: string;
  checks: EnvironmentCheckItem[];
  /** Distinct fixes aggregated from all failing checks, in priority order. */
  fixes: EnvironmentCheckFix[];
}
