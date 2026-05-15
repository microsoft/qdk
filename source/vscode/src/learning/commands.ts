// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { LessonPanelManager } from "./panel.js";
import type { LearningService } from "./service.js";
import type { ActivityLocation } from "./types.js";
import type { LearningProgressNode } from "./progressTreeView.js";

/**
 * These are typically commands that will be wired up to the progress
 * tree view or code lenses.
 */
export function registerLearningCommands(
  context: vscode.ExtensionContext,
  service: LearningService,
  panelManager: LessonPanelManager,
): void {
  context.subscriptions.push(
    vscode.commands.registerCommand("qsharp-vscode.learningShowActivity", () =>
      panelManager.show(),
    ),

    // Code lens commands

    vscode.commands.registerCommand(
      "qsharp-vscode.learningCheckSolution",
      async () => {
        await panelManager.checkAndShowResult();
      },
    ),

    vscode.commands.registerCommand(
      "qsharp-vscode.learningResetExercise",
      async () => {
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

    // Progress tree commands

    vscode.commands.registerCommand(
      "qsharp-vscode.learningRefresh",
      async () => {
        await service.refresh();
      },
    ),

    vscode.commands.registerCommand(
      "qsharp-vscode.learningContinue",
      async () => {
        // No position recorded yet — open chat with a generic start prompt.
        await vscode.commands.executeCommand("workbench.action.chat.open", {
          query: "/qdk-learning Let's start the Quantum Katas.",
          isPartialQuery: false,
        });
      },
    ),

    vscode.commands.registerCommand(
      "qsharp-vscode.learningOpenActivity",
      async (node: LearningProgressNode) => {
        const location = nodeToLocation(node);
        if (!location) {
          return;
        }

        service.goTo(location, "tree");
        await panelManager.show();
      },
    ),

    vscode.commands.registerCommand(
      "qsharp-vscode.learningAskInChat",
      async (node: LearningProgressNode) => {
        const location = nodeToLocation(node);
        if (!location) {
          return;
        }

        // Include #goto with precise IDs so the agent can call the
        // tool without fuzzy matching.
        const prompt = `/qdk-learning #goto ${location.unitId} ${location.activityId} — Go to this activity`;
        await vscode.commands.executeCommand("workbench.action.chat.open", {
          query: prompt,
          isPartialQuery: false,
        });
        // Navigation telemetry will fire when the chat agent calls goTo via LM tools.
      },
    ),
  );
}

function nodeToLocation(
  node: LearningProgressNode,
): ActivityLocation | undefined {
  switch (node.kind) {
    case "continue":
      return node.location;
    case "activity":
      return {
        courseId: node.courseId,
        unitId: node.unitId,
        activityId: node.activity.id,
      };
    case "unit": {
      const first = node.unit.activities[0];
      if (!first) return undefined;
      return {
        courseId: node.courseId,
        unitId: node.unit.id,
        activityId: first.id,
      };
    }
  }
}
