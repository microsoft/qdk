// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { qsharpExtensionId } from "../common.js";
import { KatasPanelManager } from "./panel.js";
import type { ProgressWatcher } from "../katasProgress/progressReader.js";
import type { LearningService } from "../learningService/index.js";

/**
 * Register the `qsharp-vscode.showKatas` command that opens the Quantum Katas
 * webview panel, and register the WebviewPanelSerializer so the panel persists
 * across VS Code restarts.
 */
export function registerKatasPanelCommand(
  context: vscode.ExtensionContext,
  progressWatcher: ProgressWatcher,
  learningService: LearningService,
): void {
  // Register the serializer synchronously so VS Code knows this extension
  // handles the "qsharp-katas" viewType on restart.
  context.subscriptions.push(
    vscode.window.registerWebviewPanelSerializer("qsharp-katas", {
      async deserializeWebviewPanel(
        panel: vscode.WebviewPanel,
        _state: unknown,
      ) {
        const manager = KatasPanelManager.getInstance(
          context.extensionUri,
          progressWatcher,
          learningService,
        );
        await manager.restore(panel);
      },
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(`${qsharpExtensionId}.showKatas`, () => {
      const manager = KatasPanelManager.getInstance(
        context.extensionUri,
        progressWatcher,
        learningService,
      );
      return manager.show();
    }),

    vscode.commands.registerCommand(
      "qsharp-vscode.learningCheckSolution",
      async () => {
        if (!learningService.initialized) {
          vscode.window.showWarningMessage(
            "The QDK Learning workspace has not been initialized yet.",
          );
          return;
        }
        const pos = learningService.getPosition();
        if (pos.item.type !== "exercise") {
          vscode.window.showInformationMessage(
            "Navigate to an exercise to check your solution.",
          );
          return;
        }
        const manager = KatasPanelManager.getInstance(
          context.extensionUri,
          progressWatcher,
          learningService,
        );
        const passed = await manager.checkAndShowResult();

        // Trigger pass/fail flash decoration.
        void vscode.commands.executeCommand(
          "qsharp-vscode._learningFlash",
          passed,
        );
      },
    ),
  );
}
