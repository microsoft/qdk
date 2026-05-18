// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import {
  LearningService,
  LEARNING_WORKSPACE_FOLDER,
  detectLearningWorkspace,
  resolveNewWorkspaceRoot,
  type HintContext,
  type UnitSummary,
  type OverallProgress,
  type CurrentActivity,
  type RunResult,
  type SolutionCheckResult,
} from "../learning/index.js";
import { CopilotToolError } from "./types.js";

/**
 * Compact snapshot of the learner's current position and progress.
 *
 * Returned alongside every learning-tool response so the language model
 * always has up-to-date context about where the student is in the
 * curriculum without needing a separate round-trip.
 */
export interface SerializedLearningState {
  position: CurrentActivity;
  progress: {
    totalActivities: number;
    completedActivities: number;
    currentUnitCompleted: number;
    currentUnitTotal: number;
  };
}

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
    | { initialized: false }
    | { initialized: true; state: SerializedLearningState }
  > {
    if (!this.service.initialized) {
      const detected = await detectLearningWorkspace();
      if (!detected) {
        return { initialized: false };
      }
      await this.ensureInitialized();
    }
    return { initialized: true, state: this.serializeState() };
  }

  /**
   * Return the full per-kata progress breakdown.
   */
  async getProgress(): Promise<{
    progress: OverallProgress;
    state: SerializedLearningState;
  }> {
    await this.ensureInitialized();
    const progress = this.service.getProgress();
    return {
      progress,
      state: this.serializeState(),
    };
  }

  /**
   * List all available units with completion status.
   */
  async listUnits(): Promise<{
    units: UnitSummary[];
    state: SerializedLearningState;
  }> {
    await this.ensureInitialized();
    return {
      units: this.service.listUnits(),
      state: this.serializeState(),
    };
  }

  /**
   * Read the user's current Q# code at the active exercise or example.
   */
  async readCode(): Promise<{
    code: string;
    filePath: string;
    state: SerializedLearningState;
  }> {
    await this.ensureInitialized();
    const uri = this.getCurrentFileUri();
    const isExercise =
      this.service.getCurrentActivity().content.type === "exercise";
    const code = isExercise
      ? await this.service.readUserCode()
      : new TextDecoder().decode(await vscode.workspace.fs.readFile(uri));
    return { code, filePath: uri.fsPath, state: this.serializeState() };
  }

  /**
   * Return all built-in hints for the current exercise.
   */
  async hint(): Promise<{
    result: HintContext | null;
    state: SerializedLearningState;
  }> {
    await this.ensureInitialized();
    return this.invoke(() => {
      const r = this.service.getHintContext("chat");
      return { result: r.result, state: this.serializeState() };
    });
  }

  // ─── Navigation & actions (open the panel) ───

  /**
   * Show the current learning activity.
   */
  async show(): Promise<{ state: SerializedLearningState }> {
    await this.ensureInitialized();
    await this.showActivity();
    return { state: this.serializeState() };
  }

  /**
   * Move to the next item.
   */
  async next(): Promise<{ moved: boolean; state: SerializedLearningState }> {
    await this.ensureInitialized();
    const r = this.service.next("chat");
    await this.showActivity();
    return { moved: r.moved, state: this.serializeState() };
  }

  /**
   * Move to the previous item.
   */
  async previous(): Promise<{
    moved: boolean;
    state: SerializedLearningState;
  }> {
    await this.ensureInitialized();
    const r = this.service.previous("chat");
    await this.showActivity();
    return { moved: r.moved, state: this.serializeState() };
  }

  /**
   * Jump to a specific unit/activity.
   */
  async goTo(input: {
    courseId?: string;
    unitId: string;
    activityId?: string;
  }): Promise<{ state: SerializedLearningState }> {
    await this.ensureInitialized();
    return this.invoke(async () => {
      this.service.goTo(input, "chat");
      await this.showActivity();
      return { state: this.serializeState() };
    });
  }

  /**
   * Run the Q# code at the current position.
   */
  async run(input: {
    shots?: number;
  }): Promise<{ result: RunResult; state: SerializedLearningState }> {
    await this.ensureInitialized();
    return this.invoke(async () => {
      const r = await this.service.run(input.shots ?? 1, "chat");
      await this.showActivity();
      return { result: r.result, state: this.serializeState() };
    });
  }

  /**
   * Check the student's solution. Marks it complete on pass.
   */
  async check(): Promise<{
    result: SolutionCheckResult;
    state: SerializedLearningState;
  }> {
    await this.ensureInitialized();
    return this.invoke(async () => {
      const r = await this.service.checkSolution("chat");
      await this.showActivity();
      return { result: r.result, state: this.serializeState() };
    });
  }

  /**
   * Reset the current exercise to its original placeholder code
   * and clear its completion status.
   */
  async resetExercise(): Promise<{ state: SerializedLearningState }> {
    await this.ensureInitialized();
    return this.invoke(async () => {
      await this.service.resetExercise("chat");
      await this.showActivity();
      return { state: this.serializeState() };
    });
  }

  /**
   * Show the full reference solution code.
   */
  async solution(): Promise<{
    result: string;
    state: SerializedLearningState;
  }> {
    await this.ensureInitialized();
    return this.invoke(async () => {
      const result = this.service.getFullSolution("chat");
      await this.showActivity();
      return { result, state: this.serializeState() };
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
  private serializeState(): SerializedLearningState {
    const state = this.service.getState();
    const progress = state.progress;
    const cur = progress.currentPosition?.unitId;
    const currentUnit = cur
      ? progress.units.find((u) => u.id === cur)
      : undefined;

    return {
      position: state.position,
      progress: {
        totalActivities: progress.stats.totalActivities,
        completedActivities: progress.stats.completedActivities,
        currentUnitCompleted: currentUnit?.completed ?? 0,
        currentUnitTotal: currentUnit?.total ?? 0,
      },
    };
  }
}
