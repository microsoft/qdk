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
 * with a fresh copy derived from the authored notebook. Returns `false` if
 * the workbook could not be written.
 */
export async function rematerializeUnitWorkbook(
  unit: NotebookCatalogUnit,
): Promise<boolean> {
  return materializeNotebook(
    sourceNotebookUri(unit),
    workbookUri(unit),
    unit.id,
  );
}

/**
 * Restore a single cell in a unit's working copy to its authored state,
 * leaving the learner's other cells untouched. When the workbook is open the
 * in-editor edit is authoritative — an open notebook won't reliably observe an
 * external file write, and a later save would clobber it. Returns `false` when
 * the cell can't be found or the restore fails.
 */
export async function restoreUnitWorkbookCell(
  unit: NotebookCatalogUnit,
  cellId: string,
): Promise<boolean> {
  try {
    const srcText = new TextDecoder().decode(
      await vscode.workspace.fs.readFile(sourceNotebookUri(unit)),
    );
    const authoredSource = findCellSource(srcText, cellId, unit.id);
    if (authoredSource === undefined) {
      log.warn(
        `Cell ${cellId} not found in the authored notebook for unit "${unit.id}".`,
      );
      return false;
    }

    const dest = workbookUri(unit);
    const open = vscode.workspace.notebookDocuments.find(
      (n) => n.uri.toString() === dest.toString(),
    );
    if (open) {
      return replaceOpenCell(open, cellId, authoredSource);
    }

    const destText = new TextDecoder().decode(
      await vscode.workspace.fs.readFile(dest),
    );
    const updated = replaceCellSource(
      destText,
      cellId,
      authoredSource,
      unit.id,
    );
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
 * Replace one cell of an open notebook with its authored source, preserving
 * the cell id and tags while dropping outputs and execution state. The
 * in-editor edit is the reset: it takes effect the moment the cell shows the
 * authored source, and the notebook is then saved to disk best-effort. Returns
 * `false` only when the cell is missing or the edit is rejected. A save that
 * can't complete leaves the reset cell unsaved in the editor — like any other
 * pending edit — instead of undoing the reset, so we never leave the cell
 * showing placeholder code and then report the reset as a failure.
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

  // Persist the reset best-effort. The edit already updated the editor, so a
  // save that doesn't land just leaves the cell unsaved, not un-reset; log it
  // for diagnostics but still report the cell as reset.
  try {
    if (!(await notebook.save())) {
      log.warn(
        `Reset cell ${cellId} in the editor, but saving the workbook to disk didn't complete.`,
      );
    }
  } catch (e) {
    log.warn(
      `Reset cell ${cellId} in the editor, but saving the workbook to disk failed: ${String(e)}`,
    );
  }
  return true;
}

/**
 * Write a unit's working copy: the authored notebook minus its author-only
 * cells (hints, solutions, explanations). Returns `false` if the copy could
 * not be written.
 *
 * If the notebook can't be parsed we fall back to copying it verbatim, so a
 * malformed notebook still leaves the learner with something to work in
 * rather than nothing.
 */
async function materializeNotebook(
  src: vscode.Uri,
  dest: vscode.Uri,
  unitId: string,
): Promise<boolean> {
  try {
    await ensureParentDir(dest);
    const text = new TextDecoder().decode(
      await vscode.workspace.fs.readFile(src),
    );
    const stripped = stripAuthoringCells(text, unitId);
    if (stripped === undefined) {
      await vscode.workspace.fs.copy(src, dest, { overwrite: true });
    } else {
      await vscode.workspace.fs.writeFile(
        dest,
        new TextEncoder().encode(stripped),
      );
    }
    return true;
  } catch (e) {
    log.warn(
      `Failed to materialize ${src.fsPath} → ${dest.fsPath}: ${String(e)}`,
    );
    return false;
  }
}
