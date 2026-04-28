// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Type declarations for qsharp-lang sub-module imports used by the learning panel.
// The main declarations live in src/learning/qsharp-lang.d.ts but that
// directory is excluded from the main tsconfig.

declare module "qsharp-lang/katas-md" {
  export function getAllKatas(options?: {
    includeUnpublished?: boolean;
  }): Promise<Kata[]>;
  export function getExerciseSources(exercise: Exercise): string[];

  export interface Kata {
    id: string;
    title: string;
    published?: boolean;
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
    openQasm?: { operationName: string };
    sourceIds: string[];
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
