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
        // Initialize the workspace before opening chat so the agent
        // finds it already set up and skips the confirmation prompt.
        await service.tryInitialize({ createIfMissing: true });

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

        // If the activity lives in a non-active course, switch first so the
        // service's active course matches before navigating.
        if (
          service.initialized &&
          location.courseId !== service.getActiveCourseId()
        ) {
          await service.switchCourse(location.courseId, "tree");
        }

        await service.goTo(location, "tree");

        // For python-notebook exercise activities, open the notebook
        // directly instead of showing the lesson panel.
        if (
          service.getActiveCourseInfo().kind === "python-notebook" &&
          node.kind === "activity" &&
          node.activity.type === "exercise"
        ) {
          const notebookUri = service.getCurrentCodeFileUri();
          if (notebookUri) {
            await vscode.commands.executeCommand(
              "vscode.openWith",
              notebookUri,
              "jupyter-notebook",
              { viewColumn: vscode.ViewColumn.Active, preview: false },
            );
            return;
          }
        }

        await panelManager.show();
      },
    ),

    // Multi-course commands

    vscode.commands.registerCommand(
      "qsharp-vscode.learningSwitchCourse",
      async (node?: LearningProgressNode) => {
        const courseId = await resolveCourseId(service, node);
        if (!courseId) {
          return;
        }
        await service.switchCourse(courseId, "tree");
        await panelManager.show();
      },
    ),

    vscode.commands.registerCommand(
      "qsharp-vscode.learningCourseInfo",
      async (node?: LearningProgressNode) => {
        const courseId = await resolveCourseId(service, node);
        if (!courseId) {
          return;
        }
        await showCourseInfo(service, courseId);
      },
    ),

    vscode.commands.registerCommand(
      "qsharp-vscode.learningCheckEnvironment",
      async (node?: LearningProgressNode) => {
        await runEnvironmentCheckCommand(service, node);
      },
    ),

    vscode.commands.registerCommand(
      "qsharp-vscode.learningAskInChat",
      async (node: LearningProgressNode) => {
        const location = nodeToLocation(node);
        if (!location) {
          return;
        }

        // Navigate first so the panel shows the activity.
        await service.goTo(location, "tree");
        await panelManager.show();

        // Open chat with a friendly prompt referencing the activity title.
        const title = nodeToTitle(node);
        const prompt = `/qdk-learning Let's work on "${title}".`;
        await vscode.commands.executeCommand("workbench.action.chat.open", {
          query: prompt,
          isPartialQuery: false,
        });
      },
    ),

    vscode.commands.registerCommand(
      "qsharp-vscode.learningNotebookHint",
      async (arg?: string | { cell: vscode.NotebookCell }) => {
        if (!service.initialized) {
          return;
        }

        const courseInfo = service.getActiveCourseInfo();
        if (courseInfo.kind !== "python-notebook") {
          return;
        }

        // Resolve cell ID from the argument:
        // - string: passed directly from the cell status bar item
        // - { cell }: passed by VS Code when invoked from notebook/cell/title
        let cellId: string | undefined;
        if (typeof arg === "string") {
          cellId = arg;
        } else if (arg && "cell" in arg) {
          const id = arg.cell.metadata?.id;
          if (typeof id === "string") {
            cellId = id;
          }
        }

        // Navigate to the exercise so the service state matches.
        if (cellId) {
          await service.goToExerciseByCellId(cellId, "panel");
        }

        await vscode.commands.executeCommand("workbench.action.chat.open", {
          query: `/qdk-learning Give me a hint`,
        });
      },
    ),
  );
}

function nodeToTitle(node: LearningProgressNode): string {
  switch (node.kind) {
    case "course":
      return node.descriptor.title;
    case "continue":
      return node.activityTitle;
    case "activity":
      return node.activity.title;
    case "unit":
      return node.unit.title;
  }
}

function nodeToLocation(
  node: LearningProgressNode,
): ActivityLocation | undefined {
  switch (node.kind) {
    case "course":
      return undefined;
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

/**
 * Resolve a target course id from a tree node, or prompt the user with a
 * quick pick when invoked without one (e.g. from the command palette).
 */
async function resolveCourseId(
  service: LearningService,
  node?: LearningProgressNode,
): Promise<string | undefined> {
  if (node?.kind === "course") {
    return node.descriptor.id;
  }
  if (!service.initialized) {
    const ok = await service.tryInitialize({ createIfMissing: true });
    if (!ok) {
      return undefined;
    }
  }
  const courses = await service.getCourses();
  if (courses.length === 0) {
    return undefined;
  }
  const activeId = service.getActiveCourseId();
  const picked = await vscode.window.showQuickPick(
    courses.map((c) => ({
      label: c.title,
      description: c.id === activeId ? "current" : undefined,
      detail: c.shortDescription,
      id: c.id,
    })),
    { placeHolder: "Select a course" },
  );
  return picked?.id;
}

/** Show a course's README in a markdown preview, or a fallback message. */
async function showCourseInfo(
  service: LearningService,
  courseId: string,
): Promise<void> {
  const courses = await service.getCourses();
  const descriptor = courses.find((c) => c.id === courseId);
  if (!descriptor) {
    return;
  }
  if (descriptor.readmePath) {
    const uri = vscode.Uri.parse(descriptor.readmePath);
    await vscode.commands.executeCommand("markdown.showPreview", uri);
    return;
  }
  const detail = descriptor.shortDescription
    ? `\n\n${descriptor.shortDescription}`
    : "";
  await vscode.window.showInformationMessage(`${descriptor.title}${detail}`, {
    modal: false,
  });
}

/**
 * Run environment diagnostics for a course and present a rich, readable
 * report, offering the fixes the report surfaces (e.g. one-click
 * environment setup, install extensions).
 */
async function runEnvironmentCheckCommand(
  service: LearningService,
  node?: LearningProgressNode,
): Promise<void> {
  // TODO (acasey): don't allow overlapping runs
  if (!service.initialized) {
    const ok = await service.tryInitialize({ createIfMissing: true });
    if (!ok) {
      vscode.window.showWarningMessage("Open a learning workspace first.");
      return;
    }
  }
  // If invoked on a specific course node, diagnose that course.
  const courseId = node?.kind === "course" ? node.descriptor.id : undefined;
  if (courseId && courseId !== service.getActiveCourseId()) {
    await service.switchCourse(courseId, "tree");
  }

  const report = await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: "Running course diagnostics…",
    },
    () => service.runEnvironmentCheck(),
  );

  const icon: Record<string, string> = {
    ok: "✓",
    warn: "▲",
    fail: "✗",
    skip: "–",
  };
  const statusBadge: Record<string, string> = {
    ok: "✓ OK",
    warning: "▲ Warning",
    error: "✗ Error",
  };

  const lines = report.checks.map((c) => {
    const head = `${icon[c.status] ?? "•"} ${c.label}`;
    const detail = c.detail ? `\n    ${c.detail}` : "";
    const hint = c.hint ? `\n    → ${c.hint}` : "";
    return `${head}${detail}${hint}`;
  });

  const body = [
    `${statusBadge[report.overallStatus] ?? report.overallStatus} · ${report.summary}`,
    "",
    ...lines,
  ].join("\n");

  const actions = report.fixes.map((r) => r.label);
  const choice = await vscode.window.showInformationMessage(
    body,
    { modal: true },
    ...actions,
  );
  if (!choice) {
    return;
  }
  const fix = report.fixes.find((r) => r.label === choice);
  if (fix) {
    await service.applyEnvironmentCheckFix(fix);
  }
}
