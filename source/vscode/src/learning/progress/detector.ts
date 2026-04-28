// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { KATAS_WS_FOLDER_REL } from "../index.js";

/** Well-known file that marks a workspace folder as a katas workspace. */
export const LEARNING_FILE = "qdk-learning.json";

export interface KatasWorkspaceInfo {
  /**
   * The parent directory (the workspace folder that contains `qdk-learning.json`).
   * Passed to the katas server / MCP CLI as `--workspace`.
   */
  workspaceRoot: vscode.Uri;
  /** The katas content folder, resolved from the well-known folder name. */
  katasRoot: vscode.Uri;
  /** Path to `qdk-learning.json`. */
  learningFile: vscode.Uri;
  /** True when `katasRoot` already exists on disk. */
  katasDirExists: boolean;
}

async function uriExists(uri: vscode.Uri): Promise<boolean> {
  try {
    await vscode.workspace.fs.stat(uri);
    return true;
  } catch {
    return false;
  }
}

/**
 * Detect an existing Quantum Katas workspace by scanning all open workspace
 * folders for a `qdk-learning.json` file.
 *
 * Returns `undefined` if no katas workspace can be found.
 */
export async function detectKatasWorkspace(): Promise<
  KatasWorkspaceInfo | undefined
> {
  for (const folder of vscode.workspace.workspaceFolders ?? []) {
    const learningFile = vscode.Uri.joinPath(folder.uri, LEARNING_FILE);
    if (!(await uriExists(learningFile))) {
      continue;
    }

    const katasRoot = vscode.Uri.joinPath(folder.uri, KATAS_WS_FOLDER_REL);
    return {
      workspaceRoot: folder.uri,
      katasRoot,
      learningFile,
      katasDirExists: await uriExists(katasRoot),
    };
  }

  return undefined;
}
