// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { LearningService } from "../learningService/index.js";
import { detectKatasWorkspace } from "../katasProgress/detector.js";
import { CopilotToolError } from "./types.js";
import { QSharpTools } from "./qsharpTools.js";

/**
 * Wraps the shared {@link LearningService} singleton for use as
 * `vscode.lm` language model tools. One method per tool, following
 * the same pattern as {@link QSharpTools}.
 */
export class LearningTools {
  constructor(
    private readonly service: LearningService,
    private readonly qsharpTools: QSharpTools,
  ) {}

  private ensureInitialized(): void {
    if (!this.service.initialized) {
      throw new CopilotToolError(
        "The QDK Learning workspace has not been initialized. " +
          "Call the qdk-learning-init tool first to set up the learning workspace.",
      );
    }
  }

  /**
   * Initialize the learning workspace. Auto-detects the workspace root
   * from the open VS Code workspace folders, or uses the provided path.
   */
  async init(input: {
    workspacePath?: string;
  }): Promise<{ workspacePath: string; state: unknown }> {
    let workspaceRoot: vscode.Uri;

    if (input.workspacePath) {
      workspaceRoot = vscode.Uri.file(input.workspacePath);
    } else {
      const detected = await detectKatasWorkspace();
      if (detected) {
        workspaceRoot = detected.workspaceRoot;
      } else {
        // Fall back to first workspace folder
        const folders = vscode.workspace.workspaceFolders;
        if (!folders || folders.length === 0) {
          throw new CopilotToolError(
            "No workspace folder is open. Open a folder first, then try again.",
          );
        }
        workspaceRoot = folders[0].uri;
      }
    }

    const katasRoot = vscode.Uri.joinPath(workspaceRoot, "qdk-learning-ws");

    // Create qdk-learning.json if it doesn't exist
    const learningFile = vscode.Uri.joinPath(
      workspaceRoot,
      "qdk-learning.json",
    );
    try {
      await vscode.workspace.fs.stat(learningFile);
    } catch {
      // File doesn't exist — create it
      const defaultData = {
        version: 1,
        katasRoot: "./qdk-learning-ws",
        position: { kataId: "", sectionId: "", itemIndex: 0 },
        completions: {},
        startedAt: new Date().toISOString(),
      };
      await vscode.workspace.fs.writeFile(
        learningFile,
        new TextEncoder().encode(JSON.stringify(defaultData, null, 2)),
      );
    }

    if (!this.service.initialized) {
      await this.service.initialize(workspaceRoot, katasRoot);
    }

    return {
      workspacePath: workspaceRoot.fsPath,
      state: this.serializeState(),
    };
  }

  /**
   * Open the full-size Quantum Katas panel at the current position.
   */
  async showPanel(): Promise<{ state: unknown }> {
    this.ensureInitialized();
    await this.openPanel();
    return { state: this.serializeState() };
  }

  /**
   * Read the current learning position and progress.
   */
  getState(): { state: unknown } {
    this.ensureInitialized();
    return { state: this.serializeState() };
  }

  /**
   * Return the full per-kata progress breakdown.
   */
  getProgress(): { progress: unknown; state: unknown } {
    this.ensureInitialized();
    const progress = this.service.getProgress();
    return {
      progress: this.serializeProgressFull(progress),
      state: this.serializeState(),
    };
  }

  /**
   * List all available katas with completion status.
   */
  listKatas(): { katas: unknown; state: unknown } {
    this.ensureInitialized();
    return {
      katas: this.service.listKatas(),
      state: this.serializeState(),
    };
  }

  /**
   * Move to the next item.
   */
  async next(): Promise<{ moved: boolean; state: unknown }> {
    this.ensureInitialized();
    const r = this.service.next();
    await this.openPanel();
    return { moved: r.moved, state: this.serializeState() };
  }

  /**
   * Move to the previous item.
   */
  async previous(): Promise<{ moved: boolean; state: unknown }> {
    this.ensureInitialized();
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
    this.ensureInitialized();
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
    this.ensureInitialized();
    const r = await this.service.run(input.shots ?? 1);
    await this.openPanel();
    return { result: r.result, state: this.serializeState() };
  }

  /**
   * Run with noise simulation.
   */
  async runWithNoise(input: {
    shots?: number;
  }): Promise<{ result: unknown; state: unknown }> {
    this.ensureInitialized();
    // For now, noise simulation uses the same run path with more shots.
    // The LearningService.run method handles the execution.
    const r = await this.service.run(input.shots ?? 100);
    await this.openPanel();
    return { result: r.result, state: this.serializeState() };
  }

  /**
   * Generate the quantum circuit diagram for the current Q# code.
   * Delegates to the existing qdk-generate-circuit tool with the current file.
   */
  async circuit(): Promise<{ result: unknown; state: unknown }> {
    this.ensureInitialized();
    const filePath = this.getCurrentFilePath();
    const result = await this.qsharpTools.generateCircuit({ filePath });
    await this.openPanel();
    return { result, state: this.serializeState() };
  }

  /**
   * Estimate physical resources for the current Q# code.
   * Delegates to the existing qdk-run-resource-estimator tool with the current file.
   */
  async estimate(): Promise<{ result: unknown; state: unknown }> {
    this.ensureInitialized();
    const filePath = this.getCurrentFilePath();
    const result = await this.qsharpTools.runResourceEstimator({ filePath });
    await this.openPanel();
    return { result, state: this.serializeState() };
  }

  /**
   * Check the student's solution. Marks it complete on pass.
   */
  async check(): Promise<{ result: unknown; state: unknown }> {
    this.ensureInitialized();
    const r = await this.service.checkSolution();
    await this.openPanel();
    return { result: r.result, state: this.serializeState() };
  }

  /**
   * Return all built-in hints for the current exercise.
   */
  async hint(): Promise<{ result: unknown; state: unknown }> {
    this.ensureInitialized();
    const r = this.service.getAllHints();
    return { result: r.result, state: this.serializeState() };
  }

  /**
   * Reveal the answer to the current lesson question.
   */
  async revealAnswer(): Promise<{ result: unknown; state: unknown }> {
    this.ensureInitialized();
    const r = this.service.revealAnswer();
    await this.openPanel();
    return { result: r.result, state: this.serializeState() };
  }

  /**
   * Show the full reference solution code.
   */
  async solution(): Promise<{ result: string; state: unknown }> {
    this.ensureInitialized();
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
    const currentKata = cur ? progress.katas.get(cur) : undefined;
    return {
      position: state.position,
      actions: state.actions,
      progress: {
        stats: progress.stats,
        currentPosition: progress.currentPosition,
        katas: currentKata ? { [cur as string]: currentKata } : {},
      },
    };
  }

  private serializeProgressFull(
    progress: import("../learningService/types.js").OverallProgress,
  ): object {
    return {
      ...progress,
      katas: Object.fromEntries(progress.katas),
    };
  }
}
