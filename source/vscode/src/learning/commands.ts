// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { EventType, sendTelemetryEvent } from "../telemetry.js";
import { KatasPanelManager } from "./panel.js";
import type { LearningService } from "./service.js";
import type { ActivityLocation, OverallProgress } from "./types.js";
import { KATAS_COURSE_ID } from "./constants.js";

/**
 * These are typically commands that will be wired up to the progress
 * tree view or code lenses.
 */
export function registerLearningCommands(
  context: vscode.ExtensionContext,
  service: LearningService,
): void {
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "qsharp-vscode.learningShowActivity",
      () => {
        const manager = KatasPanelManager.getInstance(
          context.extensionUri,
          service,
        );
        return manager.show();
      },
    ),

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
          await openSection(context, service, {
            courseId: pos.courseId,
            unitId: pos.unitId,
            activityId: pos.activityId,
          });
          return;
        }
      }
      // No position recorded yet — open chat with a generic start prompt.
      await vscode.commands.executeCommand("workbench.action.chat.open", {
        query: "/qdk-learning Let's get started.",
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
        await openSection(context, service, args);
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
 * Navigate to a unit activity and show it.
 */
async function openSection(
  context: vscode.ExtensionContext,
  learningService: LearningService,
  args: ActivityLocation,
): Promise<void> {
  await learningService.ensureInitialized();
  learningService.goTo(args.courseId, args.unitId, args.activityId);
  await vscode.commands.executeCommand("qsharp-vscode.learningShowActivity");
  sendTelemetryEvent(
    EventType.KatasPanelAction,
    { action: "navigateWidget" },
    {},
  );
}

async function askInChat(
  service: LearningService,
  args: ActivityLocation,
): Promise<void> {
  const found = findActivity(
    service.lastSnapshot,
    args.unitId,
    args.activityId,
  );
  const unitTitle = found?.unit.title ?? args.unitId;
  const activityTitle = found?.activity.title;

  const location = activityTitle ?? unitTitle;
  // Include #qdkLearningGoto with precise IDs so the agent can call the
  // tool without fuzzy matching.

  const courseArg =
    args.courseId !== KATAS_COURSE_ID ? ` courseId=${args.courseId}` : "";
  const prompt = `/qdk-learning #qdkLearningGoto${courseArg} ${args.unitId} ${args.activityId} — Go to ${location}`;
  await vscode.commands.executeCommand("workbench.action.chat.open", {
    query: prompt,
    isPartialQuery: false,
  });
  sendTelemetryEvent(EventType.KatasPanelAction, { action: "askInChat" }, {});
}

function normalizeSectionArgs(input: unknown): ActivityLocation | undefined {
  if (!input || typeof input !== "object") {
    return undefined;
  }
  const obj = input as Record<string, unknown>;

  // Tree node shape — see progressTreeView.ts `LearningProgressNode`.
  if (obj.kind === "continue") {
    const courseId = (obj.courseId as string | undefined) ?? KATAS_COURSE_ID;
    const unitId = obj.unitId as string | undefined;
    const activityId = obj.activityId as string | undefined;
    if (unitId && activityId) {
      return { courseId, unitId, activityId };
    }
  }
  if (obj.kind === "activity") {
    const courseId = (obj.courseId as string | undefined) ?? KATAS_COURSE_ID;
    const unitId = obj.unitId as string | undefined;
    const activity = obj.activity as { id?: string } | undefined;
    if (unitId && activity?.id) {
      return { courseId, unitId, activityId: activity.id };
    }
  }
  if (obj.kind === "unit") {
    const courseId = (obj.courseId as string | undefined) ?? KATAS_COURSE_ID;
    const unit = obj.unit as
      | { id?: string; activities?: Array<{ id?: string }> }
      | undefined;
    if (unit?.id && unit.activities && unit.activities.length > 0) {
      const first = unit.activities[0];
      return {
        courseId,
        unitId: unit.id,
        activityId: first.id ?? "",
      };
    }
  }

  return undefined;
}
