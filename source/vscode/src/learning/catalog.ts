// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { getAllKatas } from "qsharp-lang/katas-md";
import { KATAS_COURSE_ID } from "./constants.js";
import type {
  CatalogUnit,
  CatalogCourse,
  CatalogActivity,
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
    activities: kata.sections.map<CatalogActivity>((s) => {
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
          .map(renderLessonItemAsMarkdown)
          .filter((c): c is string => !!c)
          .join("\n");
        const after = items
          .slice(exIdx + 1)
          .map(renderLessonItemAsMarkdown)
          .filter((c): c is string => !!c)
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

      // No example — merge all renderable items.
      const content = items
        .map(renderLessonItemAsMarkdown)
        .filter((c): c is string => !!c)
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

/**
 * Project a single `LessonItem` down to a markdown string for the
 * VS Code learning view, which renders one merged HTML blob per
 * lesson section. `text-content` passes through unchanged. The
 * interactive `bloch` item degrades to a short note + code block
 * (and a link out to the playground), so it shows up in VS Code as
 * informative text instead of disappearing. Anything else (examples,
 * questions) is rendered elsewhere by the learning view and returns
 * `null` here so the merger skips it.
 */
function renderLessonItemAsMarkdown(item: {
  type: string;
  content?: string;
  gates?: string;
  title?: string;
}): string | null {
  if (item.type === "text-content" && item.content !== undefined) {
    return item.content;
  }
  if (item.type === "bloch" && item.gates !== undefined) {
    const caption = item.title ? ` (${item.title})` : "";
    return (
      `> **Interactive Bloch sphere demo${caption}.** ` +
      `Gate sequence: \`${item.gates}\`. ` +
      `Open this sequence in the Bloch sphere in the playground ` +
      `to step through it visually.`
    );
  }
  return null;
}
