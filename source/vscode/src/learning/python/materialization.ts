// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";
import {
  notebookUnits,
  sourceNotebookUri,
  workbookUri,
} from "../courseLayout.js";
import { ensureParentDir, uriExists } from "../fsUtils.js";
import { stripAuthoringCells } from "../notebookExercises.js";
import type { CatalogCourse } from "../types.js";

/**
 * Materialize the working copy for every unit in the course: derive each
 * `*.workbook.ipynb` sibling from the authored notebook. Existing workbooks
 * are never overwritten, preserving learner edits.
 */
export async function materializeCourseWorkbooks(
  course: CatalogCourse,
): Promise<void> {
  for (const unit of notebookUnits(course)) {
    const dest = workbookUri(course, unit);
    if (await uriExists(dest)) {
      continue;
    }
    await materializeNotebook(sourceNotebookUri(course, unit), dest, unit.id);
  }
}

/**
 * Re-materialize a single unit: overwrite its `*.workbook.ipynb`
 * with a fresh copy derived from the authored notebook.
 */
export async function rematerializeUnitWorkbook(
  course: CatalogCourse,
  unitId: string,
): Promise<void> {
  const unit = course.units.find((u) => u.id === unitId);
  if (!unit) {
    throw new Error(`Unit "${unitId}" not found in course "${course.id}".`);
  }
  await materializeNotebook(
    sourceNotebookUri(course, unit),
    workbookUri(course, unit),
    unit.id,
  );
}

/**
 * Write a unit's working copy: the authored notebook minus its author-only
 * cells (hints, solutions, explanations).
 *
 * If the notebook can't be parsed we fall back to copying it verbatim, so a
 * malformed notebook still leaves the learner with something to work in
 * rather than nothing.
 */
async function materializeNotebook(
  src: vscode.Uri,
  dest: vscode.Uri,
  unitId: string,
): Promise<void> {
  try {
    await ensureParentDir(dest);
    const text = new TextDecoder().decode(
      await vscode.workspace.fs.readFile(src),
    );
    const stripped = stripAuthoringCells(text, unitId);
    if (stripped === undefined) {
      await vscode.workspace.fs.copy(src, dest, { overwrite: true });
      return;
    }
    await vscode.workspace.fs.writeFile(
      dest,
      new TextEncoder().encode(stripped),
    );
  } catch (e) {
    log.warn(
      `Failed to materialize ${src.fsPath} → ${dest.fsPath}: ${String(e)}`,
    );
  }
}
