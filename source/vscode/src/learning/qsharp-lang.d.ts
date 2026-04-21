// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Type declarations for qsharp-lang internal modules
// These are not exposed via the package exports map but are needed at runtime

declare module "qsharp-lang/katas" {
  export function getAllKatas(): Promise<Kata[]>;
  export function getExerciseSources(exercise: Exercise): string[];

  export interface Kata {
    id: string;
    title: string;
    sections: KataSection[];
  }
  export type KataSection = Lesson | Exercise;
  export interface Lesson {
    type: "lesson";
    id: string;
    title: string;
    items: LessonItem[];
  }
  export interface Exercise {
    type: "exercise";
    id: string;
    title: string;
    description: TextContent;
    placeholderCode: string;
    explainedSolution: ExplainedSolution;
  }
  export type LessonItem = TextContent | Example | Question;
  export type ContentItem = TextContent | Example | Solution;
  export interface TextContent {
    type: "text-content";
    content: string;
  }
  export interface Example {
    type: "example";
    id: string;
    code: string;
  }
  export interface Question {
    type: "question";
    description: TextContent;
    answer: Answer;
  }
  export interface Answer {
    type: "answer";
    items: (TextContent | Example)[];
  }
  export interface ExplainedSolution {
    type: "explained-solution";
    items: ContentItem[];
  }
  export interface Solution {
    type: "solution";
    code: string;
  }
}

// katas-md has the same API as katas (minus getKata), re-export the types
declare module "qsharp-lang/katas-md" {
  export { getAllKatas, getExerciseSources } from "qsharp-lang/katas";
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
  } from "qsharp-lang/katas";
}

declare module "qsharp-lang/dist/compiler/events.js" {
  export interface IQscEventTarget {
    addEventListener<T extends string>(
      type: T,
      listener: (event: any) => void,
    ): void;
    removeEventListener<T extends string>(
      type: T,
      listener: (event: any) => void,
    ): void;
    dispatchEvent(event: any): boolean;
  }

  export class QscEventTarget implements IQscEventTarget {
    constructor(captureEvents: boolean);
    addEventListener<T extends string>(
      type: T,
      listener: (event: any) => void,
    ): void;
    removeEventListener<T extends string>(
      type: T,
      listener: (event: any) => void,
    ): void;
    dispatchEvent(event: any): boolean;
    getResults(): import("qsharp-lang/dist/compiler/common.js").ShotResult[];
    resultCount(): number;
    clearResults(): void;
  }

  export type QscEventData =
    | { type: "Message"; detail: string }
    | {
        type: "DumpMachine";
        detail: {
          state: import("qsharp-lang/dist/compiler/common.js").Dump;
          stateLatex: string | null;
          qubitCount: number;
        };
      }
    | {
        type: "Matrix";
        detail: { matrix: number[][][]; matrixLatex: string };
      }
    | {
        type: "Result";
        detail: import("qsharp-lang/dist/compiler/common.js").Result;
      };
}

declare module "qsharp-lang/dist/compiler/common.js" {
  export type Dump = { [index: string]: [number, number] };

  export type Result =
    | { success: true; value: string }
    | {
        success: false;
        value: { message?: string; errors?: Array<{ message?: string }> };
      };

  export interface ShotResult {
    success: boolean;
    result: string | { message?: string; errors?: Array<{ message?: string }> };
    events: Array<
      | { type: "Message"; message: string }
      | {
          type: "DumpMachine";
          state: Dump;
          stateLatex: string | null;
          qubitCount: number;
        }
      | { type: "Matrix"; matrix: number[][][]; matrixLatex: string }
    >;
  }
}

declare module "qsharp-lang/dist/compiler/compiler.js" {
  import type { IQscEventTarget } from "qsharp-lang/dist/compiler/events.js";
  import type { CircuitGroup } from "qsharp-lang/dist/data-structures/circuit.js";

  export type TargetProfile = "base" | "adaptive_ri" | "unrestricted";

  export type ProgramConfig = {
    sources: [string, string][];
    languageFeatures: string[];
    profile?: TargetProfile;
  };

  export interface ICircuitConfig {
    generationMethod: "simulate" | "classicalEval" | "static";
    maxOperations: number;
    sourceLocations: boolean;
    groupByScope: boolean;
  }

  export interface IOperationInfo {
    operation: string;
    totalNumQubits: number;
  }

  export interface ICompiler {
    checkCode(code: string): Promise<any[]>;
    run(
      program: ProgramConfig,
      expr: string,
      shots: number,
      eventHandler: IQscEventTarget,
    ): Promise<void>;
    runWithNoise(
      program: ProgramConfig,
      expr: string,
      shots: number,
      pauliNoise: number[],
      qubitLoss: number,
      eventHandler: IQscEventTarget,
    ): Promise<void>;
    getCircuit(
      program: ProgramConfig,
      config: ICircuitConfig,
      operation?: IOperationInfo,
    ): Promise<CircuitGroup>;
    getEstimates(
      program: ProgramConfig,
      expr: string,
      params: string,
    ): Promise<string>;
    checkExerciseSolution(
      userCode: string,
      exerciseSources: string[],
      eventHandler: IQscEventTarget,
    ): Promise<boolean>;
  }
}

declare module "qsharp-lang/dist/data-structures/circuit.js" {
  export interface Register {
    qubit: number;
    result?: number;
  }

  export interface CircuitGroup {
    circuits: Circuit[];
    version: number;
  }

  export interface Circuit {
    qubits: Qubit[];
    componentGrid: Column[];
  }

  export interface Qubit {
    id: number;
    numResults?: number;
  }

  export interface Column {
    components: Component[];
  }

  export type Component = Unitary | Measurement;

  export interface BaseOperation {
    gate: string;
    args?: string[];
    isConditional?: boolean;
  }

  export interface Unitary extends BaseOperation {
    kind: "unitary";
    targets: Register[];
    controls?: Register[];
    isAdjoint?: boolean;
  }

  export interface Measurement extends BaseOperation {
    kind: "measurement";
    qubits: Register[];
    results: Register[];
  }
}

declare module "marked-terminal" {
  import type { MarkedExtension } from "marked";
  export function markedTerminal(
    options?: Record<string, unknown>,
  ): MarkedExtension;
}
