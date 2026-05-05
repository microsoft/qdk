// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { getAllKatas, getExerciseSources } from "qsharp-lang/katas-md";
import { KATAS_COURSE_ID } from "./constants.js";
import type {
  CatalogUnit,
  CatalogCourse,
  CatalogSection,
  CatalogExercise,
} from "./types.js";

/**
 * Adapter boundary between the raw `qsharp-lang/katas-md` types and the
 * learning module's own `Catalog*` types.  Only this file imports from the
 * katas content packages — the rest of the learning module works exclusively
 * with the flattened types declared in `./types.d.ts`.
 */

let cached: CatalogUnit[] | undefined;

/**
 * Load all katas and flatten them into `CatalogUnit[]`.
 */
export async function loadKatas(): Promise<CatalogUnit[]> {
  if (!cached) {
    const raw = await getAllKatas();
    cached = raw.map((kata) => ({
      id: kata.id,
      title: kata.title,
      sections: kata.sections.map<CatalogSection>((s) => {
        if (s.type === "exercise") {
          const solutionItem = s.explainedSolution.items.find(
            (i) => i.type === "solution",
          );
          const explanationParts: string[] = [];
          for (const item of s.explainedSolution.items) {
            if (item.type === "text-content") {
              explanationParts.push(item.content);
            }
          }
          return {
            type: "exercise",
            id: s.id,
            title: s.title,
            description: s.description.content,
            placeholderCode: s.placeholderCode,
            sourceIds: s.sourceIds,
            hints: s.hints ?? [],
            solutionCode:
              solutionItem?.type === "solution" ? solutionItem.code : "",
            solutionExplanation: explanationParts.join("\n"),
          } satisfies CatalogExercise;
        }

        // Lesson — pre-split text around the first example (if any).
        const items = s.items;
        const exampleItem = items.find((i) => i.type === "example");

        if (exampleItem && exampleItem.type === "example") {
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
            type: "lesson",
            id: s.id,
            title: s.title,
            example: { id: exampleItem.id, code: exampleItem.code },
            contentBefore: before || undefined,
            contentAfter: after || undefined,
          };
        }

        // No example — merge all text items.
        const merged = items
          .map((i) => (i.type === "text-content" ? i.content : ""))
          .filter(Boolean)
          .join("\n");
        return {
          type: "lesson",
          id: s.id,
          title: s.title,
          content: merged,
        };
      }),
    }));
  }
  return cached;
}

/**
 * Resolve the source files needed to check an exercise solution.
 */
export async function getExerciseSourceFiles(
  exercise: CatalogExercise,
): Promise<string[]> {
  // `getExerciseSources` only reads `sourceIds` on its argument.
  return getExerciseSources(exercise as never);
}

/**
 * Load the built-in Quantum Katas as a single `CatalogCourse`.
 */
export async function loadKatasCourse(): Promise<CatalogCourse> {
  const units = await loadKatas();
  return {
    id: KATAS_COURSE_ID,
    title: "Quantum Katas",
    units,
  };
}
