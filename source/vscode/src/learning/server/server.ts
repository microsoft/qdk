// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { join } from "node:path";
import { getExerciseSources } from "qsharp-lang/katas-md";
import { getAllKatas as getAllKatasMd } from "qsharp-lang/katas-md";
import { getAllKatas as getAllKatasHtml } from "qsharp-lang/katas";
import type {
  Kata,
  Exercise,
  Lesson,
  LessonItem,
  Solution,
} from "qsharp-lang/katas-md";
import { CompilerService } from "./compiler.js";
import { WorkspaceManager } from "./workspace.js";
import { ProgressManager } from "./progress.js";
import { NoOpAIProvider } from "./ai.js";
import type {
  IKatasServer,
  InitConfig,
  IAIProvider,
  KataSummary,
  KataDetail,
  Position,
  NavigationItem,
  LessonTextItem,
  LessonExampleItem,
  LessonQuestionItem,
  ExerciseItem,
  RunResult,
  SolutionCheckResult,
  CircuitResult,
  EstimateResult,
  OverallProgress,
  AIErrorContext,
  PrimaryAction,
  HintResult,
  ActionGroup,
  ServerState,
  StatefulResult,
  NavigationResult,
} from "./types.js";

/** Recommended pedagogical order for katas */
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

/** Internal registry entry for a resolvable item */
interface RegistryEntry {
  kataId: string;
  sectionIndex: number;
  itemIndex: number;
  section: Lesson | Exercise;
  /** The lesson sub-item, or null for exercise sections */
  lessonItem: LessonItem | null;
}

/** A flattened position in the global navigation list */
interface FlatPosition {
  kataId: string;
  sectionIndex: number;
  itemIndex: number;
}

export class KatasServer implements IKatasServer {
  private katas: Kata[] = [];
  private compiler = new CompilerService();
  private workspace!: WorkspaceManager;
  private progress!: ProgressManager;
  private aiProvider: IAIProvider = new NoOpAIProvider();

  /** Map from example/exercise ID to its registry entry */
  private itemRegistry = new Map<string, RegistryEntry>();

  /** Flat ordered list of every navigable position */
  private flatPositions: FlatPosition[] = [];

  /** Current index into flatPositions */
  private currentFlatIndex = 0;

  /** AI conversation history, scoped by section */
  private aiConversationHistory: Array<{
    role: "user" | "assistant";
    content: string;
  }> = [];

  /** AI hint history per exercise */
  private aiHintHistory = new Map<string, string[]>();

  /** Track which examples have been run this session */
  private ranExamples = new Set<string>();

  /** Track how many hints have been revealed per exercise */
  private hintRevealCount = new Map<string, number>();

  // ─── Lifecycle ───

  static readonly WORKSPACE_FOLDER = "quantum-katas";

  async initialize(config: InitConfig): Promise<void> {
    const root = join(config.workspacePath, KatasServer.WORKSPACE_FOLDER);
    this.workspace = new WorkspaceManager(root);
    this.progress = new ProgressManager(root);
    if (config.aiProvider) this.aiProvider = config.aiProvider;

    // Load katas
    const getAllKatas =
      config.contentFormat === "html" ? getAllKatasHtml : getAllKatasMd;
    const allKatas = await getAllKatas();
    if (config.kataIds.length === 0) {
      this.katas = allKatas;
    } else {
      const idSet = new Set(config.kataIds);
      this.katas = allKatas.filter((k) => idSet.has(k.id));
      for (const id of config.kataIds) {
        if (!this.katas.some((k) => k.id === id)) {
          throw new Error(`Kata not found: ${id}`);
        }
      }
    }

    // Sort by recommended order
    this.katas.sort((a, b) => {
      const ai = RECOMMENDED_ORDER.indexOf(a.id);
      const bi = RECOMMENDED_ORDER.indexOf(b.id);
      return (ai === -1 ? 999 : ai) - (bi === -1 ? 999 : bi);
    });

    // Build registry and flat position list
    this.itemRegistry.clear();
    this.flatPositions = [];

    for (const kata of this.katas) {
      for (let si = 0; si < kata.sections.length; si++) {
        const section = kata.sections[si];
        if (section.type === "lesson") {
          // Collapse all items in a lesson section into a single flat
          // position so the widget shows them as one page.
          this.flatPositions.push({
            kataId: kata.id,
            sectionIndex: si,
            itemIndex: 0,
          });
          // Still register examples by ID for quick lookup.
          for (let ii = 0; ii < section.items.length; ii++) {
            const item = section.items[ii];
            if (item.type === "example") {
              this.itemRegistry.set(item.id, {
                kataId: kata.id,
                sectionIndex: si,
                itemIndex: ii,
                section,
                lessonItem: item,
              });
            }
          }
        } else {
          // Exercise has a single item at index 0
          this.flatPositions.push({
            kataId: kata.id,
            sectionIndex: si,
            itemIndex: 0,
          });
          this.itemRegistry.set(section.id, {
            kataId: kata.id,
            sectionIndex: si,
            itemIndex: 0,
            section,
            lessonItem: null,
          });
        }
      }
    }

    // Scaffold exercise files
    await this.workspace.scaffoldExercises(this.katas);

    // Scaffold example files (read-only reference, overwritten on each init)
    await this.workspace.scaffoldExamples(this.katas);

    // Load progress
    await this.progress.load(this.katas);

    // Restore position
    const savedPos = this.progress.getPosition();
    if (savedPos.kataId) {
      let idx = this.flatPositions.findIndex(
        (fp) =>
          fp.kataId === savedPos.kataId &&
          fp.sectionIndex === savedPos.sectionIndex &&
          fp.itemIndex === savedPos.itemIndex,
      );
      // Saved position may have itemIndex > 0 from before lesson-section
      // collapsing; fall back to itemIndex 0 for the same section.
      if (idx < 0) {
        idx = this.flatPositions.findIndex(
          (fp) =>
            fp.kataId === savedPos.kataId &&
            fp.sectionIndex === savedPos.sectionIndex,
        );
      }
      if (idx >= 0) this.currentFlatIndex = idx;
    }
  }

  dispose(): void {
    // Persist progress on shutdown
    this.progress
      .save()
      .catch((err) => console.error("Failed to save progress:", err));
  }

  // ─── Catalog ───

  listKatas(): KataSummary[] {
    const completed = new Set<string>();
    // Check which katas are fully complete
    for (const kata of this.katas) {
      let allDone = true;
      for (let i = 0; i < kata.sections.length; i++) {
        if (!this.progress.isComplete(kata.id, i)) {
          allDone = false;
          break;
        }
      }
      if (allDone && kata.sections.length > 0) completed.add(kata.id);
    }

    return this.katas.map((kata, idx) => {
      let completedCount = 0;
      for (let i = 0; i < kata.sections.length; i++) {
        if (this.progress.isComplete(kata.id, i)) completedCount++;
      }

      // Recommended if it's the first incomplete kata in order
      const recommended =
        !completed.has(kata.id) &&
        (idx === 0 ||
          this.katas.slice(0, idx).every((k) => completed.has(k.id)));

      return {
        id: kata.id,
        title: kata.title,
        sectionCount: kata.sections.length,
        completedCount,
        recommended,
      };
    });
  }

  getKataDetail(kataId: string): KataDetail {
    const kata = this.findKata(kataId);
    return {
      id: kata.id,
      title: kata.title,
      sections: kata.sections.map((s, i) => ({
        index: i,
        type: s.type,
        id: s.id,
        title: s.title,
        isComplete: this.progress.isComplete(kata.id, i),
        itemCount: s.type === "lesson" ? s.items.length : 1,
      })),
    };
  }

  // ─── Navigation ───

  getPosition(): Position {
    const fp = this.flatPositions[this.currentFlatIndex];
    if (!fp) {
      throw new Error("No position available — have you called initialize()?");
    }
    return {
      kataId: fp.kataId,
      sectionIndex: fp.sectionIndex,
      itemIndex: fp.itemIndex,
      item: this.resolveNavigationItem(fp),
    };
  }

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
    const hasAI = !(this.aiProvider instanceof NoOpAIProvider);

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
      { key: "m", label: "Menu", action: "menu" },
    ];

    const quitGroup: ActionGroup = [
      { key: "q", label: "Quit", action: "quit" },
    ];

    switch (pos.item.type) {
      case "lesson-text": {
        // Back only (no "next" as explicit secondary — it's already primary)
        const nav: ActionGroup = [
          { key: "b", label: "Back", action: "back" },
          { key: "p", label: "Progress", action: "progress" },
          { key: "m", label: "Menu", action: "menu" },
        ];
        const groups = [primaryGroup, nav];
        if (hasAI)
          groups.push([{ key: "a", label: "Ask AI", action: "ask-ai" }]);
        groups.push(quitGroup);
        return groups;
      }

      case "lesson-example": {
        const codeTools: ActionGroup = [
          { key: "r", label: "Run", action: "run" },
          { key: "n", label: "Run+noise", action: "run-noise" },
          { key: "c", label: "Circuit", action: "circuit" },
          { key: "e", label: "Estimate", action: "estimate" },
        ];
        const groups = [primaryGroup, codeTools, navGroup];
        if (hasAI)
          groups.push([{ key: "a", label: "Ask AI", action: "ask-ai" }]);
        groups.push(quitGroup);
        return groups;
      }

      case "lesson-question": {
        const groups = [primaryGroup, navGroup];
        if (hasAI)
          groups.push([{ key: "a", label: "Ask AI", action: "ask-ai" }]);
        groups.push(quitGroup);
        return groups;
      }

      case "exercise": {
        const codeTools: ActionGroup = [
          { key: "r", label: "Run", action: "run" },
          { key: "n", label: "Run+noise", action: "run-noise" },
          { key: "c", label: "Circuit", action: "circuit" },
          { key: "e", label: "Estimate", action: "estimate" },
        ];
        const helpGroup: ActionGroup = [
          { key: "h", label: "Hint", action: "hint" },
        ];
        if (hasAI)
          helpGroup.push({ key: "i", label: "AI Hint", action: "ai-hint" });
        helpGroup.push({ key: "s", label: "Solution", action: "solution" });
        if (hasAI)
          helpGroup.push({ key: "a", label: "Ask AI", action: "ask-ai" });
        return [primaryGroup, codeTools, helpGroup, navGroup, quitGroup];
      }
    }
  }

  next(): NavigationResult {
    if (this.currentFlatIndex >= this.flatPositions.length - 1) {
      return { moved: false, state: this.getState() };
    }

    // Check if we're crossing a section boundary → auto-mark complete
    const oldFp = this.flatPositions[this.currentFlatIndex];
    this.currentFlatIndex++;
    const newFp = this.flatPositions[this.currentFlatIndex];

    if (
      oldFp.kataId !== newFp.kataId ||
      oldFp.sectionIndex !== newFp.sectionIndex
    ) {
      // Mark the old section complete (for lessons; exercises are marked on check)
      const oldKata = this.findKata(oldFp.kataId);
      if (oldKata.sections[oldFp.sectionIndex].type === "lesson") {
        this.progress.markComplete(oldFp.kataId, oldFp.sectionIndex);
      }
      // Clear AI conversation when moving to a new section
      this.aiConversationHistory = [];
    }

    this.syncPosition();
    return { moved: true, state: this.getState() };
  }

  previous(): NavigationResult {
    if (this.currentFlatIndex <= 0) {
      return { moved: false, state: this.getState() };
    }

    const oldFp = this.flatPositions[this.currentFlatIndex];
    this.currentFlatIndex--;
    const newFp = this.flatPositions[this.currentFlatIndex];

    if (
      oldFp.kataId !== newFp.kataId ||
      oldFp.sectionIndex !== newFp.sectionIndex
    ) {
      this.aiConversationHistory = [];
    }

    this.syncPosition();
    return { moved: true, state: this.getState() };
  }

  goTo(
    kataId: string,
    sectionIndex: number,
    itemIndex: number = 0,
  ): ServerState {
    let idx = this.flatPositions.findIndex(
      (fp) =>
        fp.kataId === kataId &&
        fp.sectionIndex === sectionIndex &&
        fp.itemIndex === itemIndex,
    );
    // Lesson sections are collapsed to itemIndex 0; fall back when the
    // caller supplies a stale sub-item index.
    if (idx < 0 && itemIndex !== 0) {
      idx = this.flatPositions.findIndex(
        (fp) => fp.kataId === kataId && fp.sectionIndex === sectionIndex,
      );
    }
    if (idx < 0) {
      throw new Error(
        `Position not found: ${kataId} section ${sectionIndex} item ${itemIndex}`,
      );
    }
    this.currentFlatIndex = idx;
    this.aiConversationHistory = [];
    this.syncPosition();
    return this.getState();
  }

  /** Bundled snapshot of current position, available actions, and progress. */
  getState(): ServerState {
    return {
      position: this.getPosition(),
      actions: this.getAvailableActions(),
      progress: this.getProgress(),
    };
  }

  // ─── Actions on current item ───

  async run(shots: number = 1): Promise<StatefulResult<RunResult>> {
    const pos = this.getPosition();
    const sources = await this.resolveSources();
    const result = await this.compiler.run(sources, shots);
    if (pos.item.type === "lesson-example") {
      this.ranExamples.add(pos.item.id);
    }
    return { result, state: this.getState() };
  }

  async runWithNoise(shots: number = 100): Promise<StatefulResult<RunResult>> {
    const sources = await this.resolveSources();
    const result = await this.compiler.runWithNoise(sources, shots);
    return { result, state: this.getState() };
  }

  async getCircuit(): Promise<StatefulResult<CircuitResult>> {
    const sources = await this.resolveSources();
    const result = await this.compiler.getCircuit(sources);
    return { result, state: this.getState() };
  }

  async getResourceEstimate(): Promise<StatefulResult<EstimateResult>> {
    const sources = await this.resolveSources();
    const result = await this.compiler.getResourceEstimate(sources);
    return { result, state: this.getState() };
  }

  async checkSolution(): Promise<StatefulResult<SolutionCheckResult>> {
    const { kataId, sectionIndex, exercise } = this.resolveExercise();
    const userCode = await this.workspace.readUserCode(kataId, exercise.id);
    const sources = await getExerciseSources(exercise);
    const result = await this.compiler.checkSolution(userCode, sources);

    if (result.passed) {
      this.progress.markComplete(kataId, sectionIndex);
      await this.progress.save();
    }

    return { result, state: this.getState() };
  }

  revealAnswer(): StatefulResult<string> {
    const pos = this.getPosition();
    if (pos.item.type !== "lesson-question") {
      throw new Error("Current item is not a question");
    }
    return { result: pos.item.answer, state: this.getState() };
  }

  getNextHint(): StatefulResult<HintResult | null> {
    const { exercise } = this.resolveExercise();
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
      result: {
        hint: hints[current - 1],
        current,
        total: hints.length,
      },
      state: this.getState(),
    };
  }

  getFullSolution(): string {
    const { exercise } = this.resolveExercise();
    const solution = exercise.explainedSolution.items.find(
      (item): item is Solution => item.type === "solution",
    );
    return solution?.code ?? "";
  }

  // ─── Exercise file access ───

  getExerciseFilePath(): string {
    const { kataId, exercise } = this.resolveExercise();
    return this.workspace.getExerciseFilePath(kataId, exercise.id);
  }

  async readUserCode(): Promise<string> {
    const { kataId, exercise } = this.resolveExercise();
    return this.workspace.readUserCode(kataId, exercise.id);
  }

  // ─── Progress ───

  getProgress(): OverallProgress {
    return this.progress.getOverallProgress();
  }

  resetProgress(kataId?: string): void {
    this.progress.reset(kataId);
    if (!kataId) {
      this.currentFlatIndex = 0;
    }
    this.progress.save().catch(() => {});
  }

  // ─── AI ───

  async getAIHint(): Promise<StatefulResult<string | null>> {
    const { kataId, exercise } = this.resolveExercise();
    const userCode = await this.workspace.readUserCode(kataId, exercise.id);
    const previousHints = this.aiHintHistory.get(exercise.id) ?? [];
    const hintLevel = previousHints.length + 1;

    let checkResult: SolutionCheckResult | undefined;
    try {
      const sources = await getExerciseSources(exercise);
      checkResult = await this.compiler.checkSolution(userCode, sources);
    } catch {
      // Don't fail AI hint if check fails
    }

    const hint = await this.aiProvider.getHint({
      exerciseDescription: exercise.description.content,
      userCode,
      checkResult,
      previousHints,
      hintLevel,
    });

    if (hint) {
      if (!this.aiHintHistory.has(exercise.id)) {
        this.aiHintHistory.set(exercise.id, []);
      }
      this.aiHintHistory.get(exercise.id)!.push(hint);
    }

    return { result: hint, state: this.getState() };
  }

  async explainError(errorContext: AIErrorContext): Promise<string | null> {
    return this.aiProvider.explainError(errorContext);
  }

  async reviewSolution(): Promise<StatefulResult<string | null>> {
    const { kataId, exercise } = this.resolveExercise();
    const userCode = await this.workspace.readUserCode(kataId, exercise.id);
    const referenceSolution = this.getFullSolution();

    const review = await this.aiProvider.reviewSolution({
      exerciseDescription: exercise.description.content,
      userCode,
      referenceSolution,
    });
    return { result: review, state: this.getState() };
  }

  async askConceptQuestion(
    question: string,
  ): Promise<StatefulResult<string | null>> {
    // Gather context from current position
    const pos = this.getPosition();
    const kata = this.findKata(pos.kataId);
    let lessonContent: string;

    // Get content from current section
    const section = kata.sections[pos.sectionIndex];
    if (section.type === "lesson") {
      lessonContent = section.items
        .map((item) => {
          if (item.type === "text-content") return item.content;
          if (item.type === "example")
            return `\`\`\`qsharp\n${item.code}\n\`\`\``;
          if (item.type === "question") return item.description.content;
          return "";
        })
        .join("\n\n");
    } else {
      lessonContent = section.description.content;
    }

    const answer = await this.aiProvider.askQuestion({
      lessonContent,
      question,
      kataTitle: kata.title,
      history: this.aiConversationHistory.slice(-8),
    });

    // Track conversation
    this.aiConversationHistory.push({ role: "user", content: question });
    if (answer) {
      this.aiConversationHistory.push({ role: "assistant", content: answer });
    }

    return { result: answer, state: this.getState() };
  }

  // ─── Private helpers ───

  private findKata(kataId: string): Kata {
    const kata = this.katas.find((k) => k.id === kataId);
    if (!kata) throw new Error(`Kata not found: ${kataId}`);
    return kata;
  }

  private syncPosition(): void {
    const fp = this.flatPositions[this.currentFlatIndex];
    if (fp) {
      this.progress.setPosition(fp.kataId, fp.sectionIndex, fp.itemIndex);
      this.progress.save().catch(() => {});
    }
  }

  private resolveNavigationItem(fp: FlatPosition): NavigationItem {
    const kata = this.findKata(fp.kataId);
    const section = kata.sections[fp.sectionIndex];

    if (section.type === "exercise") {
      const exercise = section as Exercise;
      return {
        type: "exercise",
        id: exercise.id,
        title: exercise.title,
        description: exercise.description.content,
        filePath: this.workspace.getExerciseFilePath(kata.id, exercise.id),
        isComplete: this.progress.isComplete(kata.id, fp.sectionIndex),
        hintCount: exercise.explainedSolution.items.filter(
          (i) => i.type === "text-content",
        ).length,
      } satisfies ExerciseItem;
    }

    // Lesson section — all items are collapsed into a single page.
    const lesson = section as Lesson;
    const items = lesson.items;

    // Find the example item (if any) to determine the primary item type.
    const exampleItem = items.find((i) => i.type === "example");

    if (exampleItem && exampleItem.type === "example") {
      // Collect surrounding text content.
      const exIdx = items.indexOf(exampleItem);
      const before = items
        .slice(0, exIdx)
        .filter((i) => i.type === "text-content")
        .map((i) => (i as { content: string }).content)
        .join("\n");
      const after = items
        .slice(exIdx + 1)
        .filter((i) => i.type === "text-content")
        .map((i) => (i as { content: string }).content)
        .join("\n");

      return {
        type: "lesson-example",
        id: exampleItem.id,
        code: exampleItem.code,
        filePath: this.workspace.getExampleFilePath(kata.id, exampleItem.id),
        sectionTitle: lesson.title,
        contentBefore: before || undefined,
        contentAfter: after || undefined,
      } satisfies LessonExampleItem;
    }

    // No example — merge all text/question items into a single page.
    // If there's exactly one item, resolve it directly.
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

    // Multiple text-only items (or unexpected mix) — concatenate all content.
    const merged = items
      .map((i) => {
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

  /**
   * Resolve sources for the current item. Returns an array of [filename, code] tuples.
   * For examples: single source with the example code.
   * For exercises: user code + all exercise verification/library sources.
   */
  private async resolveSources(): Promise<[string, string][]> {
    const pos = this.getPosition();
    if (pos.item.type === "lesson-example") {
      return [["main.qs", pos.item.code]];
    }
    if (pos.item.type === "exercise") {
      const kata = this.findKata(pos.kataId);
      const exercise = kata.sections[pos.sectionIndex] as Exercise;
      return this.buildExerciseSources(pos.kataId, exercise);
    }
    throw new Error(
      "Current item is not runnable (not an example or exercise)",
    );
  }

  /**
   * Build the full source set for an exercise: user code + verification/library sources.
   * The verification source contains the @EntryPoint needed to run.
   */
  private async buildExerciseSources(
    kataId: string,
    exercise: Exercise,
  ): Promise<[string, string][]> {
    const userCode = await this.workspace.readUserCode(kataId, exercise.id);
    const exerciseSources = await getExerciseSources(exercise);
    const sources: [string, string][] = [["user.qs", userCode]];
    for (let i = 0; i < exerciseSources.length; i++) {
      sources.push([`source_${i}.qs`, exerciseSources[i]]);
    }
    // Some verification sources already declare `@EntryPoint() operation CheckSolution`;
    // others don't. Only add a synthetic entry point if none of the sources already
    // contain one — otherwise the compiler errors with "duplicate entry point".
    const hasEntryPoint = exerciseSources.some((s) =>
      /@EntryPoint\s*\(/.test(s),
    );
    if (!hasEntryPoint) {
      sources.push([
        "entry.qs",
        "@EntryPoint()\noperation Main() : Bool {\n    Kata.Verification.CheckSolution()\n}\n",
      ]);
    }
    return sources;
  }

  /**
   * Resolve the current exercise from the current position.
   */
  private resolveExercise(): {
    kataId: string;
    sectionIndex: number;
    exercise: Exercise;
  } {
    const pos = this.getPosition();
    if (pos.item.type !== "exercise") {
      throw new Error("Current position is not an exercise");
    }
    const kata = this.findKata(pos.kataId);
    const section = kata.sections[pos.sectionIndex];
    if (section.type !== "exercise") {
      throw new Error("Current section is not an exercise");
    }
    return {
      kataId: pos.kataId,
      sectionIndex: pos.sectionIndex,
      exercise: section as Exercise,
    };
  }
}
