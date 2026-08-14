// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { WORKBOOK_SUFFIX } from "./constants.js";
import type {
  CatalogCourse,
  NotebookCatalogCourse,
  NotebookCatalogUnit,
} from "./types.js";

// Where a course's files live on disk. `sourceNotebookUri` is authored
// content that ships with the course; `workbookUri` is the learner's
// editable copy, which exists only once the course has been materialized.

type CourseWithKind = Pick<CatalogCourse, "kind">;

export function isNotebookCourse(
  course: CatalogCourse,
): course is NotebookCatalogCourse;
export function isNotebookCourse<Course extends CourseWithKind>(
  course: Course,
): course is Course & { kind: "python-notebook" };
export function isNotebookCourse(course: CourseWithKind): boolean {
  return course.kind === "python-notebook";
}

/** The authored notebook that a unit's workbook is derived from. */
export function sourceNotebookUri(unit: NotebookCatalogUnit): vscode.Uri {
  return unit.sourceNotebookUri;
}

/**
 * The learner's editable copy of a unit's notebook: a `*.workbook.ipynb`
 * file beside the authored source.
 *
 * Keeping it as a sibling means the learner's notebook resolves the same
 * relative imports (`_course_lib.py`, `_unit.py`) as the source. Returns a
 * URI whether or not the file exists yet.
 */
export function workbookUri(unit: NotebookCatalogUnit): vscode.Uri {
  const src = unit.sourceNotebookUri;
  return src.with({ path: src.path.replace(/\.ipynb$/i, WORKBOOK_SUFFIX) });
}
