// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { default as katasContent } from "./katas-content.generated.js";

export type Example = {
  type: "example";
  id: string;
  code: string;
};

export type TextContent = {
  type: "text-content";
  content: string;
};

export type ContentItem = Example | TextContent;

export type Solution = {
  type: "solution";
  id: string;
  code: string;
};

export type ExplainedSolutionItem = ContentItem | Solution;

export type ExplainedSolution = {
  type: "explained-solution";
  items: ExplainedSolutionItem[];
};

export type Exercise = {
  type: "exercise";
  id: string;
  title: string;
  description: TextContent;
  sourceIds: string[];
  placeholderCode: string;
  explainedSolution: ExplainedSolution;
  /**
   * Hints extracted from index.md <details> blocks.
   *  Only populated in the Markdown bundle (used by VS Code); undefined in the HTML bundle (playground).
   */
  hints?: string[];
};

export type Answer = {
  type: "answer";
  items: ContentItem[];
};

export type Question = {
  type: "question";
  description: TextContent;
  answer: Answer;
};

export type LessonItem = ContentItem | Question;

export type Lesson = {
  type: "lesson";
  id: string;
  title: string;
  items: LessonItem[];
};

export type KataSection = Exercise | Lesson;

export type Kata = {
  id: string;
  title: string;
  sections: KataSection[];
  published: boolean;
};

export async function getAllKatas(
  options: { includeUnpublished?: boolean } = { includeUnpublished: false },
): Promise<Kata[]> {
  return katasContent.katas.filter(
    (k) => options.includeUnpublished || k.published,
  ) as Kata[];
}

export async function getKata(id: string): Promise<Kata> {
  const katas = await getAllKatas({ includeUnpublished: true });
  return (
    katas.find((k) => k.id === id) ||
    Promise.reject(`Failed to get kata with id: ${id}`)
  );
}

export async function getExerciseSources(
  exercise: Exercise,
): Promise<string[]> {
  return katasContent.globalCodeSources
    .filter((source) => exercise.sourceIds.indexOf(source.id) > -1)
    .map((source) => source.code);
}
