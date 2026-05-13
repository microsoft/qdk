// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { getAllKatas } from "qsharp-lang/katas-md";
import { KATAS_COURSE_ID } from "./constants.js";
import type {
  CatalogUnit,
  CatalogCourse,
  CatalogSection,
  CatalogExercise,
} from "./types.js";

/**
 * Load the built-in Quantum Katas as a single `CatalogCourse`.
 */
export async function loadKatasCourse(): Promise<CatalogCourse> {
  const raw = await getAllKatas();
  const units: CatalogUnit[] = raw.map((kata) => ({
    id: kata.id,
    title: kata.title,
    sections: kata.sections.map<CatalogSection>((s) => {
      if (s.type === "exercise") {
        const solutionItem = s.explainedSolution.items.find(
          (i) => i.type === "solution",
        );
        const solutionExplanation = s.explainedSolution.items
          .filter((i) => i.type === "text-content")
          .map((i) => i.content)
          .join("\n");
        return {
          type: "exercise",
          id: s.id,
          title: s.title,
          description: s.description.content,
          placeholderCode: s.placeholderCode,
          sourceIds: s.sourceIds,
          hints: s.hints ?? [],
          solutionCode: solutionItem?.code ?? "",
          solutionExplanation,
        } satisfies CatalogExercise;
      }

      // Lesson — pre-split text around the first example (if any).
      const items = s.items;
      const exampleItem = items.find((i) => i.type === "example");

      if (exampleItem) {
        const exIdx = items.indexOf(exampleItem);
        const before = items
          .slice(0, exIdx)
          .filter((i) => i.type === "text-content")
          .map((i) => i.content)
          .join("\n");
        const after = items
          .slice(exIdx + 1)
          .filter((i) => i.type === "text-content")
          .map((i) => i.content)
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
      const content = items
        .filter((i) => i.type === "text-content")
        .map((i) => i.content)
        .join("\n");
      return {
        type: "lesson",
        id: s.id,
        title: s.title,
        content,
      };
    }),
  }));

  return { id: KATAS_COURSE_ID, title: "Quantum Katas", units };
}
