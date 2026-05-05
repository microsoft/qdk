// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/** Well-known workspace folder name for katas exercise/example files. */
export const LEARNING_WORKSPACE_FOLDER = "qdk-learning-ws";

/** Relative path form of {@link LEARNING_WORKSPACE_FOLDER}, for use in URI joins. */
export const LEARNING_WORKSPACE_RELATIVE_PATH = `./${LEARNING_WORKSPACE_FOLDER}`;

/** Well-known file that marks a workspace folder as a katas workspace. */
export const LEARNING_FILE = "qdk-learning.json";

/** Context key set when a katas workspace is detected. */
export const KATAS_DETECTED_CONTEXT = "qsharp-vscode.katasDetected";

/** Well-known folder name for filesystem-discovered learning courses. */
export const LEARNING_CONTENT_FOLDER = "qdk-learning-content";

/** The built-in course ID for the Quantum Katas. */
export const KATAS_COURSE_ID = "katas";
