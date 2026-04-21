// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";

/** Matches `WORKSPACE_FOLDER` in source/vscode/src/learning/server/server.ts. */
export const KATAS_SUBFOLDER = "quantum-katas";
/** Matches `PROGRESS_FILE` in source/vscode/src/learning/server/progress.ts. */
export const PROGRESS_FILE = ".katas-progress.json";

export interface KatasWorkspaceInfo {
  /**
   * The parent directory passed to the katas server / MCP CLI as
   * `--workspace`. The katas server creates / consumes the
   * `quantum-katas` subfolder inside it.
   */
  workspaceRoot: vscode.Uri;
  /** The `quantum-katas` folder itself (`workspaceRoot/quantum-katas`). */
  katasRoot: vscode.Uri;
  /** Path to the `.katas-progress.json` file inside `katasRoot`. */
  progressFile: vscode.Uri;
  /** True when `katasRoot` (the `quantum-katas` directory) already exists on disk. */
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

function infoFor(
  workspaceRoot: vscode.Uri,
  katasDirExists: boolean,
): KatasWorkspaceInfo {
  const katasRoot = vscode.Uri.joinPath(workspaceRoot, KATAS_SUBFOLDER);
  const progressFile = vscode.Uri.joinPath(katasRoot, PROGRESS_FILE);
  return { workspaceRoot, katasRoot, progressFile, katasDirExists };
}

/**
 * Detect an existing Quantum Katas workspace.
 *
 * Priority:
 *  1. `Q#.learning.workspaceRoot` setting — used verbatim (the katas server
 *     creates a `quantum-katas` subfolder under this path).
 *  2. Each `vscode.workspace.workspaceFolders[i]` containing a
 *     `quantum-katas/.katas-progress.json` or `quantum-katas/exercises/` directory.
 *
 * Returns `undefined` if no katas workspace can be found.
 */
export async function detectKatasWorkspace(): Promise<
  KatasWorkspaceInfo | undefined
> {
  const cfg = vscode.workspace.getConfiguration("Q#");
  const configured = (cfg.get<string>("learning.workspaceRoot") ?? "").trim();

  if (configured.length > 0) {
    const root = vscode.Uri.file(configured);
    const katasDir = vscode.Uri.joinPath(root, KATAS_SUBFOLDER);
    return infoFor(root, await uriExists(katasDir));
  }

  for (const folder of vscode.workspace.workspaceFolders ?? []) {
    const katasRoot = vscode.Uri.joinPath(folder.uri, KATAS_SUBFOLDER);
    const progressFile = vscode.Uri.joinPath(katasRoot, PROGRESS_FILE);
    const exercisesDir = vscode.Uri.joinPath(katasRoot, "exercises");
    if ((await uriExists(progressFile)) || (await uriExists(exercisesDir))) {
      return infoFor(folder.uri, true);
    }
  }

  return undefined;
}
