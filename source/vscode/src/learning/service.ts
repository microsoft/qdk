// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * In-proc Learning Service for the VS Code extension host.
 *
 * A singleton service that manages learning state (navigation, progress,
 * file scaffolding) and provides code execution (run, circuit, check)
 * using the compiler worker. Both the Katas Panel webview and the
 * `qdk-learning-*` LM tools use this service.
 *
 * Lifted from `katasPanel/engine.ts` with compiler execution added
 * (following the `QSharpTools` pattern from `gh-copilot/qsharpTools.ts`).
 */

import * as vscode from "vscode";
import { QscEventTarget } from "qsharp-lang";
import { loadCompilerWorker } from "../common.js";
import { loadKatasCourse, getExerciseSourceFiles } from "./catalog.js";
import { scanForCourses } from "./courseScanner.js";
import {
  LEARNING_WORKSPACE_FOLDER,
  LEARNING_WORKSPACE_RELATIVE_PATH,
  LEARNING_FILE,
  KATAS_DETECTED_CONTEXT,
  KATAS_COURSE_ID,
} from "./constants.js";
import { computeOverallProgress } from "./computeProgress.js";
import {
  activityLocationKey,
  activityLocationsEqual,
  findCompletion,
} from "./activityLocation.js";
import type {
  ActivityLocation,
  CurrentActivity,
  ActivityContent,
  LessonTextContent,
  LessonExampleContent,
  ExerciseContent,
  ExampleContent,
  PrimaryAction,
  ActionGroup,
  LearningState,
  NavigationResult,
  HintContext,
  OverallProgress,
  ProgressFileData,
  SolutionCheckResult,
  RunResult,
  UnitSummary,
  OutputEvent,
  CatalogCourse,
  CatalogUnit,
  CatalogExercise,
} from "./types.js";

export interface KatasWorkspaceInfo {
  /** The workspace folder that contains `qdk-learning.json`. */
  workspaceRoot: vscode.Uri;
  /** The katas content folder, resolved from the well-known folder name. */
  katasRoot: vscode.Uri;
  /** Path to `qdk-learning.json`. */
  learningFile: vscode.Uri;
  /** True when `katasRoot` already exists on disk. */
  katasDirExists: boolean;
}

async function uriExistsOnDisk(uri: vscode.Uri): Promise<boolean> {
  try {
    await vscode.workspace.fs.stat(uri);
    return true;
  } catch {
    return false;
  }
}

/**
 * Detect an existing Quantum Katas workspace by scanning all open workspace
 * folders for a `qdk-learning.json` file.
 *
 * Returns `undefined` if no katas workspace can be found.
 */
export async function detectLearningWorkspace(): Promise<
  KatasWorkspaceInfo | undefined
> {
  for (const folder of vscode.workspace.workspaceFolders ?? []) {
    const learningFile = vscode.Uri.joinPath(folder.uri, LEARNING_FILE);
    if (!(await uriExistsOnDisk(learningFile))) {
      continue;
    }

    const katasRoot = vscode.Uri.joinPath(
      folder.uri,
      LEARNING_WORKSPACE_RELATIVE_PATH,
    );
    return {
      workspaceRoot: folder.uri,
      katasRoot,
      learningFile,
      katasDirExists: await uriExistsOnDisk(katasRoot),
    };
  }

  return undefined;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8");

export class LearningService {
  private courses: CatalogCourse[] = [];
  private flatPositions: ActivityLocation[] = [];
  private currentFlatIndex = 0;
  private ranExamples = new Set<string>();

  private workspaceRoot!: vscode.Uri;
  private katasRoot!: vscode.Uri;
  private learningFile!: vscode.Uri;

  // ── Progress data (mirrors qdk-learning.json) ──
  private progressData!: ProgressFileData;

  private _initialized = false;

  // ── State change event ──
  private readonly _onDidChangeState = new vscode.EventEmitter<LearningState>();
  readonly onDidChangeState = this._onDidChangeState.event;

  // ── Progress watching ──
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
    return this._initialized;
  }

  /** Root URI for scaffolded katas content (exercises/, examples/, etc.). */
  getKatasRoot(): vscode.Uri {
    return this.katasRoot;
  }

  get lastSnapshot(): OverallProgress | undefined {
    return this._lastSnapshot;
  }

  /** Force a fresh progress reload from disk. */
  async refresh(): Promise<void> {
    if (this._initialized) {
      await this.reloadProgress();
    }
  }

  // ─── Lifecycle ───

  async initialize(
    workspaceRoot: vscode.Uri,
    katasRoot: vscode.Uri,
  ): Promise<void> {
    this.workspaceRoot = workspaceRoot;
    this.katasRoot = katasRoot;
    this.learningFile = vscode.Uri.joinPath(workspaceRoot, LEARNING_FILE);

    // Load the built-in Quantum Katas course.
    const katasCourse = await loadKatasCourse();
    katasCourse.iconPath = vscode.Uri.joinPath(
      this.extensionUri,
      "resources",
      "course-katas.svg",
    ).fsPath;

    // Discover filesystem courses from qdk-learning-content folders.
    const fsCourses = await scanForCourses();

    this.courses = [katasCourse, ...fsCourses];

    // Build flat position list across all courses
    this.flatPositions = [];
    for (const course of this.courses) {
      for (const unit of course.units) {
        for (const section of unit.sections) {
          this.flatPositions.push({
            courseId: course.id,
            unitId: unit.id,
            activityId: section.id,
          });
        }
      }
    }

    // Scaffold exercise and example files (only for built-in katas)
    await this.scaffoldExercises(katasCourse);
    await this.scaffoldExamples(katasCourse);

    // Load progress
    await this.loadProgress();

    // Restore position from progress
    const savedPos = this.progressData.position;
    if (savedPos.unitId) {
      const target: ActivityLocation = {
        courseId: savedPos.courseId ?? KATAS_COURSE_ID,
        unitId: savedPos.unitId,
        activityId: savedPos.activityId,
      };
      const idx = this.flatPositions.findIndex((fp) =>
        activityLocationsEqual(fp, target),
      );
      if (idx >= 0) {
        this.currentFlatIndex = idx;
      }
    }

    this._initialized = true;
  }

  /**
   * Idempotent initialization entry point.
   *
   * Detects an existing katas workspace on disk or, when
   * `createIfMissing` is set, creates the progress file in the first
   * open workspace folder. Returns `true` when the service is ready.
   */
  async ensureInitialized(options?: {
    createIfMissing?: boolean;
  }): Promise<boolean> {
    if (this._initialized) {
      return true;
    }

    // Cache the in-flight promise so concurrent callers (e.g. show + restore)
    // don't double-initialize.
    if (!this._initPromise) {
      this._initPromise = this.doInitialize(options).finally(() => {
        this._initPromise = undefined;
      });
    }
    return this._initPromise;
  }

  private async doInitialize(options?: {
    createIfMissing?: boolean;
  }): Promise<boolean> {
    const detected = await detectLearningWorkspace();

    if (detected) {
      await this.initialize(detected.workspaceRoot, detected.katasRoot);
      this.startWatcher();
      return true;
    }

    if (!options?.createIfMissing) {
      return false;
    }

    // No existing workspace — bootstrap in the first open folder.
    const folders = vscode.workspace.workspaceFolders;
    if (!folders || folders.length === 0) {
      return false;
    }

    const workspaceRoot = folders[0].uri;
    const katasRoot = vscode.Uri.joinPath(
      workspaceRoot,
      LEARNING_WORKSPACE_FOLDER,
    );
    const learningFile = vscode.Uri.joinPath(workspaceRoot, LEARNING_FILE);

    const defaultData = {
      version: 1,
      position: { unitId: "", activityId: "" },
      completions: {},
      startedAt: new Date().toISOString(),
    };
    this._writingProgress = true;
    try {
      await vscode.workspace.fs.writeFile(
        learningFile,
        new TextEncoder().encode(JSON.stringify(defaultData, null, 2)),
      );
    } finally {
      this._writingProgress = false;
    }

    await this.initialize(workspaceRoot, katasRoot);
    this.startWatcher();
    return true;
  }

  dispose(): void {
    this.saveProgress().catch(() => {});
    this._onDidChangeState.dispose();
    this._onDidChangeProgress.dispose();
    this._progressFileWatcher?.dispose();
  }

  // ─── Navigation ───

  getPosition(): CurrentActivity {
    const fp = this.flatPositions[this.currentFlatIndex];
    if (!fp) {
      throw new Error("No position available — have you called initialize()?");
    }
    const unit = this.findUnit(fp.courseId, fp.unitId);
    const section = unit.sections.find((s) => s.id === fp.activityId)!;
    return {
      courseId: fp.courseId,
      unitId: fp.unitId,
      unitTitle: unit.title,
      activityId: fp.activityId,
      activityTitle: section.title,
      content: this.resolveActivityContent(fp),
    };
  }

  getState(): LearningState {
    return {
      position: this.getPosition(),
      actions: this.getAvailableActions(),
      progress: this.getProgress(),
    };
  }

  next(): NavigationResult {
    if (this.currentFlatIndex >= this.flatPositions.length - 1) {
      return { moved: false, state: this.getState() };
    }

    const oldFp = this.flatPositions[this.currentFlatIndex];
    this.currentFlatIndex++;
    const newFp = this.flatPositions[this.currentFlatIndex];

    // Auto-mark lesson/example sections complete when crossing section boundary
    if (
      oldFp.unitId !== newFp.unitId ||
      oldFp.activityId !== newFp.activityId
    ) {
      const oldUnit = this.findUnit(oldFp.courseId, oldFp.unitId);
      const oldSection = oldUnit.sections.find(
        (s) => s.id === oldFp.activityId,
      );
      if (oldSection?.type === "lesson" || oldSection?.type === "example") {
        this.markComplete(oldFp);
      }
    }

    this.syncPosition();
    const state = this.getState();
    this._onDidChangeState.fire(state);
    return { moved: true, state };
  }

  previous(): NavigationResult {
    if (this.currentFlatIndex <= 0) {
      return { moved: false, state: this.getState() };
    }

    this.currentFlatIndex--;
    this.syncPosition();
    const state = this.getState();
    this._onDidChangeState.fire(state);
    return { moved: true, state };
  }

  goTo(courseId: string, unitId: string, activityId?: string): LearningState {
    if (!activityId) {
      const firstIdx = this.flatPositions.findIndex(
        (fp) => fp.courseId === courseId && fp.unitId === unitId,
      );
      if (firstIdx < 0) {
        throw new Error(`Unit not found: ${courseId}/${unitId}`);
      }
      this.currentFlatIndex = firstIdx;
      this.syncPosition();
      const state = this.getState();
      this._onDidChangeState.fire(state);
      return state;
    }

    const target: ActivityLocation = { courseId, unitId, activityId };
    const idx = this.flatPositions.findIndex((fp) =>
      activityLocationsEqual(fp, target),
    );
    if (idx < 0) {
      throw new Error(
        `Position not found: ${courseId}/${unitId} activity ${activityId}`,
      );
    }
    this.currentFlatIndex = idx;
    this.syncPosition();
    const state = this.getState();
    this._onDidChangeState.fire(state);
    return state;
  }

  // ─── Actions ───

  private getPrimaryAction(): PrimaryAction {
    const pos = this.getPosition();
    switch (pos.content.type) {
      case "lesson-text":
        return "next";
      case "lesson-example":
        return this.ranExamples.has(pos.content.id) ? "next" : "run";
      case "exercise":
        return pos.content.isComplete ? "next" : "check";
      case "example":
        return "next";
    }
  }

  private getAvailableActions(): ActionGroup[] {
    const pos = this.getPosition();
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

    switch (pos.content.type) {
      case "lesson-text": {
        const aiGroup: ActionGroup = [
          {
            key: "e",
            label: "Explain",
            action: "explain-chat",
            codicon: "sparkle",
          },
        ];
        return [primaryGroup, aiGroup, navGroup];
      }
      case "lesson-example": {
        // Only show Run in codeTools when it's not already the primary.
        const codeTools: ActionGroup =
          primary === "run" ? [] : [{ key: "r", label: "Run", action: "run" }];
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
      case "exercise": {
        // When completed, keep Check available so users can re-validate.
        const codeTools: ActionGroup = pos.content.isComplete
          ? [
              { key: "c", label: "Check", action: "check" },
              { key: "r", label: "Run", action: "run" },
            ]
          : [{ key: "r", label: "Run", action: "run" }];
        const helpGroup: ActionGroup = pos.content.isComplete
          ? []
          : [
              {
                key: "h",
                label: "Hint",
                action: "hint-chat",
                codicon: "sparkle",
              },
            ];
        return [primaryGroup, codeTools, helpGroup, navGroup];
      }
      case "example": {
        // Example activities: primary is Next, no code tools / hints.
        return [primaryGroup, navGroup];
      }
    }
  }

  // ─── Hints & solutions ───

  getHintContext(): { result: HintContext | null; state: LearningState } {
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

  getFullSolution(): string {
    const exercise = this.resolveExercise();
    return exercise.solutionCode;
  }

  // ─── Exercise file access ───

  getExerciseFileUri(): vscode.Uri {
    const pos = this.getPosition();
    if (pos.content.type !== "exercise") {
      throw new Error("Current activity is not an exercise");
    }
    const exercise = this.resolveExercise();
    return vscode.Uri.joinPath(
      this.katasRoot,
      "exercises",
      pos.unitId,
      `${exercise.id}.qs`,
    );
  }

  getExampleFileUri(): vscode.Uri {
    const pos = this.getPosition();
    if (pos.content.type === "lesson-example") {
      return vscode.Uri.joinPath(
        this.katasRoot,
        "examples",
        pos.unitId,
        `${pos.content.id}.qs`,
      );
    }
    if (pos.content.type === "example") {
      return vscode.Uri.file(pos.content.filePath);
    }
    throw new Error("Current activity is not an example");
  }

  async readUserCode(): Promise<string> {
    const uri = this.getExerciseFileUri();
    const bytes = await vscode.workspace.fs.readFile(uri);
    return decoder.decode(bytes);
  }

  markExampleRun(exampleId: string): void {
    this.ranExamples.add(exampleId);
  }

  /**
   * Reset the current exercise file to the original placeholder code
   * and clear its completion status.
   */
  async resetExercise(): Promise<void> {
    const exercise = this.resolveExercise();
    const uri = this.getExerciseFileUri();
    await vscode.workspace.fs.writeFile(
      uri,
      encoder.encode(exercise.placeholderCode),
    );
    const pos = this.getPosition();
    this.markIncomplete(pos);
    await this.saveProgress();
    this._onDidChangeState.fire(this.getState());
  }

  async markExerciseComplete(location: ActivityLocation): Promise<void> {
    this.markComplete(location);
    await this.saveProgress();
    this._onDidChangeState.fire(this.getState());
  }

  // ─── Code execution (compiler worker) ───

  async run(
    shots: number = 1,
  ): Promise<{ result: RunResult; state: LearningState }> {
    const pos = this.getPosition();
    const fileUri = this.getCurrentCodeFileUri();

    // Mark examples as run
    if (pos.content.type === "lesson-example") {
      this.markExampleRun(pos.content.id);
    }

    const worker = loadCompilerWorker(this.extensionUri);
    const eventTarget = new QscEventTarget(true);

    try {
      const code = await this.readCurrentCode(fileUri);
      const program = {
        sources: [["code.qs", code]] as [string, string][],
        languageFeatures: [] as string[],
        profile: "unrestricted" as const,
      };
      await worker.run(program, "", shots, eventTarget);

      const events = this.extractEvents(eventTarget);
      const shotResults = eventTarget.getResults();
      const allPassed = shotResults.every((s) => s.success);

      return {
        result: {
          success: allPassed,
          events,
          result: allPassed ? this.extractResult(eventTarget) : undefined,
          error: allPassed ? undefined : this.extractError(events),
        },
        state: this.getState(),
      };
    } catch (err: unknown) {
      return {
        result: {
          success: false,
          events: [],
          error: err instanceof Error ? err.message : String(err),
        },
        state: this.getState(),
      };
    } finally {
      worker.terminate();
    }
  }

  async checkSolution(): Promise<{
    result: SolutionCheckResult;
    state: LearningState;
  }> {
    const pos = this.getPosition();
    if (pos.content.type !== "exercise") {
      throw new Error("Current activity is not an exercise.");
    }

    const exercise = this.resolveExercise();
    const userCode = await this.readUserCode();
    const sources = await getExerciseSourceFiles(exercise);

    const worker = loadCompilerWorker(this.extensionUri);
    const eventTarget = new QscEventTarget(true);

    try {
      const passed = await worker.checkExerciseSolution(
        userCode,
        sources,
        eventTarget,
      );

      const events = this.extractEvents(eventTarget);

      if (passed) {
        await this.markExerciseComplete(pos);
      }

      return {
        result: {
          passed,
          events,
          error: passed
            ? undefined
            : events.length === 0
              ? "Solution check failed."
              : undefined,
        },
        state: this.getState(),
      };
    } catch (err: unknown) {
      return {
        result: {
          passed: false,
          events: [],
          error: err instanceof Error ? err.message : String(err),
        },
        state: this.getState(),
      };
    } finally {
      worker.terminate();
    }
  }

  // ─── Catalog ───

  /** Return the list of loaded courses (for tree view display). */
  getCourses(): CatalogCourse[] {
    return this.courses;
  }

  listUnits(courseId?: string): UnitSummary[] {
    const progress = this.getProgress();
    let foundFirstIncomplete = false;

    const targetCourses = courseId
      ? this.courses.filter((c) => c.id === courseId)
      : this.courses;

    const allUnits: UnitSummary[] = [];
    for (const course of targetCourses) {
      for (const unit of course.units) {
        const unitProgress = progress.units.find((u) => u.id === unit.id);
        const completedCount = unitProgress?.completed ?? 0;
        const activityCount = unitProgress?.total ?? unit.sections.length;
        const allComplete =
          completedCount === activityCount && activityCount > 0;

        let recommended = false;
        if (!allComplete && !foundFirstIncomplete) {
          foundFirstIncomplete = true;
          recommended = true;
        }

        allUnits.push({
          id: unit.id,
          title: unit.title,
          courseId: course.id,
          activityCount,
          completedCount,
          recommended,
        });
      }
    }
    return allUnits;
  }

  // ─── Progress ───

  getProgress(): OverallProgress {
    const catalog = this.courses.flatMap((course) =>
      course.units.map((k) => ({
        courseId: course.id,
        id: k.id,
        title: k.title,
        activities: k.sections.map((s) => ({
          id: s.id,
          title: s.title,
          type: s.type,
        })),
      })),
    );
    return computeOverallProgress(catalog, this.progressData);
  }

  /** Reload progress from disk (called when file watcher fires). */
  async reloadProgress(): Promise<void> {
    await this.loadProgress();
    // Restore position
    const savedPos = this.progressData.position;
    if (savedPos.unitId) {
      const target: ActivityLocation = {
        courseId: savedPos.courseId ?? KATAS_COURSE_ID,
        unitId: savedPos.unitId,
        activityId: savedPos.activityId,
      };
      const idx = this.flatPositions.findIndex((fp) =>
        activityLocationsEqual(fp, target),
      );
      if (idx >= 0) {
        this.currentFlatIndex = idx;
      }
    }
    this.emitProgress();
  }

  // ─── Private: progress watching ───

  /**
   * Set up a file watcher on the known progress file path.
   * Called once after the first successful initialization.
   */
  private startWatcher(): void {
    if (this._progressFileWatcher) {
      return; // Already watching.
    }

    void vscode.commands.executeCommand(
      "setContext",
      KATAS_DETECTED_CONTEXT,
      true,
    );

    const pattern = new vscode.RelativePattern(
      this.workspaceRoot,
      LEARNING_FILE,
    );
    this._progressFileWatcher =
      vscode.workspace.createFileSystemWatcher(pattern);

    const onChangeOrCreate = () => {
      if (!this._writingProgress) {
        // File (re-)appeared or changed — ensure the context key is set
        // (may have been cleared by a prior delete) and reload.
        void vscode.commands.executeCommand(
          "setContext",
          KATAS_DETECTED_CONTEXT,
          true,
        );
        void this.reloadProgress();
      }
    };
    const onDelete = () => {
      if (!this._writingProgress) {
        // File removed externally — reset progress and update tree.
        this.progressData = this.freshProgressData();
        this.currentFlatIndex = 0;
        this._lastSnapshot = undefined;
        this._onDidChangeProgress.fire(undefined);
        void vscode.commands.executeCommand(
          "setContext",
          KATAS_DETECTED_CONTEXT,
          false,
        );
        this._onDidChangeState.fire(this.getState());
      }
    };

    this._progressFileWatcher.onDidChange(onChangeOrCreate);
    this._progressFileWatcher.onDidCreate(onChangeOrCreate);
    this._progressFileWatcher.onDidDelete(onDelete);

    // Emit initial snapshot.
    this.emitProgress();
  }

  private emitProgress(): void {
    if (!this._initialized) {
      this._lastSnapshot = undefined;
      this._onDidChangeProgress.fire(undefined);
      return;
    }
    this._lastSnapshot = this.getProgress();
    this._onDidChangeProgress.fire(this._lastSnapshot);
  }

  // ─── Private: compiler helpers ───

  private getCurrentCodeFileUri(): vscode.Uri {
    const pos = this.getPosition();
    if (pos.content.type === "exercise") {
      return this.getExerciseFileUri();
    } else if (pos.content.type === "lesson-example") {
      return this.getExampleFileUri();
    } else if (pos.content.type === "example") {
      return vscode.Uri.file(pos.content.filePath);
    }
    throw new Error("Current activity cannot be run.");
  }

  private async readCurrentCode(fileUri: vscode.Uri): Promise<string> {
    const bytes = await vscode.workspace.fs.readFile(fileUri);
    return decoder.decode(bytes);
  }

  private extractEvents(eventTarget: QscEventTarget): OutputEvent[] {
    const events: OutputEvent[] = [];
    const results = eventTarget.getResults();
    for (const r of results) {
      for (const evt of r.events) {
        switch (evt.type) {
          case "Message":
            events.push({ type: "message", message: evt.message });
            break;
          case "DumpMachine":
            events.push({ type: "dump", dump: { state: evt.state } });
            break;
          case "Matrix":
            events.push({ type: "matrix", matrix: { matrix: evt.matrix } });
            break;
        }
      }
      // Extract compiler/runtime errors from the shot result
      if (!r.success && typeof r.result !== "string") {
        const errors = r.result?.errors ?? [];
        for (const e of errors) {
          const msg = e.diagnostic?.message ?? String(e);
          if (msg) {
            events.push({ type: "message", message: msg });
          }
        }
      }
    }
    return events;
  }

  private extractResult(eventTarget: QscEventTarget): string | undefined {
    const resultCount = eventTarget.resultCount();
    const results = eventTarget.getResults();
    if (resultCount > 0) {
      const lastResult = results[resultCount - 1];
      if (typeof lastResult.result === "string") {
        return lastResult.result;
      }
      // VSDiagnostic error shape — extract error messages
      const errors = lastResult.result.errors ?? [];
      return (
        errors.map((e) => e.diagnostic?.message ?? String(e)).join("\n") ||
        undefined
      );
    }
    return undefined;
  }

  private extractError(events: OutputEvent[]): string | undefined {
    const messages = events
      .filter(
        (e): e is OutputEvent & { type: "message" } => e.type === "message",
      )
      .map((e) => e.message);
    return messages.length > 0 ? messages.join("\n") : "Execution failed.";
  }

  // ─── Private: progress persistence via vscode.workspace.fs ───

  private freshProgressData(): ProgressFileData {
    return {
      version: 1,
      position: {
        courseId: KATAS_COURSE_ID,
        unitId: this.courses[0]?.units[0]?.id ?? "",
        activityId: this.courses[0]?.units[0]?.sections[0]?.id ?? "",
      },
      completions: {},
      startedAt: new Date().toISOString(),
    };
  }

  private async loadProgress(): Promise<void> {
    try {
      const bytes = await vscode.workspace.fs.readFile(this.learningFile);
      const parsed = JSON.parse(decoder.decode(bytes)) as ProgressFileData;
      if (parsed.version === 1) {
        this.progressData = parsed;
        // Validate position references a known unit
        const courseId = parsed.position.courseId ?? KATAS_COURSE_ID;
        const course = this.courses.find((c) => c.id === courseId);
        if (
          this.courses.length > 0 &&
          (!course ||
            !course.units.find(
              (u) => u.id === this.progressData.position.unitId,
            ))
        ) {
          this.progressData.position = {
            courseId: KATAS_COURSE_ID,
            unitId: this.courses[0]?.units[0]?.id ?? "",
            activityId: this.courses[0]?.units[0]?.sections[0]?.id ?? "",
          };
        }
        return;
      }
    } catch {
      // File missing or corrupt
    }
    this.progressData = this.freshProgressData();
  }

  private async saveProgress(): Promise<void> {
    const json = JSON.stringify(this.progressData, null, 2);
    this._writingProgress = true;
    try {
      await vscode.workspace.fs.writeFile(
        this.learningFile,
        encoder.encode(json),
      );
    } finally {
      this._writingProgress = false;
    }
    this.emitProgress();
  }

  private completionKey(loc: ActivityLocation): string {
    return activityLocationKey(loc);
  }

  private isComplete(loc: ActivityLocation): boolean {
    return findCompletion(this.progressData.completions, loc) != null;
  }

  private markComplete(loc: ActivityLocation): void {
    const key = this.completionKey(loc);
    if (!(key in this.progressData.completions)) {
      this.progressData.completions[key] = {
        completedAt: new Date().toISOString(),
      };
    }
  }

  private markIncomplete(loc: ActivityLocation): void {
    const key = this.completionKey(loc);
    delete this.progressData.completions[key];
  }

  private syncPosition(): void {
    const fp = this.flatPositions[this.currentFlatIndex];
    if (fp) {
      this.progressData.position = {
        courseId: fp.courseId,
        unitId: fp.unitId,
        activityId: fp.activityId,
      };
      this.saveProgress().catch(() => {});
    }
  }

  // ─── Private: scaffolding via vscode.workspace.fs ───

  private async scaffoldExercises(course: CatalogCourse): Promise<void> {
    for (const kata of course.units) {
      for (const section of kata.sections) {
        if (section.type !== "exercise") {
          continue;
        }
        const fileUri = vscode.Uri.joinPath(
          this.katasRoot,
          "exercises",
          kata.id,
          `${section.id}.qs`,
        );
        if (await this.uriExists(fileUri)) {
          continue;
        }
        await this.ensureParentDir(fileUri);
        await vscode.workspace.fs.writeFile(
          fileUri,
          encoder.encode(section.placeholderCode),
        );
      }
    }
  }

  private async scaffoldExamples(course: CatalogCourse): Promise<void> {
    for (const kata of course.units) {
      for (const section of kata.sections) {
        if (section.type !== "lesson" || !section.example) {
          continue;
        }
        const fileUri = vscode.Uri.joinPath(
          this.katasRoot,
          "examples",
          kata.id,
          `${section.example.id}.qs`,
        );
        await this.ensureParentDir(fileUri);
        await vscode.workspace.fs.writeFile(
          fileUri,
          encoder.encode(section.example.code),
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
      // Directory may already exist
    }
  }

  // ─── Private: navigation part resolution ───

  private findUnit(courseId: string, unitId: string): CatalogUnit {
    const course = this.courses.find((c) => c.id === courseId);
    if (!course) {
      throw new Error(`Course not found: ${courseId}`);
    }
    const unit = course.units.find((k) => k.id === unitId);
    if (!unit) {
      throw new Error(`Unit not found: ${courseId}/${unitId}`);
    }
    return unit;
  }

  resolveExercise(): CatalogExercise {
    const pos = this.getPosition();
    const unit = this.findUnit(pos.courseId, pos.unitId);
    const section = unit.sections.find((s) => s.id === pos.activityId);
    if (!section || section.type !== "exercise") {
      throw new Error("Current activity is not an exercise");
    }
    return section;
  }

  private resolveActivityContent(fp: ActivityLocation): ActivityContent {
    const unit = this.findUnit(fp.courseId, fp.unitId);
    const section = unit.sections.find((s) => s.id === fp.activityId)!;

    if (section.type === "exercise") {
      const fileUri = vscode.Uri.joinPath(
        this.katasRoot,
        "exercises",
        unit.id,
        `${section.id}.qs`,
      );
      return {
        type: "exercise",
        id: section.id,
        title: section.title,
        description: section.description,
        filePath: fileUri.fsPath,
        isComplete: this.isComplete(fp),
        hintCount: section.hints.length + (section.solutionExplanation ? 1 : 0),
      } satisfies ExerciseContent;
    }

    if (section.type === "example") {
      return {
        type: "example",
        filePath: section.filePath,
        activityTitle: section.title,
      } satisfies ExampleContent;
    }

    // Lesson with a code example
    if (section.example) {
      const fileUri = vscode.Uri.joinPath(
        this.katasRoot,
        "examples",
        unit.id,
        `${section.example.id}.qs`,
      );
      return {
        type: "lesson-example",
        id: section.example.id,
        code: section.example.code,
        filePath: fileUri.fsPath,
        activityTitle: section.title,
        contentBefore: section.contentBefore,
        contentAfter: section.contentAfter,
      } satisfies LessonExampleContent;
    }

    // Text-only lesson
    return {
      type: "lesson-text",
      content: section.content ?? "",
      activityTitle: section.title,
    } satisfies LessonTextContent;
  }
}
