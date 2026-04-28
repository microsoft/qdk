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
import { getAllKatas, getExerciseSources } from "qsharp-lang/katas-md";
import type { Kata, Exercise } from "qsharp-lang/katas-md";
import type { Lesson, LessonItem, Solution } from "qsharp-lang/katas";
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore - there are no types for this
import mk from "@vscode/markdown-it-katex";
import markdownIt from "markdown-it";
import { loadCompilerWorker } from "../common.js";
import { LEARNING_FILE } from "./index.js";
import type {
  Position,
  NavigationItem,
  LessonTextItem,
  LessonExampleItem,
  LessonQuestionItem,
  ExerciseItem,
  PrimaryAction,
  ActionGroup,
  LearningState,
  NavigationResult,
  HintContext,
  OverallProgress,
  KataProgress,
  SectionProgress,
  ProgressFileData,
  SolutionCheckResult,
  RunResult,
  KataSummary,
} from "./types.js";

/** Recommended pedagogical order — kept in sync with the learning server. */
const RECOMMENDED_ORDER = [
  "getting_started",
  "complex_arithmetic",
  "linear_algebra",
  "qubit",
  "single_qubit_gates",
  "multi_qubit_systems",
  "multi_qubit_gates",
  "preparing_states",
  "distinguishing_states",
  "measurements",
  "random_numbers",
  "deutsch_jozsa",
  "grover",
  "key_distribution",
  "graphs",
];

interface FlatPosition {
  kataId: string;
  sectionId: string;
  itemIndex: number;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8");

export class LearningService {
  private katas: Kata[] = [];
  private flatPositions: FlatPosition[] = [];
  private currentFlatIndex = 0;
  private ranExamples = new Set<string>();

  private workspaceRoot!: vscode.Uri;
  private katasRoot!: vscode.Uri;
  private learningFile!: vscode.Uri;
  private renderMarkdown: (input: string) => string;

  // ── Progress data (mirrors qdk-learning.json) ──
  private progressData!: ProgressFileData;

  private _initialized = false;

  // ── State change event ──
  private readonly _onDidChangeState = new vscode.EventEmitter<LearningState>();
  readonly onDidChangeState = this._onDidChangeState.event;

  constructor(private readonly extensionUri: vscode.Uri) {
    // Set up markdown-it + KaTeX renderer (same pipeline as the API docs panel)
    const md = markdownIt("commonmark");
    md.use(mk, {
      enableMathBlockInHtml: true,
      enableMathInlineInHtml: true,
    });
    this.renderMarkdown = (input: string) => md.render(input);
  }

  get initialized(): boolean {
    return this._initialized;
  }

  // ─── Lifecycle ───

  async initialize(
    workspaceRoot: vscode.Uri,
    katasRoot: vscode.Uri,
  ): Promise<void> {
    this.workspaceRoot = workspaceRoot;
    this.katasRoot = katasRoot;
    this.learningFile = vscode.Uri.joinPath(workspaceRoot, LEARNING_FILE);

    // Load all katas (HTML format for webview rendering)
    const allKatas = await getAllKatas();
    this.katas = [...allKatas];

    // Sort by recommended order
    this.katas.sort((a, b) => {
      const ai = RECOMMENDED_ORDER.indexOf(a.id);
      const bi = RECOMMENDED_ORDER.indexOf(b.id);
      return (ai === -1 ? 999 : ai) - (bi === -1 ? 999 : bi);
    });

    // Build flat position list
    this.flatPositions = [];
    for (const kata of this.katas) {
      for (const section of kata.sections) {
        if (section.type === "lesson") {
          // Lessons collapse to a single flat position
          this.flatPositions.push({
            kataId: kata.id,
            sectionId: section.id,
            itemIndex: 0,
          });
        } else {
          // Exercise is a single item
          this.flatPositions.push({
            kataId: kata.id,
            sectionId: section.id,
            itemIndex: 0,
          });
        }
      }
    }

    // Scaffold exercise and example files
    await this.scaffoldExercises();
    await this.scaffoldExamples();

    // Load progress
    await this.loadProgress();

    // Restore position from progress
    const savedPos = this.progressData.position;
    if (savedPos.kataId) {
      let idx = this.flatPositions.findIndex(
        (fp) =>
          fp.kataId === savedPos.kataId &&
          fp.sectionId === savedPos.sectionId &&
          fp.itemIndex === savedPos.itemIndex,
      );
      if (idx < 0) {
        idx = this.flatPositions.findIndex(
          (fp) =>
            fp.kataId === savedPos.kataId &&
            fp.sectionId === savedPos.sectionId,
        );
      }
      if (idx >= 0) {
        this.currentFlatIndex = idx;
      }
    }

    this._initialized = true;
  }

  dispose(): void {
    this.saveProgress().catch(() => {});
    this._onDidChangeState.dispose();
  }

  // ─── Navigation ───

  getPosition(): Position {
    const fp = this.flatPositions[this.currentFlatIndex];
    if (!fp) {
      throw new Error("No position available — have you called initialize()?");
    }
    const kata = this.findKata(fp.kataId);
    const section = kata.sections.find((s) => s.id === fp.sectionId)!;
    return {
      kataId: fp.kataId,
      kataTitle: kata.title,
      sectionId: fp.sectionId,
      sectionTitle: section.title,
      itemIndex: fp.itemIndex,
      item: this.resolveNavigationItem(fp),
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

    // Auto-mark lesson sections complete when crossing section boundary
    if (oldFp.kataId !== newFp.kataId || oldFp.sectionId !== newFp.sectionId) {
      const oldKata = this.findKata(oldFp.kataId);
      const oldSection = oldKata.sections.find((s) => s.id === oldFp.sectionId);
      if (oldSection?.type === "lesson") {
        this.markComplete(oldFp.kataId, oldFp.sectionId);
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

  goTo(
    kataId: string,
    sectionId?: string,
    itemIndex: number = 0,
  ): LearningState {
    if (!sectionId) {
      const firstIdx = this.flatPositions.findIndex(
        (fp) => fp.kataId === kataId,
      );
      if (firstIdx < 0) {
        throw new Error(`Kata not found: ${kataId}`);
      }
      this.currentFlatIndex = firstIdx;
      this.syncPosition();
      const state = this.getState();
      this._onDidChangeState.fire(state);
      return state;
    }

    let idx = this.flatPositions.findIndex(
      (fp) =>
        fp.kataId === kataId &&
        fp.sectionId === sectionId &&
        fp.itemIndex === itemIndex,
    );
    if (idx < 0 && itemIndex !== 0) {
      idx = this.flatPositions.findIndex(
        (fp) => fp.kataId === kataId && fp.sectionId === sectionId,
      );
    }
    if (idx < 0) {
      throw new Error(
        `Position not found: ${kataId} section ${sectionId} item ${itemIndex}`,
      );
    }
    this.currentFlatIndex = idx;
    this.syncPosition();
    const state = this.getState();
    this._onDidChangeState.fire(state);
    return state;
  }

  // ─── Actions ───

  getPrimaryAction(): PrimaryAction {
    const pos = this.getPosition();
    switch (pos.item.type) {
      case "lesson-text":
        return "next";
      case "lesson-example":
        return this.ranExamples.has(pos.item.id) ? "next" : "run";
      case "lesson-question":
        return "reveal-answer";
      case "exercise":
        return pos.item.isComplete ? "next" : "check";
    }
  }

  getAvailableActions(): ActionGroup[] {
    const pos = this.getPosition();
    const primary = this.getPrimaryAction();

    const primaryLabel: Record<PrimaryAction, string> = {
      next: "Next",
      run: "Run",
      check: "Check",
      "reveal-answer": "Show Answer",
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

    switch (pos.item.type) {
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
      case "lesson-question": {
        const aiGroup: ActionGroup = [
          {
            key: "d",
            label: "Discuss",
            action: "discuss-chat",
            codicon: "sparkle",
          },
        ];
        return [primaryGroup, aiGroup, navGroup];
      }
      case "exercise": {
        // When completed, keep Check available so users can re-validate.
        const codeTools: ActionGroup = pos.item.isComplete
          ? [
              { key: "c", label: "Check", action: "check" },
              { key: "r", label: "Run", action: "run" },
            ]
          : [{ key: "r", label: "Run", action: "run" }];
        const helpGroup: ActionGroup = pos.item.isComplete
          ? [{ key: "s", label: "Solution", action: "solution" }]
          : [
              {
                key: "h",
                label: "Hint",
                action: "hint-chat",
                codicon: "sparkle",
              },
              { key: "s", label: "Solution", action: "solution" },
            ];
        return [primaryGroup, codeTools, helpGroup, navGroup];
      }
    }
  }

  // ─── Hints & solutions ───

  getHintContext(): { result: HintContext | null; state: LearningState } {
    const exercise = this.resolveExercise();

    // Author-written pedagogical hints from index.md <details> blocks.
    const hints = (exercise.hints ?? []).map((h) => this.renderMarkdown(h));

    // Solution explanation prose from solution.md (text-content blocks).
    const explanationParts: string[] = [];
    for (const item of exercise.explainedSolution.items) {
      if (item.type === "text-content") {
        explanationParts.push(this.renderMarkdown(item.content));
      }
    }
    const solutionExplanation = explanationParts.join("\n");

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
    const solution = exercise.explainedSolution.items.find(
      (item): item is Solution => item.type === "solution",
    );
    return solution?.code ?? "";
  }

  revealAnswer(): { result: string; state: LearningState } {
    const pos = this.getPosition();
    if (pos.item.type !== "lesson-question") {
      throw new Error("Current item is not a question");
    }
    return { result: pos.item.answer, state: this.getState() };
  }

  // ─── Exercise file access ───

  getExerciseFileUri(): vscode.Uri {
    const pos = this.getPosition();
    if (pos.item.type !== "exercise") {
      throw new Error("Current item is not an exercise");
    }
    const exercise = this.resolveExercise();
    return vscode.Uri.joinPath(
      this.katasRoot,
      "exercises",
      pos.kataId,
      `${exercise.id}.qs`,
    );
  }

  getExampleFileUri(): vscode.Uri {
    const pos = this.getPosition();
    if (pos.item.type !== "lesson-example") {
      throw new Error("Current item is not an example");
    }
    return vscode.Uri.joinPath(
      this.katasRoot,
      "examples",
      pos.kataId,
      `${pos.item.id}.qs`,
    );
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
   * Reset the current exercise file to the original placeholder code.
   */
  async resetExercise(): Promise<void> {
    const exercise = this.resolveExercise();
    const uri = this.getExerciseFileUri();
    await vscode.workspace.fs.writeFile(
      uri,
      encoder.encode(exercise.placeholderCode),
    );
  }

  async markExerciseComplete(kataId: string, sectionId: string): Promise<void> {
    this.markComplete(kataId, sectionId);
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
    if (pos.item.type === "lesson-example") {
      this.markExampleRun(pos.item.id);
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
    if (pos.item.type !== "exercise") {
      throw new Error("Current item is not an exercise.");
    }

    const exercise = this.resolveExercise();
    const userCode = await this.readUserCode();
    const sources = await getExerciseSources(exercise);

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
        await this.markExerciseComplete(pos.kataId, pos.sectionId);
      }

      return {
        result: {
          passed,
          events,
          error: passed
            ? undefined
            : events.length > 0
              ? events
                  .filter((e) => e.message)
                  .map((e) => e.message)
                  .join("\n") || undefined
              : "Solution check failed.",
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

  listKatas(): KataSummary[] {
    const progress = this.getProgress();
    let foundFirstIncomplete = false;

    return this.katas.map((kata) => {
      const kataProgress = progress.katas[kata.id];
      const completedCount = kataProgress?.completed ?? 0;
      const sectionCount = kataProgress?.total ?? kata.sections.length;
      const allComplete = completedCount === sectionCount && sectionCount > 0;

      let recommended = false;
      if (!allComplete && !foundFirstIncomplete) {
        foundFirstIncomplete = true;
        recommended = true;
      }

      return {
        id: kata.id,
        title: kata.title,
        sectionCount,
        completedCount,
        recommended,
      };
    });
  }

  // ─── Progress ───

  getProgress(): OverallProgress {
    const katasMap = new Map<string, KataProgress>();
    let totalSections = 0;
    let completedSections = 0;

    for (const kata of this.katas) {
      const sections: SectionProgress[] = kata.sections.map((s) => {
        const isComplete = this.isComplete(kata.id, s.id);
        const key = this.completionKey(kata.id, s.id);
        return {
          id: s.id,
          title: s.title,
          type: s.type,
          isComplete,
          completedAt: this.progressData.completions[key]?.completedAt,
        };
      });
      const completed = sections.filter((s) => s.isComplete).length;
      katasMap.set(kata.id, { total: sections.length, completed, sections });
      totalSections += sections.length;
      completedSections += completed;
    }

    const currentKataId = this.progressData.position.kataId;
    const currentKata = currentKataId
      ? this.katas.find((k) => k.id === currentKataId)
      : undefined;

    return {
      katas: Object.fromEntries(katasMap),
      currentPosition: {
        ...this.progressData.position,
        kataTitle: currentKata?.title ?? currentKataId,
      },
      stats: { totalSections, completedSections },
    };
  }

  /** Reload progress from disk (called when file watcher fires). */
  async reloadProgress(): Promise<void> {
    await this.loadProgress();
    // Restore position
    const savedPos = this.progressData.position;
    if (savedPos.kataId) {
      const idx = this.flatPositions.findIndex(
        (fp) =>
          fp.kataId === savedPos.kataId && fp.sectionId === savedPos.sectionId,
      );
      if (idx >= 0) {
        this.currentFlatIndex = idx;
      }
    }
  }

  // ─── Private: compiler helpers ───

  private getCurrentCodeFileUri(): vscode.Uri {
    const pos = this.getPosition();
    if (pos.item.type === "exercise") {
      return this.getExerciseFileUri();
    } else if (pos.item.type === "lesson-example") {
      return this.getExampleFileUri();
    }
    throw new Error("Current item cannot be run.");
  }

  private async readCurrentCode(fileUri: vscode.Uri): Promise<string> {
    const bytes = await vscode.workspace.fs.readFile(fileUri);
    return decoder.decode(bytes);
  }

  private extractEvents(
    eventTarget: QscEventTarget,
  ): { type: string; message?: string }[] {
    const events: { type: string; message?: string }[] = [];
    const resultCount = eventTarget.resultCount();
    const results = eventTarget.getResults();
    for (let i = 0; i < resultCount; i++) {
      const r = results[i];
      for (const evt of r.events) {
        if (evt.type === "Message") {
          events.push({
            type: "Message",
            message: (evt as { message: string }).message,
          });
        } else {
          events.push({ type: evt.type });
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

  private extractError(
    events: { type: string; message?: string }[],
  ): string | undefined {
    const messages = events.filter((e) => e.message).map((e) => e.message);
    return messages.length > 0 ? messages.join("\n") : "Execution failed.";
  }

  // ─── Private: progress persistence via vscode.workspace.fs ───

  private freshProgressData(): ProgressFileData {
    return {
      version: 1,
      position: {
        kataId: this.katas[0]?.id ?? "",
        sectionId: this.katas[0]?.sections[0]?.id ?? "",
        itemIndex: 0,
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
        // Validate position references a known kata
        if (
          this.katas.length > 0 &&
          !this.katas.find((k) => k.id === this.progressData.position.kataId)
        ) {
          this.progressData.position = {
            kataId: this.katas[0].id,
            sectionId: this.katas[0].sections[0]?.id ?? "",
            itemIndex: 0,
          };
        }
        return;
      }
    } catch {
      // File missing or corrupt
    }
    this.progressData = this.freshProgressData();
  }

  async saveProgress(): Promise<void> {
    const json = JSON.stringify(this.progressData, null, 2);
    await vscode.workspace.fs.writeFile(
      this.learningFile,
      encoder.encode(json),
    );
  }

  private completionKey(kataId: string, sectionId: string): string {
    return `${kataId}__${sectionId}`;
  }

  private isComplete(kataId: string, sectionId: string): boolean {
    return (
      this.completionKey(kataId, sectionId) in this.progressData.completions
    );
  }

  private markComplete(kataId: string, sectionId: string): void {
    const key = this.completionKey(kataId, sectionId);
    if (!(key in this.progressData.completions)) {
      this.progressData.completions[key] = {
        completedAt: new Date().toISOString(),
      };
    }
  }

  private syncPosition(): void {
    const fp = this.flatPositions[this.currentFlatIndex];
    if (fp) {
      this.progressData.position = {
        kataId: fp.kataId,
        sectionId: fp.sectionId,
        itemIndex: fp.itemIndex,
      };
      this.saveProgress().catch(() => {});
    }
  }

  // ─── Private: scaffolding via vscode.workspace.fs ───

  private async scaffoldExercises(): Promise<void> {
    for (const kata of this.katas) {
      for (const section of kata.sections) {
        if (section.type !== "exercise") {
          continue;
        }
        const exercise = section as Exercise;
        const fileUri = vscode.Uri.joinPath(
          this.katasRoot,
          "exercises",
          kata.id,
          `${exercise.id}.qs`,
        );
        if (await this.uriExists(fileUri)) {
          continue;
        }
        await this.ensureParentDir(fileUri);
        await vscode.workspace.fs.writeFile(
          fileUri,
          encoder.encode(exercise.placeholderCode),
        );
      }
    }
  }

  private async scaffoldExamples(): Promise<void> {
    for (const kata of this.katas) {
      for (const section of kata.sections) {
        if (section.type !== "lesson") {
          continue;
        }
        const lesson = section as Lesson;
        for (const item of lesson.items) {
          if (item.type !== "example") {
            continue;
          }
          const fileUri = vscode.Uri.joinPath(
            this.katasRoot,
            "examples",
            kata.id,
            `${item.id}.qs`,
          );
          await this.ensureParentDir(fileUri);
          await vscode.workspace.fs.writeFile(
            fileUri,
            encoder.encode(item.code),
          );
        }
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

  // ─── Private: navigation item resolution ───

  private findKata(kataId: string): Kata {
    const kata = this.katas.find((k) => k.id === kataId);
    if (!kata) {
      throw new Error(`Kata not found: ${kataId}`);
    }
    return kata;
  }

  resolveExercise(): Exercise {
    const pos = this.getPosition();
    const kata = this.findKata(pos.kataId);
    const section = kata.sections.find((s) => s.id === pos.sectionId);
    if (!section || section.type !== "exercise") {
      throw new Error("Current item is not an exercise");
    }
    return section as Exercise;
  }

  private resolveNavigationItem(fp: FlatPosition): NavigationItem {
    const kata = this.findKata(fp.kataId);
    const section = kata.sections.find((s) => s.id === fp.sectionId)!;

    if (section.type === "exercise") {
      const exercise = section as Exercise;
      const fileUri = vscode.Uri.joinPath(
        this.katasRoot,
        "exercises",
        kata.id,
        `${exercise.id}.qs`,
      );
      return {
        type: "exercise",
        id: exercise.id,
        title: exercise.title,
        description: this.renderMarkdown(exercise.description.content),
        filePath: fileUri.fsPath,
        isComplete: this.isComplete(kata.id, fp.sectionId),
        hintCount:
          (exercise.hints?.length ?? 0) +
          exercise.explainedSolution.items.filter(
            (i) => i.type === "text-content",
          ).length,
      } satisfies ExerciseItem;
    }

    // Lesson section — all items collapsed into a single page
    const lesson = section as Lesson;
    const items = lesson.items;
    const exampleItem = items.find((i) => i.type === "example");

    if (exampleItem && exampleItem.type === "example") {
      const exIdx = items.indexOf(exampleItem);
      const before = items
        .slice(0, exIdx)
        .filter((i: LessonItem) => i.type === "text-content")
        .map((i: LessonItem) => (i as { content: string }).content)
        .join("\n");
      const after = items
        .slice(exIdx + 1)
        .filter((i: LessonItem) => i.type === "text-content")
        .map((i: LessonItem) => (i as { content: string }).content)
        .join("\n");

      const fileUri = vscode.Uri.joinPath(
        this.katasRoot,
        "examples",
        kata.id,
        `${exampleItem.id}.qs`,
      );
      return {
        type: "lesson-example",
        id: exampleItem.id,
        code: exampleItem.code,
        filePath: fileUri.fsPath,
        sectionTitle: lesson.title,
        contentBefore: before ? this.renderMarkdown(before) : undefined,
        contentAfter: after ? this.renderMarkdown(after) : undefined,
      } satisfies LessonExampleItem;
    }

    // No example — handle text/question items
    if (items.length === 1) {
      const item = items[0];
      if (item.type === "text-content") {
        return {
          type: "lesson-text",
          content: this.renderMarkdown(item.content),
          sectionTitle: lesson.title,
        } satisfies LessonTextItem;
      }
      if (item.type === "question") {
        const answerContent = item.answer.items
          .map((ai) => {
            if (ai.type === "text-content") {
              return ai.content;
            }
            if (ai.type === "example") {
              return `\`\`\`qsharp\n${ai.code}\n\`\`\``;
            }
            return "";
          })
          .join("\n\n");
        return {
          type: "lesson-question",
          description: this.renderMarkdown(item.description.content),
          answer: this.renderMarkdown(answerContent),
          sectionTitle: lesson.title,
        } satisfies LessonQuestionItem;
      }
    }

    // Multiple items — concatenate
    const merged = items
      .map((i: LessonItem) => {
        if (i.type === "text-content") {
          return i.content;
        }
        return "";
      })
      .filter(Boolean)
      .join("\n");

    return {
      type: "lesson-text",
      content: this.renderMarkdown(merged),
      sectionTitle: lesson.title,
    } satisfies LessonTextItem;
  }
}
