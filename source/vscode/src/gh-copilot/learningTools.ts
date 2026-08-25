// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import {
  LearningService,
  LEARNING_WORKSPACE_FOLDER,
  detectLearningWorkspace,
  resolveNewWorkspaceRoot,
  type CourseDescriptor,
  type CurrentActivity,
  type HintContext,
  type UnitSummary,
  type OverallProgress,
  type RunResult,
  type SolutionCheckResult,
} from "../learning/index.js";
import { CopilotToolError } from "./types.js";
import { LearningState } from "../learning/types.js";

/**
 * Compact snapshot of the learner's current position and progress.
 *
 * Returned alongside every learning-tool response so the language model
 * always has up-to-date context about where the student is in the
 * curriculum without needing a separate round-trip.
 */
export interface SerializedLearningState {
  /** The currently-active course. */
  course: Pick<CourseDescriptor, "id" | "title" | "kind">;
  position: CurrentActivity;
  progress: {
    totalActivities: number;
    completedActivities: number;
    currentUnitCompleted: number;
    currentUnitTotal: number;
  };
}

/**
 * Mixin carrying the current learning state snapshot.
 * Intersected into every tool response type.
 */
export type StateSnapshot = { state: SerializedLearningState };

/**
 * Wraps the shared {@link LearningService} singleton for use as
 * `vscode.lm` language model tools.
 */
export class LearningTools {
  constructor(private readonly service: LearningService) {}

  /**
   * Called by `prepareInvocation` on almost every learning tool.
   *
   * Returns a confirmation prompt when the workspace needs first-time
   * setup, or `undefined` to skip confirmation when setup already exists
   * or the service is loaded.
   *
   * **Must be free of side-effects** — only reads state and the filesystem.
   */
  async confirmInit(): Promise<vscode.PreparedToolInvocation | undefined> {
    if (this.service.initialized) {
      return undefined;
    }

    // If the progress file already exists on disk, skip confirmation —
    // the workspace was previously set up and we just need to re-load state.
    const detected = await detectLearningWorkspace();
    if (detected) {
      return undefined;
    }

    const newRoot = resolveNewWorkspaceRoot();
    if (!newRoot) {
      // No workspace — let invoke() surface the error.
      return undefined;
    }
    const workspacePath = newRoot.fsPath;

    return {
      confirmationMessages: {
        title: "Initialize QDK Learning workspace",
        message:
          `Set up a Quantum Katas learning workspace in **${workspacePath}**? ` +
          `Exercise files and progress tracking will be created in a \`${LEARNING_WORKSPACE_FOLDER}\` subfolder.`,
      },
    };
  }

  /**
   * Ensures the learning service is initialized, creating workspace
   * files if needed. Called at the start of every tool invocation
   * (after the user has already approved via {@link confirmInit}).
   */
  private async ensureInitialized(): Promise<void> {
    const ok = await this.service.tryInitialize({ createIfMissing: true });
    if (!ok) {
      throw new CopilotToolError(
        "No workspace folder is open. Open a folder first, then try again.",
      );
    }
  }

  // ─── Read-only queries (do not open the panel) ───

  /**
   * Read the current learning position and progress.
   *
   * Returns `{ initialized: false }` when the workspace is not yet set up,
   * so the caller can decide whether to prompt for initialization.
   */
  async getState(): Promise<
    | { initialized: false; error?: string }
    | ({ initialized: true } & StateSnapshot)
  > {
    if (!this.service.initialized) {
      const progressError = this.service.progressLoadingError;
      if (progressError) {
        return { initialized: false, error: progressError };
      }
      const detected = await detectLearningWorkspace();
      if (!detected) {
        return { initialized: false };
      }
      await this.ensureInitialized();
    }
    return { initialized: true, state: this.serializeState(true) }; // match user experience
  }

  /**
   * Return the full per-kata progress breakdown.
   */
  async getProgress(): Promise<{ progress: OverallProgress }> {
    await this.ensureInitialized();
    const progress = this.service.getProgress();
    return { progress };
  }

  /**
   * List all available units with completion status.
   */
  async listUnits(): Promise<{ units: UnitSummary[] }> {
    await this.ensureInitialized();
    return { units: this.service.listUnits() };
  }

  /**
   * List all available courses (loaded or not) with the active course id.
   */
  async listCourses(): Promise<{
    courses: CourseDescriptor[];
    activeCourseId: string;
  }> {
    await this.ensureInitialized();
    return {
      courses: await this.service.getCourses(),
      activeCourseId: this.service.getActiveCourseId(),
    };
  }

  /**
   * Switch the active course, moving to its first incomplete activity.
   */
  async switchCourse(input: { courseId: string }): Promise<StateSnapshot> {
    await this.ensureInitialized();
    return this.invoke(async () => {
      await this.service.switchCourse(input.courseId, "chat");
      await this.showActivity();
      return { state: this.serializeState(false) }; // workspace state is correct after switch
    });
  }

  /**
   * Return the descriptor for a course. Defaults to the active course
   * when no id is provided.
   */
  async courseInfo(input?: { courseId?: string }): Promise<{
    descriptor: CourseDescriptor | undefined;
  }> {
    await this.ensureInitialized();
    return this.invoke(async () => {
      const courseId = input?.courseId ?? this.service.getActiveCourseId();
      const courses = await this.service.getCourses();
      const descriptor = courses.find((c) => c.id === courseId);
      return { descriptor };
    });
  }

  /**
   * Read the user's current code at the active exercise or example.
   * For python-notebook courses, returns the notebook file path.
   */
  async readCode(): Promise<{ code: string; filePath: string }> {
    await this.ensureInitialized();
    return this.invoke(async () => {
      const uri = this.getCurrentFileUri();
      const editor = vscode.window.activeNotebookEditor;
      if (editor) {
        let code = "";
        if (editor.notebook.uri.toString() === uri.toString()) {
          // Prefer the cell the user actually has focused in the editor.
          const selection = editor.selections[0];
          if (selection) {
            // Not guaranteed to be a code cell, but not important enough to check
            const cell = editor.notebook.cellAt(selection.start);
            code = cell.document.getText();
          }
        }
        return { code, filePath: uri.fsPath };
      }
      const code = await this.service.readUserCode();
      return { code, filePath: uri.fsPath };
    });
  }

  /**
   * Return all built-in hints for the current exercise.
   */
  async hint(): Promise<{ result: HintContext | undefined } & StateSnapshot> {
    await this.ensureInitialized();
    return this.invoke(() => {
      const state = this.serializeState(true); // use editor for hints
      const result = this.service.getHintContext(
        state.position.location,
        "chat",
      );
      return { result, state };
    });
  }

  // ─── Navigation & actions (open the panel) ───

  /**
   * Show the current learning activity.
   */
  async show(): Promise<StateSnapshot> {
    await this.ensureInitialized();
    return this.invoke(async () => {
      await this.showActivity();
      return { state: this.serializeState(true) }; // no-ops for notebooks, so editor state is more accurate
    });
  }

  /**
   * Move to the next item.
   */
  async next(): Promise<{ moved: boolean } & StateSnapshot> {
    await this.ensureInitialized();
    this.throwIfNotQSharpCourse();
    return this.invoke(async () => {
      const r = await this.service.next("chat");
      await this.showActivity();
      return { moved: r.moved, state: this.serializeState(false) }; // Q# only
    });
  }

  /**
   * Move to the previous item.
   */
  async previous(): Promise<{ moved: boolean } & StateSnapshot> {
    await this.ensureInitialized();
    this.throwIfNotQSharpCourse();
    return this.invoke(async () => {
      const r = await this.service.previous("chat");
      await this.showActivity();
      return { moved: r.moved, state: this.serializeState(false) }; // Q# only
    });
  }

  /**
   * Jump to a specific unit/activity.
   */
  async goTo(input: {
    courseId?: string;
    unitId: string;
    activityId?: string;
  }): Promise<StateSnapshot> {
    await this.ensureInitialized();
    return this.invoke(async () => {
      const courseId = input.courseId;
      if (courseId && courseId !== this.service.getActiveCourseId()) {
        await this.service.switchCourse(courseId, "chat");
      }
      await this.service.goTo(input, "chat");
      await this.showActivity();
      return { state: this.serializeState(false) }; // workspace state is correct after goto
    });
  }

  /**
   * Run the Q# code at the current position.
   */
  async run(input: {
    shots?: number;
  }): Promise<{ result: RunResult } & StateSnapshot> {
    await this.ensureInitialized();
    this.throwIfNotQSharpCourse();
    return this.invoke(async () => {
      const r = await this.service.run(input.shots ?? 1, "chat");
      await this.showActivity();
      return { result: r.result, state: this.serializeState(false) }; // Q# only
    });
  }

  /**
   * Check the student's solution. Marks it complete on pass.
   */
  async check(): Promise<{ result: SolutionCheckResult } & StateSnapshot> {
    await this.ensureInitialized();
    this.throwIfNotQSharpCourse();
    return this.invoke(async () => {
      const r = await this.service.checkSolution("chat");
      await this.showActivity();
      return { result: r.result, state: this.serializeState(false) }; // Q# only
    });
  }

  /**
   * Reset the current exercise to its original placeholder code
   * and clear its completion status.
   */
  async resetExercise(): Promise<StateSnapshot> {
    await this.ensureInitialized();
    this.throwIfNotQSharpCourse();
    return this.invoke(async () => {
      await this.service.resetExercise("chat");
      await this.showActivity();
      return { state: this.serializeState(false) }; // Q# only
    });
  }

  /**
   * Show the reference solution code(s).
   */
  async solution(): Promise<{ solutions: string[] } & StateSnapshot> {
    await this.ensureInitialized();
    return this.invoke(async () => {
      const state = this.serializeState(true); // use editor for solutions
      const solutions = this.service.getAllSolutions(
        state.position.location,
        "chat",
      );
      await this.showActivity();
      return { solutions, state };
    });
  }

  // ─── Helpers ───

  /**
   * Wrap a service call so that plain `Error`s thrown for expected
   * conditions (wrong activity type, unknown unit ID, etc.) are
   * surfaced to the model as {@link CopilotToolError}.
   */
  private async invoke<T>(fn: () => T | Promise<T>): Promise<T> {
    try {
      return await fn();
    } catch (e) {
      if (e instanceof CopilotToolError) {
        throw e;
      }
      if (e instanceof Error) {
        throw new CopilotToolError(e.message);
      }
      throw e;
    }
  }

  /**
   * If the active course isn't a Q# course (i.e. the katas), throw an exception indicating
   * that the current operation isn't supported.
   */
  private throwIfNotQSharpCourse(): void {
    const { kind } = this.service.getActiveCourseInfo();
    if (kind !== "qsharp") {
      throw new CopilotToolError(
        "This operation is only supported for Q# courses.",
      );
    }
  }

  private async showActivity(): Promise<void> {
    await vscode.commands.executeCommand("qsharp-vscode.learningShowActivity");
  }

  private getCurrentFileUri(): vscode.Uri {
    const uri = this.service.getCurrentCodeFileUri();
    if (!uri) {
      throw new CopilotToolError(
        "Current activity is not an exercise or example — there is no code to read.",
      );
    }
    return uri;
  }

  /**
   * Build a compact snapshot of position and progress to attach to
   * every tool response.
   */
  private serializeState(considerEditor: boolean): SerializedLearningState {
    // The notebook learning state will be undefined if we're not in a notebook
    // or if we couldn't identify the current cell for some reason.
    // In that case, we fall back to the current state.
    const state =
      (considerEditor ? this.notebookLearningState() : undefined) ??
      this.service.getState();

    const progress = state.progress;
    const unitId = progress.currentPosition?.unitId;
    const currentUnit = unitId
      ? progress.units.find((u) => u.id === unitId)
      : undefined;

    return {
      course: state.course,
      position: state.position,
      progress: {
        totalActivities: progress.stats.totalActivities,
        completedActivities: progress.stats.completedActivities,
        currentUnitCompleted: currentUnit?.completed ?? 0,
        currentUnitTotal: currentUnit?.total ?? 0,
      },
    };
  }

  /**
   * For notebook courses, build LearningState from the selected cell
   * rather than from the service's stored position.
   */
  private notebookLearningState(): LearningState | undefined {
    const editor = vscode.window.activeNotebookEditor;
    const selection = editor?.selections[0];
    if (!selection || !editor) {
      return undefined;
    }

    const cell = editor.notebook.cellAt(selection.start);
    const cellId = cell.metadata?.id;
    if (typeof cellId !== "string") {
      return undefined;
    }

    return this.service.getLearningStateForCell(
      cellId,
      cell.document.getText(),
      editor.notebook.uri,
    );
  }
}
