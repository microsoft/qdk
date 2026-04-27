// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * In-proc Katas engine for the VS Code extension host.
 *
 * Re-implements the navigation / progress / scaffolding logic from
 * `learning/server/server.ts` using `vscode.workspace.fs` instead of
 * `node:fs`. No compiler dependency — run/circuit/check are handled
 * externally by the panel manager via existing VS Code commands and the
 * compiler worker.
 */

import * as vscode from "vscode";
import { getAllKatas } from "qsharp-lang/katas-md";
import type {
  Kata,
  Exercise,
  Lesson,
  LessonItem,
  Solution,
  ContentItem,
} from "qsharp-lang/katas-md";
import type {
  Position,
  NavigationItem,
  LessonTextItem,
  LessonExampleItem,
  LessonQuestionItem,
  ExerciseItem,
  PrimaryAction,
  ActionGroup,
  KatasState,
  NavigationResult,
  HintResult,
  OverallProgress,
  KataProgress,
  SectionProgress,
  ProgressFileData,
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

export class KatasEngine {
  private katas: Kata[] = [];
  private flatPositions: FlatPosition[] = [];
  private currentFlatIndex = 0;
  private ranExamples = new Set<string>();
  private hintRevealCount = new Map<string, number>();

  private workspaceRoot!: vscode.Uri;
  private katasRoot!: vscode.Uri;
  private learningFile!: vscode.Uri;
  private katasRootRel = "./quantum-katas";

  // ── Progress data (mirrors qdk-learning.json) ──
  private progressData!: ProgressFileData;

  // ─── Lifecycle ───

  async initialize(
    workspaceRoot: vscode.Uri,
    katasRoot: vscode.Uri,
  ): Promise<void> {
    this.workspaceRoot = workspaceRoot;
    this.katasRoot = katasRoot;
    this.learningFile = vscode.Uri.joinPath(workspaceRoot, "qdk-learning.json");

    // Read katasRootRel from the learning file if it exists
    try {
      const bytes = await vscode.workspace.fs.readFile(this.learningFile);
      const parsed = JSON.parse(
        decoder.decode(bytes),
      ) as Partial<ProgressFileData>;
      if (parsed.katasRoot && typeof parsed.katasRoot === "string") {
        this.katasRootRel = parsed.katasRoot;
      }
    } catch {
      // Missing or corrupt — use default
    }

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
      if (idx >= 0) this.currentFlatIndex = idx;
    }
  }

  dispose(): void {
    this.saveProgress().catch(() => {});
  }

  // ─── Navigation ───

  getPosition(): Position {
    const fp = this.flatPositions[this.currentFlatIndex];
    if (!fp) {
      throw new Error("No position available — have you called initialize()?");
    }
    return {
      kataId: fp.kataId,
      sectionId: fp.sectionId,
      itemIndex: fp.itemIndex,
      item: this.resolveNavigationItem(fp),
    };
  }

  getState(): KatasState {
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
    return { moved: true, state: this.getState() };
  }

  previous(): NavigationResult {
    if (this.currentFlatIndex <= 0) {
      return { moved: false, state: this.getState() };
    }

    this.currentFlatIndex--;
    this.syncPosition();
    return { moved: true, state: this.getState() };
  }

  goTo(kataId: string, sectionId?: string, itemIndex: number = 0): KatasState {
    if (!sectionId) {
      const firstIdx = this.flatPositions.findIndex(
        (fp) => fp.kataId === kataId,
      );
      if (firstIdx < 0) throw new Error(`Kata not found: ${kataId}`);
      this.currentFlatIndex = firstIdx;
      this.syncPosition();
      return this.getState();
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
    return this.getState();
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
      "reveal-answer": "Reveal",
    };

    const primaryGroup: ActionGroup = [
      {
        key: "space",
        label: primaryLabel[primary],
        action: primary,
        primary: true,
      },
    ];

    const navGroup: ActionGroup = [
      { key: "f", label: "Next", action: "next" },
      { key: "b", label: "Back", action: "back" },
      { key: "p", label: "Progress", action: "progress" },
    ];

    switch (pos.item.type) {
      case "lesson-text": {
        return [
          primaryGroup,
          [
            { key: "b", label: "Back", action: "back" },
            { key: "p", label: "Progress", action: "progress" },
          ],
        ];
      }
      case "lesson-example": {
        const codeTools: ActionGroup = [
          { key: "r", label: "Run", action: "run" },
          { key: "c", label: "Circuit", action: "circuit" },
        ];
        return [primaryGroup, codeTools, navGroup];
      }
      case "lesson-question": {
        return [primaryGroup, navGroup];
      }
      case "exercise": {
        const codeTools: ActionGroup = [
          { key: "r", label: "Run", action: "run" },
          { key: "c", label: "Circuit", action: "circuit" },
        ];
        const helpGroup: ActionGroup = [
          { key: "h", label: "Hint", action: "hint" },
          { key: "s", label: "Solution", action: "solution" },
        ];
        return [primaryGroup, codeTools, helpGroup, navGroup];
      }
    }
  }

  // ─── Hints & solutions ───

  getNextHint(): { result: HintResult | null; state: KatasState } {
    const exercise = this.resolveExercise();
    const hints: string[] = [];
    for (const item of exercise.explainedSolution.items) {
      if (item.type === "text-content") {
        hints.push(item.content);
      }
    }
    if (hints.length === 0) {
      return { result: null, state: this.getState() };
    }

    const current = Math.min(
      (this.hintRevealCount.get(exercise.id) ?? 0) + 1,
      hints.length,
    );
    this.hintRevealCount.set(exercise.id, current);

    return {
      result: { hint: hints[current - 1], current, total: hints.length },
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

  revealAnswer(): { result: string; state: KatasState } {
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

  /**
   * Mark the current example as run (affects primary action).
   * Called by the panel manager after executing the run command.
   */
  markExampleRun(exampleId: string): void {
    this.ranExamples.add(exampleId);
  }

  /**
   * Mark an exercise as complete and save progress. Called by the panel
   * manager after a successful check.
   */
  async markExerciseComplete(kataId: string, sectionId: string): Promise<void> {
    this.markComplete(kataId, sectionId);
    await this.saveProgress();
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

    return {
      katas: katasMap,
      currentPosition: { ...this.progressData.position },
      stats: { totalSections, completedSections },
    };
  }

  /** Reload progress from disk (called when file watcher fires). */
  async reloadProgress(): Promise<void> {
    await this.loadProgress();
    // Restore position
    const savedPos = this.progressData.position;
    if (savedPos.kataId) {
      let idx = this.flatPositions.findIndex(
        (fp) =>
          fp.kataId === savedPos.kataId && fp.sectionId === savedPos.sectionId,
      );
      if (idx >= 0) this.currentFlatIndex = idx;
    }
  }

  // ─── Private: progress persistence via vscode.workspace.fs ───

  private freshProgressData(): ProgressFileData {
    return {
      version: 1,
      katasRoot: this.katasRootRel,
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
        this.progressData = { ...parsed, katasRoot: this.katasRootRel };
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
        if (section.type !== "exercise") continue;
        const exercise = section as Exercise;
        const fileUri = vscode.Uri.joinPath(
          this.katasRoot,
          "exercises",
          kata.id,
          `${exercise.id}.qs`,
        );
        if (await this.uriExists(fileUri)) continue;
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
        if (section.type !== "lesson") continue;
        const lesson = section as Lesson;
        for (const item of lesson.items) {
          if (item.type !== "example") continue;
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
    if (!kata) throw new Error(`Kata not found: ${kataId}`);
    return kata;
  }

  private resolveExercise(): Exercise {
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
        description: exercise.description.content,
        filePath: fileUri.fsPath,
        isComplete: this.isComplete(kata.id, fp.sectionId),
        hintCount: exercise.explainedSolution.items.filter(
          (i: ContentItem) => i.type === "text-content",
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
        contentBefore: before || undefined,
        contentAfter: after || undefined,
      } satisfies LessonExampleItem;
    }

    // No example — handle text/question items
    if (items.length === 1) {
      const item = items[0];
      if (item.type === "text-content") {
        return {
          type: "lesson-text",
          content: item.content,
          sectionTitle: lesson.title,
        } satisfies LessonTextItem;
      }
      if (item.type === "question") {
        const answerContent = item.answer.items
          .map((ai) => {
            if (ai.type === "text-content") return ai.content;
            if (ai.type === "example")
              return `\`\`\`qsharp\n${ai.code}\n\`\`\``;
            return "";
          })
          .join("\n\n");
        return {
          type: "lesson-question",
          description: item.description.content,
          answer: answerContent,
          sectionTitle: lesson.title,
        } satisfies LessonQuestionItem;
      }
    }

    // Multiple items — concatenate
    const merged = items
      .map((i: LessonItem) => {
        if (i.type === "text-content") return i.content;
        return "";
      })
      .filter(Boolean)
      .join("\n");

    return {
      type: "lesson-text",
      content: merged,
      sectionTitle: lesson.title,
    } satisfies LessonTextItem;
  }
}
