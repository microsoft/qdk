// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { registerKatasCommands } from "./commands.js";
import { KatasOverviewProvider } from "./overviewProvider.js";
import { ProgressWatcher } from "./progressReader.js";
import { KatasTreeProvider } from "./treeProvider.js";

/**
 * Wire up the Quantum Katas activity-bar panel:
 *   - a `ProgressWatcher` that tracks the detected katas workspace + progress file,
 *   - a native `TreeView` of Kata → Section nodes,
 *   - a `WebviewView` header with an overall progress bar and "Continue" button,
 *   - the `qsharp-vscode.katas*` commands.
 */
export function registerKatasProgressView(
  context: vscode.ExtensionContext,
): void {
  const watcher = new ProgressWatcher();
  context.subscriptions.push(watcher);

  const treeProvider = new KatasTreeProvider();
  const treeView = vscode.window.createTreeView("qsharp-vscode.katasTree", {
    treeDataProvider: treeProvider,
    showCollapseAll: true,
  });
  context.subscriptions.push(treeView);
  context.subscriptions.push({ dispose: () => treeProvider.dispose() });

  const overviewProvider = new KatasOverviewProvider(context.extensionUri);
  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider(
      KatasOverviewProvider.viewType,
      overviewProvider,
    ),
  );

  context.subscriptions.push(
    watcher.onDidChange((snapshot) => {
      treeProvider.update(snapshot);
      overviewProvider.update(snapshot, watcher.workspaceInfo !== undefined);
    }),
  );

  registerKatasCommands(context, watcher);

  // Kick off initial detection + load; fire-and-forget.
  void watcher.start();
}
