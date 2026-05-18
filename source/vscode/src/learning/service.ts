// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { getExerciseSources } from "qsharp-lang/katas-md";
import * as vscode from "vscode";
import { FullProgramConfig, getProgramForDocument } from "../programConfig.js";
import { ProgramRunStatus, runProgram } from "../run.js";
import { EventType, sendTelemetryEvent } from "../telemetry.js";
import { loadKatasCourse } from "./catalog.js";
import {
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
  CurrentActivity,
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
  /** Currently, only a single course is supported. */
  catalog: CatalogCourse;
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
  private _writingProgress = false;
  private _initPromise: Promise<boolean> | undefined;

  constructor(private readonly extensionUri: vscode.Uri) {}

  get initialized(): boolean {
    return this.workspace !== undefined;
  }

  get learningContentRoot(): vscode.Uri {
    return this.requireWorkspace().learningContentRoot;
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
      position: this.getCurrentActivity(),
      actions: this.getAvailableActions(),
      progress: this.getProgress(),
    };
  }

  next(source: TelemetrySource): NavigationResult {
    const ws = this.requireWorkspace();
    const currentPos = ws.progressData.position;
    const nextPos = this.nextActivity(currentPos);
    if (!nextPos) {
      return { moved: false };
    }

    // Auto-mark lesson activities complete when moving forward
    const oldKata = this.findUnit(currentPos.unitId);
    const oldActivity = oldKata.activities.find(
      (s) => s.id === currentPos.activityId,
    );
    if (oldActivity?.type === "lesson") {
      this.markComplete(currentPos);
    }

    ws.progressData.position = nextPos;
    this.saveProgress().catch(() => {});
    this._onDidChangeState.fire(this.getState());
    this.sendActivityActionTelemetry("navigate", source);
    return { moved: true };
  }

  previous(source: TelemetrySource): NavigationResult {
    const ws = this.requireWorkspace();
    const prevPos = this.previousActivity(ws.progressData.position);
    if (!prevPos) {
      return { moved: false };
    }

    ws.progressData.position = prevPos;
    this.saveProgress().catch(() => {});
    this._onDidChangeState.fire(this.getState());
    this.sendActivityActionTelemetry("navigate", source);
    return { moved: true };
  }

  goTo(
    location: { unitId: string; activityId?: string },
    source?: TelemetrySource,
  ): LearningState {
    const ws = this.requireWorkspace();
    const unit = ws.catalog.units.find((u) => u.id === location.unitId);
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
      courseId: ws.catalog.id,
      unitId: location.unitId,
      activityId: activity.id,
    };
    this.saveProgress().catch(() => {});
    const state = this.getState();
    this._onDidChangeState.fire(state);
    if (source) {
      this.sendActivityActionTelemetry("navigate", source);
    }
    return state;
  }

  listUnits(): UnitSummary[] {
    const ws = this.requireWorkspace();
    let foundFirstIncomplete = false;

    return ws.catalog.units.map((kata) => {
      const activityCount = kata.activities.length;
      let completedCount = 0;
      for (const activity of kata.activities) {
        if (
          this.findCompletion({
            courseId: ws.catalog.id,
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
    const ws = this.requireWorkspace();
    let totalActivities = 0;
    let completedActivities = 0;

    const units: UnitProgress[] = ws.catalog.units.map((k) => {
      const activities: ActivityProgress[] = k.activities.map((s) => {
        const completion = this.findCompletion({
          courseId: ws.catalog.id,
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
    const exercise = this.resolveExercise();

    const hints = exercise.hints;
    const solutionExplanation = exercise.solutionExplanation;

    if (hints.length === 0 && solutionExplanation.length === 0) {
      return { result: null, state: this.getState() };
    }

    if (source) {
      this.sendActivityActionTelemetry("hint", source);
    }

    return {
      result: { hints, solutionExplanation },
      state: this.getState(),
    };
  }

  getFullSolution(source?: TelemetrySource): string {
    const exercise = this.resolveExercise();
    if (source) {
      this.sendActivityActionTelemetry("solution", source);
    }
    return exercise.solutionCode;
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
    const uri = this.getExerciseFileUri();
    const bytes = await vscode.workspace.fs.readFile(uri);
    return new TextDecoder().decode(bytes);
  }

  async markExampleRun(): Promise<void> {
    const location = this.requireWorkspace().progressData.position;
    this.markComplete(location);
    await this.saveProgress();
    this._onDidChangeState.fire(this.getState());
  }

  getCurrentCodeFileUri(): vscode.Uri | undefined {
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
   * Reset the current exercise file to the original placeholder code
   * and clear its completion status.
   */
  async resetExercise(source?: TelemetrySource): Promise<void> {
    const exercise = this.resolveExercise();
    const uri = this.getExerciseFileUri();
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
    const fileUri = this.getCurrentCodeFileUri();
    if (!fileUri) {
      throw new Error("Current activity cannot be run.");
    }

    if (activity.type === "lesson" && activity.example) {
      await this.markExampleRun();
    }

    if (source) {
      this.sendActivityActionTelemetry("run", source);
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

    const exercise = this.resolveExercise();
    const userCode = await this.readUserCode();
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
    options?: { entry?: string; shots?: number },
  ): Promise<RunResult> {
    const messages: string[] = [];

    try {
      const runResult = await runProgram(this.extensionUri, programConfig, {
        entry: options?.entry,
        shots: options?.shots ?? 1,
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

    const course = await loadKatasCourse();

    // Build workspace state; assigned to this.workspace only after all
    // async setup succeeds so that `initialized` stays false on failure.
    const ws: WorkspaceState = {
      workspaceRoot,
      learningContentRoot: katasRoot,
      learningFile,
      catalog: course,
      progressData: {
        version: 1,
        position: {
          courseId: course.id,
          unitId: course.units[0]?.id ?? "",
          activityId: course.units[0]?.activities[0]?.id ?? "",
        },
        completions: {},
        startedAt: new Date().toISOString(),
      },
    };

    await this.scaffoldExercises(ws);
    await this.scaffoldExamples(ws);
    await this.loadProgress(ws);

    // All async setup succeeded — publish the workspace.
    this.workspace = ws;
    this.syncContextKey();
  }

  private requireWorkspace(): WorkspaceState {
    if (!this.workspace) {
      throw new Error(
        "No active learning workspace. Call tryInitialize() before using this method.",
      );
    }
    return this.workspace;
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
        ? [[{ key: "c", label: "Check", action: "check" }]]
        : [
            [
              {
                key: "h",
                label: "Hint",
                action: "hint-chat",
                codicon: "sparkle",
              },
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

  /** Turns a catalog activity into the typed content payload (exercise, lesson-example, or lesson-text). */
  private resolveActivityContent(
    location: ActivityLocation,
    kata: CatalogUnit,
    activity: CatalogActivity,
  ): ActivityContent {
    const ws = this.requireWorkspace();

    if (activity.type === "exercise") {
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
    const ws = this.requireWorkspace();
    let found = false;
    for (const unit of ws.catalog.units) {
      for (const a of unit.activities) {
        if (found) {
          return {
            courseId: ws.catalog.id,
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
    const ws = this.requireWorkspace();
    let prev: ActivityLocation | undefined;
    for (const unit of ws.catalog.units) {
      for (const a of unit.activities) {
        if (unit.id === location.unitId && a.id === location.activityId) {
          return prev;
        }
        prev = {
          courseId: ws.catalog.id,
          unitId: unit.id,
          activityId: a.id,
        };
      }
    }
    return undefined;
  }

  private findUnit(unitId: string): CatalogUnit {
    const kata = this.requireWorkspace().catalog.units.find(
      (k) => k.id === unitId,
    );
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

    const unit = this.requireWorkspace().catalog.units.find(
      (u) => u.id === location.unitId,
    );
    const exercises =
      unit?.activities.filter((s) => s.type === "exercise") ?? [];
    const exerciseIndex = exercises.findIndex(
      (e) => e.id === location.activityId,
    );
    sendTelemetryEvent(
      EventType.LearningExerciseCompleted,
      {},
      {
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
        // Validate saved position references a known unit and activity
        if (ws.catalog.units.length > 0) {
          const unit = ws.catalog.units.find(
            (k) => k.id === ws.progressData.position.unitId,
          );
          const activityValid =
            unit &&
            unit.activities.some(
              (s) => s.id === ws.progressData.position.activityId,
            );
          if (!activityValid) {
            ws.progressData.position = {
              courseId: ws.catalog.id,
              unitId: ws.catalog.units[0].id,
              activityId: ws.catalog.units[0].activities[0]?.id ?? "",
            };
          }
        }
        return;
      }
    } catch {
      // expected when file is missing or corrupt
    }
    ws.progressData = this.freshProgressData();
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

  private freshProgressData(): ProgressFileData {
    const ws = this.requireWorkspace();
    return {
      version: 1,
      position: {
        courseId: ws.catalog.id,
        unitId: ws.catalog.units[0]?.id ?? "",
        activityId: ws.catalog.units[0]?.activities[0]?.id ?? "",
      },
      completions: {},
      startedAt: new Date().toISOString(),
    };
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

  private async scaffoldExercises(ws: WorkspaceState): Promise<void> {
    for (const kata of ws.catalog.units) {
      for (const activity of kata.activities) {
        if (activity.type !== "exercise") {
          continue;
        }
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
      }
    }
  }

  private async scaffoldExamples(ws: WorkspaceState): Promise<void> {
    for (const kata of ws.catalog.units) {
      for (const activity of kata.activities) {
        if (activity.type !== "lesson" || !activity.example) {
          continue;
        }
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
