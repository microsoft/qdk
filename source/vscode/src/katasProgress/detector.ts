// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import type { ProgressFileData } from "./types.js";

/** Well-known file that marks a workspace folder as a katas workspace. */
export const LEARNING_FILE = "qdk-learning.json";
/** Matches `NAVIGATE_FILE` in source/vscode/src/learning/mcp/server.ts. */
export const NAVIGATE_FILE = ".navigate.json";
/**
 * Signal file written by the MCP server to navigate the full-size panel.
 * Separate from `NAVIGATE_FILE` (which is tree-view → MCP server direction)
 * to avoid races between the two watchers.
 *
 * This is a temporary signal-file IPC workaround while the katas MCP tools
 * run out-of-process. Replace with a direct in-proc call when tools are
 * moved into the extension host.
 */
export const PANEL_NAVIGATE_FILE = ".panel-navigate.json";

export interface KatasWorkspaceInfo {
  /**
   * The parent directory (the workspace folder that contains `qdk-learning.json`).
   * Passed to the katas server / MCP CLI as `--workspace`.
   */
  workspaceRoot: vscode.Uri;
  /** The katas content folder, resolved from `katasRoot` in the learning file. */
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
    if (!(await uriExists(learningFile))) continue;

    let katasRootRel = "./quantum-katas";
    try {
      const bytes = await vscode.workspace.fs.readFile(learningFile);
      const raw = new TextDecoder("utf-8").decode(bytes);
      const parsed = JSON.parse(raw) as Partial<ProgressFileData>;
      if (parsed.katasRoot && typeof parsed.katasRoot === "string") {
        katasRootRel = parsed.katasRoot;
      }
    } catch {
      // Corrupt or unreadable — use default katasRoot.
    }

    const katasRoot = vscode.Uri.joinPath(folder.uri, katasRootRel);
    return {
      workspaceRoot: folder.uri,
      katasRoot,
      learningFile,
      katasDirExists: await uriExists(katasRoot),
    };
  }

  return undefined;
}
