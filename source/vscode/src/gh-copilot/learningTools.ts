// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { LearningService, KATAS_WS_FOLDER } from "../learning/index.js";
import {
  detectKatasWorkspace,
  LEARNING_FILE,
} from "../learning/progress/detector.js";
import { CopilotToolError } from "./types.js";
import { QSharpTools } from "./qsharpTools.js";

/**
 * Wraps the shared {@link LearningService} singleton for use as
 * `vscode.lm` language model tools.
 */
export class LearningTools {
  constructor(
    private readonly service: LearningService,
    private readonly qsharpTools: QSharpTools,
  ) {}

  // ─── Auto-init helpers ───

  /**
   * Resolve the workspace root to use for initialization.
   * Checks for an existing `qdk-learning.json`, then falls back to the
   * first open workspace folder.
   *
   * Side-effect free — safe for {@link confirmInit}.
   */
  private async resolveWorkspaceRoot(): Promise<vscode.Uri> {
    const detected = await detectKatasWorkspace();
    if (detected) return detected.workspaceRoot;

    const folders = vscode.workspace.workspaceFolders;
    if (!folders || folders.length === 0) {
      throw new CopilotToolError(
        "No workspace folder is open. Open a folder first, then try again.",
      );
    }
    return folders[0].uri;
  }

  /**
   * Called by `prepareInvocation` on every learning tool.
   *
   * Returns a confirmation prompt when the service has not yet been
   * initialized (so the user can approve file creation), or `undefined`
   * to skip confirmation when already initialized.
   *
   * **Must be free of side-effects** — only reads state and the filesystem.
   */
  async confirmInit(): Promise<vscode.PreparedToolInvocation | undefined> {
    if (this.service.initialized) return undefined;

    let workspacePath: string;
    try {
      workspacePath = (await this.resolveWorkspaceRoot()).fsPath;
    } catch {
      // Can't resolve — let invoke() surface the error.
      return undefined;
    }

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
   * Ensures the learning service is initialized, performing first-time
   * setup if necessary. Called at the start of every tool invocation
   * (after the user has already approved via {@link confirmInit}).
   */
  private async ensureInitialized(): Promise<void> {
    if (this.service.initialized) return;

    const workspaceRoot = await this.resolveWorkspaceRoot();
    const katasRoot = vscode.Uri.joinPath(workspaceRoot, KATAS_WS_FOLDER);

    // Create qdk-learning.json if it doesn't exist
    const learningFile = vscode.Uri.joinPath(workspaceRoot, LEARNING_FILE);
    try {
      await vscode.workspace.fs.stat(learningFile);
    } catch {
      const defaultData = {
        version: 1,
        position: { kataId: "", sectionId: "", itemIndex: 0 },
        completions: {},
        startedAt: new Date().toISOString(),
      };
      await vscode.workspace.fs.writeFile(
        learningFile,
        new TextEncoder().encode(JSON.stringify(defaultData, null, 2)),
      );
    }

    await this.service.initialize(workspaceRoot, katasRoot);
  }

  /**
   * Open the full-size Quantum Katas panel at the current position.
   */
  async showPanel(): Promise<{ state: unknown }> {
    await this.ensureInitialized();
    await this.openPanel();
    return { state: this.serializeState() };
  }

  /**
   * Read the current learning position and progress.
   */
  async getState(): Promise<{ state: unknown }> {
    await this.ensureInitialized();
    return { state: this.serializeState() };
  }

  /**
   * Return the full per-kata progress breakdown.
   */
  async getProgress(): Promise<{ progress: unknown; state: unknown }> {
    await this.ensureInitialized();
    const progress = this.service.getProgress();
    return {
      progress: this.serializeProgressFull(progress),
      state: this.serializeState(),
    };
  }

  /**
   * List all available katas with completion status.
   */
  async listKatas(): Promise<{ katas: unknown; state: unknown }> {
    await this.ensureInitialized();
    return {
      katas: this.service.listKatas(),
      state: this.serializeState(),
    };
  }

  /**
   * Move to the next item.
   */
  async next(): Promise<{ moved: boolean; state: unknown }> {
    await this.ensureInitialized();
    const r = this.service.next();
    await this.openPanel();
    return { moved: r.moved, state: this.serializeState() };
  }

  /**
   * Move to the previous item.
   */
  async previous(): Promise<{ moved: boolean; state: unknown }> {
    await this.ensureInitialized();
    const r = this.service.previous();
    await this.openPanel();
    return { moved: r.moved, state: this.serializeState() };
  }

  /**
   * Jump to a specific kata/section.
   */
  async goTo(input: {
    kataId: string;
    sectionId?: string;
    itemIndex?: number;
  }): Promise<{ state: unknown }> {
    await this.ensureInitialized();
    this.service.goTo(input.kataId, input.sectionId, input.itemIndex ?? 0);
    await this.openPanel();
    return { state: this.serializeState() };
  }

  /**
   * Run the Q# code at the current position.
   */
  async run(input: {
    shots?: number;
  }): Promise<{ result: unknown; state: unknown }> {
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
    state: unknown;
  }> {
    await this.ensureInitialized();
    const pos = this.service.getPosition();
    let code: string;
    let filePath: string;
    if (pos.item.type === "exercise") {
      code = await this.service.readUserCode();
      filePath = this.service.getExerciseFileUri().fsPath;
    } else if (pos.item.type === "lesson-example") {
      const uri = this.service.getExampleFileUri();
      const bytes = await vscode.workspace.fs.readFile(uri);
      code = new TextDecoder().decode(bytes);
      filePath = uri.fsPath;
    } else {
      throw new CopilotToolError(
        "Current item is not an exercise or example — there is no code to read.",
      );
    }
    return { code, filePath, state: this.serializeState() };
  }

  /**
   * Reset the current exercise to its original placeholder code.
   */
  async resetExercise(): Promise<{ state: unknown }> {
    await this.ensureInitialized();
    await this.service.resetExercise();
    await this.openPanel();
    return { state: this.serializeState() };
  }

  /**
   * Estimate physical resources for the current Q# code.
   * Delegates to the existing qdk-run-resource-estimator tool with the current file.
   */
  async estimate(): Promise<{ result: unknown; state: unknown }> {
    await this.ensureInitialized();
    const filePath = this.getCurrentFilePath();
    const result = await this.qsharpTools.runResourceEstimator({ filePath });
    await this.openPanel();
    return { result, state: this.serializeState() };
  }

  /**
   * Check the student's solution. Marks it complete on pass.
   */
  async check(): Promise<{ result: unknown; state: unknown }> {
    await this.ensureInitialized();
    const r = await this.service.checkSolution();
    await this.openPanel();
    return { result: r.result, state: this.serializeState() };
  }

  /**
   * Return all built-in hints for the current exercise.
   */
  async hint(): Promise<{ result: unknown; state: unknown }> {
    await this.ensureInitialized();
    const r = this.service.getAllHints();
    return { result: r.result, state: this.serializeState() };
  }

  /**
   * Reveal the answer to the current lesson question.
   */
  async revealAnswer(): Promise<{ result: unknown; state: unknown }> {
    await this.ensureInitialized();
    const r = this.service.revealAnswer();
    await this.openPanel();
    return { result: r.result, state: this.serializeState() };
  }

  /**
   * Show the full reference solution code.
   */
  async solution(): Promise<{ result: string; state: unknown }> {
    await this.ensureInitialized();
    const result = this.service.getFullSolution();
    await this.openPanel();
    return { result, state: this.serializeState() };
  }

  // ─── Helpers ───

  private async openPanel(): Promise<void> {
    await vscode.commands.executeCommand("qsharp-vscode.showKatas");
  }

  private getCurrentFilePath(): string {
    const pos = this.service.getPosition();
    if (pos.item.type === "exercise") {
      return this.service.getExerciseFileUri().fsPath;
    } else if (pos.item.type === "lesson-example") {
      return this.service.getExampleFileUri().fsPath;
    }
    throw new CopilotToolError(
      "Current item is not an exercise or example — cannot run code operations on it.",
    );
  }

  // ─── Serialization helpers ───

  private serializeState(): object {
    const state = this.service.getState();
    const progress = state.progress;
    // Compact progress: only current kata's progress + headline stats
    const cur = progress.currentPosition?.kataId;
    const currentKata = cur ? progress.katas[cur] : undefined;

    // Strip the answer from lesson-question items so the model
    // doesn't see it until reveal-answer is explicitly called.
    let position = state.position;
    if (position.item.type === "lesson-question") {
      const { answer: _answer, ...itemWithoutAnswer } = position.item;
      position = {
        ...position,
        item: itemWithoutAnswer as typeof position.item,
      };
    }

    return {
      position,
      actions: state.actions,
      progress: {
        stats: progress.stats,
        currentPosition: progress.currentPosition,
        katas: currentKata ? { [cur as string]: currentKata } : {},
      },
    };
  }

  private serializeProgressFull(
    progress: import("../learning/types.js").OverallProgress,
  ): object {
    return progress;
  }
}
