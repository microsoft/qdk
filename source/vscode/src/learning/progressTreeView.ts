// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import type {
  ActivityLocation,
  CourseDescriptor,
  CourseKind,
  UnitProgress,
  OverallProgress,
  ActivityProgress,
} from "./types.js";
import type { LearningService } from "./service.js";
import { LEARNING_TREE_VIEW_ID } from "./constants.js";

/**
 * Wire up the QDK Learning progress panel, a `TreeView` of
 * Course → Unit → Activity nodes with action buttons and progress
 * indicators.
 */
export function registerLearningProgressView(
  context: vscode.ExtensionContext,
  service: LearningService,
): void {
  const treeDataProvider = new LearningProgressTreeProvider(service);
  const treeView = vscode.window.createTreeView(LEARNING_TREE_VIEW_ID, {
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
        service.tryInitialize();
      }
    }),
    treeView,
    treeDataProvider,
  );

  // If the panel is already visible at registration time (e.g. VS Code
  // restored the activity bar on startup), initialize immediately.
  if (treeView.visible) {
    service.tryInitialize();
  }
}

class LearningProgressTreeProvider implements vscode.TreeDataProvider<LearningProgressNode> {
  private readonly emitter = new vscode.EventEmitter<
    LearningProgressNode | undefined
  >();
  readonly onDidChangeTreeData = this.emitter.event;

  private snapshot: OverallProgress | undefined;

  constructor(private readonly service: LearningService) {}

  update(snapshot: OverallProgress | undefined): void {
    this.snapshot = snapshot;
    this.emitter.fire(undefined);
  }

  getTreeItem(node: LearningProgressNode): vscode.TreeItem {
    if (node.kind === "course") {
      const { descriptor, progress, isActive } = node;
      const totalUnits = progress.units.length;
      const completedUnits = progress.units.filter(
        (u) => u.total > 0 && u.completed === u.total,
      ).length;
      const item = new vscode.TreeItem(
        descriptor.title,
        isActive
          ? vscode.TreeItemCollapsibleState.Expanded
          : vscode.TreeItemCollapsibleState.Collapsed,
      );
      item.description =
        totalUnits > 0 ? `${completedUnits}/${totalUnits}` : undefined;
      item.iconPath =
        descriptor.kind === "python-notebook" ? iconPython : iconCourse;
      // The context value drives which package.json menu actions appear.
      // Python courses get a distinct value so Python-only actions (the
      // environment check) can be scoped to them.
      item.contextValue =
        descriptor.kind === "python-notebook" ? "coursePython" : "course";
      // TODO (acasey): is this valuable?  It just adds " - Python environment" to the tooltip
      const envNote =
        descriptor.kind === "python-notebook"
          ? " \u00b7 Python environment"
          : "";
      item.tooltip = `${descriptor.title}${envNote}${
        descriptor.shortDescription ? `\n${descriptor.shortDescription}` : ""
      }`;
      item.id = isActive
        ? `course:${descriptor.id}:active`
        : `course:${descriptor.id}`;
      return item;
    }

    if (node.kind === "continue") {
      // TODO (acasey): does this make sense for notebook courses?
      const item = new vscode.TreeItem(
        `Up next: ${node.activityTitle}`,
        vscode.TreeItemCollapsibleState.None,
      );
      item.description = node.unitTitle;
      item.iconPath = iconContinue;
      item.contextValue = node.kind;
      item.tooltip = `Continue learning — ${node.unitTitle}: ${node.activityTitle}`;
      item.id = "continue";
      item.command = {
        command: "qsharp-vscode.learningOpenActivity",
        title: "Go to Activity",
        arguments: [node],
      };
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
      item.description =
        unit.completed > 0 && unit.completed < unit.total
          ? `${unit.completed}/${unit.total}`
          : undefined;
      item.iconPath = unitIcon(unit);
      item.contextValue = node.kind;
      item.tooltip = `${unit.title} — ${unit.completed}/${unit.total} activities complete`;
      // Vary the id by `isCurrent` so VS Code sees a new node when the active
      // unit changes and applies the collapsibleState we set above.
      item.id = isCurrent ? `unit:${unit.id}:current` : `unit:${unit.id}`;
      item.command = {
        command: "qsharp-vscode.learningOpenActivity",
        title: "Go to Activity",
        arguments: [node],
      };
      return item;
    }

    const { unitId, activity, isCurrent } = node;
    const item = new vscode.TreeItem(
      activity.title,
      vscode.TreeItemCollapsibleState.None,
    );
    item.iconPath = activityIcon(activity, isCurrent);
    item.contextValue = activity.type;
    item.tooltip = activity.isComplete
      ? `Completed${activity.completedAt ? ` \u00b7 ${new Date(activity.completedAt).toLocaleString()}` : ""}`
      : activity.type === "exercise"
        ? "Exercise"
        : "Lesson";
    // Vary the id by `isCurrent` so VS Code drops the stale selection when the
    // active activity changes (e.g. after pressing Next in the lesson panel).
    item.id = isCurrent
      ? `activity:${unitId}:${activity.id}:current`
      : `activity:${unitId}:${activity.id}`;
    item.command = {
      command: "qsharp-vscode.learningOpenActivity",
      title: "Go to Activity",
      arguments: [node],
    };
    return item;
  }

  async getChildren(
    node?: LearningProgressNode,
  ): Promise<LearningProgressNode[]> {
    // Root: one node per available course.
    if (!node) {
      if (!this.service.initialized) {
        return [];
      }
      let descriptors: CourseDescriptor[];
      try {
        descriptors = await this.service.getCourses();
      } catch {
        return [];
      }
      const activeCourseId = this.service.getActiveCourseId();
      const nodes: LearningProgressNode[] = [];
      for (const descriptor of descriptors) {
        const isActive = descriptor.id === activeCourseId;
        let progress: OverallProgress | undefined;
        if (isActive && this.snapshot) {
          progress = this.snapshot;
        } else {
          try {
            progress = await this.service.getCourseProgress(descriptor.id);
          } catch {
            progress = undefined;
          }
        }
        if (!progress) {
          continue;
        }
        nodes.push({ kind: "course", descriptor, progress, isActive });
      }
      return nodes;
    }

    if (node.kind === "course") {
      const { descriptor, progress, isActive } = node;
      const children: LearningProgressNode[] = [];

      // The "Up next" shortcut targets the active course's saved position.
      if (isActive) {
        const { courseId, unitId, activityId } = progress.currentPosition;
        const unit = progress.units.find((u) => u.id === unitId);
        const activity = unit?.activities.find((a) => a.id === activityId);
        if (unit && activity) {
          children.push({
            kind: "continue",
            location: { courseId, unitId: unit.id, activityId: activity.id },
            unitTitle: unit.title,
            activityTitle: activity.title,
          });
        }
      }

      const currentUnitId = isActive
        ? progress.currentPosition.unitId
        : undefined;
      for (const u of progress.units) {
        children.push({
          kind: "unit",
          courseId: descriptor.id,
          courseKind: descriptor.kind,
          unit: u,
          isCurrent: u.id === currentUnitId,
        });
      }
      return children;
    }

    if (node.kind === "unit") {
      const currentUnitId = this.snapshot?.currentPosition.unitId;
      const currentActivityId = this.snapshot?.currentPosition.activityId;
      const isActiveCourse =
        this.service.initialized &&
        node.courseId === this.service.getActiveCourseId();
      // Hide the synthetic "intro" lesson for python-notebook courses —
      // the panel already shows unit-level content.
      // TODO (acasey): can we just not synthesize it?
      const activities =
        node.courseKind === "python-notebook"
          ? node.unit.activities.filter((a) => a.id !== "intro")
          : node.unit.activities;
      return activities.map<LearningProgressNode>((activity) => ({
        kind: "activity",
        courseId: node.courseId,
        unitId: node.unit.id,
        unitTitle: node.unit.title,
        activity,
        isCurrent:
          isActiveCourse &&
          node.unit.id === currentUnitId &&
          activity.id === currentActivityId,
      }));
    }

    return [];
  }

  dispose(): void {
    this.emitter.dispose();
  }
}

/** Builds the italic summary shown at the top of the tree view. */
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
  } else if (ratio === 0 && snapshot.stats.completedActivities === 0) {
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

/** Discriminated union for the four kinds of tree nodes. */
export type LearningProgressNode =
  | {
      /** Top-level course node (expandable). */
      kind: "course";
      descriptor: CourseDescriptor;
      progress: OverallProgress;
      isActive: boolean;
    }
  | {
      /** Pinned "Up next" shortcut at the top of the tree. */
      kind: "continue";
      location: ActivityLocation;
      unitTitle: string;
      activityTitle: string;
    }
  | {
      /** Unit node (expandable). */
      kind: "unit";
      courseId: string;
      courseKind: CourseKind;
      unit: UnitProgress;
      isCurrent: boolean;
    }
  | {
      /** Leaf node representing a lesson or exercise within a unit. */
      kind: "activity";
      courseId: string;
      unitId: string;
      unitTitle: string;
      activity: ActivityProgress;
      isCurrent: boolean;
    };

// ─── Tree node icons ───

const iconCourse = new vscode.ThemeIcon("mortar-board");
const iconPython = new vscode.ThemeIcon(
  "notebook",
  new vscode.ThemeColor("charts.blue"),
);
const iconContinue = new vscode.ThemeIcon(
  "sparkle",
  new vscode.ThemeColor("charts.blue"),
);
const iconPassed = new vscode.ThemeIcon(
  "pass",
  new vscode.ThemeColor("testing.iconPassed"),
);
const iconCurrent = new vscode.ThemeIcon(
  "circle-filled",
  new vscode.ThemeColor("charts.blue"),
);
const iconIncomplete = new vscode.ThemeIcon("circle-large-outline");
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
