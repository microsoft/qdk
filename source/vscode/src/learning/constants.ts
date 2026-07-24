// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/** Well-known workspace folder name for katas exercise/example files. */
export const LEARNING_WORKSPACE_FOLDER = "qdk-learning";

/** Relative path form of {@link LEARNING_WORKSPACE_FOLDER}, for use in URI joins. */
export const LEARNING_WORKSPACE_RELATIVE_PATH = `./${LEARNING_WORKSPACE_FOLDER}`;

/** Well-known file that marks a workspace folder as a katas workspace. */
export const LEARNING_FILE = "qdk-learning.json";

/** Subfolder (under the learning folder) that holds drop-in courses. */
export const LEARNING_COURSES_SUBDIR = "courses";

/** Filename describing a drop-in course. */
export const COURSE_MANIFEST_FILE = "course.json";

/** Filename containing the overview for a drop-in course. */
export const COURSE_README_FILE = "README.md";

/** Context key set when a learning workspace is detected. */
export const LEARNING_WORKSPACE_DETECTED_CONTEXT =
  "qsharp-vscode.learningWorkspaceDetected";

/** Suffix of the learner-editable working copy of a course notebook. */
export const WORKBOOK_SUFFIX = ".workbook.ipynb";

/**
 * Context key set while the active notebook editor is a course workbook.
 * Scopes notebook toolbar actions to learning content.
 */
export const LEARNING_NOTEBOOK_ACTIVE_CONTEXT =
  "qsharp-vscode.learningNotebookActive";

/** Course ID for the built-in Quantum Katas. */
export const KATAS_COURSE_ID = "katas";

/** Tree view ID for the learning progress panel. */
export const LEARNING_TREE_VIEW_ID = "qsharp-vscode.learningTree";
