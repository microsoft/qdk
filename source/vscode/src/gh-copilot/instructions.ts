// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";

/**
 * Removes deprecated Copilot instructions from previous releases.
 * We have transitioned to a tool-based approach for providing instructions to Copilot.
 */
export async function removeDeprecatedCopilotInstructions(
  context: vscode.ExtensionContext,
): Promise<void> {
  const removedOldInstructions = await removeOldQSharpCopilotInstructions();
  if (removedOldInstructions) {
    log.info("Removed old Q# instructions from copilot-instructions.md.");
  }

  await removeOldCopilotInstructionsConfig(context);
  await removeOldInstructionsFilesFromGlobalStorage(context);
}

/**
/**
 * Removes old Q# instructions from the copilot-instructions.md file if they exist.
 * These were only added by the QDK extension in the April 2025 release.
 *
 * @returns true if instructions were found and removed, false otherwise.
 */
async function removeOldQSharpCopilotInstructions(): Promise<boolean> {
  const oldCodingInstructionsTitle =
    "# Q# coding instructions (updated April 2025)";
  const oldCodingInstructionsFooter = `<!-- End: Q# coding instructions -->\n\n`;

  const workspaceFolders = vscode.workspace.workspaceFolders;
  if (!workspaceFolders || workspaceFolders.length === 0) {
    return false;
  }

  let removed = false;

  for (const workspaceFolder of workspaceFolders) {
    const instructionsFile = vscode.Uri.joinPath(
      workspaceFolder.uri,
      ".github",
      "copilot-instructions.md",
    );

    let text = "";
    try {
      const content = await vscode.workspace.fs.readFile(instructionsFile);
      text = new TextDecoder("utf-8").decode(content);
      const startIndex = text.indexOf(oldCodingInstructionsTitle);
      if (startIndex === -1) {
        continue;
      }
      let endIndex = text.indexOf(oldCodingInstructionsFooter, startIndex);

      if (endIndex !== -1) {
        endIndex += oldCodingInstructionsFooter.length;
        // Skip any trailing newlines after the footer
        while (
          endIndex < text.length &&
          (text[endIndex] === "\n" || text[endIndex] === "\r")
        ) {
          endIndex++;
        }

        // Create new content without the Q# instructions
        const newContent =
          text.substring(0, startIndex) + text.substring(endIndex);

        // Write back the file without the Q# instructions
        await vscode.workspace.fs.writeFile(
          instructionsFile,
          new TextEncoder().encode(newContent),
        );
      }
      removed = true;
    } catch {
      // file doesn't exist or we couldn't edit it
    }
  }

  return removed;
}

/**
 * Removes the extension's instructions directory from `chat.instructionsFilesLocations`.
 */
async function removeOldCopilotInstructionsConfig(
  context: vscode.ExtensionContext,
): Promise<void> {
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
  }
}

/**
 * Removes instructions `.md` files previously copied to global storage.
 */
async function removeOldInstructionsFilesFromGlobalStorage(
  context: vscode.ExtensionContext,
): Promise<void> {
  const dir = vscode.Uri.joinPath(
    context.globalStorageUri,
    "chat-instructions",
  );

  for (const file of ["qsharp.instructions.md", "openqasm.instructions.md"]) {
    try {
      await vscode.workspace.fs.delete(vscode.Uri.joinPath(dir, file));
    } catch {
      // file doesn't exist or we couldn't delete it
    }
  }

  try {
    await vscode.workspace.fs.delete(dir);
  } catch {
    // directory doesn't exist or isn't empty
  }
}
