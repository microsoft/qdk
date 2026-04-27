// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { qsharpExtensionId } from "../common.js";
import { KatasPanelManager } from "./panel.js";
import type { ProgressWatcher } from "../katasProgress/progressReader.js";

/**
 * Register the `qsharp-vscode.showKatas` command that opens the Quantum Katas
 * webview panel.
 */
export function registerKatasPanelCommand(
  context: vscode.ExtensionContext,
  progressWatcher: ProgressWatcher,
): void {
  context.subscriptions.push(
    vscode.commands.registerCommand(`${qsharpExtensionId}.showKatas`, () => {
      const manager = KatasPanelManager.getInstance(
        context.extensionUri,
        progressWatcher,
      );
      return manager.show();
    }),
  );
}
