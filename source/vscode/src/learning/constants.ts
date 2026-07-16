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

/** Context key set when a learning workspace is detected. */
export const LEARNING_WORKSPACE_DETECTED_CONTEXT =
  "qsharp-vscode.learningWorkspaceDetected";

/** Course ID for the built-in Quantum Katas. */
export const KATAS_COURSE_ID = "katas";

/** Per-course virtual environment folder (under the course working copy). */
// TODO (acasey): is there a way we can make it search recursively during discovery?  Sounds like there might be a workspace setting
export const LEARNING_VENV_DIR = ".venv";

/** Tree view ID for the learning progress panel. */
export const LEARNING_TREE_VIEW_ID = "qsharp-vscode.learningTree";
