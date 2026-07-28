// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import type { NotebookExerciseInfo } from "./types.js";

/**
 * Authoring model for `python-notebook` course units.
 *
 * A unit's exercise metadata lives in the authored notebook itself, marked up
 * with standard Jupyter cell tags (added via the built-in "Add Cell Tag"
 * command). Authors never edit the raw `.ipynb` JSON and never write cell IDs
 * by hand.
 *
 * | Tag           | Cell kind | Meaning                                       |
 * | ------------- | --------- | --------------------------------------------- |
 * | `exercise`    | code      | The cell the learner edits.                   |
 * | `hint`        | markdown  | One hint. Multiple allowed, in document order.|
 * | `solution`    | code      | A reference solution. Multiple allowed.       |
 * | `explanation` | markdown  | Prose explanation of the solution.            |
 *
 * `hint`, `solution` and `explanation` cells bind to the nearest preceding
 * `exercise` cell, and are stripped from the learner's working copy during
 * materialization (see {@link stripAuthoringCells}).
 *
 * The exercise id is the name of the `@exercise`-decorated function in the
 * exercise cell. That name is the source of truth linking the notebook cell,
 * this metadata, and the Python checker registered for it in `_unit.py`.
 *
 * This module works on raw notebook JSON rather than VS Code's notebook API
 * because both course load and materialization happen with the file closed.
 * The transform is delete-only, so no nbformat cells are ever constructed.
 */

/** Tag marking the code cell a learner edits. */
export const EXERCISE_TAG = "exercise";

/** Tags marking author-only cells, removed from the learner's working copy. */
export const AUTHORING_TAGS = ["hint", "solution", "explanation"] as const;

type AuthoringTag = (typeof AUTHORING_TAGS)[number];

/** The subset of an nbformat cell this module reads. */
interface RawCell {
  id?: unknown;
  cell_type?: unknown;
  source?: unknown;
  metadata?: { tags?: unknown };
}

/** The subset of an nbformat notebook this module reads. */
interface RawNotebook {
  cells?: unknown;
}

/**
 * Parse the exercise metadata out of an authored notebook's JSON text.
 *
 * Malformed input never throws: problems are logged and the affected exercise
 * or cell is skipped, so a bad notebook degrades to fewer exercises rather
 * than an unloadable course. `unitLabel` identifies the unit in those logs.
 */
export function parseNotebookExercises(
  text: string,
  unitLabel: string,
): NotebookExerciseInfo[] {
  const cells = readCells(text, unitLabel);
  if (!cells) {
    return [];
  }

  const exercises: NotebookExerciseInfo[] = [];
  const seenIds = new Set<string>();

  // The exercise most recently seen, and therefore the one that any
  // subsequent authoring cells belong to. `undefined` until the first
  // exercise cell, which makes leading authoring cells detectable as orphans.
  let current: NotebookExerciseInfo | undefined;

  for (let i = 0; i < cells.length; i++) {
    const cell = cells[i];
    const tags = cellTags(cell);

    if (tags.includes(EXERCISE_TAG)) {
      current = undefined;

      if (cellKind(cell) !== "code") {
        log.warn(
          `Learning: ignoring "${EXERCISE_TAG}" tag on a non-code cell in unit "${unitLabel}".`,
        );
        continue;
      }

      const id = exerciseId(cellSource(cell));
      if (!id) {
        log.warn(
          `Learning: skipping an "${EXERCISE_TAG}" cell in unit "${unitLabel}": ` +
            "no @exercise-decorated function found. The exercise id comes from " +
            "that function's name.",
        );
        continue;
      }
      if (seenIds.has(id)) {
        log.warn(
          `Learning: skipping duplicate exercise "${id}" in unit "${unitLabel}".`,
        );
        continue;
      }

      const cellId = cellIdOf(cell);
      if (!cellId) {
        log.warn(
          `Learning: skipping exercise "${id}" in unit "${unitLabel}": the cell has no id.`,
        );
        continue;
      }

      seenIds.add(id);
      const { title, description } = precedingPrompt(cells, i, id);
      current = {
        id,
        cellId,
        title,
        description,
        hints: [],
        solutions: [],
        solutionExplanation: "",
      };
      exercises.push(current);
      continue;
    }

    const authoringTag = AUTHORING_TAGS.find((t) => tags.includes(t));
    if (!authoringTag) {
      continue;
    }

    if (!current) {
      log.warn(
        `Learning: ignoring a "${authoringTag}" cell in unit "${unitLabel}": ` +
          `it does not follow an "${EXERCISE_TAG}" cell.`,
      );
      continue;
    }

    if (!hasExpectedKind(cell, authoringTag)) {
      log.warn(
        `Learning: ignoring a "${authoringTag}" cell for exercise "${current.id}" ` +
          `in unit "${unitLabel}": expected a ${expectedKind(authoringTag)} cell.`,
      );
      continue;
    }

    const source = cellSource(cell);
    switch (authoringTag) {
      case "hint":
        current.hints.push(source);
        break;
      case "solution":
        current.solutions.push(source);
        break;
      case "explanation":
        if (current.solutionExplanation) {
          log.warn(
            `Learning: ignoring an extra "explanation" cell for exercise ` +
              `"${current.id}" in unit "${unitLabel}".`,
          );
          break;
        }
        current.solutionExplanation = source;
        break;
    }
  }

  return exercises;
}

// TODO (acasey): share code with readCells
/**
 * Remove the author-only cells from a notebook's JSON text, returning the
 * notebook the learner works in.
 *
 * Everything else — including cell ids and the `exercise` tag — is preserved
 * verbatim, so metadata parsed from the authored notebook still resolves
 * against the working copy. Returns `undefined` if the text isn't a notebook,
 * leaving the caller to decide on a fallback.
 */
export function stripAuthoringCells(
  text: string,
  unitLabel: string,
): string | undefined {
  let notebook: RawNotebook;
  try {
    notebook = JSON.parse(text) as RawNotebook;
  } catch (e) {
    log.warn(
      `Learning: failed to parse the notebook for unit "${unitLabel}": ${String(e)}`,
    );
    return undefined;
  }
  if (!Array.isArray(notebook.cells)) {
    log.warn(
      `Learning: the notebook for unit "${unitLabel}" has no "cells" array.`,
    );
    return undefined;
  }

  notebook.cells = (notebook.cells as RawCell[]).filter((cell) => {
    const tags = cellTags(cell);
    return !AUTHORING_TAGS.some((t) => tags.includes(t));
  });

  // Match the ipynb serializer's formatting so the file stays diff-stable
  // once VS Code starts saving it: one space of indent, trailing newline.
  return `${JSON.stringify(notebook, undefined, 1)}\n`;
}

// ─── Cell readers ───

function readCells(text: string, unitLabel: string): RawCell[] | undefined {
  let notebook: RawNotebook;
  try {
    notebook = JSON.parse(text) as RawNotebook;
  } catch (e) {
    log.warn(
      `Learning: failed to parse the notebook for unit "${unitLabel}": ${String(e)}`,
    );
    return undefined;
  }
  if (!Array.isArray(notebook.cells)) {
    log.warn(
      `Learning: the notebook for unit "${unitLabel}" has no "cells" array.`,
    );
    return undefined;
  }
  return (notebook.cells as unknown[]).filter(
    (c): c is RawCell => !!c && typeof c === "object",
  );
}

function cellTags(cell: RawCell): string[] {
  const tags = cell.metadata?.tags;
  return Array.isArray(tags) ? tags.filter((t) => typeof t === "string") : [];
}

function cellKind(cell: RawCell): "code" | "markdown" | "other" {
  const kind = cell.cell_type;
  return kind === "code" || kind === "markdown" ? kind : "other";
}

function cellIdOf(cell: RawCell): string | undefined {
  return typeof cell.id === "string" && cell.id.length > 0
    ? cell.id
    : undefined;
}

/** nbformat allows a cell's source to be a string or an array of lines. */
function cellSource(cell: RawCell): string {
  const source = cell.source;
  if (typeof source === "string") {
    return source;
  }
  if (Array.isArray(source)) {
    return source.filter((line) => typeof line === "string").join("");
  }
  return "";
}

function expectedKind(tag: AuthoringTag): "code" | "markdown" {
  return tag === "solution" ? "code" : "markdown";
}

function hasExpectedKind(cell: RawCell, tag: AuthoringTag): boolean {
  return cellKind(cell) === expectedKind(tag);
}

// ─── Field derivation ───

/**
 * The exercise id: the name of the `@exercise`-decorated function. The
 * decorator may be applied bare or called, and other decorators may sit
 * between it and the `def`.
 */
function exerciseId(source: string): string | undefined {
  const match =
    /^[ \t]*@exercise\b[^\n]*\n(?:[^\n]*\n)*?[ \t]*def[ \t]+(\w+)/m.exec(
      source,
    );
  return match?.[1];
}

/**
 * Title and description for an exercise, taken from the markdown cell that
 * introduces it: the nearest preceding markdown cell that isn't itself tagged.
 *
 * The cell's last heading becomes the title (dropping a leading "Exercise:",
 * which reads naturally in the notebook but is redundant in the progress tree);
 * the remaining prose becomes the description.
 */
function precedingPrompt(
  cells: RawCell[],
  exerciseIndex: number,
  id: string,
): { title: string; description: string } {
  for (let i = exerciseIndex - 1; i >= 0; i--) {
    const cell = cells[i];
    const tags = cellTags(cell);
    if (
      tags.includes(EXERCISE_TAG) ||
      AUTHORING_TAGS.some((t) => tags.includes(t))
    ) {
      break;
    }
    if (cellKind(cell) !== "markdown") {
      continue;
    }
    return splitPrompt(cellSource(cell), id);
  }
  return { title: id, description: "" };
}

function splitPrompt(
  markdown: string,
  id: string,
): { title: string; description: string } {
  const lines = markdown.split(/\r?\n/);
  let headingIndex = -1;
  for (let i = 0; i < lines.length; i++) {
    if (/^\s{0,3}#{1,6}\s+\S/.test(lines[i])) {
      headingIndex = i;
    }
  }
  if (headingIndex < 0) {
    return { title: id, description: markdown.trim() };
  }

  const title = lines[headingIndex]
    .replace(/^\s{0,3}#{1,6}\s+/, "")
    .replace(/\s+#*\s*$/, "")
    .replace(/^exercise\s*[:—-]\s*/i, "")
    .trim();

  return {
    title: title || id,
    description: lines
      .slice(headingIndex + 1)
      .join("\n")
      .trim(),
  };
}
