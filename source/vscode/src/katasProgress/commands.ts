// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { log } from "qsharp-lang";
import { EventType, sendTelemetryEvent } from "../telemetry.js";
import type { ProgressWatcher } from "./progressReader.js";
import type { SectionKind, OverallProgress } from "./types.js";

export interface OpenSectionArgs {
  kataId: string;
  sectionIndex: number;
  kind: SectionKind;
}

function findSection(
  snapshot: OverallProgress | undefined,
  kataId: string,
  sectionIndex: number,
) {
  if (!snapshot) return undefined;
  const kata = snapshot.katas.find((k) => k.id === kataId);
  if (!kata) return undefined;
  const section = kata.sections[sectionIndex];
  if (!section) return undefined;
  return { kata, section };
}

/**
 * Open a kata section. For exercises, opens the scaffolded `.qs` file
 * directly. For lessons (and for exercises whose file is missing), route
 * to chat with a prompt that triggers the `quantum-katas` skill.
 */
async function openSection(
  watcher: ProgressWatcher,
  args: OpenSectionArgs,
): Promise<void> {
  const info = watcher.workspaceInfo;
  const found = findSection(
    watcher.lastSnapshot,
    args.kataId,
    args.sectionIndex,
  );

  if (args.kind === "exercise" && info && found) {
    // Exercise filename convention mirrors WorkspaceManager.getExerciseFilePath.
    const fileUri = vscode.Uri.joinPath(
      info.katasRoot,
      "exercises",
      args.kataId,
      `${found.section.id}.qs`,
    );
    try {
      const doc = await vscode.workspace.openTextDocument(fileUri);
      await vscode.window.showTextDocument(doc);
      sendTelemetryEvent(
        EventType.KatasPanelAction,
        { action: "openExercise" },
        {},
      );
      return;
    } catch (err) {
      log.warn(
        `[katasProgress] exercise file not found (${fileUri.fsPath}): ${err}. Falling back to chat.`,
      );
      // Fall through to chat routing.
    }
  }

  await askInChat(watcher, args);
  sendTelemetryEvent(EventType.KatasPanelAction, { action: "openLesson" }, {});
}

function buildChatPrompt(
  watcher: ProgressWatcher,
  args: OpenSectionArgs,
): string {
  const found = findSection(
    watcher.lastSnapshot,
    args.kataId,
    args.sectionIndex,
  );
  const kataTitle = found?.kata.title ?? args.kataId;
  const sectionTitle = found?.section.title;

  return sectionTitle
    ? `Open the Quantum Katas at the "${sectionTitle}" ${args.kind} in the "${kataTitle}" kata.`
    : `Open the Quantum Katas at the "${kataTitle}" kata.`;
}

async function askInChat(
  watcher: ProgressWatcher,
  args: OpenSectionArgs,
): Promise<void> {
  const prompt = buildChatPrompt(watcher, args);
  try {
    await vscode.commands.executeCommand("workbench.action.chat.open", {
      query: prompt,
      isPartialQuery: false,
      mode: "agent",
    });
  } catch {
    // Older VS Code builds may not accept the options object — fall back
    // to passing a plain string.
    await vscode.commands.executeCommand("workbench.action.chat.open", prompt);
  }
  sendTelemetryEvent(EventType.KatasPanelAction, { action: "askInChat" }, {});
}

export function registerKatasCommands(
  context: vscode.ExtensionContext,
  watcher: ProgressWatcher,
): void {
  context.subscriptions.push(
    vscode.commands.registerCommand("qsharp-vscode.katasRefresh", async () => {
      sendTelemetryEvent(EventType.KatasPanelAction, { action: "refresh" }, {});
      await watcher.refresh();
    }),

    vscode.commands.registerCommand("qsharp-vscode.katasContinue", async () => {
      sendTelemetryEvent(
        EventType.KatasPanelAction,
        { action: "continue" },
        {},
      );
      const snap = watcher.lastSnapshot;
      const pos = snap?.currentPosition;
      if (snap && pos && pos.kataId) {
        const found = findSection(snap, pos.kataId, pos.sectionIndex);
        if (found) {
          await openSection(watcher, {
            kataId: pos.kataId,
            sectionIndex: pos.sectionIndex,
            kind: found.section.kind,
          });
          return;
        }
      }
      // No position recorded yet — open chat with a generic start prompt.
      await vscode.commands.executeCommand("workbench.action.chat.open", {
        query: "Let's start the Quantum Katas.",
        isPartialQuery: false,
        mode: "agent",
      });
    }),

    vscode.commands.registerCommand(
      "qsharp-vscode.katasOpenSection",
      async (input: unknown) => {
        const args = normalizeSectionArgs(input);
        if (!args) return;
        await openSection(watcher, args);
      },
    ),

    vscode.commands.registerCommand(
      "qsharp-vscode.katasAskInChat",
      async (input: unknown) => {
        const args = normalizeSectionArgs(input);
        if (!args) return;
        await askInChat(watcher, args);
      },
    ),

    vscode.commands.registerCommand("qsharp-vscode.katasSetup", async () => {
      sendTelemetryEvent(EventType.KatasPanelAction, { action: "setup" }, {});
      const OPEN_SETTING = "Set path in settings";
      const ASK_CHAT = "Ask the Quantum Katas skill to set it up";
      const pick = await vscode.window.showQuickPick([OPEN_SETTING, ASK_CHAT], {
        title: "Set up Quantum Katas workspace",
        placeHolder:
          "Pick how you'd like to configure the Quantum Katas workspace",
      });
      if (pick === OPEN_SETTING) {
        await vscode.commands.executeCommand(
          "workbench.action.openSettings",
          "Q#.learning.workspaceRoot",
        );
      } else if (pick === ASK_CHAT) {
        await vscode.commands.executeCommand("workbench.action.chat.open", {
          query:
            "Set up a Quantum Katas workspace for me and open the first kata.",
          isPartialQuery: false,
          mode: "agent",
        });
      }
    }),
  );
}

function normalizeSectionArgs(input: unknown): OpenSectionArgs | undefined {
  if (!input || typeof input !== "object") return undefined;
  const obj = input as Record<string, unknown>;

  // Tree node shape — see treeProvider.ts `KatasNode`.
  if (obj.kind === "section") {
    const kataId = obj.kataId as string | undefined;
    const section = obj.section as
      | { index?: number; kind?: SectionKind }
      | undefined;
    if (
      kataId &&
      section &&
      typeof section.index === "number" &&
      section.kind
    ) {
      return { kataId, sectionIndex: section.index, kind: section.kind };
    }
  }
  if (obj.kind === "kata") {
    const kata = obj.kata as
      | { id?: string; sections?: Array<{ kind?: SectionKind }> }
      | undefined;
    if (kata?.id && kata.sections && kata.sections.length > 0) {
      const firstKind = kata.sections[0].kind ?? "lesson";
      return { kataId: kata.id, sectionIndex: 0, kind: firstKind };
    }
  }

  // Already in `OpenSectionArgs` shape.
  const kataId = obj.kataId as string | undefined;
  const sectionIndex = obj.sectionIndex as number | undefined;
  const kind = obj.kind as SectionKind | undefined;
  if (
    kataId &&
    typeof sectionIndex === "number" &&
    (kind === "lesson" || kind === "exercise")
  ) {
    return { kataId, sectionIndex, kind };
  }
  return undefined;
}
