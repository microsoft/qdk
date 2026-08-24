// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";
import { sourceNotebookUri, workbookUri } from "../courseLayout.js";
import { ensureParentDir, uriExists } from "../fsUtils.js";
import {
  findCellSource,
  replaceCellSource,
  stripAuthoringCells,
} from "../notebookExercises.js";
import type { NotebookCatalogCourse, NotebookCatalogUnit } from "../types.js";

/**
 * Materialize the working copy for every unit in the course: derive each
 * `*.workbook.ipynb` sibling from the authored notebook. Existing workbooks
 * are never overwritten, preserving learner edits.
 */
export async function materializeCourseWorkbooks(
  course: NotebookCatalogCourse,
): Promise<void> {
  for (const unit of course.units) {
    const dest = workbookUri(unit);
    if (await uriExists(dest)) {
      continue;
    }
    await materializeNotebook(sourceNotebookUri(unit), dest, unit.id);
  }
}

/**
 * Re-materialize a single unit: overwrite its `*.workbook.ipynb`
 * with a fresh copy derived from the authored notebook.
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
 * Restore a single cell in a unit's working copy to its authored state,
 * leaving the learner's other cells untouched. Uses the notebook API when the
 * workbook is open, since an open notebook doesn't reliably pick up external
 * writes. Returns `false` when the cell can't be found.
 */
export async function restoreUnitWorkbookCell(
  unit: NotebookCatalogUnit,
  cellId: string,
): Promise<boolean> {
  try {
    const srcText = new TextDecoder().decode(
      await vscode.workspace.fs.readFile(sourceNotebookUri(unit)),
    );
    const original = findCellSource(srcText, cellId, unit.id);
    if (original === undefined) {
      log.warn(
        `Cell ${cellId} not found in the authored notebook for unit "${unit.id}".`,
      );
      return false;
    }

    const dest = workbookUri(unit);
    const open = vscode.workspace.notebookDocuments.find(
      (n) => n.uri.toString() === dest.toString(),
    );
    if (open && (await replaceOpenCell(open, cellId, original))) {
      return true;
    }

    const destText = new TextDecoder().decode(
      await vscode.workspace.fs.readFile(dest),
    );
    const updated = replaceCellSource(destText, cellId, original, unit.id);
    if (updated === undefined) {
      log.warn(
        `Cell ${cellId} not found in the workbook for unit "${unit.id}".`,
      );
      return false;
    }
    await vscode.workspace.fs.writeFile(
      dest,
      new TextEncoder().encode(updated),
    );
    return true;
  } catch (e) {
    log.warn(
      `Failed to restore cell ${cellId} in unit "${unit.id}": ${String(e)}`,
    );
    return false;
  }
}

/**
 * Replace one cell of an open notebook, preserving its id and tags while
 * dropping outputs and execution state. Returns `false` if the cell isn't
 * present or the edit is rejected, leaving the caller to fall back to disk.
 */
async function replaceOpenCell(
  notebook: vscode.NotebookDocument,
  cellId: string,
  source: string,
): Promise<boolean> {
  const index = notebook.getCells().findIndex((c) => c.metadata?.id === cellId);
  if (index < 0) {
    return false;
  }

  const existing = notebook.cellAt(index);
  const data = new vscode.NotebookCellData(
    existing.kind,
    source,
    existing.document.languageId,
  );
  // Keep the metadata so the stable cell id and its tags survive the replace.
  data.metadata = existing.metadata;
  data.outputs = [];
  data.executionSummary = undefined;

  const edit = new vscode.WorkspaceEdit();
  edit.set(notebook.uri, [
    vscode.NotebookEdit.replaceCells(
      new vscode.NotebookRange(index, index + 1),
      [data],
    ),
  ]);
  if (!(await vscode.workspace.applyEdit(edit))) {
    return false;
  }
  await notebook.save();
  return true;
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
