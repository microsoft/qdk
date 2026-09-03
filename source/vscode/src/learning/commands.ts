// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";
import { isNotebookCourse } from "./courseLayout.js";
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
        // The whole unit was reset, so the learner's old position no longer
        // means anything — start them at the top of the fresh notebook.
        await openCourseNotebook(service, { reveal: "top" });
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

        const query = service.hasUserSelectedCourse()
          ? "/qdk-learning Let's start learning."
          : "/qdk-learning What courses are available?";
        await vscode.commands.executeCommand("workbench.action.chat.open", {
          query,
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
        if (isNotebookCourse(service.getActiveCourseInfo())) {
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
        const courseId =
          node?.kind === "course" ? node.descriptor.id : undefined;
        if (!courseId) {
          return;
        }
        await service.switchCourse(courseId, "tree");

        // python-notebook courses don't use the lesson panel — open the
        // notebook directly. Switching lands on a unit rather than a specific
        // activity, so start at the top of the notebook: the first activity is
        // a code cell, which would otherwise skip the unit's introduction.
        if (isNotebookCourse(service.getActiveCourseInfo())) {
          await openCourseNotebook(service, { reveal: "top" });
          return;
        }

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
        if (!isNotebookCourse(courseInfo)) {
          return;
        }

        const cellId = resolveCellId(arg);

        // Navigate to the exercise so the service state matches.
        if (cellId) {
          await service.goToActivityByCellId(cellId, "notebook");
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
        if (!isNotebookCourse(courseInfo)) {
          return;
        }

        const cellId = resolveCellId(arg);
        if (cellId) {
          // This will no-op if it's not an activity cell
          void (await service.goToActivityByCellId(cellId, "notebook"));
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
 * Open the current unit's notebook working copy.
 *
 * By default this reveals the current exercise cell; pass `reveal: "top"` to
 * start at the beginning of the notebook instead.
 */
async function openCourseNotebook(
  service: LearningService,
  options?: { reveal?: "exercise" | "top" },
): Promise<void> {
  await service.setCourseSelectedAndSave();
  const notebookUri = service.getCurrentCodeFileUri();
  if (!notebookUri) {
    log.warn("No notebook associated with the current position.");
    return;
  }

  let opened = false;

  // Try to open via the Jupyter extension's unstable API so the course's
  // Python environment is automatically set as the active kernel.
  try {
    // Grab before awaiting, in case it changes while we're yielding
    // and gets out of sync with notebookUri
    const courseId = service.getActiveCourseId();
    const jupyter = vscode.extensions.getExtension("ms-toolsai.jupyter");
    const api = await jupyter?.activate();
    if (api && typeof api.openNotebook === "function") {
      const envPath = await service.getJupyterEnvironmentPath(courseId);
      if (envPath) {
        await api.openNotebook(notebookUri, envPath);
        const document = vscode.workspace.notebookDocuments.find(
          (notebook) => notebook.uri.toString() === notebookUri.toString(),
        );
        if (document) {
          await vscode.window.showNotebookDocument(document, {
            viewColumn: vscode.ViewColumn.Active,
            preview: false,
          });
        }
        opened = true;
      } else {
        log.info("Didn't find a course virtual environment to use in notebook");
      }
    }
    if (!opened) {
      log.warn(
        "Jupyter openNotebook API is not available; falling back to generic open.",
      );
    }
  } catch (e) {
    log.warn(
      `Jupyter openNotebook API call failed: ${e}; falling back to generic open.`,
    );
  }

  if (!opened) {
    await vscode.commands.executeCommand(
      "vscode.openWith",
      notebookUri,
      "jupyter-notebook",
      { viewColumn: vscode.ViewColumn.Active, preview: false },
    );
  }

  const cellId = service.getCurrentExerciseCellId();

  if (options?.reveal === "top") {
    revealNotebookTop(notebookUri);
  } else if (cellId) {
    revealNotebookCell(notebookUri, cellId);
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
