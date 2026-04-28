// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { EventType, sendTelemetryEvent } from "../telemetry.js";
import type { LearningService } from "./service.js";
import type { ProgressWatcher } from "./progress/progressReader.js";
import type { SectionKind, OverallProgress } from "./progress/types.js";

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

/**
 * Register all learning commands: editor-facing (hint, reset, next, open panel)
 * and activity-bar tree (refresh, continue, open section, ask in chat).
 *
 * The "check solution" command lives in `panel/index.ts` because it
 * needs the panel manager to render results in the webview.
 */
export function registerLearningCommands(
  context: vscode.ExtensionContext,
  service: LearningService,
  watcher: ProgressWatcher,
): void {
  context.subscriptions.push(
    // ─── Editor-facing commands ───

    vscode.commands.registerCommand("qsharp-vscode.learningShowHint", () => {
      if (!service.initialized) return;
      // Redirect to chat agent for hint delivery.
      void vscode.commands.executeCommand("workbench.action.chat.open", {
        query: "/qdk-learning Give me a hint",
      });
    }),

    vscode.commands.registerCommand(
      "qsharp-vscode.learningResetExercise",
      async () => {
        if (!service.initialized) return;

        const pos = service.getPosition();
        if (pos.item.type !== "exercise") {
          vscode.window.showInformationMessage(
            "Navigate to an exercise to reset it.",
          );
          return;
        }

        const confirmed = await vscode.window.showWarningMessage(
          "Reset this exercise to the original placeholder code? Your current code will be lost.",
          { modal: true },
          "Reset",
        );
        if (confirmed !== "Reset") return;

        await service.resetExercise();
        vscode.window.showInformationMessage("Exercise has been reset.");
      },
    ),

    vscode.commands.registerCommand("qsharp-vscode.learningNext", async () => {
      if (!service.initialized) return;
      const { moved } = service.next();
      if (!moved) {
        vscode.window.showInformationMessage(
          "You've reached the end of the available content!",
        );
      }
    }),

    vscode.commands.registerCommand("qsharp-vscode.learningOpenPanel", () => {
      void vscode.commands.executeCommand("qsharp-vscode.showKatas");
    }),

    // ─── Activity-bar tree commands ───

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
          await openSection(service, {
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
        await openSection(service, args);
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
