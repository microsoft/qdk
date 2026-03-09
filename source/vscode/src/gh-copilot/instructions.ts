// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { EventType, sendTelemetryEvent } from "../telemetry";

/**
 * Removes deprecated Copilot instructions from previous releases.
 * We have transitioned to a tool-based approach for providing instructions to Copilot.
 */
export async function removeDeprecatedCopilotInstructions(
  context: vscode.ExtensionContext,
): Promise<void> {
  const removedConfig = await removeOldCopilotInstructionsConfig(context);
  const removedFiles =
    await removeOldInstructionsFilesFromGlobalStorage(context);

  if (removedConfig || removedFiles) {
    sendTelemetryEvent(EventType.RemoveOldCopilotInstructions);
  }
}

/**
 * Removes the extension's instructions directory from `chat.instructionsFilesLocations`.
 */
async function removeOldCopilotInstructionsConfig(
  context: vscode.ExtensionContext,
): Promise<boolean> {
  const config = vscode.workspace.getConfiguration("chat");
  const locations = config.get<Record<string, boolean>>(
    "instructionsFilesLocations",
    {},
  );

  const instructionsDir = vscode.Uri.joinPath(
    context.globalStorageUri,
    "chat-instructions",
  )
    .fsPath.replace(/[/\\]$/, "")
    .replace(/\\/g, "/");

  if (locations[instructionsDir]) {
    delete locations[instructionsDir];
    await config.update(
      "instructionsFilesLocations",
      locations,
      vscode.ConfigurationTarget.Global,
    );
    return true;
  }
  return false;
}

/**
 * Removes instructions `.md` files previously copied to global storage.
 */
async function removeOldInstructionsFilesFromGlobalStorage(
  context: vscode.ExtensionContext,
): Promise<boolean> {
  let result = false;
  const dir = vscode.Uri.joinPath(
    context.globalStorageUri,
    "chat-instructions",
  );

  for (const file of ["qsharp.instructions.md", "openqasm.instructions.md"]) {
    try {
      await vscode.workspace.fs.delete(vscode.Uri.joinPath(dir, file));
      result = true;
    } catch {
      // file doesn't exist or we couldn't delete it
    }
  }

  try {
    await vscode.workspace.fs.delete(dir);
    result = true;
  } catch {
    // directory doesn't exist or isn't empty
  }
  return result;
}
