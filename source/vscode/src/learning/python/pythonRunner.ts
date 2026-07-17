// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";
import type { CatalogCourse } from "../types.js";

// TODO (acasey): rename this

/**
 * Manages `python-notebook` course files. All Jupyter/notebook execution
 * is handled by VS Code's native notebook UI — this class only handles
 * materialization (copying course source to a working copy) and extension
 * readiness checks.
 */
export class PythonCourseRunner {
  /**
   * Soft-check that the Python and Jupyter extensions are available. On
   * VS Code for the Web (where they can't run) returns a desktop-only
   * message. Returns `undefined` when everything required is present.
   */
  async ensureExtensions(): Promise<string | undefined> {
    if (vscode.env.uiKind === vscode.UIKind.Web) {
      return (
        "Python notebook courses require the desktop version of VS Code " +
        "with the Python and Jupyter extensions."
      );
    }
    const missing: { id: string; name: string }[] = [];
    if (!vscode.extensions.getExtension("ms-python.python")) {
      missing.push({ id: "ms-python.python", name: "Python" });
    }
    if (!vscode.extensions.getExtension("ms-toolsai.jupyter")) {
      missing.push({ id: "ms-toolsai.jupyter", name: "Jupyter" });
    }
    if (missing.length === 0) {
      return undefined;
    }
    return `This course needs the ${missing
      .map((m) => m.name)
      .join(" and ")} extension${missing.length > 1 ? "s" : ""}.`;
  }

  /**
   * Prompt the user to install any missing required extensions. Safe to
   * call when nothing is missing (it no-ops).
   */
  async promptInstallExtensions(): Promise<void> {
    if (vscode.env.uiKind === vscode.UIKind.Web) {
      return;
    }
    // TODO (acasey): share code with ensureExtensions
    const required: { id: string; name: string }[] = [
      { id: "ms-python.python", name: "Python" },
      { id: "ms-toolsai.jupyter", name: "Jupyter" },
    ].filter((e) => !vscode.extensions.getExtension(e.id));
    if (required.length === 0) {
      return;
    }
    const choice = await vscode.window.showInformationMessage(
      `This course needs the ${required
        .map((r) => r.name)
        .join(" and ")} extension${required.length > 1 ? "s" : ""}.`,
      "Install",
    );
    if (choice !== "Install") {
      return;
    }
    for (const ext of required) {
      await vscode.commands.executeCommand(
        "workbench.extensions.installExtension",
        ext.id,
      );
    }
  }

  /**
   * Working-copy URI of a unit's notebook: a `*.workbook.ipynb` file that
   * sits beside the authored source notebook in the same unit folder.
   *
   * Keeping the working copy as a sibling means the learner's notebook
   * resolves the same relative imports (`_course_lib.py`, `_unit.py`, etc.) as the
   * source.
   */
  workbookFileUri(course: CatalogCourse, notebookRel: string): vscode.Uri {
    if (!course.sourceDir) {
      throw new Error(`Course "${course.id}" has no source folder.`);
    }
    const sourceRoot = vscode.Uri.parse(course.sourceDir);
    return vscode.Uri.joinPath(sourceRoot, toWorkbookRel(notebookRel));
  }

  /**
   * Materialize the working copy for every unit in the course: copy each
   * authored notebook to its `*.workbook.ipynb` sibling. Existing workbooks
   * are never overwritten, preserving learner edits.
   */
  async materializeCourse(course: CatalogCourse): Promise<void> {
    if (!course.sourceDir) {
      throw new Error(`Course "${course.id}" has no source folder.`);
    }
    const sourceRoot = vscode.Uri.parse(course.sourceDir);

    for (const unit of course.units) {
      if (!unit.notebookRel) {
        continue;
      }
      await this.copyIfMissing(
        vscode.Uri.joinPath(sourceRoot, unit.notebookRel),
        vscode.Uri.joinPath(sourceRoot, toWorkbookRel(unit.notebookRel)),
      );
    }
  }

  /**
   * Re-materialize a single unit: overwrite its `*.workbook.ipynb`
   * with a fresh copy of the authored notebook.
   */
  async rematerializeUnit(
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
    const src = vscode.Uri.joinPath(sourceRoot, unit.notebookRel);
    const dest = vscode.Uri.joinPath(
      sourceRoot,
      toWorkbookRel(unit.notebookRel),
    );
    await ensureParentDir(dest);
    await vscode.workspace.fs.copy(src, dest, { overwrite: true });
  }

  /** Copy a file only if the destination doesn't already exist. */
  private async copyIfMissing(
    src: vscode.Uri,
    dest: vscode.Uri,
  ): Promise<void> {
    if (await uriExists(dest)) {
      return;
    }
    try {
      await ensureParentDir(dest);
      await vscode.workspace.fs.copy(src, dest, { overwrite: false });
    } catch (e) {
      log.warn(`Failed to copy ${src.fsPath} → ${dest.fsPath}: ${String(e)}`);
    }
  }
}

// ─── Helpers ───

/**
 * Map a source notebook's relative path to its working-copy sibling by
 * swapping the `.ipynb` extension for `.workbook.ipynb`
 * (e.g. `01-intro/intro.ipynb` → `01-intro/intro.workbook.ipynb`).
 */
function toWorkbookRel(notebookRel: string): string {
  return notebookRel.replace(/\.ipynb$/i, ".workbook.ipynb");
}

async function uriExists(uri: vscode.Uri): Promise<boolean> {
  try {
    await vscode.workspace.fs.stat(uri);
    return true;
  } catch {
    return false;
  }
}

async function ensureParentDir(fileUri: vscode.Uri): Promise<void> {
  const parentUri = vscode.Uri.joinPath(fileUri, "..");
  try {
    await vscode.workspace.fs.createDirectory(parentUri);
  } catch {
    // already exists
  }
}
