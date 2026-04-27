// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// ─── Re-exports from qsharp-lang that we use ───
import type {
  Kata,
  KataSection,
  Lesson,
  Exercise,
  Example,
  TextContent,
  Question,
  Answer,
  ExplainedSolution,
  Solution,
  LessonItem,
  ContentItem,
} from "qsharp-lang/katas-md";

export type {
  Kata,
  KataSection,
  Lesson,
  Exercise,
  Example,
  TextContent,
  Question,
  Answer,
  ExplainedSolution,
  Solution,
  LessonItem,
  ContentItem,
};

import type { CircuitGroup } from "qsharp-lang/dist/data-structures/circuit.js";
export type { CircuitGroup };

import type { Dump, ShotResult } from "qsharp-lang/dist/compiler/common.js";
export type { Dump, ShotResult };

// ─── Server configuration ───

export interface InitConfig {
  /** IDs of katas to load (e.g. ["getting_started", "complex_arithmetic"]) */
  kataIds: string[];
  /** Absolute path to `qdk-learning.json`. */
  learningFilePath: string;
  /**
   * Absolute path to the katas content folder (exercises, examples, etc.).
   * Resolved from `katasRoot` in `qdk-learning.json` against the workspace root.
   */
  katasRoot: string;
  /**
   * Relative katasRoot value to persist back into `qdk-learning.json`.
   * Typically `"./quantum-katas"`.
   */
  katasRootRel: string;
  /** Optional AI provider for intelligent features */
  aiProvider?: IAIProvider;
  /** Content format: "markdown" for TUI (from katas-md), "html" for web (from katas). Default: "markdown". */
  contentFormat?: "html" | "markdown";
}

// ─── Catalog ───

export interface KataSummary {
  id: string;
  title: string;
  sectionCount: number;
  completedCount: number;
  /** True if this kata is in the recommended learning order and all prerequisites are done */
  recommended: boolean;
}

export interface KataDetail {
  id: string;
  title: string;
  sections: SectionSummary[];
}

export interface SectionSummary {
  type: "lesson" | "exercise";
  id: string;
  title: string;
  isComplete: boolean;
  /** Number of sub-items (lesson items or 1 for exercise) */
  itemCount: number;
}

// ─── Navigation ───

export interface Position {
  kataId: string;
  sectionId: string;
  itemIndex: number;
  item: NavigationItem;
}

export type NavigationItem =
  | LessonTextItem
  | LessonExampleItem
  | LessonQuestionItem
  | ExerciseItem;

export interface LessonTextItem {
  type: "lesson-text";
  /** HTML content from the kata corpus */
  content: string;
  /** Parent section title */
  sectionTitle: string;
}

export interface LessonExampleItem {
  type: "lesson-example";
  /** Globally unique example ID */
  id: string;
  /** Q# source code */
  code: string;
  /** Absolute path to the standalone .qs file scaffolded into the learning workspace */
  filePath: string;
  sectionTitle: string;
  /** HTML from lesson-text items preceding this example in the section */
  contentBefore?: string;
  /** HTML from lesson-text items following this example in the section */
  contentAfter?: string;
}

export interface LessonQuestionItem {
  type: "lesson-question";
  /** HTML description */
  description: string;
  /** HTML answer (revealed on demand) */
  answer: string;
  sectionTitle: string;
}

export interface ExerciseItem {
  type: "exercise";
  /** Globally unique exercise ID */
  id: string;
  title: string;
  /** HTML description */
  description: string;
  /** Absolute path to the user's .qs solution file */
  filePath: string;
  isComplete: boolean;
  /** Number of available built-in hints */
  hintCount: number;
}

// ─── Code operation results ───

export interface DumpInfo {
  /** Qubit state label → [real, imag] amplitude */
  state: Dump;
  /** LaTeX representation, if available */
  stateLatex: string | null;
  qubitCount: number;
}

export interface MatrixInfo {
  /** 3D array: matrix[row][col] = [real, imag] */
  matrix: number[][][];
  matrixLatex: string;
}

export type RunEvent =
  | { type: "message"; message: string }
  | { type: "dump"; dump: DumpInfo }
  | { type: "matrix"; matrix: MatrixInfo };

export interface RunResult {
  success: boolean;
  /** Ordered stream of events (messages, state dumps, matrices) as they occurred */
  events: RunEvent[];
  /** Final result value (on success) */
  result?: string;
  /** Error message (on failure) */
  error?: string;
}

export interface SolutionCheckResult {
  passed: boolean;
  events: RunEvent[];
  error?: string;
}

export interface CircuitResult {
  /** Raw circuit data from qsharp-lang */
  circuit: CircuitGroup;
  /** Pre-rendered ASCII representation */
  ascii: string;
}

export interface EstimateResult {
  physicalQubits: number;
  runtime: string;
  /** Full estimate data */
  raw: Record<string, unknown>;
}

export interface NoiseConfig {
  /** Pauli noise rates per gate [px, py, pz] */
  pauliNoise: number[];
  /** Probability of qubit loss */
  qubitLoss: number;
}

// ─── Progress ───

export interface OverallProgress {
  katas: Map<string, KataProgress>;
  currentPosition: { kataId: string; sectionId: string; itemIndex: number };
  stats: { totalSections: number; completedSections: number };
}

export interface KataProgress {
  total: number;
  completed: number;
  sections: SectionProgress[];
}

export interface SectionProgress {
  id: string;
  title: string;
  type: "lesson" | "exercise";
  isComplete: boolean;
  completedAt?: string;
}

// ─── Progress file schema ───

export interface ProgressFileData {
  version: 1;
  /** Relative path from the file's parent directory to the katas content folder. */
  katasRoot: string;
  position: { kataId: string; sectionId: string; itemIndex: number };
  completions: Record<string, { completedAt: string }>;
  startedAt: string;
}

// ─── AI ───

export interface AIHintContext {
  exerciseDescription: string;
  userCode: string;
  checkResult?: SolutionCheckResult;
  previousHints: string[];
  hintLevel: number;
}

export interface AIErrorContext {
  code: string;
  error: string;
  exerciseDescription?: string;
}

export interface AIReviewContext {
  exerciseDescription: string;
  userCode: string;
  referenceSolution: string;
}

export interface AIQuestionContext {
  /** Current lesson or exercise content (HTML) */
  lessonContent: string;
  question: string;
  kataTitle: string;
  /** Recent conversation history */
  history?: Array<{ role: "user" | "assistant"; content: string }>;
}

export interface IAIProvider {
  getHint(ctx: AIHintContext): Promise<string | null>;
  explainError(ctx: AIErrorContext): Promise<string | null>;
  reviewSolution(ctx: AIReviewContext): Promise<string | null>;
  askQuestion(ctx: AIQuestionContext): Promise<string | null>;
}

// ─── Primary action ───

export type PrimaryAction = "next" | "run" | "check" | "reveal-answer";

export type Action =
  | "next"
  | "back"
  | "run"
  | "run-noise"
  | "circuit"
  | "estimate"
  | "check"
  | "hint"
  | "ai-hint"
  | "solution"
  | "ask-ai"
  | "reveal-answer"
  | "progress"
  | "menu"
  | "quit";

export interface ActionBinding {
  key: string;
  label: string;
  action: Action;
  /** True if this is the primary (most prominent) action for the current context. */
  primary?: boolean;
}

/** A group of related action bindings displayed together. */
export type ActionGroup = ActionBinding[];

export interface HintResult {
  /** The hint text (markdown) */
  hint: string;
  /** Which hint number this is (1-based) */
  current: number;
  /** Total available hints */
  total: number;
}

// ─── Bundled state snapshot ───

/**
 * A bundled snapshot of everything a UI needs to render after an interaction.
 * Returned alongside the result of every mutating server method so UI layers
 * never need a follow-up call to refresh their view.
 */
export interface ServerState {
  position: Position;
  actions: ActionGroup[];
  progress: OverallProgress;
}

/** Generic envelope pairing an action result with the resulting server state. */
export interface StatefulResult<T> {
  result: T;
  state: ServerState;
}

/** Return shape for next()/previous(): navigation may or may not have moved. */
export interface NavigationResult {
  /** False when already at the beginning/end (no change occurred). */
  moved: boolean;
  state: ServerState;
}

// ─── Server interface ───

export interface IKatasServer {
  // Lifecycle
  initialize(config: InitConfig): Promise<void>;
  dispose(): void;

  // Catalog
  listKatas(): KataSummary[];
  getKataDetail(kataId: string): KataDetail;

  // Granular reads (primarily for tests and internal composition)
  getPosition(): Position;
  getPrimaryAction(): PrimaryAction;
  getAvailableActions(): ActionGroup[];
  getProgress(): OverallProgress;

  // Bundled state snapshot
  getState(): ServerState;

  // Navigation
  next(): NavigationResult;
  previous(): NavigationResult;
  goTo(kataId: string, sectionId?: string, itemIndex?: number): ServerState;

  // Actions on current item
  run(shots?: number): Promise<StatefulResult<RunResult>>;
  runWithNoise(shots?: number): Promise<StatefulResult<RunResult>>;
  getCircuit(): Promise<StatefulResult<CircuitResult>>;
  getResourceEstimate(): Promise<StatefulResult<EstimateResult>>;
  checkSolution(): Promise<StatefulResult<SolutionCheckResult>>;
  revealAnswer(): StatefulResult<string>;
  getNextHint(): StatefulResult<HintResult | null>;
  getFullSolution(): string;

  // Exercise file access
  getExerciseFilePath(): string;
  readUserCode(): Promise<string>;

  // Progress
  resetProgress(kataId?: string): void;

  // AI
  getAIHint(): Promise<StatefulResult<string | null>>;
  explainError(errorContext: AIErrorContext): Promise<string | null>;
  reviewSolution(): Promise<StatefulResult<string | null>>;
  askConceptQuestion(question: string): Promise<StatefulResult<string | null>>;
}
