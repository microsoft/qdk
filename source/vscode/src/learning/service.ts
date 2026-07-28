// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import { getExerciseSources } from "qsharp-lang/katas-md";
import * as vscode from "vscode";
import { FullProgramConfig, getProgramForDocument } from "../programConfig.js";
import { ProgramRunStatus, runProgram } from "../run.js";
import { EventType, sendTelemetryEvent } from "../telemetry.js";
import { createCourseRegistry } from "./catalog.js";
import { CourseRegistry } from "./courseProvider.js";
import { EnvironmentManager } from "./python/environment.js";
import {
  checkPythonExtensions,
  promptInstallPythonExtensions,
} from "./python/extensionUtils.js";
import {
  materializeCourseWorkbooks,
  rematerializeUnitWorkbook,
  workbookFileUri,
} from "./python/materialization.js";
import {
  KATAS_COURSE_ID,
  LEARNING_FILE,
  LEARNING_WORKSPACE_DETECTED_CONTEXT,
  LEARNING_WORKSPACE_FOLDER,
  LEARNING_WORKSPACE_RELATIVE_PATH,
} from "./constants.js";
import { ensureParentDir, uriExists } from "./fsUtils.js";
import type {
  ActionGroup,
  ActivityContent,
  ActivityLocation,
  ActivityProgress,
  CatalogCourse,
  CatalogExercise,
  CatalogActivity,
  CatalogUnit,
  CourseDescriptor,
  CurrentActivity,
  EnvironmentCheckFix,
  EnvironmentCheckItem,
  EnvironmentCheckReport,
  EnvironmentStatus,
  ExerciseContent,
  HintContext,
  LearningState,
  LessonExampleContent,
  LessonTextContent,
  NavigationResult,
  OverallProgress,
  PrimaryAction,
  ProgressFileData,
  RunResult,
  SolutionCheckResult,
  TelemetrySource,
  UnitProgress,
  UnitSummary,
} from "./types.js";
import type { EnvironmentCheckStatus } from "./types.js";

/** Build an {@link EnvironmentCheckItem}. */
function check(
  id: string,
  label: string,
  status: EnvironmentCheckStatus,
  extras?: Pick<EnvironmentCheckItem, "detail" | "hint" | "fixes">,
): EnvironmentCheckItem {
  return {
    id,
    label,
    status,
    detail: extras?.detail,
    hint: extras?.hint,
    fixes: extras?.fixes,
  };
}

/** Returns the first open workspace folder URI, or `undefined`. */
export function resolveNewWorkspaceRoot(): vscode.Uri | undefined {
  const folders = vscode.workspace.workspaceFolders;
  if (!folders || folders.length === 0) {
    return undefined;
  }
  return folders[0].uri;
}

/**
 * Detect an existing learning workspace by scanning all open workspace
 * folders for a `qdk-learning.json` file.
 *
 * Returns `undefined` if no learning workspace can be found.
 */
export async function detectLearningWorkspace(): Promise<
  LearningWorkspaceInfo | undefined
> {
  for (const folder of vscode.workspace.workspaceFolders ?? []) {
    const learningFile = vscode.Uri.joinPath(folder.uri, LEARNING_FILE);
    try {
      await vscode.workspace.fs.stat(learningFile);
    } catch {
      continue;
    }

    const learningContentRoot = vscode.Uri.joinPath(
      folder.uri,
      LEARNING_WORKSPACE_RELATIVE_PATH,
    );
    return {
      workspaceRoot: folder.uri,
      learningContentRoot,
      learningFile,
    };
  }

  return undefined;
}

interface LearningWorkspaceInfo {
  /** The workspace folder that contains `qdk-learning.json`. */
  workspaceRoot: vscode.Uri;
  /** The learning content folder, resolved from the well-known folder name. */
  learningContentRoot: vscode.Uri;
  /** Path to `qdk-learning.json`. */
  learningFile: vscode.Uri;
}

/** All state that exists only while a learning workspace is loaded. */
interface WorkspaceState extends LearningWorkspaceInfo {
  /** Loaded courses, keyed by course id. May contain more than one. */
  courses: Map<string, CatalogCourse>;
  /** Registry used to enumerate and lazily load additional courses. */
  registry: CourseRegistry;
  progressData: ProgressFileData;
}

export class LearningService {
  private workspace: WorkspaceState | undefined;

  private readonly _onDidChangeState = new vscode.EventEmitter<LearningState>();
  readonly onDidChangeState = this._onDidChangeState.event;

  private readonly _onDidChangeProgress = new vscode.EventEmitter<
    OverallProgress | undefined
  >();
  readonly onDidChangeProgress = this._onDidChangeProgress.event;

  private _progressFileWatcher: vscode.FileSystemWatcher | undefined;
  private _writingProgress = false;
  private _initPromise: Promise<boolean> | undefined;
  private readonly _disposables: vscode.Disposable[] = [];
  private _environment: EnvironmentManager | undefined;

  constructor(private readonly extensionUri: vscode.Uri) {
    // Navigating away from an activity leaves its file behind. Close those
    // tabs here rather than in the lesson panel, which isn't shown for every
    // course kind.
    this._disposables.push(
      this.onDidChangeState(() => {
        void this.closeStaleEditorTabs(this.getCurrentCodeFileUri());
      }),
    );
  }

  get initialized(): boolean {
    return this.workspace !== undefined;
  }

  get learningContentRoot(): vscode.Uri {
    return this.requireWorkspace().learningContentRoot;
  }

  /** The workspace folder that owns the learning content. */
  get workspaceFolder(): vscode.Uri {
    return this.requireWorkspace().workspaceRoot;
  }

  /** Lazily-created per-course Python environment manager. */
  private get environment(): EnvironmentManager {
    if (!this._environment) {
      this._environment = new EnvironmentManager();
    }
    return this._environment;
  }

  /**
   * Try to initialize the service. Returns `true` when ready, `false`
   * when no learning workspace could be found (or created).
   *
   * Detects an existing `qdk-learning.json` on disk. When
   * `createIfMissing` is set, bootstraps a new workspace in the first
   * open folder instead of returning `false`.
   *
   * Safe to call multiple times — concurrent calls are coalesced and
   * subsequent calls after success return immediately.
   */
  async tryInitialize(options?: {
    createIfMissing?: boolean;
  }): Promise<boolean> {
    if (this.workspace) {
      return true;
    }

    // If there's an in-flight attempt, wait for it first.
    if (this._initPromise) {
      const result = await this._initPromise;
      // If init succeeded, or the caller doesn't need creation, we're done.
      if (result || !options?.createIfMissing) {
        return result;
      }
      if (this.workspace) {
        return true;
      }
      // The in-flight attempt didn't create — fall through to retry.
      // TODO (acasey): this retry isn't safe if A wins the initial race, leaving B and C waiting,
      // and then fails to actually initialize, B and C will race to call detectAndLoadWorkspace.
    }

    this._initPromise = this.detectAndLoadWorkspace(options).finally(() => {
      this._initPromise = undefined;
    });
    return await this._initPromise;
  }

  dispose(): void {
    if (this.workspace) {
      this.saveProgress().catch(() => {});
    }
    this._onDidChangeState.dispose();
    this._onDidChangeProgress.dispose();
    this._progressFileWatcher?.dispose();
    this._environment?.dispose();
    for (const d of this._disposables) {
      d.dispose();
    }
  }

  /** Force a fresh progress reload from disk. */
  async refresh(): Promise<void> {
    if (this.workspace) {
      await this.reloadProgress();
    }
  }

  /** The current position in the learning workspace. */
  get position(): ActivityLocation {
    return this.requireWorkspace().progressData.position;
  }

  /** Resolves the current position into a rich object with
   * titles and content for rendering. */
  getCurrentActivity(): CurrentActivity {
    const pos = this.position;
    const kata = this.findUnit(pos.unitId);
    const activity = kata.activities.find((s) => s.id === pos.activityId)!;
    return {
      location: pos,
      unitTitle: kata.title,
      activityTitle: activity.title,
      content: this.resolveActivityContent(pos, kata, activity),
    };
  }

  /** Full snapshot of position, available actions, and progress.
   * The payload sent to the webview. */
  getState(): LearningState {
    return {
      course: this.getActiveCourseInfo(),
      position: this.getCurrentActivity(),
      actions: this.getAvailableActions(),
      progress: this.getProgress(),
    };
  }

  async next(source: TelemetrySource): Promise<NavigationResult> {
    const ws = this.requireWorkspace();
    const currentPos = ws.progressData.position;
    const nextPos = this.nextActivity(currentPos);

    // Auto-mark lesson activities complete when moving forward
    const oldKata = this.findUnit(currentPos.unitId);
    const oldActivity = oldKata.activities.find(
      (s) => s.id === currentPos.activityId,
    );
    if (oldActivity?.type === "lesson") {
      this.markComplete(currentPos);
    }

    const hasNext = !!nextPos;

    if (hasNext) {
      ws.progressData.position = nextPos;
    }

    await this.saveProgress();
    this._onDidChangeState.fire(this.getState());
    this.sendActivityActionTelemetry("navigate", source);

    return { moved: hasNext };
  }

  async previous(source: TelemetrySource): Promise<NavigationResult> {
    const ws = this.requireWorkspace();
    const prevPos = this.previousActivity(ws.progressData.position);
    if (!prevPos) {
      return { moved: false };
    }

    ws.progressData.position = prevPos;
    await this.saveProgress();
    this._onDidChangeState.fire(this.getState());
    this.sendActivityActionTelemetry("navigate", source);
    return { moved: true };
  }

  async goTo(
    location: { unitId: string; activityId?: string },
    source?: TelemetrySource,
  ): Promise<LearningState> {
    const ws = this.requireWorkspace();
    const course = this.activeCourse;
    const unit = course.units.find((u) => u.id === location.unitId);
    if (!unit || unit.activities.length === 0) {
      throw new Error(`Position not found: ${location.unitId}`);
    }
    const activity = location.activityId
      ? unit.activities.find((s) => s.id === location.activityId)
      : unit.activities[0];
    if (!activity) {
      throw new Error(
        `Position not found: ${location.unitId} activity ${location.activityId}`,
      );
    }
    ws.progressData.position = {
      courseId: course.id,
      unitId: location.unitId,
      activityId: activity.id,
    };
    await this.saveProgress();
    const state = this.getState();
    this._onDidChangeState.fire(state);
    if (source) {
      this.sendActivityActionTelemetry("navigate", source);
    }
    return state;
  }

  /**
   * Navigate to the exercise activity whose `cellId` matches the given
   * notebook cell ID. Returns `true` if the position was updated.
   * Only meaningful for python-notebook courses.
   *
   * Updates the position silently — does **not** fire the state-change
   * event, so the lesson panel won't pop up or rearrange the editor layout.
   */
  async goToExerciseByCellId(
    cellId: string,
    source?: TelemetrySource,
  ): Promise<boolean> {
    if (this.activeCourse.kind !== "python-notebook") {
      return false;
    }
    const unit = this.findUnit(this.position.unitId);
    const exercise = unit.notebookExercises?.find((e) => e.cellId === cellId);
    if (!exercise) {
      return false;
    }
    // Only move if we're not already on this exercise.
    if (this.position.activityId === exercise.id) {
      return true;
    }

    const ws = this.requireWorkspace();
    ws.progressData.position = {
      courseId: this.activeCourse.id,
      unitId: unit.id,
      activityId: exercise.id,
    };
    await this.saveProgress();
    if (source) {
      this.sendActivityActionTelemetry("navigate", source);
    }
    return true;
  }

  /**
   * Mark the exercise activity with the given cell ID as complete.
   * Returns `true` if the exercise was found and marked (or already complete).
   * Fires the state-change event so the treeview updates.
   */
  async markExerciseCompleteByCellId(cellId: string): Promise<boolean> {
    if (this.activeCourse.kind !== "python-notebook") {
      return false;
    }
    const unit = this.findUnit(this.position.unitId);
    const exercise = unit.notebookExercises?.find((e) => e.cellId === cellId);
    if (!exercise) {
      log.warn(`Unable to find exercise corresponding to cell ${cellId}`);
      return false;
    }
    const location: ActivityLocation = {
      courseId: this.activeCourse.id,
      unitId: unit.id,
      activityId: exercise.id,
    };
    if (this.isComplete(location)) {
      return true;
    }
    this.markComplete(location);
    await this.saveProgress();
    this._onDidChangeState.fire(this.getState());
    return true;
  }

  /**
   * Move the current position to the unit backing the given workbook URI,
   * so that unit-scoped UI (hint status bar items, notebook toolbar actions,
   * completion tracking) applies to the notebook the learner is looking at.
   *
   * Returns `true` when the URI belongs to a known course workbook, whether
   * or not the position actually had to move.
   */
  async syncToWorkbook(uri: vscode.Uri): Promise<boolean> {
    if (!this.workspace) {
      return false;
    }
    const resolved = this.resolveWorkbookLocation(uri);
    if (!resolved) {
      return false;
    }
    const { course, unit } = resolved;

    // Compare on the unit and never the activity: commands navigate to a
    // specific activity and *then* open its notebook, so re-deriving the
    // activity here would undo that. The guard is also what terminates the
    // open-notebook -> active-editor-change -> sync feedback loop.
    const pos = this.position;
    if (pos.courseId === course.id && pos.unitId === unit.id) {
      return true;
    }

    this.workspace.progressData.position = this.firstIncompleteInUnit(
      course,
      unit,
    );
    await this.saveProgress();
    this._onDidChangeState.fire(this.getState());
    return true;
  }

  /**
   * Resolve a `*.workbook.ipynb` URI to the course and unit that own it.
   * Only python-notebook courses have workbooks.
   *
   * Purely in-memory: every course is already loaded by `loadWorkspace`,
   * so this is a handful of string comparisons and cheap enough to run on
   * every active-editor change.
   */
  private resolveWorkbookLocation(
    uri: vscode.Uri,
  ): { course: CatalogCourse; unit: CatalogUnit } | undefined {
    const target = uri.toString();
    for (const course of this.requireWorkspace().courses.values()) {
      if (course.kind !== "python-notebook" || !course.sourceDir) {
        continue;
      }
      for (const unit of course.units) {
        if (!unit.notebookRel) {
          continue;
        }
        const workbook = workbookFileUri(course, unit.notebookRel);
        if (workbook.toString() === target) {
          return { course, unit };
        }
      }
    }
    return undefined;
  }

  /**
   * The first activity in a unit that has not been completed, or the unit's
   * first activity when everything in it is already done.
   */
  private firstIncompleteInUnit(
    course: CatalogCourse,
    unit: CatalogUnit,
  ): ActivityLocation {
    for (const activity of unit.activities) {
      const location: ActivityLocation = {
        courseId: course.id,
        unitId: unit.id,
        activityId: activity.id,
      };
      if (!this.isComplete(location)) {
        return location;
      }
    }
    return {
      courseId: course.id,
      unitId: unit.id,
      activityId: unit.activities[0]?.id ?? "",
    };
  }

  // TODO (acasey): isExerciseCellId
  /**
   * Returns the set of cell IDs that correspond to exercises in the
   * current unit. Empty if the course isn't a python-notebook course or
   * there are no exercises.
   */
  getExerciseCellIds(): Set<string> {
    if (this.activeCourse.kind !== "python-notebook") {
      return new Set();
    }
    const unit = this.findUnit(this.position.unitId);
    const ids = new Set<string>();
    if (unit.notebookExercises) {
      for (const ex of unit.notebookExercises) {
        ids.add(ex.cellId);
      }
    }
    return ids;
  }

  /**
   * The notebook cell ID backing the current activity — the inverse of
   * {@link goToExerciseByCellId}. `undefined` when the course isn't a
   * python-notebook course or the activity has no associated cell.
   */
  getCurrentExerciseCellId(): string | undefined {
    if (this.activeCourse.kind !== "python-notebook") {
      return undefined;
    }
    const { unit, activity } = this.findCurrentActivity();
    return unit.notebookExercises?.find((e) => e.id === activity.id)?.cellId;
  }

  /** Enumerate all available courses (loaded or not). */
  async getCourses(): Promise<CourseDescriptor[]> {
    return this.requireWorkspace().registry.listCourses();
  }

  /** The id of the currently-active course. */
  getActiveCourseId(): string {
    // Don't do the extra work that this.activeCourse.id would require
    return this.requireWorkspace().progressData.position.courseId;
  }

  /** Compact info about the active course for serialization to chat tools. */
  getActiveCourseInfo(): Pick<CourseDescriptor, "id" | "title" | "kind"> {
    const course = this.activeCourse;
    return { id: course.id, title: course.title, kind: course.kind };
  }

  /**
   * Ensure a python-notebook course's per-course environment exists:
   * create or update the environment and install required packages. No-ops
   * for Q# courses, on the Web, or when the environment already exists
   * (unless `force` is set).
   */
  async ensureEnvironment(
    course: CatalogCourse,
    options?: { force?: boolean },
  ): Promise<void> {
    if (course.kind !== "python-notebook") {
      return;
    }
    const env = this.environment;
    if (!env.supported) {
      return;
    }
    if (!course.sourceDir) {
      return;
    }
    const courseRoot = vscode.Uri.parse(course.sourceDir);
    if (!options?.force && (await env.environmentExists(courseRoot))) {
      return;
    }
    await vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: `Setting up the environment for "${course.title}"…`,
      },
      async () => {
        await env.ensureEnvironment(courseRoot);
      },
    );
  }

  /**
   * Apply a fix surfaced by {@link runEnvironmentCheck}. Centralizes the
   * mapping from an {@link EnvironmentCheckFix.kind} to a concrete action so
   * the command and chat tool can offer fixes without duplicating the logic.
   */
  async applyEnvironmentCheckFix(fix: EnvironmentCheckFix): Promise<void> {
    switch (fix.kind) {
      case "setup":
        await this.ensureEnvironment(this.activeCourse, { force: true });
        return;
      case "install-extensions":
        await promptInstallPythonExtensions();
        return;
    }
  }

  /**
   * Run environment diagnostics for the active course and return a rich,
   * structured report: an ordered list of checks (each `ok`/`warn`/`fail`/
   * `skip` with detail, a fix hint, and fixes), an overall status, a
   * one-line summary, and the aggregated fixes the UI can offer.
   *
   * Q# courses need no environment and pass trivially.
   */
  async runEnvironmentCheck(): Promise<EnvironmentCheckReport> {
    const course = this.activeCourse;

    if (course.kind !== "python-notebook") {
      const checks: EnvironmentCheckItem[] = [
        check("course-kind", "Course type", "ok", {
          detail: "Q# course — runs on the built-in simulator.",
        }),
        check("environment", "Python environment", "skip", {
          detail: "Not required for Q# courses.",
        }),
      ];
      return this.assembleReport(course, checks);
    }

    const env = this.environment;

    // Hard stop: environment management can't run on the Web.
    if (!env.supported) {
      const checks: EnvironmentCheckItem[] = [
        check("host", "Desktop VS Code", "fail", {
          detail: "Python courses require the desktop version of VS Code.",
          hint: "Open this workspace in desktop VS Code to run Python courses.",
        }),
      ];
      return this.assembleReport(course, checks);
    }

    // Resolve the course's working root (its source folder); the venv
    // lives here, beside the authored notebooks.
    if (!course.sourceDir) {
      return this.assembleReport(course, [
        check("course-folder", "Course folder", "fail", {
          detail: "This course has no source folder on disk.",
        }),
      ]);
    }
    const courseRoot = vscode.Uri.parse(course.sourceDir);

    const checks: EnvironmentCheckItem[] = [];

    // 1. Required extensions (Python + Jupyter).
    const extMessage = checkPythonExtensions();
    checks.push(
      check(
        "extensions",
        "Python & Jupyter extensions",
        extMessage ? "fail" : "ok",
        {
          detail: extMessage ?? "Installed.",
          hint: extMessage
            ? "Install the Python and Jupyter extensions to run notebook courses."
            : undefined,
          fixes: extMessage
            ? [{ label: "Install extensions", kind: "install-extensions" }]
            : undefined,
        },
      ),
    );

    // 2. The per-course environment.
    const envExists = await env.environmentExists(courseRoot);
    checks.push(
      check("venv", "Course environment", envExists ? "ok" : "fail", {
        detail: envExists
          ? "Environment found."
          : "No environment found for this course.",
        hint: envExists
          ? undefined
          : "Run environment setup to create the course environment.",
        fixes: envExists
          ? undefined
          : [{ label: "Set up environment", kind: "setup" }],
      }),
    );

    // 3. Required packages import in the environment.
    const importChecks = course.environment?.importChecks ?? [];
    if (envExists && importChecks.length > 0) {
      const report = await env.importsReport(courseRoot, importChecks);
      const missing = report.filter((r) => !r.ok).map((r) => r.module);
      checks.push(
        check(
          "packages",
          "Required packages",
          missing.length === 0 ? "ok" : "fail",
          {
            detail:
              missing.length === 0
                ? report.map((r) => r.module).join(", ")
                : `Missing or broken: ${missing.join(", ")}`,
            hint:
              missing.length === 0
                ? undefined
                : "Re-run environment setup to (re)install the course's pinned packages.",
            fixes:
              missing.length === 0
                ? undefined
                : [{ label: "Set up environment", kind: "setup" }],
          },
        ),
      );
    } else if (importChecks.length > 0) {
      checks.push(
        check("packages", "Required packages", "skip", {
          detail: "No environment yet.",
        }),
      );
    }

    return this.assembleReport(course, checks);
  }

  /**
   * Fold a list of diagnostic checks into an {@link EnvironmentCheckReport}:
   * compute the overall status, a human summary, and the de-duplicated fix
   * list.
   */
  private assembleReport(
    course: CatalogCourse,
    checks: EnvironmentCheckItem[],
  ): EnvironmentCheckReport {
    const hasFail = checks.some((c) => c.status === "fail");
    const hasWarn = checks.some((c) => c.status === "warn");
    const overallStatus: EnvironmentStatus = hasFail
      ? "error"
      : hasWarn
        ? "warning"
        : "ok";

    // De-duplicate fixes by kind+label, preserving first-seen order.
    const fixes: EnvironmentCheckFix[] = [];
    const seen = new Set<string>();
    for (const c of checks) {
      for (const r of c.fixes ?? []) {
        const key = `${r.kind}:${r.label}`;
        if (!seen.has(key)) {
          seen.add(key);
          fixes.push(r);
        }
      }
    }

    const failed = checks.filter((c) => c.status === "fail").length;
    const warned = checks.filter((c) => c.status === "warn").length;
    const summary =
      overallStatus === "ok"
        ? `"${course.title}" is ready to go.`
        : overallStatus === "warning"
          ? `"${course.title}" works, but ${warned} thing${warned === 1 ? "" : "s"} could be improved.`
          : `"${course.title}" has ${failed} problem${failed === 1 ? "" : "s"} to fix before it will run.`;

    return {
      courseId: course.id,
      overallStatus,
      summary,
      checks,
      fixes,
    };
  }

  /**
   * Switch the active course. Lazily loads the course (and scaffolds its
   * files) if it isn't loaded yet, moves the position to the first
   * incomplete activity, persists, and fires change events.
   */
  async switchCourse(
    courseId: string,
    source?: TelemetrySource,
  ): Promise<LearningState> {
    const ws = this.requireWorkspace();
    let course = ws.courses.get(courseId);
    if (!course) {
      course = await ws.registry.loadCourse(courseId);
      ws.courses.set(course.id, course);
      await this.scaffoldCourse(ws, course);
      // TODO (acasey): if scaffolding fails, you basically have to reload the window.
      // That's probably fine, but confirm.
    }
    if (course.kind === "python-notebook") {
      // Need to await extension installation since environment setup depends
      // on the Python Environments extension
      await promptInstallPythonExtensions();
      void this.ensureEnvironment(course);
    }
    ws.progressData.position = this.firstIncompletePosition(course);
    await this.saveProgress();
    const state = this.getState();
    this._onDidChangeState.fire(state);
    if (source) {
      this.sendActivityActionTelemetry("navigate", source);
    }
    return state;
  }

  /**
   * The first activity in a course that has not been completed, or the
   * course's first activity when everything is already complete.
   */
  private firstIncompletePosition(course: CatalogCourse): ActivityLocation {
    for (const unit of course.units) {
      // TODO (acasey): reuse firstIncompleteInUnit
      for (const activity of unit.activities) {
        const location: ActivityLocation = {
          courseId: course.id,
          unitId: unit.id,
          activityId: activity.id,
        };
        if (!this.isComplete(location)) {
          return location;
        }
      }
    }
    const first = course.units[0];
    return {
      courseId: course.id,
      unitId: first?.id ?? "",
      activityId: first?.activities[0]?.id ?? "",
    };
  }

  listUnits(): UnitSummary[] {
    const course = this.activeCourse;
    let foundFirstIncomplete = false;

    return course.units.map((kata) => {
      const activityCount = kata.activities.length;
      let completedCount = 0;
      for (const activity of kata.activities) {
        if (
          this.findCompletion({
            courseId: course.id,
            unitId: kata.id,
            activityId: activity.id,
          })
        ) {
          completedCount++;
        }
      }

      let firstIncomplete = false;
      if (completedCount < activityCount && !foundFirstIncomplete) {
        foundFirstIncomplete = true;
        firstIncomplete = true;
      }

      return {
        id: kata.id,
        title: kata.title,
        activityCount,
        completedCount,
        firstIncomplete,
      };
    });
  }

  getProgress(): OverallProgress {
    return this.computeProgress(this.activeCourse);
  }

  /**
   * Compute progress for an arbitrary course, lazily loading it if needed.
   * Does **not** change the active course or position. Used to populate
   * per-course progress badges in the tree view.
   */
  async getCourseProgress(courseId: string): Promise<OverallProgress> {
    const ws = this.requireWorkspace();
    let course = ws.courses.get(courseId);
    if (!course) {
      course = await ws.registry.loadCourse(courseId);
      ws.courses.set(course.id, course);
    }
    return this.computeProgress(course);
  }

  private computeProgress(course: CatalogCourse): OverallProgress {
    const ws = this.requireWorkspace();
    let totalActivities = 0;
    let completedActivities = 0;

    const units: UnitProgress[] = course.units.map((k) => {
      const activities: ActivityProgress[] = k.activities.map((s) => {
        const completion = this.findCompletion({
          courseId: course.id,
          unitId: k.id,
          activityId: s.id,
        });
        return {
          id: s.id,
          title: s.title,
          type: s.type,
          isComplete: completion != null,
          completedAt: completion?.completedAt,
        };
      });
      const completed = activities.filter((a) => a.isComplete).length;
      totalActivities += activities.length;
      completedActivities += completed;
      return {
        id: k.id,
        title: k.title,
        total: activities.length,
        completed,
        activities,
      };
    });

    return {
      units,
      currentPosition: ws.progressData.position,
      stats: { totalActivities, completedActivities },
    };
  }

  /** Returns hints and solution explanation for the current exercise, or `null` if none exist. */
  getHintContext(source?: TelemetrySource): {
    result: HintContext | null;
    state: LearningState;
  } {
    if (source) {
      this.sendActivityActionTelemetry("hint", source);
    }

    const exercise = this.resolveExercise();
    const hints = exercise.hints;
    const solutionExplanation = exercise.solutionExplanation;

    if (hints.length === 0 && solutionExplanation.length === 0) {
      return { result: null, state: this.getState() };
    }

    return {
      result: { hints, solutionExplanation },
      state: this.getState(),
    };
  }

  getAllSolutions(source?: TelemetrySource): string[] {
    if (source) {
      this.sendActivityActionTelemetry("solution", source);
    }

    return this.resolveExercise().solutionCodes;
  }

  getExerciseFileUri(): vscode.Uri {
    const exercise = this.resolveExercise();
    return vscode.Uri.joinPath(
      this.requireWorkspace().learningContentRoot,
      "exercises",
      this.position.unitId,
      `${exercise.id}.qs`,
    );
  }

  getExampleFileUri(): vscode.Uri {
    const { unit, activity } = this.findCurrentActivity();
    if (activity.type !== "lesson" || !activity.example) {
      throw new Error("Current activity is not an example");
    }
    return vscode.Uri.joinPath(
      this.requireWorkspace().learningContentRoot,
      "examples",
      unit.id,
      `${activity.example.id}.qs`,
    );
  }

  async readUserCode(): Promise<string> {
    const uri = this.getCurrentCodeFileUri();
    if (!uri) {
      throw new Error("Current activity has no associated code file.");
    }
    await this.saveOpenDocument(uri);
    const bytes = await vscode.workspace.fs.readFile(uri);
    return new TextDecoder().decode(bytes);
  }

  /** Save the document to disk if it's open and has unsaved edits. */
  private async saveOpenDocument(uri: vscode.Uri): Promise<void> {
    const doc = vscode.workspace.textDocuments.find(
      (d) => d.uri.toString() === uri.toString(),
    );
    if (doc?.isDirty) {
      await doc.save();
    }
  }

  async markExampleRun(): Promise<void> {
    const location = this.requireWorkspace().progressData.position;
    this.markComplete(location);
    await this.saveProgress();
    this._onDidChangeState.fire(this.getState());
  }

  getCurrentCodeFileUri(): vscode.Uri | undefined {
    // Python-notebook courses: the "code" is the notebook itself.
    if (this.activeCourse.kind === "python-notebook") {
      const { unit } = this.findCurrentActivity();
      if (unit.notebookRel) {
        return this.notebookFileUri(unit.notebookRel);
      }
      return undefined;
    }
    const { activity } = this.findCurrentActivity();
    if (activity.type === "exercise") {
      return this.getExerciseFileUri();
    }
    if (activity.type === "lesson" && activity.example) {
      return this.getExampleFileUri();
    }
    return undefined;
  }

  /**
   * Reset the current exercise/unit to its original state and clear
   * completion status.
   */
  async resetExercise(source?: TelemetrySource): Promise<void> {
    // Python-notebook courses: close the notebook, re-copy the entire unit
    // from source, and clear completion.
    if (this.activeCourse.kind === "python-notebook") {
      const { unit } = this.findCurrentActivity();
      // Close any open notebook tabs for this unit.
      if (unit.notebookRel) {
        const notebookUri = this.notebookFileUri(unit.notebookRel);
        await this.closeNotebookTab(notebookUri);
      }
      // Re-materialize the unit from source.
      await rematerializeUnitWorkbook(this.activeCourse, unit.id);
      // Clear completion for every activity in the unit, not just the
      // current one, since the whole unit was re-materialized.
      this.markUnitIncomplete(this.activeCourse.id, unit);
      await this.saveProgress();
      this._onDidChangeState.fire(this.getState());
      if (source) {
        this.sendActivityActionTelemetry("reset", source);
      }
      return;
    }

    const exercise = this.resolveExercise();
    const uri = this.getExerciseFileUri();
    // Save any unsaved edits first so the editor is clean, then overwrite
    // the file on disk. The editor will pick up the change automatically
    // because it's no longer dirty.
    await this.saveOpenDocument(uri);
    await vscode.workspace.fs.writeFile(
      uri,
      new TextEncoder().encode(exercise.placeholderCode),
    );
    this.markIncomplete(this.requireWorkspace().progressData.position);
    await this.saveProgress();
    this._onDidChangeState.fire(this.getState());
    if (source) {
      this.sendActivityActionTelemetry("reset", source);
    }
  }

  async run(
    shots: number = 1,
    source?: TelemetrySource,
  ): Promise<{ result: RunResult; state: LearningState }> {
    const { activity } = this.findCurrentActivity();
    if (activity.type === "exercise") {
      throw new Error("Exercises cannot be run. Use checkSolution() instead.");
    }

    if (activity.type === "lesson" && activity.example) {
      await this.markExampleRun();
    }

    if (source) {
      this.sendActivityActionTelemetry("run", source);
    }

    // Python-notebook courses use native VS Code notebook execution.
    if (this.activeCourse.kind === "python-notebook") {
      return {
        result: {
          success: false,
          messages: [],
          error:
            "This course uses native notebook execution. " +
            "Run cells directly in the notebook.",
        },
        state: this.getState(),
      };
    }

    const fileUri = this.getCurrentCodeFileUri();
    if (!fileUri) {
      throw new Error("Current activity cannot be run.");
    }

    const doc = await vscode.workspace.openTextDocument(fileUri);
    const programResult = await getProgramForDocument(doc);
    if (!programResult.success) {
      return {
        result: { success: false, messages: [], error: programResult.errorMsg },
        state: this.getState(),
      };
    }

    const result = await this.executeProgram(programResult.programConfig, {
      shots,
    });
    return { result, state: this.getState() };
  }

  async checkSolution(source?: TelemetrySource): Promise<{
    result: SolutionCheckResult;
    state: LearningState;
  }> {
    const { activity } = this.findCurrentActivity();
    if (activity.type !== "exercise") {
      throw new Error("Current activity is not an exercise.");
    }

    if (source) {
      this.sendActivityActionTelemetry("check", source);
    }

    // Python-notebook courses verify in the notebook itself: running an
    // exercise cell runs its checker, and the extension records completion
    // from the cell's execution result rather than through this method.
    if (this.activeCourse.kind === "python-notebook") {
      return {
        result: {
          passed: false,
          messages: [],
          error:
            "This course uses native notebook execution. " +
            "Run the exercise cell in the notebook — each cell that " +
            "succeeds marks that exercise complete.",
        },
        state: this.getState(),
      };
    }

    const exercise = this.resolveExercise();
    const userCode = await this.readUserCode();
    // Drop-in courses carry their own verification sources inline; the
    // built-in katas resolve them from the bundled content by `sourceIds`.
    const exerciseSources = await getExerciseSources(
      // CatalogExercise is structurally incompatible with Exercise (different
      // description/solution shapes), but getExerciseSources only reads sourceIds.
      exercise as any,
    );

    // Build a synthetic program config combining the user's solution
    // with the exercise verification sources from the katas bundle.
    const programConfig: FullProgramConfig = {
      projectName: "exercise-check",
      projectUri: "",
      packageGraphSources: {
        root: {
          sources: [
            ["solution", userCode],
            ...exerciseSources.map(
              (code, i) => [String(i), code] as [string, string],
            ),
          ],
          languageFeatures: [],
          dependencies: {},
          packageType: "exe",
        },
        packages: {},
        hasManifest: false,
      },
      lints: [],
      errors: [],
      projectType: "qsharp",
      profile: "unrestricted",
    };

    const execResult = await this.executeProgram(programConfig, {
      entry: "Kata.Verification.CheckSolution()",
      suppressResultOutput: true,
    });

    const passed = execResult.success && execResult.result === "true";

    if (passed) {
      await this.markExerciseComplete(
        this.requireWorkspace().progressData.position,
      );
    }

    return {
      result: {
        passed,
        messages: execResult.messages,
        error: passed
          ? undefined
          : (execResult.error ??
            (execResult.messages.length === 0
              ? "Solution check failed."
              : undefined)),
      },
      state: this.getState(),
    };
  }

  sendActivityActionTelemetry(
    action: "navigate" | "run" | "check" | "hint" | "solution" | "reset",
    source: TelemetrySource,
  ): void {
    const activityType =
      this.findCurrentActivity().activity.type === "exercise"
        ? "exercise"
        : "lesson";
    sendTelemetryEvent(
      EventType.LearningActivityAction,
      { action, activityType, source },
      {},
    );
  }

  // ─── Private: execution ───

  private async executeProgram(
    programConfig: FullProgramConfig,
    options?: {
      entry?: string;
      shots?: number;
      suppressResultOutput?: boolean;
    },
  ): Promise<RunResult> {
    const messages: string[] = [];

    try {
      const runResult = await runProgram(this.extensionUri, programConfig, {
        entry: options?.entry,
        shots: options?.shots ?? 1,
        suppressResultOutput: options?.suppressResultOutput,
        onConsoleOut: (msg) => {
          messages.push(msg);
        },
      });

      if (runResult.status === ProgramRunStatus.CompilationErrors) {
        return {
          success: false,
          messages,
          error:
            runResult.errors
              .map((e) => e.diagnostic?.message ?? String(e))
              .join("\n") || "Compilation failed.",
        };
      }

      const success = runResult.status === ProgramRunStatus.AllShotsDone;
      let result: string | undefined;
      if (success) {
        const shot = runResult.shotResults.at(-1);
        if (shot && !Array.isArray(shot) && shot.success) {
          result = shot.result;
        }
      }

      return {
        success,
        messages,
        result,
        error: success
          ? undefined
          : `Program ended with status: ${runResult.status}.`,
      };
    } catch (err: unknown) {
      return {
        success: false,
        messages: [],
        error: err instanceof Error ? err.message : String(err),
      };
    }
  }

  // ─── Private: initialization ───

  private async detectAndLoadWorkspace(options?: {
    createIfMissing?: boolean;
  }): Promise<boolean> {
    const detected = await detectLearningWorkspace();

    if (detected) {
      await this.loadWorkspace(
        detected.workspaceRoot,
        detected.learningContentRoot,
      );
      this.startWatcher();
      // TODO (acasey): make sure we're firing this an appropriate number of times
      sendTelemetryEvent(
        EventType.LearningSessionStarted,
        { isFirstTime: "false" },
        {},
      );
      // Surfaces registered before initialization (notebook cell status bar
      // items, the lesson panel) need a nudge to re-query now that there is
      // state to read.
      this._onDidChangeState.fire(this.getState());
      return true;
    }

    if (!options?.createIfMissing) {
      return false;
    }

    // No existing workspace — bootstrap in the first open folder.
    const workspaceRoot = resolveNewWorkspaceRoot();
    if (!workspaceRoot) {
      return false;
    }
    const katasRoot = vscode.Uri.joinPath(
      workspaceRoot,
      LEARNING_WORKSPACE_FOLDER,
    );

    await this.loadWorkspace(workspaceRoot, katasRoot);
    this._writingProgress = true;
    try {
      await this.saveProgress();
    } finally {
      this._writingProgress = false;
    }
    this.startWatcher();
    sendTelemetryEvent(
      EventType.LearningSessionStarted,
      { isFirstTime: "true" },
      {},
    );
    this._onDidChangeState.fire(this.getState());
    return true;
  }

  private async loadWorkspace(
    workspaceRoot: vscode.Uri,
    katasRoot: vscode.Uri,
  ): Promise<void> {
    const learningFile = vscode.Uri.joinPath(workspaceRoot, LEARNING_FILE);

    const registry = createCourseRegistry(workspaceRoot);

    // Eagerly load all available courses so that the saved position
    // (which may reference a drop-in course) resolves correctly.
    const courses = new Map<string, CatalogCourse>();
    const descriptors = await registry.listCourses();
    for (const descriptor of descriptors) {
      try {
        // TODO (acasey): parsing all courses seems fine, but we probably only want to materialize the active one
        // TODO (acasey): this shouldn't redo discovery for each course
        const course = await registry.loadCourse(descriptor.id);
        courses.set(course.id, course);
      } catch {
        // Skip courses that fail to load.
      }
    }

    const defaultCourse =
      courses.get(KATAS_COURSE_ID) ?? courses.values().next().value;

    // Build workspace state; assigned to this.workspace only after all
    // async setup succeeds so that `initialized` stays false on failure.
    const ws: WorkspaceState = {
      workspaceRoot,
      learningContentRoot: katasRoot,
      learningFile,
      courses,
      registry,
      progressData: {
        version: 1,
        position: {
          courseId: defaultCourse?.id ?? "",
          unitId: defaultCourse?.units[0]?.id ?? "",
          activityId: defaultCourse?.units[0]?.activities[0]?.id ?? "",
        },
        completions: {},
        startedAt: new Date().toISOString(),
      },
    };

    await this.loadProgress(ws);

    // Publish the workspace before scaffolding so that methods relying on
    // `requireWorkspace()` can resolve.
    this.workspace = ws;
    this.syncContextKey();

    for (const course of courses.values()) {
      try {
        await this.scaffoldCourse(ws, course);
      } catch {
        // A failing scaffold should not block workspace initialization.
        // TODO (acasey): log
      }
    }
  }

  private requireWorkspace(): WorkspaceState {
    if (!this.workspace) {
      throw new Error(
        "No active learning workspace. Call tryInitialize() before using this method.",
      );
    }
    return this.workspace;
  }

  /** The currently-active course, resolved from the progress position. */
  private get activeCourse(): CatalogCourse {
    const ws = this.requireWorkspace();
    return this.requireCourse(ws, ws.progressData.position.courseId);
  }

  private requireCourse(ws: WorkspaceState, courseId: string): CatalogCourse {
    const course = ws.courses.get(courseId);
    if (!course) {
      throw new Error(`Course not loaded: ${courseId}`);
    }
    return course;
  }

  private syncContextKey(): void {
    void vscode.commands.executeCommand(
      "setContext",
      LEARNING_WORKSPACE_DETECTED_CONTEXT,
      this.workspace !== undefined,
    );
  }

  /** The default action: "check" or "run" if incomplete, "next" once done. */
  private getPrimaryAction(): PrimaryAction {
    const { activity } = this.findCurrentActivity();
    if (activity.type === "exercise") {
      return this.isComplete(this.position) ? "next" : "check";
    }
    if (activity.type === "lesson" && activity.example) {
      return this.isComplete(this.position) ? "next" : "run";
    }
    return "next";
  }

  /** Builds the button groups shown in the webview toolbar for the current activity. */
  private getAvailableActions(): ActionGroup[] {
    const { activity } = this.findCurrentActivity();

    const primary = this.getPrimaryAction();

    const primaryLabel: Record<PrimaryAction, string> = {
      next: "Next",
      run: "Run",
      check: "Check",
    };

    const primaryGroup: ActionGroup = [
      {
        key: "space",
        label: primaryLabel[primary],
        action: primary,
        primary: true,
      },
    ];

    const navGroup: ActionGroup = [{ key: "b", label: "Back", action: "back" }];

    if (activity.type === "exercise") {
      // When completed, keep Check available so users can re-validate.
      // When incomplete, offer a Hint button instead.
      const isComplete = this.isComplete(this.position);
      const extraGroups: ActionGroup[] = isComplete
        ? [
            [
              { key: "c", label: "Check", action: "check" },
              { key: "r", label: "Reset", action: "reset" },
            ],
          ]
        : [
            [
              {
                key: "h",
                label: "Hint",
                action: "hint-chat",
                codicon: "sparkle",
              },
              { key: "r", label: "Reset", action: "reset" },
            ],
          ];
      return [primaryGroup, ...extraGroups, navGroup].filter(
        (g) => g.length > 0,
      );
    }

    // Lesson (text or example)
    const codeTools: ActionGroup =
      activity.example && primary !== "run"
        ? [{ key: "r", label: "Run", action: "run" }]
        : [];
    const aiGroup: ActionGroup = [
      {
        key: "e",
        label: "Explain",
        action: "explain-chat",
        codicon: "sparkle",
      },
    ];
    return [primaryGroup, codeTools, aiGroup, navGroup].filter(
      (g) => g.length > 0,
    );
  }

  /**
   * Close every open text or notebook tab whose URI matches {@link predicate}.
   * Tabs backed by any other input kind (diff views, webviews, terminals) are
   * skipped, since they have no single URI to match against.
   */
  private async closeTabs(
    predicate: (uri: vscode.Uri, tab: vscode.Tab) => boolean,
  ): Promise<void> {
    const matches: vscode.Tab[] = [];
    for (const group of vscode.window.tabGroups.all) {
      for (const tab of group.tabs) {
        const input = tab.input;
        const tabUri =
          input instanceof vscode.TabInputText ||
          input instanceof vscode.TabInputNotebook
            ? input.uri
            : undefined;
        if (tabUri && predicate(tabUri, tab)) {
          matches.push(tab);
        }
      }
    }
    if (matches.length > 0) {
      await vscode.window.tabGroups.close(matches);
    }
  }

  /**
   * Close any open editor or notebook tabs under the QDK Learning root that
   * don't match {@link keepUri}. When {@link keepUri} is undefined, all such
   * tabs are closed.
   */
  async closeStaleEditorTabs(keepUri: vscode.Uri | undefined): Promise<void> {
    if (!this.workspace) {
      return;
    }
    const learningRoot = this.learningContentRoot.toString();
    const keepStr = keepUri?.toString();

    await this.closeTabs((uri) => {
      const uriStr = uri.toString();
      return uriStr.startsWith(learningRoot) && uriStr !== keepStr;
    });
  }

  /** Turns a catalog activity into the typed content payload (exercise, lesson-example, or lesson-text). */
  private resolveActivityContent(
    location: ActivityLocation,
    kata: CatalogUnit,
    activity: CatalogActivity,
  ): ActivityContent {
    const ws = this.requireWorkspace();

    if (activity.type === "exercise") {
      // Python-notebook exercises live in the notebook — show their
      // description as lesson text so the panel renders something useful.
      if (this.activeCourse.kind === "python-notebook") {
        return {
          type: "lesson-text",
          content: activity.description,
        } satisfies LessonTextContent;
      }

      const fileUri = vscode.Uri.joinPath(
        ws.learningContentRoot,
        "exercises",
        kata.id,
        `${activity.id}.qs`,
      );
      return {
        type: "exercise",
        id: activity.id,
        title: activity.title,
        description: activity.description,
        filePath: fileUri.toString(),
        isComplete: this.isComplete(location),
        hasMultipleSolutions: activity.solutionCodes.length > 1,
      } satisfies ExerciseContent;
    }

    // Lesson with a code example
    if (activity.example) {
      const fileUri = vscode.Uri.joinPath(
        ws.learningContentRoot,
        "examples",
        kata.id,
        `${activity.example.id}.qs`,
      );
      return {
        type: "lesson-example",
        id: activity.example.id,
        code: activity.example.code,
        filePath: fileUri.toString(),
        contentBefore: activity.contentBefore,
        contentAfter: activity.contentAfter,
      } satisfies LessonExampleContent;
    }

    // Text-only lesson
    return {
      type: "lesson-text",
      content: activity.content ?? "",
    } satisfies LessonTextContent;
  }

  /** Working-copy (`*.workbook.ipynb`) URI of a notebook for the active python-notebook course. */
  private notebookFileUri(notebookRel: string): vscode.Uri {
    return workbookFileUri(this.activeCourse, notebookRel);
  }

  private findCurrentActivity(): {
    unit: CatalogUnit;
    activity: CatalogActivity;
  } {
    const pos = this.position;
    const unit = this.findUnit(pos.unitId);
    const activity = unit.activities.find((s) => s.id === pos.activityId);
    if (!activity) {
      throw new Error(`Activity not found: ${pos.activityId}`);
    }
    return { unit, activity };
  }

  private resolveExercise(): CatalogExercise {
    const { activity } = this.findCurrentActivity();
    if (activity.type !== "exercise") {
      throw new Error("Current activity is not an exercise");
    }
    return activity;
  }

  /** Returns the next activity in catalog order, or `undefined` at the end. */
  private nextActivity(
    location: ActivityLocation,
  ): ActivityLocation | undefined {
    const course = this.activeCourse;
    let found = false;
    for (const unit of course.units) {
      for (const a of unit.activities) {
        if (found) {
          return {
            courseId: course.id,
            unitId: unit.id,
            activityId: a.id,
          };
        }
        if (unit.id === location.unitId && a.id === location.activityId) {
          found = true;
        }
      }
    }
    return undefined;
  }

  /** Returns the previous activity in catalog order, or `undefined` at the start. */
  private previousActivity(
    location: ActivityLocation,
  ): ActivityLocation | undefined {
    const course = this.activeCourse;
    let prev: ActivityLocation | undefined;
    for (const unit of course.units) {
      for (const a of unit.activities) {
        if (unit.id === location.unitId && a.id === location.activityId) {
          return prev;
        }
        prev = {
          courseId: course.id,
          unitId: unit.id,
          activityId: a.id,
        };
      }
    }
    return undefined;
  }

  private findUnit(unitId: string): CatalogUnit {
    const kata = this.activeCourse.units.find((k) => k.id === unitId);
    if (!kata) {
      throw new Error(`Unit not found: ${unitId}`);
    }
    return kata;
  }

  private async markExerciseComplete(
    location: ActivityLocation,
  ): Promise<void> {
    this.markComplete(location);
    await this.saveProgress();
    this._onDidChangeState.fire(this.getState());

    // TODO (acasey): do we actually want telemetry for other courses?
    // We need to either drop it so that all telemetry is about the katas
    // or introduce a new property to distinguish kata telemetry from python telemetry.
    // We may want to have an allow-list of known python courses and record others
    // as "other" (unless one-way hashing is allowed).
    const units = this.activeCourse.units;
    const unitIndex = units.findIndex((u) => u.id === location.unitId);
    const unit = unitIndex >= 0 ? units[unitIndex] : undefined;
    const exercises =
      unit?.activities.filter((s) => s.type === "exercise") ?? [];
    const exerciseIndex = exercises.findIndex(
      (e) => e.id === location.activityId,
    );
    sendTelemetryEvent(
      EventType.LearningExerciseCompleted,
      {},
      {
        unitNumber: unitIndex + 1,
        exerciseNumber: exerciseIndex + 1,
        totalExercises: exercises.length,
      },
    );
  }

  private async loadProgress(ws: WorkspaceState): Promise<void> {
    try {
      const bytes = await vscode.workspace.fs.readFile(ws.learningFile);
      const parsed = JSON.parse(new TextDecoder().decode(bytes));
      if (
        parsed &&
        typeof parsed === "object" &&
        parsed.version === 1 &&
        typeof parsed.completions === "object" &&
        parsed.completions !== null &&
        typeof parsed.position === "object" &&
        parsed.position !== null
      ) {
        ws.progressData = parsed as ProgressFileData;
        // Resolve the course the saved position points at, falling back to
        // the default loaded course if it references one not yet loaded.
        const course =
          ws.courses.get(ws.progressData.position.courseId) ??
          this.defaultCourseOf(ws);
        // Validate saved position references a known unit and activity
        if (course && course.units.length > 0) {
          const unit =
            ws.progressData.position.courseId === course.id
              ? course.units.find(
                  (k) => k.id === ws.progressData.position.unitId,
                )
              : undefined;
          const activityValid =
            unit &&
            unit.activities.some(
              (s) => s.id === ws.progressData.position.activityId,
            );
          if (!activityValid) {
            ws.progressData.position = {
              courseId: course.id,
              unitId: course.units[0].id,
              activityId: course.units[0].activities[0]?.id ?? "",
            };
          }
        }
        return;
      }
    } catch {
      // expected when file is missing or corrupt
    }
    const course = this.defaultCourseOf(ws);
    // TODO (acasey): is this identical to what was passed in?
    ws.progressData = {
      version: 1,
      position: {
        courseId: course?.id ?? "",
        unitId: course?.units[0]?.id ?? "",
        activityId: course?.units[0]?.activities[0]?.id ?? "",
      },
      completions: {},
      startedAt: new Date().toISOString(),
    };
  }

  /** The default course for a workspace (built-in katas, else the first loaded). */
  private defaultCourseOf(ws: WorkspaceState): CatalogCourse | undefined {
    return ws.courses.get(KATAS_COURSE_ID) ?? ws.courses.values().next().value;
  }

  private async saveProgress(): Promise<void> {
    const ws = this.requireWorkspace();
    const json = JSON.stringify(ws.progressData, null, 2);
    this._writingProgress = true;
    try {
      await vscode.workspace.fs.writeFile(
        ws.learningFile,
        new TextEncoder().encode(json),
      );
    } finally {
      this._writingProgress = false;
    }
    this.emitProgress();
  }

  async reloadProgress(): Promise<void> {
    const ws = this.requireWorkspace();
    await this.loadProgress(ws);
    this.emitProgress();
    this._onDidChangeState.fire(this.getState());
  }

  private completionKey(location: ActivityLocation): string {
    return `${location.courseId}__${location.unitId}__${location.activityId}`;
  }

  private findCompletion(
    location: ActivityLocation,
  ): { completedAt: string } | undefined {
    return this.requireWorkspace().progressData.completions[
      this.completionKey(location)
    ];
  }

  private isComplete(location: ActivityLocation): boolean {
    return this.findCompletion(location) != null;
  }

  private markComplete(location: ActivityLocation): void {
    const key = this.completionKey(location);
    const completions = this.requireWorkspace().progressData.completions;
    if (!(key in completions)) {
      completions[key] = {
        completedAt: new Date().toISOString(),
      };
    }
  }

  private markIncomplete(location: ActivityLocation): void {
    const key = this.completionKey(location);
    delete this.requireWorkspace().progressData.completions[key];
  }

  /** Clear completion for every activity in the given unit. */
  private markUnitIncomplete(courseId: string, unit: CatalogUnit): void {
    for (const activity of unit.activities) {
      this.markIncomplete({
        courseId,
        unitId: unit.id,
        activityId: activity.id,
      });
    }
  }

  private startWatcher(): void {
    if (this._progressFileWatcher) {
      return;
    }

    const ws = this.requireWorkspace();
    const pattern = new vscode.RelativePattern(ws.workspaceRoot, LEARNING_FILE);
    this._progressFileWatcher =
      vscode.workspace.createFileSystemWatcher(pattern);

    const onDelete = () => {
      if (this._writingProgress) {
        return;
      }
      // File removed externally — tear down all workspace state.
      this.workspace = undefined;
      this.syncContextKey();
      this._onDidChangeProgress.fire(undefined);
    };

    this._progressFileWatcher.onDidCreate(() => {
      if (!this.workspace) {
        void this.tryInitialize();
      }
    });
    this._progressFileWatcher.onDidDelete(onDelete);

    this.emitProgress();
  }

  private emitProgress(): void {
    if (!this.workspace) {
      this._onDidChangeProgress.fire(undefined);
      return;
    }
    this._onDidChangeProgress.fire(this.getProgress());
  }

  /**
   * Close any open editor tabs whose URI matches the given notebook URI.
   */
  private async closeNotebookTab(uri: vscode.Uri): Promise<void> {
    const uriStr = uri.toString();
    await this.closeTabs(
      (tabUri, tab) =>
        tab.input instanceof vscode.TabInputNotebook &&
        tabUri.toString() === uriStr,
    );
  }

  /**
   * Materialize the editable files (exercise placeholders and example code)
   * for a Q# course into the learning content folder. No-op for non-qsharp
   * courses (those are scaffolded by their own runtime).
   */
  private async scaffoldCourse(
    ws: WorkspaceState,
    course: CatalogCourse,
  ): Promise<void> {
    if (course.kind === "python-notebook") {
      // Copy the course's notebooks into the workspace working copy so the
      // learner edits a stable location, then surface any missing tooling.
      await materializeCourseWorkbooks(course);
      return;
    }
    if (course.kind !== "qsharp") {
      return;
    }
    for (const kata of course.units) {
      for (const activity of kata.activities) {
        if (activity.type === "exercise") {
          const fileUri = vscode.Uri.joinPath(
            ws.learningContentRoot,
            "exercises",
            kata.id,
            `${activity.id}.qs`,
          );
          if (await uriExists(fileUri)) {
            continue;
          }
          await ensureParentDir(fileUri);
          await vscode.workspace.fs.writeFile(
            fileUri,
            new TextEncoder().encode(activity.placeholderCode),
          );
        } else if (activity.type === "lesson" && activity.example) {
          const fileUri = vscode.Uri.joinPath(
            ws.learningContentRoot,
            "examples",
            kata.id,
            `${activity.example.id}.qs`,
          );
          await ensureParentDir(fileUri);
          await vscode.workspace.fs.writeFile(
            fileUri,
            new TextEncoder().encode(activity.example.code),
          );
        }
      }
    }
  }
}
