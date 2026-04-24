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
      sectionIndex: number;
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

  /** Tracks the section currently being navigated to (shows a spinner). */
  private navigatingKataId: string | undefined;
  private navigatingSectionIndex: number | undefined;

  update(snapshot: OverallProgress | undefined): void {
    this.snapshot = snapshot;
    this.emitter.fire(undefined);
  }

  /** Show a spinner on the given section while awaiting widget navigation. */
  setNavigating(kataId: string, sectionIndex: number): void {
    this.navigatingKataId = kataId;
    this.navigatingSectionIndex = sectionIndex;
    this.emitter.fire(undefined);
  }

  /** Clear the navigation spinner. */
  clearNavigating(): void {
    if (this.navigatingKataId !== undefined) {
      this.navigatingKataId = undefined;
      this.navigatingSectionIndex = undefined;
      this.emitter.fire(undefined);
    }
  }

  private isNavigating(kataId: string, sectionIndex: number): boolean {
    return (
      this.navigatingKataId === kataId &&
      this.navigatingSectionIndex === sectionIndex
    );
  }

  getTreeItem(node: KatasNode): vscode.TreeItem {
    if (node.kind === "continue") {
      const item = new vscode.TreeItem(
        `Up next: ${node.sectionTitle}`,
        vscode.TreeItemCollapsibleState.None,
      );
      item.description = node.kataTitle;
      item.iconPath = this.isNavigating(node.kataId, node.sectionIndex)
        ? new vscode.ThemeIcon("loading~spin")
        : new vscode.ThemeIcon("sparkle", new vscode.ThemeColor("charts.blue"));
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
    item.iconPath = this.isNavigating(kataId, section.index)
      ? new vscode.ThemeIcon("loading~spin")
      : sectionIcon(section, isCurrent);
    item.contextValue =
      section.kind === "exercise" ? "exerciseSection" : "lessonSection";
    const baseTooltip = section.isComplete
      ? `Completed${section.completedAt ? ` · ${new Date(section.completedAt).toLocaleString()}` : ""}`
      : section.kind === "exercise"
        ? "Exercise — click the action icon to open"
        : "Lesson — click the action icon to open";
    item.tooltip = section.hasExample
      ? `${baseTooltip} · contains a code example`
      : baseTooltip;
    item.id = `section:${kataId}:${section.index}`;
    return item;
  }

  getChildren(node?: KatasNode): KatasNode[] {
    const snap = this.snapshot;
    if (!snap) return [];

    if (!node) {
      const children: KatasNode[] = [];

      // Pinned "Up next" shortcut at the top.
      const pos = snap.currentPosition;
      if (pos?.kataId) {
        const kata = snap.katas.find((k) => k.id === pos.kataId);
        if (kata) {
          const section = kata.sections[pos.sectionIndex];
          if (section) {
            children.push({
              kind: "continue",
              kataId: kata.id,
              kataTitle: kata.title,
              sectionIndex: pos.sectionIndex,
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
          isCurrent: kata.id === snap.currentPosition.kataId,
        });
      }

      return children;
    }

    if (node.kind === "kata") {
      const currentKataId = snap.currentPosition.kataId;
      const currentSectionIndex = snap.currentPosition.sectionIndex;
      return node.kata.sections.map<KatasNode>((section) => ({
        kind: "section",
        kataId: node.kata.id,
        kataTitle: node.kata.title,
        section,
        isCurrent:
          node.kata.id === currentKataId &&
          section.index === currentSectionIndex,
      }));
    }

    return [];
  }

  dispose(): void {
    this.emitter.dispose();
  }
}
