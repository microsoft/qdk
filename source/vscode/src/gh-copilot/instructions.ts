// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";
import { EventType, sendTelemetryEvent, UserFlowStatus } from "../telemetry";

/**
 * Previously installed instruction files into the user's global storage.
 * Now a no-op — Q# and OpenQASM guidance is provided via extension skills.
 *
 * Kept as an export because callers in createProject.ts and tools.ts
 * fire-and-forget this function. Removing it would be a breaking change
 * in the same release.
 */
export async function updateCopilotInstructions(
  _trigger: "Command" | "Project" | "Activation" | "ChatToolCall",
  _context: vscode.ExtensionContext,
): Promise<void> {
  // No-op: instructions are now provided via extension skills.
}

/**
 * Registers the Copilot instructions command and cleans up any
 * previously-installed instruction files (now replaced by extension skills).
 */
export async function registerGhCopilotInstructionsCommand(
  context: vscode.ExtensionContext,
) {
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "qsharp-vscode.updateCopilotInstructions",
      () => {
        vscode.window.showInformationMessage(
          "Q# and OpenQASM coding guidance is now provided via built-in extension skills. " +
            "No additional configuration is needed.",
        );
      },
    ),
  );

  // Clean up old instruction files and settings from previous versions
  await cleanUpOldInstructions(context);
}

/**
 * Cleans up instruction files and settings from previous extension versions.
 *
 * Previous versions copied .instructions.md files to global storage and
 * registered the directory via chat.instructionsFilesLocations. This function
 * removes those artifacts since the content is now provided via extension skills.
 */
async function cleanUpOldInstructions(
  context: vscode.ExtensionContext,
): Promise<void> {
  try {
    // Remove the extension's directory from chat.instructionsFilesLocations
    await removeExtensionInstructionsFromUserConfig(context.globalStorageUri);

    // Delete old instruction files from global storage
    await deleteOldInstructionFiles(context.globalStorageUri);

    // If we had previously updated copilot-instructions.md with Q# instructions,
    // remove them now. Those are obsolete.
    await removeOldQSharpCopilotInstructions();
  } catch (error) {
    log.warn("Error cleaning up old Copilot instructions", error);
  }
}

/**
 * Removes the extension's chat-instructions directory from the user's
 * chat.instructionsFilesLocations setting, if present.
 */
async function removeExtensionInstructionsFromUserConfig(
  globalStateUri: vscode.Uri,
): Promise<void> {
  const extensionInstructionsDir = getExtensionInstructionsDir(globalStateUri);
  const config = vscode.workspace.getConfiguration("chat");
  const instructionsLocations = config.get<Record<string, boolean>>(
    "instructionsFilesLocations",
    {},
  );

  if (extensionInstructionsDir in instructionsLocations) {
    const updatedLocations = { ...instructionsLocations };
    delete updatedLocations[extensionInstructionsDir];

    // If the map is now empty, remove the setting entirely to keep config clean
    if (Object.keys(updatedLocations).length === 0) {
      await config.update(
        "instructionsFilesLocations",
        undefined,
        vscode.ConfigurationTarget.Global,
      );
    } else {
      await config.update(
        "instructionsFilesLocations",
        updatedLocations,
        vscode.ConfigurationTarget.Global,
      );
    }

    sendTelemetryEvent(
      EventType.UpdateCopilotInstructionsEnd,
      {
        reason: "cleaned up old instructions",
        flowStatus: UserFlowStatus.Succeeded,
      },
      {},
    );
  }
}

/**
 * Deletes old .instructions.md files from global storage.
 */
async function deleteOldInstructionFiles(
  globalStateUri: vscode.Uri,
): Promise<void> {
  const files = ["qsharp.instructions.md", "openqasm.instructions.md"];
  for (const file of files) {
    const target = vscode.Uri.joinPath(
      globalStateUri,
      "chat-instructions",
      file,
    );
    try {
      await vscode.workspace.fs.delete(target);
    } catch {
      // File doesn't exist, nothing to clean up
    }
  }

  // Try to remove the now-empty directory
  const dir = vscode.Uri.joinPath(globalStateUri, "chat-instructions");
  try {
    await vscode.workspace.fs.delete(dir);
  } catch {
    // Directory doesn't exist or isn't empty
  }
}

/**
 * Gets our extension's chat instructions directory's absolute path.
 * Used for identifying and cleaning up old settings entries.
 */
function getExtensionInstructionsDir(globalStateUri: vscode.Uri): string {
  const instructionsUri = vscode.Uri.joinPath(
    globalStateUri,
    "chat-instructions",
  );
  return instructionsUri.fsPath.replace(/[/\\]$/, "").replace(/\\/g, "/");
}

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
      continue;
    }
  }

  return removed;
}
