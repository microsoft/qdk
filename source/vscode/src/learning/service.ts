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
import { PythonCourseRunner } from "./python/pythonRunner.js";
import {
  KATAS_COURSE_ID,
  LEARNING_FILE,
  LEARNING_WORKSPACE_DETECTED_CONTEXT,
  LEARNING_WORKSPACE_FOLDER,
  LEARNING_WORKSPACE_RELATIVE_PATH,
} from "./constants.js";
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
  CourseKind,
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

  private _lastSnapshot: OverallProgress | undefined;
  private _progressFileWatcher: vscode.FileSystemWatcher | undefined;
  private _sentinelWatcher: vscode.FileSystemWatcher | undefined;
  private _writingProgress = false;
  private _initPromise: Promise<boolean> | undefined;
  private readonly _disposables: vscode.Disposable[] = [];
  private _pythonRunner: PythonCourseRunner | undefined;
  private _environment: EnvironmentManager | undefined;

  constructor(private readonly extensionUri: vscode.Uri) {}

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

  /** Lazily-created runner for `python-notebook` courses. */
  private get pythonRunner(): PythonCourseRunner {
    if (!this._pythonRunner) {
      this._pythonRunner = new PythonCourseRunner();
    }
    return this._pythonRunner;
  }

  /** Lazily-created per-course Python environment manager. */
  private get environment(): EnvironmentManager {
    if (!this._environment) {
      this._environment = new EnvironmentManager();
    }
    return this._environment;
  }

  /**
   * Re-scan available courses (e.g. after a new drop-in course is added).
   * Drop-in courses are enumerated lazily by the registry, so this just
   * refreshes the UI to pick up newly-added folders.
   */
  async reloadCourses(): Promise<void> {
    if (!this.workspace) {
      return;
    }
    this.emitProgress();
    this._onDidChangeState.fire(this.getState());
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
    this.stopSentinelWatcher();
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

  /**
   * State snapshot tailored for the lesson webview panel.
   *
   * For python-notebook courses the panel always shows the unit-level
   * summary (intro lesson) rather than drilling into a specific exercise.
   * Other course kinds fall through to {@link getState}.
   */
  getStateForPanel(): LearningState {
    if (this.activeCourse.kind !== "python-notebook") {
      return this.getState();
    }

    const pos = this.position;
    const unit = this.findUnit(pos.unitId);
    const intro = unit.activities.find((a) => a.id === "intro")!;

    const introLocation: ActivityLocation = {
      courseId: pos.courseId,
      unitId: pos.unitId,
      activityId: intro.id,
    };

    const position: CurrentActivity = {
      location: introLocation,
      unitTitle: unit.title,
      activityTitle: unit.title,
      content: this.resolveActivityContent(introLocation, unit, intro),
    };

    return {
      course: this.getActiveCourseInfo(),
      position,
      actions: this.getAvailableActionsForPanel(unit),
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

  /**
   * Navigate to the intro of the next unit. Used by the panel for
   * python-notebook courses where navigation is unit-scoped.
   */
  async nextUnit(source: TelemetrySource): Promise<NavigationResult> {
    const ws = this.requireWorkspace();
    const course = this.activeCourse;
    const currentUnitId = ws.progressData.position.unitId;
    const idx = course.units.findIndex((u) => u.id === currentUnitId);
    if (idx < 0 || idx >= course.units.length - 1) {
      return { moved: false };
    }
    const nextU = course.units[idx + 1];
    const firstActivity = nextU.activities[0];
    if (!firstActivity) {
      return { moved: false };
    }

    // Auto-mark the intro lesson of the current unit complete.
    const introLocation: ActivityLocation = {
      courseId: course.id,
      unitId: currentUnitId,
      activityId: "intro",
    };
    if (!this.isComplete(introLocation)) {
      this.markComplete(introLocation);
    }

    ws.progressData.position = {
      courseId: course.id,
      unitId: nextU.id,
      activityId: firstActivity.id,
    };
    await this.saveProgress();
    this._onDidChangeState.fire(this.getState());
    this.sendActivityActionTelemetry("navigate", source);
    return { moved: true };
  }

  /**
   * Navigate to the intro of the previous unit. Used by the panel for
   * python-notebook courses where navigation is unit-scoped.
   */
  async previousUnit(source: TelemetrySource): Promise<NavigationResult> {
    const ws = this.requireWorkspace();
    const course = this.activeCourse;
    const currentUnitId = ws.progressData.position.unitId;
    const idx = course.units.findIndex((u) => u.id === currentUnitId);
    if (idx <= 0) {
      return { moved: false };
    }
    const prevU = course.units[idx - 1];
    const firstActivity = prevU.activities[0];
    if (!firstActivity) {
      return { moved: false };
    }

    ws.progressData.position = {
      courseId: course.id,
      unitId: prevU.id,
      activityId: firstActivity.id,
    };
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
   * Navigate to the exercise activity whose `cellIndex` matches the given
   * 1-based cell number. Returns `true` if the position was updated.
   * Only meaningful for python-notebook courses.
   *
   * Updates the position silently — does **not** fire the state-change
   * event, so the lesson panel won't pop up or rearrange the editor layout.
   */
  async goToExerciseByCellIndex(
    cellIndex: number,
    source?: TelemetrySource,
  ): Promise<boolean> {
    if (this.activeCourse.kind !== "python-notebook") {
      return false;
    }
    const unit = this.findUnit(this.position.unitId);
    const exercise = unit.notebookExercises?.find(
      (e) => e.cellIndex === cellIndex,
    );
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
   * Mark the exercise activity at the given 1-based cell index as complete.
   * Returns `true` if the exercise was found and marked (or already complete).
   * Fires the state-change event so the treeview updates.
   */
  async markExerciseCompleteByCellIndex(cellIndex: number): Promise<boolean> {
    if (this.activeCourse.kind !== "python-notebook") {
      return false;
    }
    const unit = this.findUnit(this.position.unitId);
    const exercise = unit.notebookExercises?.find(
      (e) => e.cellIndex === cellIndex,
    );
    if (!exercise) {
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

  /** Enumerate all available courses (loaded or not). */
  async getCourses(): Promise<CourseDescriptor[]> {
    return this.requireWorkspace().registry.listCourses();
  }

  /** The id of the currently-active course. */
  getActiveCourseId(): string {
    return this.requireWorkspace().progressData.position.courseId;
  }

  /** Compact info about the active course for serialization to chat tools. */
  getActiveCourseInfo(): { id: string; title: string; kind: CourseKind } {
    const course = this.activeCourse;
    return { id: course.id, title: course.title, kind: course.kind };
  }

  /**
   * Ensure a python-notebook course's per-course environment exists:
   * create the venv and install pinned requirements. No-ops for Q# courses,
   * on the Web, or when the venv already exists (unless `force` is set).
   *
   * When the course ships a `pyproject.toml`, the preferred path is
   * `uv sync` which handles venv creation, Python version selection, and
   * dependency installation in one shot. Courses without `pyproject.toml`
   * fall back to the manual `createVenv` + `installRequirements` flow.
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
    if (!options?.force && (await env.venvExists(courseRoot))) {
      return;
    }
    await vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: `Setting up the environment for "${course.title}"…`,
      },
      async () => {
        const hasPyproject = await this.uriExists(
          vscode.Uri.joinPath(courseRoot, "pyproject.toml"),
        );
        if (hasPyproject) {
          // Preferred: `uv sync` resolves and installs from pyproject.toml.
          await env.syncEnvironment(courseRoot);
        } else {
          // Fallback: manual venv creation + pip install.
          await env.createVenv(courseRoot, course.environment?.python);
          await env.installRequirements(
            courseRoot,
            course.environment?.requirements ?? [],
          );
        }
      },
    );
  }

  /** Set up the environment for the currently-active course. */
  async setupActiveEnvironment(): Promise<void> {
    await this.ensureEnvironment(this.activeCourse, { force: true });
  }

  /**
   * Apply a fix surfaced by {@link runEnvironmentCheck}. Centralizes the
   * mapping from an {@link EnvironmentCheckFix.kind} to a concrete action so
   * the command and chat tool can offer fixes without duplicating the logic.
   */
  async applyEnvironmentCheckFix(fix: EnvironmentCheckFix): Promise<void> {
    switch (fix.kind) {
      case "setup":
        await this.setupActiveEnvironment();
        return;
      case "install-extensions":
        await this.pythonRunner.promptInstallExtensions();
        return;
      case "select-kernel":
        await vscode.commands.executeCommand("notebook.selectKernel");
        return;
      case "docs":
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
    log.info(
      `[env-check] Starting for course "${course.id}" (kind=${course.kind})`,
    );

    if (course.kind !== "python-notebook") {
      log.info(`[env-check] Q# course — skipping environment checks.`);
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
      log.info(`[env-check] Web host — environment management unavailable.`);
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
      log.info(`[env-check] No sourceDir — cannot resolve course root.`);
      return this.assembleReport(course, [
        check("course-folder", "Course folder", "fail", {
          detail: "This course has no source folder on disk.",
        }),
      ]);
    }
    const courseRoot = vscode.Uri.parse(course.sourceDir);
    log.info(`[env-check] Course root: ${courseRoot.fsPath}`);

    const checks: EnvironmentCheckItem[] = [];

    // 1. Required extensions (Python + Jupyter).
    log.info(`[env-check] Checking extensions…`);
    const extMessage = await this.pythonRunner.ensureExtensions();
    log.info(`[env-check] Extensions: ${extMessage ?? "ok"}`);
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

    // 2. Base Python interpreter (for bootstrapping the venv).
    log.info(`[env-check] Checking interpreter…`);
    const interpreter = await env.ensureInterpreter();
    log.info(`[env-check] Interpreter: ${interpreter ?? "not found"}`);
    checks.push(
      check("interpreter", "Python interpreter", interpreter ? "ok" : "fail", {
        detail: interpreter ?? "No interpreter found.",
        hint: interpreter
          ? undefined
          : "Install Python (3.9+) and select an interpreter via the Python extension.",
      }),
    );

    // 3. Tooling: uv (preferred) vs stdlib venv. Informational unless the
    //    venv is missing AND the stdlib module is unavailable.
    log.info(`[env-check] Checking for uv…`);
    const hasUv = await env.hasUv();
    log.info(`[env-check] uv available: ${hasUv}`);
    log.info(`[env-check] Checking venv existence…`);
    const venvOk = await env.venvExists(courseRoot);
    log.info(`[env-check] Venv exists: ${venvOk}`);
    if (hasUv) {
      checks.push(
        check("tooling", "Environment tooling", "ok", {
          detail: "uv detected — fast environment creation.",
        }),
      );
    } else if (interpreter) {
      // Only probe the stdlib venv module when we'd actually need it.
      log.info(`[env-check] Probing stdlib venv module…`);
      const venvModuleOk = venvOk
        ? true
        : await env.venvModuleSupported(interpreter);
      log.info(`[env-check] venv module supported: ${venvModuleOk}`);
      checks.push(
        check(
          "tooling",
          "Environment tooling",
          venvModuleOk ? "warn" : "fail",
          {
            detail: venvModuleOk
              ? "Using the standard-library `venv` (install `uv` for faster setup)."
              : "The `venv`/`ensurepip` modules are missing from this Python.",
            hint: venvModuleOk
              ? undefined
              : "On Debian/Ubuntu install them with `sudo apt install python3-venv` " +
                "(matching your Python version, e.g. `python3.12-venv`).",
          },
        ),
      );
    }

    // 4. The per-course virtual environment.
    checks.push(
      check("venv", "Course virtual environment", venvOk ? "ok" : "fail", {
        detail: env.venvUri(courseRoot).fsPath,
        hint: venvOk
          ? undefined
          : "Run environment setup to create the course virtual environment.",
        fixes: venvOk
          ? undefined
          : [{ label: "Set up environment", kind: "setup" }],
      }),
    );

    log.info(`[env-check] Checking venv interpreter…`);
    const venvPython = venvOk ? await env.venvPython(courseRoot) : undefined;
    log.info(`[env-check] Venv interpreter: ${venvPython ?? "n/a"}`);
    checks.push(
      check(
        "venv-interpreter",
        "Environment interpreter",
        !venvOk ? "skip" : venvPython ? "ok" : "fail",
        {
          detail: !venvOk
            ? "No environment yet."
            : (venvPython ?? "The venv exists but has no interpreter."),
          hint:
            venvOk && !venvPython
              ? "The environment looks corrupt; re-run setup to recreate it."
              : undefined,
          fixes:
            venvOk && !venvPython
              ? [{ label: "Set up environment", kind: "setup" }]
              : undefined,
        },
      ),
    );

    // 5. Required packages import in the venv.
    if (venvPython) {
      // TODO (acasey): are these supposed to come from the course metadata or are these just a baseline for all courses?
      log.info(`[env-check] Checking package imports…`);
      const report = await env.importsReport(courseRoot, [
        "qdk",
        "qsharp_widgets",
      ]);
      const missing = report.filter((r) => !r.ok).map((r) => r.module);
      log.info(
        `[env-check] Import results: ${report.map((r) => `${r.module}=${r.ok ? "ok" : "fail"}`).join(", ")}`,
      );
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
    } else {
      log.info(`[env-check] Skipping package imports — no venv interpreter.`);
      checks.push(
        check("packages", "Required packages", "skip", {
          detail: "No environment yet.",
        }),
      );
    }

    log.info(`[env-check] Assembling report (${checks.length} checks).`);
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
    }
    if (course.kind === "python-notebook") {
      void this.pythonRunner.promptInstallExtensions();
      void this.ensureEnvironment(course);
    }
    ws.progressData.position = this.firstIncompletePosition(course);
    await this.saveProgress();
    this.startSentinelWatcher();
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
      await this.pythonRunner.rematerializeUnit(this.activeCourse, unit.id);
      // Delete the sentinel file if present.
      if (unit.notebookRel) {
        const workingCopyUri = this.notebookFileUri(unit.notebookRel);
        const sentinelUri = vscode.Uri.joinPath(
          workingCopyUri,
          "..",
          ".qdk-unit-complete",
        );
        try {
          await vscode.workspace.fs.delete(sentinelUri);
        } catch {
          // may not exist
        }
      }
      this.markIncomplete(this.requireWorkspace().progressData.position);
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

    // Python-notebook courses use in-notebook verification via
    // complete_unit(). The extension detects completion via the sentinel
    // file watcher, not through this method.
    if (this.activeCourse.kind === "python-notebook") {
      return {
        result: {
          passed: false,
          messages: [],
          error:
            "This course uses native notebook execution. " +
            "Run all cells in the notebook, including the final " +
            "complete_unit() cell, to mark the unit complete.",
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
      this.startSentinelWatcher();
      sendTelemetryEvent(
        EventType.LearningSessionStarted,
        { isFirstTime: "false" },
        {},
      );
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
    this.startSentinelWatcher();
    sendTelemetryEvent(
      EventType.LearningSessionStarted,
      { isFirstTime: "true" },
      {},
    );
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
        // TODO (acasey): do this lazily?
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
    const { activity, unit } = this.findCurrentActivity();

    // Python-notebook courses: primary action is "Open Notebook" (or
    // "Next" if the unit is already complete).
    if (this.activeCourse.kind === "python-notebook" && unit.notebookRel) {
      const isComplete = this.isComplete(this.position);
      const primaryGroup: ActionGroup = isComplete
        ? [{ key: "space", label: "Next", action: "next", primary: true }]
        : [
            {
              key: "space",
              label: "Open Notebook",
              action: "open-notebook",
              primary: true,
              codicon: "notebook",
            },
          ];
      const extraGroups: ActionGroup[] = isComplete
        ? [
            [
              {
                key: "o",
                label: "Open Notebook",
                action: "open-notebook",
                codicon: "notebook",
              },
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
      const navGroup: ActionGroup = [
        { key: "b", label: "Back", action: "back" },
      ];
      return [primaryGroup, ...extraGroups, navGroup].filter(
        (g) => g.length > 0,
      );
    }

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
   * Actions for the panel in python-notebook courses. Checks whether
   * the entire unit is complete (all exercises done) rather than a
   * single activity.
   */
  private getAvailableActionsForPanel(unit: CatalogUnit): ActionGroup[] {
    const course = this.activeCourse;
    const unitComplete = unit.activities
      .filter((a) => a.type === "exercise")
      .every((a) =>
        this.isComplete({
          courseId: course.id,
          unitId: unit.id,
          activityId: a.id,
        }),
      );

    const primaryGroup: ActionGroup = unitComplete
      ? [{ key: "space", label: "Next", action: "next", primary: true }]
      : [
          {
            key: "space",
            label: "Open Notebook",
            action: "open-notebook",
            primary: true,
            codicon: "notebook",
          },
        ];

    const extraGroups: ActionGroup[] = unitComplete
      ? [
          [
            {
              key: "o",
              label: "Open Notebook",
              action: "open-notebook",
              codicon: "notebook",
            },
            { key: "r", label: "Reset", action: "reset" },
          ],
        ]
      : [[{ key: "r", label: "Reset", action: "reset" }]];

    const navGroup: ActionGroup = [{ key: "b", label: "Back", action: "back" }];

    return [primaryGroup, ...extraGroups, navGroup].filter((g) => g.length > 0);
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
    return this.pythonRunner.workbookFileUri(this.activeCourse, notebookRel);
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
      this._lastSnapshot = undefined;
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
      this._lastSnapshot = undefined;
      this._onDidChangeProgress.fire(undefined);
      return;
    }
    this._lastSnapshot = this.getProgress();
    this._onDidChangeProgress.fire(this._lastSnapshot);
  }

  /**
   * Start watching for `.qdk-unit-complete` sentinel files in the active
   * python-notebook course folder. When the notebook's `complete_unit()` writes this
   * file, we mark the unit complete.
   */
  private startSentinelWatcher(): void {
    this.stopSentinelWatcher();
    const course = this.activeCourse;
    if (course.kind !== "python-notebook" || !course.sourceDir) {
      return;
    }
    const coursesDir = vscode.Uri.parse(course.sourceDir);
    const pattern = new vscode.RelativePattern(
      coursesDir,
      "**/.qdk-unit-complete",
    );
    this._sentinelWatcher = vscode.workspace.createFileSystemWatcher(pattern);

    const onSentinel = async (uri: vscode.Uri) => {
      try {
        const bytes = await vscode.workspace.fs.readFile(uri);
        const unitId = new TextDecoder().decode(bytes).trim();
        if (!unitId) {
          return;
        }
        // Find the unit and mark all its activities complete.
        const unit = course.units.find((u) => u.id === unitId);
        if (!unit || unit.activities.length === 0) {
          return;
        }
        let changed = false;
        for (const activity of unit.activities) {
          const location: ActivityLocation = {
            courseId: course.id,
            unitId: unit.id,
            activityId: activity.id,
          };
          if (!this.isComplete(location)) {
            this.markComplete(location);
            changed = true;
          }
        }
        if (changed) {
          await this.saveProgress();
          this._onDidChangeState.fire(this.getState());
        }
      } catch {
        // sentinel may be transient or corrupt; ignore
      }
    };

    this._sentinelWatcher.onDidCreate(onSentinel);
    this._sentinelWatcher.onDidChange(onSentinel);
  }

  private stopSentinelWatcher(): void {
    this._sentinelWatcher?.dispose();
    this._sentinelWatcher = undefined;
  }

  /**
   * Close any open editor tabs whose URI matches the given notebook URI.
   */
  private async closeNotebookTab(uri: vscode.Uri): Promise<void> {
    const uriStr = uri.toString();
    const tabs: vscode.Tab[] = [];
    for (const group of vscode.window.tabGroups.all) {
      for (const tab of group.tabs) {
        if (
          tab.input instanceof vscode.TabInputNotebook &&
          tab.input.uri.toString() === uriStr
        ) {
          tabs.push(tab);
        }
      }
    }
    if (tabs.length > 0) {
      await vscode.window.tabGroups.close(tabs);
    }
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
      await this.pythonRunner.materializeCourse(course);
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
          if (await this.uriExists(fileUri)) {
            continue;
          }
          await this.ensureParentDir(fileUri);
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
          await this.ensureParentDir(fileUri);
          await vscode.workspace.fs.writeFile(
            fileUri,
            new TextEncoder().encode(activity.example.code),
          );
        }
      }
    }
  }

  // TODO (acasey): check for clones
  private async uriExists(uri: vscode.Uri): Promise<boolean> {
    try {
      await vscode.workspace.fs.stat(uri);
      return true;
    } catch {
      return false;
    }
  }

  private async ensureParentDir(fileUri: vscode.Uri): Promise<void> {
    const parentUri = vscode.Uri.joinPath(fileUri, "..");
    try {
      await vscode.workspace.fs.createDirectory(parentUri);
    } catch {
      // already exists
    }
  }
}
