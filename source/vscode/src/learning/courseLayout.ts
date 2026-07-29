// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { WORKBOOK_SUFFIX } from "./constants.js";
import type { CatalogCourse, CatalogUnit } from "./types.js";

// Where a course's files live on disk. `sourceNotebookUri` is authored
// content that ships with the course; `workbookUri` is the learner's
// editable copy, which exists only once the course has been materialized.

/** Root folder a course was loaded from. Drop-in courses only. */
export function courseRootUri(course: CatalogCourse): vscode.Uri {
  if (!course.sourceDir) {
    throw new Error(`Course "${course.id}" has no source folder.`);
  }
  return vscode.Uri.parse(course.sourceDir);
}

/** The units of a course that have an authored notebook. */
export function notebookUnits(course: CatalogCourse): CatalogUnit[] {
  return course.units.filter((u) => u.sourceNotebookRel !== undefined);
}

/** The authored notebook that a unit's workbook is derived from. */
export function sourceNotebookUri(
  course: CatalogCourse,
  unit: CatalogUnit,
): vscode.Uri {
  return vscode.Uri.joinPath(
    courseRootUri(course),
    requireSourceNotebookRel(course, unit),
  );
}

/**
 * The learner's editable copy of a unit's notebook: a `*.workbook.ipynb`
 * file beside the authored source.
 *
 * Keeping it as a sibling means the learner's notebook resolves the same
 * relative imports (`_course_lib.py`, `_unit.py`) as the source. Returns a
 * URI whether or not the file exists yet.
 */
export function workbookUri(
  course: CatalogCourse,
  unit: CatalogUnit,
): vscode.Uri {
  const rel = requireSourceNotebookRel(course, unit);
  return vscode.Uri.joinPath(
    courseRootUri(course),
    rel.replace(/\.ipynb$/i, WORKBOOK_SUFFIX),
  );
}

function requireSourceNotebookRel(
  course: CatalogCourse,
  unit: CatalogUnit,
): string {
  if (!unit.sourceNotebookRel) {
    throw new Error(
      `Unit "${unit.id}" in course "${course.id}" has no notebook.`,
    );
  }
  return unit.sourceNotebookRel;
}
