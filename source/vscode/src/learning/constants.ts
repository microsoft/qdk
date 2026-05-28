// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/** Well-known workspace folder name for katas exercise/example files. */
export const LEARNING_WORKSPACE_FOLDER = "qdk-learning";

/** Relative path form of {@link LEARNING_WORKSPACE_FOLDER}, for use in URI joins. */
export const LEARNING_WORKSPACE_RELATIVE_PATH = `./${LEARNING_WORKSPACE_FOLDER}`;

/** Well-known file that marks a workspace folder as a katas workspace. */
export const LEARNING_FILE = "qdk-learning.json";

/** Context key set when a learning workspace is detected. */
export const LEARNING_WORKSPACE_DETECTED_CONTEXT =
  "qsharp-vscode.learningWorkspaceDetected";

/** Course ID for the built-in Quantum Katas. */
export const KATAS_COURSE_ID = "katas";

/** Tree view ID for the learning progress panel. */
export const LEARNING_TREE_VIEW_ID = "qsharp-vscode.learningTree";
