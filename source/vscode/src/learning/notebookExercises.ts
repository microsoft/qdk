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
 * The exercise id is the stable nbformat cell ID. The `@exercise`-decorated
 * function name links the cell to the Python checker in `_unit.py` at
 * runtime but is not used as an identifier in the extension.
 *
 * This module works on raw notebook JSON rather than VS Code's notebook API
 * because both course load and materialization happen with the file closed.
 * The transform is delete-only, so no nbformat cells are ever constructed.
 */

/** Tag marking the code cell a learner edits. */
const EXERCISE_TAG = "exercise";

/** Tags marking author-only cells, removed from the learner's working copy. */
const AUTHORING_TAGS = ["hint", "solution", "explanation"] as const;

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

/** A {@link RawNotebook} whose `cells` array has been validated to exist. */
type ParsedNotebook = RawNotebook & { cells: RawCell[] };

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

  // The exercise most recently seen, and therefore the one that any
  // subsequent authoring cells belong to. `undefined` until the first
  // exercise cell, which makes leading authoring cells detectable as orphans.
  let current: NotebookExerciseInfo | undefined;

  for (let i = 0; i < cells.length; i++) {
    const cell = cells[i];
    const tags = cellTags(cell);

    const authoringTag = AUTHORING_TAGS.find((t) => tags.includes(t));

    // Treat all code cells as activities, but only update current for EXERCISE_TAG.
    // Treating non-exercises as exercises is a bit of a hack, but it should do the
    // right thing - they'll pass once they're run.
    if (cellKind(cell) === "code") {
      // Make sure we don't create entries for cells that won't appear in the working copy
      if (!authoringTag) {
        current = undefined;

        const cellId = cellIdOf(cell);
        if (!cellId) {
          log.warn(
            `Learning: skipping a code cell in unit "${unitLabel}": the cell has no id.`,
          );
          continue;
        }

        // This is a terrible fallback name, but it shouldn't actually happen
        const title =
          extractTitleFromPrecedingCell(cells, i) ?? `Cell ${cellId}`;
        const exercise = {
          cellId: cellId,
          title,
          description: "", // Not actually used for notebook exercises
          hints: [],
          solutions: [],
          solutionExplanation: "",
        };
        exercises.push(exercise);

        if (tags.includes(EXERCISE_TAG)) {
          current = exercise;
        }

        continue;
      }
    } else if (tags.includes(EXERCISE_TAG)) {
      log.warn(
        `Learning: ignoring "${EXERCISE_TAG}" tag on a non-code cell in unit "${unitLabel}".`,
      );
      continue;
    }

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
        `Learning: ignoring a "${authoringTag}" cell for exercise "${current.cellId}" ` +
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
              `"${current.cellId}" in unit "${unitLabel}".`,
          );
          break;
        }
        current.solutionExplanation = source;
        break;
    }
  }

  return exercises;
}

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
  const notebook = parseNotebook(text, unitLabel);
  if (!notebook) {
    return undefined;
  }

  notebook.cells = notebook.cells.filter((cell) => {
    const tags = cellTags(cell);
    return !AUTHORING_TAGS.some((t) => tags.includes(t));
  });

  // Match the ipynb serializer's formatting so the file stays diff-stable
  // once VS Code starts saving it: one space of indent, trailing newline.
  return `${JSON.stringify(notebook, undefined, 1)}\n`;
}

// ─── Cell readers ───

/**
 * Parse a notebook's JSON text and validate it has a `cells` array. Shared by
 * every entry point that needs the raw notebook rather than just its cells.
 */
function parseNotebook(
  text: string,
  unitLabel: string,
): ParsedNotebook | undefined {
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
  notebook.cells = (notebook.cells as unknown[]).filter(
    (c): c is RawCell => !!c && typeof c === "object",
  );
  return notebook as ParsedNotebook;
}

function readCells(text: string, unitLabel: string): RawCell[] | undefined {
  return parseNotebook(text, unitLabel)?.cells;
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
 * Title and description for an exercise, taken from a nearby preceding
 * markdown cell. Walks back up to three markdown cells looking for one
 * that contains a heading; stops at non-markdown cells, `exercise`-tagged
 * cells, or authoring-tagged cells.
 *
 * The cell's last heading becomes the title (dropping a leading "Exercise:",
 * which reads naturally in the notebook but is redundant in the progress tree);
 * the remaining prose becomes the description.
 */
function extractTitleFromPrecedingCell(
  cells: RawCell[],
  exerciseIndex: number,
): string | undefined {
  let checked = 0;
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
      break;
    }
    const title = extractTitleFromHeader(cellSource(cell));
    if (title) {
      return title;
    }
    if (++checked >= 3) {
      break;
    }
  }
  return undefined;
}

function extractTitleFromHeader(markdown: string): string | undefined {
  const lines = markdown.split(/\r?\n/);
  let headingIndex = -1;
  for (let i = 0; i < lines.length; i++) {
    if (/^\s{0,3}#{1,6}\s+\S/.test(lines[i])) {
      headingIndex = i;
    }
  }
  if (headingIndex < 0) {
    return undefined;
  }

  const title = lines[headingIndex]
    .replace(/^\s{0,3}#{1,6}\s+/, "")
    .replace(/\s+#*\s*$/, "")
    .trim();

  return title;
}
