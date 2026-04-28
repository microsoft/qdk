// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import type {
  KataProgress,
  OverallProgress,
  SectionProgress,
} from "./types.js";

/**
 * Node identity in the tree. Roots carry the full `KataProgress`; children
 * carry the parent's `kataId` plus the `SectionProgress` for quick lookups.
 */
export type KatasNode =
  | {
      kind: "continue";
      kataId: string;
      kataTitle: string;
      sectionId: string;
      sectionTitle: string;
      sectionKind: string;
    }
  | {
      kind: "kata";
      kata: KataProgress;
      isCurrent: boolean;
    }
  | {
      kind: "section";
      kataId: string;
      kataTitle: string;
      section: SectionProgress;
      isCurrent: boolean;
    };

function sectionIcon(s: SectionProgress, isCurrent: boolean): vscode.ThemeIcon {
  if (s.isComplete) {
    return new vscode.ThemeIcon(
      "pass",
      new vscode.ThemeColor("testing.iconPassed"),
    );
  }
  if (isCurrent) {
    return new vscode.ThemeIcon(
      "circle-filled",
      new vscode.ThemeColor("charts.blue"),
    );
  }
  return new vscode.ThemeIcon("circle-large-outline");
}

function kataIcon(k: KataProgress): vscode.ThemeIcon {
  if (k.total > 0 && k.completed === k.total) {
    return new vscode.ThemeIcon(
      "pass",
      new vscode.ThemeColor("testing.iconPassed"),
    );
  }
  if (k.completed > 0) {
    return new vscode.ThemeIcon("record", new vscode.ThemeColor("charts.blue"));
  }
  return new vscode.ThemeIcon("circle-large-outline");
}

export class KatasTreeProvider implements vscode.TreeDataProvider<KatasNode> {
  private readonly emitter = new vscode.EventEmitter<KatasNode | undefined>();
  readonly onDidChangeTreeData = this.emitter.event;

  private snapshot: OverallProgress | undefined;

  update(snapshot: OverallProgress | undefined): void {
    this.snapshot = snapshot;
    this.emitter.fire(undefined);
  }

  getTreeItem(node: KatasNode): vscode.TreeItem {
    if (node.kind === "continue") {
      const item = new vscode.TreeItem(
        `Up next: ${node.sectionTitle}`,
        vscode.TreeItemCollapsibleState.None,
      );
      item.description = node.kataTitle;
      item.iconPath = new vscode.ThemeIcon(
        "sparkle",
        new vscode.ThemeColor("charts.blue"),
      );
      item.contextValue = "continue";
      item.tooltip = `Continue learning — ${node.kataTitle}: ${node.sectionTitle}`;
      item.id = "continue";
      return item;
    }

    if (node.kind === "kata") {
      const { kata, isCurrent } = node;
      const item = new vscode.TreeItem(
        kata.title,
        isCurrent
          ? vscode.TreeItemCollapsibleState.Expanded
          : vscode.TreeItemCollapsibleState.Collapsed,
      );
      item.description = `${kata.completed}/${kata.total}`;
      item.iconPath = kataIcon(kata);
      item.contextValue = "kata";
      item.tooltip = `${kata.title} — ${kata.completed}/${kata.total} sections complete`;
      item.id = `kata:${kata.id}`;
      return item;
    }

    const { kataId, section, isCurrent } = node;
    const item = new vscode.TreeItem(
      section.title,
      vscode.TreeItemCollapsibleState.None,
    );
    item.description =
      section.kind === "exercise"
        ? "exercise"
        : section.hasExample
          ? "lesson · example"
          : "lesson";
    item.iconPath = sectionIcon(section, isCurrent);
    item.contextValue =
      section.kind === "exercise" ? "exerciseSection" : "lessonSection";
    const baseTooltip = section.isComplete
      ? `Completed${section.completedAt ? ` \u00b7 ${new Date(section.completedAt).toLocaleString()}` : ""}`
      : section.kind === "exercise"
        ? "Exercise \u2014 click the action icon to open"
        : "Lesson \u2014 click the action icon to open";
    item.tooltip = section.hasExample
      ? `${baseTooltip} \u00b7 contains a code example`
      : baseTooltip;
    item.id = `section:${kataId}:${section.id}`;
    return item;
  }

  getChildren(node?: KatasNode): KatasNode[] {
    const snap = this.snapshot;
    if (!snap) return [];

    if (!node) {
      const children: KatasNode[] = [];

      // Pinned "Up next" shortcut at the top.
      // When no position is recorded yet (fresh workspace), default to
      // the first kata / first section so the user always sees an entry.
      const pos = snap.currentPosition;
      const resolvedKataId = pos?.kataId || snap.katas[0]?.id;
      const resolvedSectionId = pos?.kataId
        ? pos.sectionId
        : snap.katas[0]?.sections[0]?.id;
      if (resolvedKataId) {
        const kata = snap.katas.find((k) => k.id === resolvedKataId);
        if (kata) {
          const section = resolvedSectionId
            ? kata.sections.find((s) => s.id === resolvedSectionId)
            : kata.sections[0];
          if (section) {
            children.push({
              kind: "continue",
              kataId: kata.id,
              kataTitle: kata.title,
              sectionId: section.id,
              sectionTitle: section.title,
              sectionKind: section.kind,
            });
          }
        }
      }

      for (const kata of snap.katas) {
        children.push({
          kind: "kata",
          kata,
          isCurrent: kata.id === resolvedKataId,
        });
      }

      return children;
    }

    if (node.kind === "kata") {
      const currentKataId = snap.currentPosition.kataId || snap.katas[0]?.id;
      const currentSectionId = snap.currentPosition.kataId
        ? snap.currentPosition.sectionId
        : snap.katas[0]?.sections[0]?.id;
      return node.kata.sections.map<KatasNode>((section) => ({
        kind: "section",
        kataId: node.kata.id,
        kataTitle: node.kata.title,
        section,
        isCurrent:
          node.kata.id === currentKataId && section.id === currentSectionId,
      }));
    }

    return [];
  }

  dispose(): void {
    this.emitter.dispose();
  }
}
