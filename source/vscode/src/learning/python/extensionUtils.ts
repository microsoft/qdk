// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";

/** Extensions required to run `python-notebook` courses. */
const REQUIRED_EXTENSIONS: { id: string; name: string }[] = [
  { id: "ms-python.python", name: "Python" },
  { id: "ms-toolsai.jupyter", name: "Jupyter" },
];

/**
 * Returns the subset of {@link REQUIRED_EXTENSIONS} that are not currently
 * installed.
 */
function getMissingExtensions(): { id: string; name: string }[] {
  return REQUIRED_EXTENSIONS.filter(
    (e) => !vscode.extensions.getExtension(e.id),
  );
}

/**
 * Prompt the user to install any missing required extensions. Safe to
 * call when nothing is missing (it no-ops).
 */
export async function promptInstallPythonExtensions(): Promise<void> {
  if (vscode.env.uiKind === vscode.UIKind.Web) {
    return;
  }
  const required = getMissingExtensions();
  if (required.length === 0) {
    return;
  }
  const choice = await vscode.window.showInformationMessage(
    `This course needs the ${required
      .map((r) => r.name)
      .join(" and ")} extension${required.length > 1 ? "s" : ""}.`,
    "Install",
  );
  if (choice !== "Install") {
    return;
  }
  for (const ext of required) {
    await vscode.commands.executeCommand(
      "workbench.extensions.installExtension",
      ext.id,
    );
  }
}
