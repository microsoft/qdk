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
      ? `Completed${section.completedAt ? ` · ${new Date(section.completedAt).toLocaleString()}` : ""}`
      : section.kind === "exercise"
        ? "Exercise — use the chat action to open in chat"
        : "Lesson — use the chat action to open in chat";
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
      return snap.katas.map((kata) => ({
        kind: "kata" as const,
        kata,
        isCurrent: kata.id === snap.currentPosition.kataId,
      }));
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
