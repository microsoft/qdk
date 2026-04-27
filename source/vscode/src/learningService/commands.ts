// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import type { LearningService } from "./service.js";

/**
 * Register editor-facing learning commands: show hint, reset exercise, next.
 *
 * The "check solution" command lives in `katasPanel/index.ts` because it
 * needs the panel manager to render results in the webview.
 */
export function registerLearningCommands(
  context: vscode.ExtensionContext,
  service: LearningService,
): void {
  context.subscriptions.push(
    vscode.commands.registerCommand("qsharp-vscode.learningShowHint", () => {
      if (!service.initialized) return;
      service.getNextHint();
      // Open the katas panel which will show the hint.
      void vscode.commands.executeCommand("qsharp-vscode.showKatas");
    }),

    vscode.commands.registerCommand(
      "qsharp-vscode.learningResetExercise",
      async () => {
        if (!service.initialized) return;

        const pos = service.getPosition();
        if (pos.item.type !== "exercise") {
          vscode.window.showInformationMessage(
            "Navigate to an exercise to reset it.",
          );
          return;
        }

        const confirmed = await vscode.window.showWarningMessage(
          "Reset this exercise to the original placeholder code? Your current code will be lost.",
          { modal: true },
          "Reset",
        );
        if (confirmed !== "Reset") return;

        await service.resetExercise();
        vscode.window.showInformationMessage("Exercise has been reset.");
      },
    ),

    vscode.commands.registerCommand("qsharp-vscode.learningNext", async () => {
      if (!service.initialized) return;
      const { moved } = service.next();
      if (!moved) {
        vscode.window.showInformationMessage(
          "You've reached the end of the available content!",
        );
      }
    }),

    vscode.commands.registerCommand("qsharp-vscode.learningOpenPanel", () => {
      void vscode.commands.executeCommand("qsharp-vscode.showKatas");
    }),
  );
}
