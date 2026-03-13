// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { EventType, sendTelemetryEvent } from "../telemetry";
import { log } from "qsharp-lang";

/**
 * Removes deprecated Copilot instructions that were placed by previous releases (May 2025 - Mar 2026)
 * We have transitioned to a chatInstructions-based approach for providing instructions to Copilot.
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
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const { [instructionsDir]: _, ...rest } = locations;
    try {
      await config.update(
        "instructionsFilesLocations",
        rest,
        vscode.ConfigurationTarget.Global,
      );
      return true;
    } catch {
      log.warn(
        `Could not remove old instructions directory from chat.instructionsFilesLocations config`,
      );
    }
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

  try {
    await vscode.workspace.fs.delete(dir, { recursive: true });
    result = true;
  } catch {
    // directory doesn't exist or we couldn't delete it
    log.warn(`Could not delete old instructions directory at ${dir.fsPath}`);
  }
  return result;
}
