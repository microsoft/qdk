// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";
import { sourceNotebookUri, workbookUri } from "../courseLayout.js";
import { ensureParentDir, uriExists } from "../fsUtils.js";
import { stripAuthoringCells } from "../notebookExercises.js";
import type { NotebookCatalogCourse, NotebookCatalogUnit } from "../types.js";

/**
 * Materialize the working copy for every unit in the course: derive each
 * `*.workbook.ipynb` sibling from the authored notebook. Existing workbooks
 * are never overwritten, preserving learner edits. If a unit fails to
 * materialize, log the error so the rest of the course can still load.
 */
export async function materializeCourseWorkbooks(
  course: NotebookCatalogCourse,
): Promise<void> {
  for (const unit of course.units) {
    const dest = workbookUri(unit);
    if (await uriExists(dest)) {
      continue;
    }
    const src = sourceNotebookUri(unit);
    try {
      await materializeNotebook(src, dest, unit.id);
    } catch (e) {
      log.warn(
        `Failed to materialize ${src.fsPath} → ${dest.fsPath}: ${String(e)}`,
      );
    }
  }
}

/**
 * Re-materialize a single unit: overwrite its `*.workbook.ipynb`
 * with a fresh copy derived from the authored notebook. Rejects on failure
 * so a reset doesn't clear progress for a workbook it didn't replace.
 */
export async function rematerializeUnitWorkbook(
  unit: NotebookCatalogUnit,
): Promise<void> {
  await materializeNotebook(
    sourceNotebookUri(unit),
    workbookUri(unit),
    unit.id,
  );
}

/**
 * Write a unit's working copy: the authored notebook minus its author-only
 * cells (hints, solutions, explanations), with dedicated quiz-cell source
 * canonicalized to named quiz calls and any saved outputs preserved.
 *
 * If the notebook can't be parsed we fall back to copying it verbatim, so a
 * malformed notebook still leaves the learner with something to work in
 * rather than nothing. A missing quiz-cell ID/name throws instead, preventing the
 * answer-bearing source from being copied into a learner workbook.
 */
async function materializeNotebook(
  src: vscode.Uri,
  dest: vscode.Uri,
  unitId: string,
): Promise<void> {
  await ensureParentDir(dest);
  const text = new TextDecoder().decode(
    await vscode.workspace.fs.readFile(src),
  );
  const stripped = stripAuthoringCells(text, unitId);
  if (stripped === undefined) {
    await vscode.workspace.fs.copy(src, dest, { overwrite: true });
    return;
  }
  await vscode.workspace.fs.writeFile(dest, new TextEncoder().encode(stripped));
}
