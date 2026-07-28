// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
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

    vscode.commands.registerCommand(
      "qsharp-vscode.learningResetUnit",
      async (node?: LearningProgressNode) => {
        if (!service.initialized) {
          return;
        }

        // Invoked from the tree, the target unit may not be the current one.
        const location = node ? nodeToLocation(node) : undefined;
        if (location) {
          if (location.courseId !== service.getActiveCourseId()) {
            await service.switchCourse(location.courseId, "tree");
          }
          await service.goTo(location, "tree");
        }

        const confirmed = await vscode.window.showWarningMessage(
          "Reset this unit to the original notebook? Your current work will be lost.",
          { modal: true },
          "Reset",
        );
        if (confirmed !== "Reset") {
          return;
        }

        await service.resetExercise();
        await openCourseNotebook(service);
        vscode.window.showInformationMessage("Unit has been reset.");
      },
    ),

    // Progress tree commands

    vscode.commands.registerCommand(
      "qsharp-vscode.learningRefresh",
      async () => {
        await service.refresh();
      },
    ),

    // In spite of the name, this is used to start the learning experience
    // (typically, via a button on the Welcome screen).
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

        // python-notebook courses don't use the lesson panel — the notebook
        // is the primary surface, so open it directly. Clicking a unit
        // targets the unit as a whole (the position lands on its first
        // activity), so start the learner at the top of the notebook rather
        // than jumping straight to an exercise.
        if (service.getActiveCourseInfo().kind === "python-notebook") {
          await openCourseNotebook(service, {
            reveal: node.kind === "unit" ? "top" : "exercise",
          });
          return;
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
          // This may simply indicate that the user declined to pick a course
          return;
        }
        await service.switchCourse(courseId, "tree");

        // python-notebook courses don't use the lesson panel. For a course
        // that hasn't been started yet, show the README so there's something
        // to read while the environment is set up in the background;
        // otherwise pick up where the learner left off.
        if (service.getActiveCourseInfo().kind === "python-notebook") {
          if (service.getProgress().stats.completedActivities === 0) {
            // TODO (acasey): close this once a notebook is open
            await showCourseInfo(service, courseId);
          } else {
            await openCourseNotebook(service);
          }
          return;
        }

        await panelManager.show();
      },
    ),

    vscode.commands.registerCommand(
      "qsharp-vscode.learningCourseInfo",
      async (node?: LearningProgressNode) => {
        const courseId = await resolveCourseId(service, node);
        if (!courseId) {
          // This may simply indicate that the user declined to pick a course
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

        const cellId = resolveCellId(arg);

        // Navigate to the exercise so the service state matches.
        if (cellId) {
          await service.goToExerciseByCellId(cellId, "notebook");
        }

        await vscode.commands.executeCommand("workbench.action.chat.open", {
          query: `/qdk-learning Give me a hint`,
        });
      },
    ),

    vscode.commands.registerCommand(
      "qsharp-vscode.learningNotebookExplain",
      async (arg?: string | { cell: vscode.NotebookCell }) => {
        if (!service.initialized) {
          return;
        }

        const courseInfo = service.getActiveCourseInfo();
        if (courseInfo.kind !== "python-notebook") {
          return;
        }

        // The button is offered on every cell, so the cell may not be an
        // exercise. Only move the service's position when it is one.
        const cellId = resolveCellId(arg);
        if (cellId && service.getExerciseCellIds().has(cellId)) {
          await service.goToExerciseByCellId(cellId, "notebook");
        }

        await vscode.commands.executeCommand("workbench.action.chat.open", {
          query: `/qdk-learning Explain this concept in more detail`,
        });
      },
    ),
  );
}

/**
 * Resolve a notebook cell ID from a command argument:
 * - string: passed directly from the cell status bar item
 * - { cell }: passed by VS Code when invoked from notebook/cell/title
 */
function resolveCellId(
  arg?: string | { cell: vscode.NotebookCell },
): string | undefined {
  if (typeof arg === "string") {
    return arg;
  }
  if (arg && "cell" in arg) {
    const id = arg.cell.metadata?.id;
    if (typeof id === "string") {
      return id;
    }
  }
  return undefined;
}

/**
 * Open the current unit's notebook working copy, pre-selecting the course's
 * Python environment as the active kernel.
 *
 * By default this reveals the current exercise cell; pass `reveal: "top"` to
 * start at the beginning of the notebook instead.
 */
async function openCourseNotebook(
  service: LearningService,
  options?: { reveal?: "exercise" | "top" },
): Promise<void> {
  const notebookUri = service.getCurrentCodeFileUri();
  if (!notebookUri) {
    log.warn("No notebook associated with the current position.");
    return;
  }
  const cellId = service.getCurrentExerciseCellId();

  // Fallback: open without pre-selecting a kernel.
  await vscode.commands.executeCommand(
    "vscode.openWith",
    notebookUri,
    "jupyter-notebook",
    { viewColumn: vscode.ViewColumn.Active, preview: false },
  );

  if (options?.reveal === "top") {
    revealNotebookTop(notebookUri);
  } else if (cellId) {
    revealNotebookCell(notebookUri, cellId);
  }

  // The notebook may appear dirty immediately after opening (e.g. cell
  // language adjustments). Save so the user starts with a clean state.
  const doc = vscode.workspace.notebookDocuments.find(
    (n) => n.uri.toString() === notebookUri.toString(),
  );
  if (doc?.isDirty) {
    await doc.save();
  }
}

/**
 * Select the cell with the given stable ID in an already-open notebook and
 * scroll it into view. When the cell is immediately preceded by a markdown
 * cell — typically the exercise's instructions — that cell is scrolled to
 * instead, so the learner sees the prompt and not just the code.
 *
 * No-op if the notebook isn't visible or the cell can't be found.
 */
function revealNotebookCell(notebookUri: vscode.Uri, cellId: string): void {
  const editor = findNotebookEditor(notebookUri);
  if (!editor) {
    return;
  }
  const cell = editor.notebook
    .getCells()
    .find((c) => c.metadata?.id === cellId);
  if (!cell) {
    log.warn(`Cell ${cellId} not found in ${notebookUri}; can't reveal it.`);
    return;
  }

  // The selection stays on the exercise cell — only the scroll target
  // widens to include the preceding prompt.
  editor.selection = new vscode.NotebookRange(cell.index, cell.index + 1);

  const previous =
    cell.index > 0 ? editor.notebook.cellAt(cell.index - 1) : undefined;
  const revealStart =
    previous?.kind === vscode.NotebookCellKind.Markup
      ? previous.index
      : cell.index;
  editor.revealRange(
    new vscode.NotebookRange(revealStart, cell.index + 1),
    vscode.NotebookEditorRevealType.AtTop,
  );
}

/**
 * Scroll an already-open notebook back to its first cell. Used when the
 * learner opens a unit as a whole rather than a specific exercise.
 */
function revealNotebookTop(notebookUri: vscode.Uri): void {
  const editor = findNotebookEditor(notebookUri);
  if (!editor || editor.notebook.cellCount === 0) {
    return;
  }
  const range = new vscode.NotebookRange(0, 1);
  editor.selection = range;
  editor.revealRange(range, vscode.NotebookEditorRevealType.AtTop);
}

/** The visible editor showing the given notebook, if there is one. */
function findNotebookEditor(
  notebookUri: vscode.Uri,
): vscode.NotebookEditor | undefined {
  const uriStr = notebookUri.toString();
  const editor = vscode.window.visibleNotebookEditors.find(
    (e) => e.notebook.uri.toString() === uriStr,
  );
  if (!editor) {
    log.warn(`Notebook editor not found for ${uriStr}; can't scroll it.`);
  }
  return editor;
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
    log.warn(`Unable to show course info for unknown course ${courseId}`);
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
  // TODO (acasey): this is ugly and unthemed - can we do better?
  const choice = await vscode.window.showInformationMessage(
    body,
    { modal: true },
    ...actions,
  );
  if (!choice) {
    return;
  }
  const fix = report.fixes.find((r) => r.label === choice);
  if (!fix) {
    return;
  }
  await service.applyEnvironmentCheckFix(fix);
}
