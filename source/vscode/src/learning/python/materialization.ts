// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";
import { WORKBOOK_SUFFIX } from "../constants.js";
import { ensureParentDir, uriExists } from "../fsUtils.js";
import { stripAuthoringCells } from "../notebookExercises.js";
import type { CatalogCourse } from "../types.js";

/**
 * Working-copy URI of a unit's notebook: a `*.workbook.ipynb` file that
 * sits beside the authored source notebook in the same unit folder.
 *
 * Keeping the working copy as a sibling means the learner's notebook
 * resolves the same relative imports (`_course_lib.py`, `_unit.py`, etc.) as the
 * source.
 */
export function workbookFileUri(
  course: CatalogCourse,
  notebookRel: string,
): vscode.Uri {
  if (!course.sourceDir) {
    throw new Error(`Course "${course.id}" has no source folder.`);
  }
  const sourceRoot = vscode.Uri.parse(course.sourceDir);
  return vscode.Uri.joinPath(sourceRoot, toWorkbookRel(notebookRel));
}

/**
 * Materialize the working copy for every unit in the course: derive each
 * `*.workbook.ipynb` sibling from the authored notebook. Existing workbooks
 * are never overwritten, preserving learner edits.
 */
export async function materializeCourseWorkbooks(
  course: CatalogCourse,
): Promise<void> {
  if (!course.sourceDir) {
    throw new Error(`Course "${course.id}" has no source folder.`);
  }
  const sourceRoot = vscode.Uri.parse(course.sourceDir);

  for (const unit of course.units) {
    if (!unit.notebookRel) {
      continue;
    }
    const dest = vscode.Uri.joinPath(
      sourceRoot,
      toWorkbookRel(unit.notebookRel),
    );
    if (await uriExists(dest)) {
      continue;
    }
    await materializeNotebook(
      vscode.Uri.joinPath(sourceRoot, unit.notebookRel),
      dest,
      unit.id,
    );
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
  if (!course.sourceDir) {
    throw new Error(`Course "${course.id}" has no source folder.`);
  }
  const unit = course.units.find((u) => u.id === unitId);
  if (!unit?.notebookRel) {
    throw new Error(`Unit "${unitId}" not found in course "${course.id}".`);
  }

  const sourceRoot = vscode.Uri.parse(course.sourceDir);
  await materializeNotebook(
    vscode.Uri.joinPath(sourceRoot, unit.notebookRel),
    vscode.Uri.joinPath(sourceRoot, toWorkbookRel(unit.notebookRel)),
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

/**
 * Map a source notebook's relative path to its working-copy sibling by
 * swapping the `.ipynb` extension for `.workbook.ipynb`
 * (e.g. `01-intro/intro.ipynb` → `01-intro/intro.workbook.ipynb`).
 */
function toWorkbookRel(notebookRel: string): string {
  return notebookRel.replace(/\.ipynb$/i, WORKBOOK_SUFFIX);
}
