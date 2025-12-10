// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { EventType, sendTelemetryEvent } from "./telemetry";
import { getRandomGuid } from "./utils";

// The latest version for which we want to show the changelog page
const CHANGELOG_VERSION = "v1.23.0"; // <-- Update this when you want to show a new changelog to users

export function registerChangelogCommand(
  context: vscode.ExtensionContext,
): vscode.Disposable[] {
  return [
    vscode.commands.registerCommand("qsharp-vscode.showChangelog", async () => {
      const changelogUri = vscode.Uri.joinPath(
        context.extensionUri,
        "changelog.md",
      );
      await vscode.commands.executeCommand(
        "markdown.showPreview",
        changelogUri,
        vscode.ViewColumn.One,
      );
    }),
  ];
}

export async function maybeShowChangelogPrompt(
  context: vscode.ExtensionContext,
) {
  const lastChangelogNotificationVersion = context.globalState.get<string>(
    "qdk.lastChangelogNotificationVersion",
  );
  const suppressUpdateNotifications = vscode.workspace
    .getConfiguration("Q#")
    .get<boolean>("notifications.suppressUpdateNotifications");

  if (
    lastChangelogNotificationVersion !== CHANGELOG_VERSION &&
    !suppressUpdateNotifications
  ) {
    await context.globalState.update(
      "qdk.lastChangelogNotificationVersion",
      CHANGELOG_VERSION,
    );
    const buttons = ["What's New?", "Don't show this again"];
    const associatedId = getRandomGuid();
    sendTelemetryEvent(EventType.ChangelogPromptStart, {
      associationId: associatedId,
      changelogVersion: CHANGELOG_VERSION,
    });
    const choice = await vscode.window.showInformationMessage(
      "The Azure Quantum Development Kit has been updated.",
      ...buttons,
    );
    if (choice === buttons[0]) {
      sendTelemetryEvent(EventType.ChangelogPromptEnd, {
        associationId: associatedId,
        action: "showChangelog",
      });
      await vscode.commands.executeCommand("qsharp-vscode.showChangelog");
    } else if (choice === buttons[1]) {
      sendTelemetryEvent(EventType.ChangelogPromptEnd, {
        associationId: associatedId,
        action: "suppressChangelog",
      });
      await vscode.workspace
        .getConfiguration("Q#")
        .update(
          "notifications.suppressUpdateNotifications",
          true,
          vscode.ConfigurationTarget.Global,
        );
      vscode.window.showInformationMessage(
        'You will no longer receive "What\'s New" notifications. You can re-enable them from the Q# settings.',
      );
    }
  }
}
