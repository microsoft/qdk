// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { EventType, sendTelemetryEvent } from "../telemetry.js";
import { KatasPanelManager } from "./panel.js";
import type { LearningService } from "./service.js";
import type { ActivityKind, OverallProgress } from "./types.js";

/**
 * These are typically commands that will be wired up to the progress
 * tree view or code lenses.
 */
export function registerLearningCommands(
  context: vscode.ExtensionContext,
  service: LearningService,
): void {
  context.subscriptions.push(
    vscode.commands.registerCommand("qsharp-vscode.learningOpenPanel", () => {
      const manager = KatasPanelManager.getInstance(
        context.extensionUri,
        service,
      );
      return manager.show();
    }),

    vscode.commands.registerCommand(
      "qsharp-vscode.learningCheckSolution",
      async () => {
        if (!service.initialized) {
          vscode.window.showWarningMessage(
            "The QDK Learning workspace has not been initialized yet.",
          );
          return;
        }
        const pos = service.getPosition();
        if (pos.content.type !== "exercise") {
          vscode.window.showInformationMessage(
            "Navigate to an exercise to check your solution.",
          );
          return;
        }
        const manager = KatasPanelManager.getInstance(
          context.extensionUri,
          service,
        );
        const passed = await manager.checkAndShowResult();

        // Trigger pass/fail flash decoration.
        void vscode.commands.executeCommand(
          "qsharp-vscode._learningFlash",
          passed,
        );
      },
    ),

    vscode.commands.registerCommand(
      "qsharp-vscode.learningResetExercise",
      async () => {
        if (!service.initialized) {
          return;
        }

        const pos = service.getPosition();
        if (pos.content.type !== "exercise") {
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
        if (confirmed !== "Reset") {
          return;
        }

        await service.resetExercise();
        vscode.window.showInformationMessage("Exercise has been reset.");
      },
    ),

    // ─── Activity-bar tree commands ───

    vscode.commands.registerCommand("qsharp-vscode.katasRefresh", async () => {
      sendTelemetryEvent(EventType.KatasPanelAction, { action: "refresh" }, {});
      await service.refresh();
    }),

    vscode.commands.registerCommand("qsharp-vscode.katasContinue", async () => {
      sendTelemetryEvent(
        EventType.KatasPanelAction,
        { action: "continue" },
        {},
      );
      const snap = service.lastSnapshot;
      const pos = snap?.currentPosition;
      if (snap && pos && pos.unitId) {
        const found = findActivity(snap, pos.unitId, pos.activityId);
        if (found) {
          await openSection(service, {
            unitId: pos.unitId,
            activityId: pos.activityId,
            kind: found.activity.type,
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
        if (!args) {
          return;
        }
        await openSection(service, args);
      },
    ),

    vscode.commands.registerCommand(
      "qsharp-vscode.katasAskInChat",
      async (input: unknown) => {
        const args = normalizeSectionArgs(input);
        if (!args) {
          return;
        }
        await askInChat(service, args);
      },
    ),
  );
}

interface OpenSectionArgs {
  unitId: string;
  activityId: string;
  kind: ActivityKind;
}

function findActivity(
  snapshot: OverallProgress | undefined,
  unitId: string,
  activityId: string,
) {
  if (!snapshot) {
    return undefined;
  }
  const unit = snapshot.units.find((u) => u.id === unitId);
  if (!unit) {
    return undefined;
  }
  const activity = unit.activities.find((a) => a.id === activityId);
  if (!activity) {
    return undefined;
  }
  return { unit, activity };
}

/**
 * Open a unit activity. Shows the panel (initializing the service if needed),
 * then navigates directly via `LearningService.goTo()`.
 */
async function openSection(
  learningService: LearningService,
  args: OpenSectionArgs,
): Promise<void> {
  await vscode.commands.executeCommand("qsharp-vscode.learningOpenPanel");
  learningService.goTo(args.unitId, args.activityId);
  sendTelemetryEvent(
    EventType.KatasPanelAction,
    { action: "navigateWidget" },
    {},
  );
}

async function askInChat(
  service: LearningService,
  args: OpenSectionArgs,
): Promise<void> {
  const found = findActivity(
    service.lastSnapshot,
    args.unitId,
    args.activityId,
  );
  const unitTitle = found?.unit.title ?? args.unitId;
  const activityTitle = found?.activity.title;

  // Include #goto with precise IDs so the agent can call the
  // tool without fuzzy matching.
  const location = activityTitle
    ? `the "${activityTitle}" ${args.kind} in "${unitTitle}"`
    : `"${unitTitle}"`;

  const prompt = `/qdk-learning #goto ${args.unitId} ${args.activityId} — Go to ${location}`;
  await vscode.commands.executeCommand("workbench.action.chat.open", {
    query: prompt,
    isPartialQuery: false,
  });
  sendTelemetryEvent(EventType.KatasPanelAction, { action: "askInChat" }, {});
}

function normalizeSectionArgs(input: unknown): OpenSectionArgs | undefined {
  if (!input || typeof input !== "object") {
    return undefined;
  }
  const obj = input as Record<string, unknown>;

  // Tree node shape — see progressTreeView.ts `LearningProgressNode`.
  if (obj.kind === "continue") {
    const unitId = obj.unitId as string | undefined;
    const activityId = obj.activityId as string | undefined;
    const activityKind = obj.activityKind as ActivityKind | undefined;
    if (unitId && activityId && activityKind) {
      return { unitId, activityId, kind: activityKind };
    }
  }
  if (obj.kind === "activity") {
    const unitId = obj.unitId as string | undefined;
    const activity = obj.activity as
      | { id?: string; type?: ActivityKind }
      | undefined;
    if (unitId && activity && activity.id && activity.type) {
      return { unitId, activityId: activity.id, kind: activity.type };
    }
  }
  if (obj.kind === "unit") {
    const unit = obj.unit as
      | {
          id?: string;
          activities?: Array<{ id?: string; type?: ActivityKind }>;
        }
      | undefined;
    if (unit?.id && unit.activities && unit.activities.length > 0) {
      const first = unit.activities[0];
      return {
        unitId: unit.id,
        activityId: first.id ?? "",
        kind: first.type ?? "lesson",
      };
    }
  }

  return undefined;
}
