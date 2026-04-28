// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import type { LearningService } from "../index.js";
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

  const ratio = completedKatas / katas.length;

  let encouragement: string;
  if (ratio >= 1) {
    return `All ${katas.length} katas complete — nicely done!`;
  } else if (ratio === 0) {
    encouragement = "let's get started!";
  } else if (ratio < 0.25) {
    encouragement = "great start!";
  } else if (ratio < 0.5) {
    encouragement = "making progress!";
  } else if (ratio < 0.75) {
    encouragement = "over halfway there!";
  } else {
    encouragement = "almost there!";
  }

  return `${completedKatas}/${katas.length} katas complete — ${encouragement}`;
}

/**
 * Wire up the Quantum Katas activity-bar panel:
 *   - a `ProgressWatcher` that tracks the detected katas workspace + progress file,
 *   - a native `TreeView` of Kata → Section nodes with an inline progress message,
 *   - the `qsharp-vscode.katas*` commands.
 *
 * Returns the `ProgressWatcher` instance so other features (e.g. the katas
 * webview panel) can subscribe to progress changes.
 */
export function registerKatasProgressView(
  context: vscode.ExtensionContext,
  learningService: LearningService,
): ProgressWatcher {
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

  // Kick off initial detection + load; fire-and-forget.
  void watcher.start();

  return watcher;
}
