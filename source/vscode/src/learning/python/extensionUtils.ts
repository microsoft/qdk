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
 * Soft-check that the Python and Jupyter extensions are available. On
 * VS Code for the Web (where they can't run) returns a desktop-only
 * message. Returns `undefined` when everything required is present.
 */
export function checkPythonExtensions(): string | undefined {
  if (vscode.env.uiKind === vscode.UIKind.Web) {
    return (
      "Python notebook courses require the desktop version of VS Code " +
      "with the Python and Jupyter extensions."
    );
  }
  const missing = getMissingExtensions();
  if (missing.length === 0) {
    return undefined;
  }
  return `This course needs the ${missing
    .map((m) => m.name)
    .join(" and ")} extension${missing.length > 1 ? "s" : ""}.`;
}

// TODO (acasey): there's no real reason to prompt here if it's only reachable from
// the environment check dialog and the user already clicked a button.
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
