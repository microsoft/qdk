// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import type {
  ActivityLocation,
  UnitProgress,
  OverallProgress,
  ActivityProgress,
  ActivityKind,
  CatalogCourse,
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
      treeDataProvider.update(snapshot, service.getCourses());
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
  private courses: CatalogCourse[] = [];

  update(
    snapshot: OverallProgress | undefined,
    courses?: CatalogCourse[],
  ): void {
    this.snapshot = snapshot;
    if (courses) {
      this.courses = courses;
    }
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

    if (node.kind === "course") {
      const item = new vscode.TreeItem(
        node.title,
        vscode.TreeItemCollapsibleState.Expanded,
      );
      item.iconPath = node.iconPath
        ? vscode.Uri.file(node.iconPath)
        : new vscode.ThemeIcon("book");
      item.contextValue = "course";
      item.tooltip = node.title;
      item.id = `course:${node.courseId}`;
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
        : activity.type === "example"
          ? "example"
          : activity.hasExample
            ? "lesson · example"
            : "lesson";
    item.iconPath = activityIcon(activity, isCurrent);
    item.contextValue = activity.type;
    item.tooltip = activity.isComplete
      ? `Completed${activity.completedAt ? ` \u00b7 ${new Date(activity.completedAt).toLocaleString()}` : ""}`
      : activity.type === "exercise"
        ? "Exercise \u2014 click the action icon to open"
        : activity.type === "example"
          ? "Example \u2014 click the action icon to open"
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
      const { courseId, unitId, activityId } = snap.currentPosition;

      const unit = snap.units.find((u) => u.id === unitId);
      const activity = unit?.activities.find((a) => a.id === activityId);
      if (unit && activity) {
        children.push({
          kind: "continue",
          courseId,
          unitId: unit.id,
          unitTitle: unit.title,
          activityId: activity.id,
          activityTitle: activity.title,
          activityKind: activity.type,
        });
      }

      // If there are multiple courses, show course-level grouping nodes.
      if (this.courses.length > 1) {
        for (const course of this.courses) {
          children.push({
            kind: "course",
            courseId: course.id,
            title: course.title,
            iconPath: course.iconPath,
          });
        }
      } else {
        // Single course — flat unit list (preserves existing katas-only UX)
        for (const u of snap.units) {
          children.push({
            kind: "unit",
            courseId: this.courses[0]?.id ?? "katas",
            unit: u,
            isCurrent: u.id === unitId,
          });
        }
      }

      return children;
    }

    if (node.kind === "course") {
      const { courseId } = node;
      const course = this.courses.find((c) => c.id === courseId);
      if (!course) {
        return [];
      }
      const { unitId } = snap.currentPosition;

      const unitNodes: LearningProgressNode[] = [];
      for (const cu of course.units) {
        const unitProgress = snap.units.find((u) => u.id === cu.id);
        if (unitProgress) {
          unitNodes.push({
            kind: "unit" as const,
            courseId,
            unit: unitProgress,
            isCurrent: unitProgress.id === unitId,
          });
        }
      }
      return unitNodes;
    }

    if (node.kind === "unit") {
      const { courseId, unitId, activityId } = snap.currentPosition;
      return node.unit.activities.map<LearningProgressNode>((activity) => ({
        kind: "activity",
        courseId: node.courseId,
        unitId: node.unit.id,
        unitTitle: node.unit.title,
        activity,
        isCurrent:
          node.courseId === courseId &&
          node.unit.id === unitId &&
          activity.id === activityId,
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
  | ({
      /** Pinned "Up next" shortcut at the top of the tree. */
      kind: "continue";
      unitTitle: string;
      activityTitle: string;
      activityKind: ActivityKind;
    } & ActivityLocation)
  | {
      /** Top-level course grouping node (when multiple courses exist). */
      kind: "course";
      courseId: string;
      title: string;
      iconPath?: string;
    }
  | {
      /** Unit node (expandable). */
      kind: "unit";
      courseId: string;
      unit: UnitProgress;
      isCurrent: boolean;
    }
  | {
      /** Leaf node representing a lesson, exercise, or example within a unit. */
      kind: "activity";
      courseId: string;
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
