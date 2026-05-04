// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import {
  LearningService,
  KATAS_WS_FOLDER,
  detectLearningWorkspace,
  type HintContext,
  type UnitSummary,
  type OverallProgress,
  type Position,
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
  position: Position;
  progress: {
    stats: { totalActivities: number; completedActivities: number };
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
   * Called by `prepareInvocation` on every learning tool.
   *
   * Returns a confirmation prompt when the workspace needs first-time
   * setup (no `qdk-learning.json` on disk), or `undefined` to skip
   * confirmation when setup already exists or the service is loaded.
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

    const folders = vscode.workspace.workspaceFolders;
    if (!folders || folders.length === 0) {
      // No workspace — let invoke() surface the error.
      return undefined;
    }
    const workspacePath = folders[0].uri.fsPath;

    return {
      confirmationMessages: {
        title: "Initialize QDK Learning workspace",
        message:
          `Set up a Quantum Katas learning workspace in **${workspacePath}**? ` +
          `Exercise files and progress tracking will be created in a \`${KATAS_WS_FOLDER}\` subfolder.`,
      },
    };
  }

  /**
   * Ensures the learning service is initialized, creating workspace
   * files if needed. Called at the start of every tool invocation
   * (after the user has already approved via {@link confirmInit}).
   */
  private async ensureInitialized(): Promise<void> {
    const ok = await this.service.ensureInitialized({ createIfMissing: true });
    if (!ok) {
      throw new CopilotToolError(
        "No workspace folder is open. Open a folder first, then try again.",
      );
    }
  }

  /**
   * Open the full-size Quantum Katas panel at the current position.
   */
  async showPanel(): Promise<{ state: SerializedLearningState }> {
    await this.ensureInitialized();
    await this.openPanel();
    return { state: this.serializeState() };
  }

  /**
   * Read the current learning position and progress.
   */
  async getState(): Promise<{ state: SerializedLearningState }> {
    await this.ensureInitialized();
    return { state: this.serializeState() };
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
      progress: this.serializeProgress(progress),
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
   * Move to the next item.
   */
  async next(): Promise<{ moved: boolean; state: SerializedLearningState }> {
    await this.ensureInitialized();
    const r = this.service.next();
    await this.openPanel();
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
    const r = this.service.previous();
    await this.openPanel();
    return { moved: r.moved, state: this.serializeState() };
  }

  /**
   * Jump to a specific unit/activity.
   */
  async goTo(input: {
    unitId: string;
    activityId?: string;
  }): Promise<{ state: SerializedLearningState }> {
    await this.ensureInitialized();
    this.service.goTo(input.unitId, input.activityId);
    await this.openPanel();
    return { state: this.serializeState() };
  }

  /**
   * Run the Q# code at the current position.
   */
  async run(input: {
    shots?: number;
  }): Promise<{ result: RunResult; state: SerializedLearningState }> {
    await this.ensureInitialized();
    const r = await this.service.run(input.shots ?? 1);
    await this.openPanel();
    return { result: r.result, state: this.serializeState() };
  }

  /**
   * Read the user's current Q# code at the active exercise or example.
   * Silent read — does not open the panel.
   */
  async readCode(): Promise<{
    code: string;
    filePath: string;
    state: SerializedLearningState;
  }> {
    await this.ensureInitialized();
    const uri = this.getCurrentFileUri();
    const pos = this.service.getPosition();
    const code =
      pos.content.type === "exercise"
        ? await this.service.readUserCode()
        : new TextDecoder().decode(await vscode.workspace.fs.readFile(uri));
    return { code, filePath: uri.fsPath, state: this.serializeState() };
  }

  /**
   * Reset the current exercise to its original placeholder code
   * and clear its completion status.
   */
  async resetExercise(): Promise<{ state: SerializedLearningState }> {
    await this.ensureInitialized();
    await this.service.resetExercise();
    await this.openPanel();
    return { state: this.serializeState() };
  }

  /**
   * Check the student's solution. Marks it complete on pass.
   */
  async check(): Promise<{
    result: SolutionCheckResult;
    state: SerializedLearningState;
  }> {
    await this.ensureInitialized();
    const r = await this.service.checkSolution();
    await this.openPanel();
    return { result: r.result, state: this.serializeState() };
  }

  /**
   * Return all built-in hints for the current exercise.
   */
  async hint(): Promise<{
    result: HintContext | null;
    state: SerializedLearningState;
  }> {
    await this.ensureInitialized();
    const r = this.service.getHintContext();
    return { result: r.result, state: this.serializeState() };
  }

  /**
   * Show the full reference solution code.
   */
  async solution(): Promise<{
    result: string;
    state: SerializedLearningState;
  }> {
    await this.ensureInitialized();
    const result = this.service.getFullSolution();
    await this.openPanel();
    return { result, state: this.serializeState() };
  }

  // ─── Helpers ───

  private async openPanel(): Promise<void> {
    await vscode.commands.executeCommand("qsharp-vscode.showKatas");
  }

  private getCurrentFileUri(): vscode.Uri {
    const pos = this.service.getPosition();
    if (pos.content.type === "exercise") {
      return this.service.getExerciseFileUri();
    } else if (pos.content.type === "lesson-example") {
      return this.service.getExampleFileUri();
    }
    throw new CopilotToolError(
      "Current activity is not an exercise or example — there is no code to read.",
    );
  }

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
        stats: progress.stats,
        currentUnitCompleted: currentUnit?.completed ?? 0,
        currentUnitTotal: currentUnit?.total ?? 0,
      },
    };
  }

  private serializeProgress(progress: OverallProgress): OverallProgress {
    return progress;
  }
}
