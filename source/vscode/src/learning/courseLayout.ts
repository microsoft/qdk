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

/** Root folder a course was loaded from. Notebook courses only. */
function courseRootUri(course: NotebookCatalogCourse): vscode.Uri {
  return vscode.Uri.parse(course.sourceDir);
}

/** The authored notebook that a unit's workbook is derived from. */
export function sourceNotebookUri(
  course: NotebookCatalogCourse,
  unit: NotebookCatalogUnit,
): vscode.Uri {
  const rel = unit.sourceNotebookRelativePath;
  return vscode.Uri.joinPath(courseRootUri(course), rel);
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
  course: NotebookCatalogCourse,
  unit: NotebookCatalogUnit,
): vscode.Uri {
  const rel = unit.sourceNotebookRelativePath;
  return vscode.Uri.joinPath(
    courseRootUri(course),
    rel.replace(/\.ipynb$/i, WORKBOOK_SUFFIX),
  );
}
