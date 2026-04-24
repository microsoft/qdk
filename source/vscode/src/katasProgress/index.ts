// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { registerKatasCommands } from "./commands.js";
import { ProgressWatcher } from "./progressReader.js";
import { KatasTreeProvider } from "./treeProvider.js";
import type { OverallProgress } from "./types.js";

function buildTreeMessage(
  snapshot: OverallProgress | undefined,
  detected: boolean,
): string | undefined {
  if (!detected || !snapshot) return undefined;

  const katas = snapshot.katas;
  if (katas.length === 0) return undefined;

  const completedKatas = katas.filter(
    (k) => k.total > 0 && k.completed === k.total,
  ).length;

  if (completedKatas >= katas.length) {
    return `All ${katas.length} katas complete — nicely done!`;
  }

  return `${completedKatas}/${katas.length} katas complete — keep it up!`;
}

/**
 * Wire up the Quantum Katas activity-bar panel:
 *   - a `ProgressWatcher` that tracks the detected katas workspace + progress file,
 *   - a native `TreeView` of Kata → Section nodes with an inline progress message,
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

  context.subscriptions.push(
    watcher.onDidChange((snapshot) => {
      treeProvider.update(snapshot);
      treeView.message = buildTreeMessage(
        snapshot,
        watcher.workspaceInfo !== undefined,
      );
    }),
  );

  registerKatasCommands(context, watcher, treeProvider);

  // Kick off initial detection + load; fire-and-forget.
  void watcher.start();
}
