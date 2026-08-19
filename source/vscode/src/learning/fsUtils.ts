// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";

/** True if something exists at the given URI. */
export async function uriExists(uri: vscode.Uri): Promise<boolean> {
  try {
    await vscode.workspace.fs.stat(uri);
    return true;
  } catch {
    return false;
  }
}

/** Create the containing directory of a file URI, if it doesn't already exist. */
export async function ensureParentDir(fileUri: vscode.Uri): Promise<void> {
  const parentUri = vscode.Uri.joinPath(fileUri, "..");
  try {
    await vscode.workspace.fs.createDirectory(parentUri);
  } catch {
    // already exists
  }
}
