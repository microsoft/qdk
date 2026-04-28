// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { EventType, sendTelemetryEvent } from "../telemetry.js";
import type { LearningService } from "../learningService/index.js";
import type { ProgressWatcher } from "./progressReader.js";
import type { SectionKind, OverallProgress } from "./types.js";

export interface OpenSectionArgs {
  kataId: string;
  sectionId: string;
  kind: SectionKind;
}

function findSection(
  snapshot: OverallProgress | undefined,
  kataId: string,
  sectionId: string,
) {
  if (!snapshot) return undefined;
  const kata = snapshot.katas.find((k) => k.id === kataId);
  if (!kata) return undefined;
  const section = kata.sections.find((s) => s.id === sectionId);
  if (!section) return undefined;
  return { kata, section };
}

/**
 * Open a kata section. Shows the panel (initializing the service if needed),
 * then navigates directly via `LearningService.goTo()`.
 */
async function openSection(
  learningService: LearningService,
  args: OpenSectionArgs,
): Promise<void> {
  // Show (or create) the katas panel — this also initializes the service.
  await vscode.commands.executeCommand("qsharp-vscode.showKatas");
  learningService.goTo(args.kataId, args.sectionId, 0);
  sendTelemetryEvent(
    EventType.KatasPanelAction,
    { action: "navigateWidget" },
    {},
  );
}

function buildChatPrompt(
  watcher: ProgressWatcher,
  args: OpenSectionArgs,
): string {
  const found = findSection(watcher.lastSnapshot, args.kataId, args.sectionId);
  const kataTitle = found?.kata.title ?? args.kataId;
  const sectionTitle = found?.section.title;

  // Include #goto with precise IDs so the agent can call the
  // tool without fuzzy matching.
  const location = sectionTitle
    ? `the "${sectionTitle}" ${args.kind} in "${kataTitle}"`
    : `"${kataTitle}"`;

  return `/qdk-learning #goto ${args.kataId} ${args.sectionId} — Go to ${location}`;
}

async function askInChat(
  watcher: ProgressWatcher,
  args: OpenSectionArgs,
): Promise<void> {
  const prompt = buildChatPrompt(watcher, args);
  await vscode.commands.executeCommand("workbench.action.chat.open", {
    query: prompt,
    isPartialQuery: false,
  });
  sendTelemetryEvent(EventType.KatasPanelAction, { action: "askInChat" }, {});
}

export function registerKatasCommands(
  context: vscode.ExtensionContext,
  watcher: ProgressWatcher,
  learningService: LearningService,
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
        const found = findSection(snap, pos.kataId, pos.sectionId);
        if (found) {
          await openSection(learningService, {
            kataId: pos.kataId,
            sectionId: pos.sectionId,
            kind: found.section.kind,
          });
          return;
        }
      }
      // No position recorded yet — open chat with a generic start prompt.
      await vscode.commands.executeCommand("workbench.action.chat.open", {
        query: "/qdk-learning Let's start the Quantum Katas.",
        isPartialQuery: false,
      });
    }),

    vscode.commands.registerCommand(
      "qsharp-vscode.katasOpenSection",
      async (input: unknown) => {
        const args = normalizeSectionArgs(input);
        if (!args) return;
        await openSection(learningService, args);
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
  );
}

function normalizeSectionArgs(input: unknown): OpenSectionArgs | undefined {
  if (!input || typeof input !== "object") return undefined;
  const obj = input as Record<string, unknown>;

  // Tree node shape — see treeProvider.ts `KatasNode`.
  if (obj.kind === "continue") {
    const kataId = obj.kataId as string | undefined;
    const sectionId = obj.sectionId as string | undefined;
    const sectionKind = obj.sectionKind as SectionKind | undefined;
    if (kataId && sectionId && sectionKind) {
      return { kataId, sectionId, kind: sectionKind };
    }
  }
  if (obj.kind === "section") {
    const kataId = obj.kataId as string | undefined;
    const section = obj.section as
      | { id?: string; kind?: SectionKind }
      | undefined;
    if (kataId && section && section.id && section.kind) {
      return { kataId, sectionId: section.id, kind: section.kind };
    }
  }
  if (obj.kind === "kata") {
    const kata = obj.kata as
      | {
          id?: string;
          sections?: Array<{ id?: string; kind?: SectionKind }>;
        }
      | undefined;
    if (kata?.id && kata.sections && kata.sections.length > 0) {
      const first = kata.sections[0];
      return {
        kataId: kata.id,
        sectionId: first.id ?? "",
        kind: first.kind ?? "lesson",
      };
    }
  }

  // Already in `OpenSectionArgs` shape.
  const kataId = obj.kataId as string | undefined;
  const sectionId = obj.sectionId as string | undefined;
  const kind = obj.kind as SectionKind | undefined;
  if (kataId && sectionId && (kind === "lesson" || kind === "exercise")) {
    return { kataId, sectionId, kind };
  }
  return undefined;
}
