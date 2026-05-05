// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import type {
  UnitProgress,
  OverallProgress,
  ActivityProgress,
  ActivityKind,
} from "./types.js";
import type { LearningService } from "./service.js";

/**
 * Wire up the Quantum Katas progress panel, a `TreeView` of Unit → Activity
 * nodes with action buttons and progress indicators.
 */
export function registerLearningProgressView(
  context: vscode.ExtensionContext,
  service: LearningService,
): void {
  const treeDataProvider = new LearningProgressTreeProvider();
  const treeView = vscode.window.createTreeView("qsharp-vscode.learningTree", {
    treeDataProvider,
    showCollapseAll: true,
  });
  context.subscriptions.push(
    service.onDidChangeProgress((snapshot) => {
      treeDataProvider.update(snapshot);
      treeView.message = buildTreeMessage(snapshot);
    }),
    treeView.onDidChangeVisibility((e) => {
      if (e.visible) {
        service.ensureInitialized();
      }
    }),
    treeView,
    treeDataProvider,
  );

  // If the panel is already visible at registration time (e.g. VS Code
  // restored the activity bar on startup), initialize immediately.
  if (treeView.visible) {
    service.ensureInitialized();
  }
}

function buildTreeMessage(
  snapshot: OverallProgress | undefined,
): string | undefined {
  if (!snapshot) {
    return undefined;
  }

  const units = snapshot.units;
  if (units.length === 0) {
    return undefined;
  }

  const completedUnits = units.filter(
    (u) => u.total > 0 && u.completed === u.total,
  ).length;

  const ratio = completedUnits / units.length;

  let encouragement: string;
  if (ratio >= 1) {
    return `All ${units.length} units complete — nicely done!`;
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

  return `${completedUnits}/${units.length} units complete — ${encouragement}`;
}

export class LearningProgressTreeProvider implements vscode.TreeDataProvider<LearningProgressNode> {
  private readonly emitter = new vscode.EventEmitter<
    LearningProgressNode | undefined
  >();
  readonly onDidChangeTreeData = this.emitter.event;

  private snapshot: OverallProgress | undefined;

  update(snapshot: OverallProgress | undefined): void {
    this.snapshot = snapshot;
    this.emitter.fire(undefined);
  }

  getTreeItem(node: LearningProgressNode): vscode.TreeItem {
    if (node.kind === "continue") {
      const item = new vscode.TreeItem(
        `Up next: ${node.activityTitle}`,
        vscode.TreeItemCollapsibleState.None,
      );
      item.description = node.unitTitle;
      item.iconPath = iconContinue;
      item.contextValue = "continue";
      item.tooltip = `Continue learning — ${node.unitTitle}: ${node.activityTitle}`;
      item.id = "continue";
      return item;
    }

    if (node.kind === "unit") {
      const { unit, isCurrent } = node;
      const item = new vscode.TreeItem(
        unit.title,
        isCurrent
          ? vscode.TreeItemCollapsibleState.Expanded
          : vscode.TreeItemCollapsibleState.Collapsed,
      );
      item.description = `${unit.completed}/${unit.total}`;
      item.iconPath = unitIcon(unit);
      item.contextValue = "unit";
      item.tooltip = `${unit.title} — ${unit.completed}/${unit.total} activities complete`;
      // Vary the id by `isCurrent` so VS Code sees a new node when the active
      // unit changes and applies the collapsibleState we set above.
      item.id = isCurrent ? `unit:${unit.id}:current` : `unit:${unit.id}`;
      return item;
    }

    const { unitId, activity, isCurrent } = node;
    const item = new vscode.TreeItem(
      activity.title,
      vscode.TreeItemCollapsibleState.None,
    );
    item.description =
      activity.type === "exercise"
        ? "exercise"
        : activity.hasExample
          ? "lesson · example"
          : "lesson";
    item.iconPath = activityIcon(activity, isCurrent);
    item.contextValue = activity.type;
    item.tooltip = activity.isComplete
      ? `Completed${activity.completedAt ? ` \u00b7 ${new Date(activity.completedAt).toLocaleString()}` : ""}`
      : activity.type === "exercise"
        ? "Exercise \u2014 click the action icon to open"
        : "Lesson \u2014 click the action icon to open";
    item.id = `activity:${unitId}:${activity.id}`;
    return item;
  }

  getChildren(node?: LearningProgressNode): LearningProgressNode[] {
    const snap = this.snapshot;
    if (!snap) {
      return [];
    }

    if (!node) {
      const children: LearningProgressNode[] = [];
      const { unitId, activityId } = snap.currentPosition;

      const unit = snap.units.find((u) => u.id === unitId);
      const activity = unit?.activities.find((a) => a.id === activityId);
      if (unit && activity) {
        children.push({
          kind: "continue",
          unitId: unit.id,
          unitTitle: unit.title,
          activityId: activity.id,
          activityTitle: activity.title,
          activityKind: activity.type,
        });
      }

      for (const u of snap.units) {
        children.push({
          kind: "unit",
          unit: u,
          isCurrent: u.id === unitId,
        });
      }

      return children;
    }

    if (node.kind === "unit") {
      const { unitId, activityId } = snap.currentPosition;
      return node.unit.activities.map<LearningProgressNode>((activity) => ({
        kind: "activity",
        unitId: node.unit.id,
        unitTitle: node.unit.title,
        activity,
        isCurrent: node.unit.id === unitId && activity.id === activityId,
      }));
    }

    return [];
  }

  dispose(): void {
    this.emitter.dispose();
  }
}

/**
 * Node identity in the tree. Roots carry the full `UnitProgress`; children
 * carry the parent's `unitId` plus the `ActivityProgress` for quick lookups.
 */
type LearningProgressNode =
  | {
      /** Pinned "Up next" shortcut at the top of the tree. */
      kind: "continue";
      unitId: string;
      unitTitle: string;
      activityId: string;
      activityTitle: string;
      activityKind: ActivityKind;
    }
  | {
      /** Top-level kata unit node (expandable). */
      kind: "unit";
      unit: UnitProgress;
      isCurrent: boolean;
    }
  | {
      /** Leaf node representing a lesson or exercise within a unit. */
      kind: "activity";
      unitId: string;
      unitTitle: string;
      activity: ActivityProgress;
      isCurrent: boolean;
    };

/** Icon for the pinned "Up next" shortcut node. */
const iconContinue = new vscode.ThemeIcon(
  "sparkle",
  new vscode.ThemeColor("charts.blue"),
);
/** Icon for a fully completed activity or unit. */
const iconPassed = new vscode.ThemeIcon(
  "pass",
  new vscode.ThemeColor("testing.iconPassed"),
);
/** Icon for the activity the user is currently on. */
const iconCurrent = new vscode.ThemeIcon(
  "circle-filled",
  new vscode.ThemeColor("charts.blue"),
);
/** Icon for an activity or unit that has not been started. */
const iconIncomplete = new vscode.ThemeIcon("circle-large-outline");
/** Icon for a unit that is partially complete. */
const iconInProgress = new vscode.ThemeIcon(
  "record",
  new vscode.ThemeColor("charts.blue"),
);

function activityIcon(
  a: ActivityProgress,
  isCurrent: boolean,
): vscode.ThemeIcon {
  if (a.isComplete) {
    return iconPassed;
  }
  if (isCurrent) {
    return iconCurrent;
  }
  return iconIncomplete;
}

function unitIcon(u: UnitProgress): vscode.ThemeIcon {
  if (u.total > 0 && u.completed === u.total) {
    return iconPassed;
  }
  if (u.completed > 0) {
    return iconInProgress;
  }
  return iconIncomplete;
}
