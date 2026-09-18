// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";
import type { CopilotActionId } from "../notebookRenderer/schema.js";
import {
  isRendererToExtensionMessage,
  RENDERER_ID,
} from "../notebookRenderer/schema.js";
import type { LearningService } from "./service.js";

/**
 * Bridges the QDK learning notebook renderer to the extension host.
 *
 * A renderer webview can't execute VS Code commands, so `createRendererMessaging`
 * is the channel out. Everything arriving here is authored notebook content and
 * therefore untrusted: the renderer may name an action id from a fixed
 * allowlist and a quiz id of a fixed shape, and nothing else. No prose from a
 * notebook reaches a prompt, because no amount of escaping stops a sentence
 * from reading as an instruction.
 */
export function registerNotebookRendererMessaging(
  context: vscode.ExtensionContext,
  service: LearningService,
): void {
  const messaging = vscode.notebooks.createRendererMessaging(RENDERER_ID);

  context.subscriptions.push(
    messaging.onDidReceiveMessage(async (event) => {
      const message: unknown = event.message;
      if (!isRendererToExtensionMessage(message)) {
        log.warn(
          "Learning: discarding malformed message from the notebook renderer.",
        );
        return;
      }

      try {
        // Detect only — never `createIfMissing`. This runs on a message a
        // notebook asked for, and `notebookSync.ts` already sets the rule: a
        // notebook-driven event must not materialize a learning workspace
        // behind the learner's back. Authorizing the sender needs a loaded
        // course, so an unstarted workspace simply means "not trusted".
        if (!service.initialized) {
          await service.tryInitialize();
        }

        // Any notebook can carry an output of this MIME type, so a message is
        // only as trustworthy as the file it came from. This proves the file
        // sits where a loaded course says its workbook lives — not that this
        // extension wrote it, since a workspace that already contained a
        // `qdk-learning` folder is loaded as a course. Treat what follows as
        // untrusted either way.
        if (
          !service.initialized ||
          !service.isCourseWorkbook(event.editor.notebook.uri)
        ) {
          log.warn(
            "Learning: ignoring a renderer message from a notebook outside the loaded course.",
          );
          return;
        }

        await openChat(
          buildQuery(message.actionId, message.quizId, message.optionIds),
        );
      } catch (e) {
        log.error(`Learning: renderer message "${message.type}" failed`, e);
      }
    }),
  );
}

/**
 * Prompt templates, owned by the extension.
 *
 * Short on purpose, like the queries the cell status bar sends: the
 * `qdk-learning-*` tools already report the learner's position, progress and
 * code, so a longer prompt would restate what the agent can look up.
 *
 * The learner reads this in the chat box, so it is worded as a question they
 * might have asked rather than as instructions to a tool. Only ids are
 * interpolated, and only in the shape `ID_PATTERN` allows, which is what stops
 * a notebook scripting the chat panel. Asking the agent to find the question
 * is necessary because no learning tool returns quiz content. Keep any new
 * template this narrow.
 */
function buildQuery(
  actionId: CopilotActionId,
  quizId?: string,
  optionIds?: string[],
): string {
  switch (actionId) {
    case "why-wrong": {
      if (!quizId) {
        return `/qdk-learning I got this question wrong. Why?`;
      }

      const picked = optionIds?.length
        ? ` I picked ${optionIds.map((id) => `"${id}"`).join(" and ")}.`
        : "";
      return (
        `/qdk-learning I got the quiz "${quizId}" in this notebook wrong.${picked}` +
        ` Can you find that question and explain why?`
      );
    }
  }
}

async function openChat(query: string): Promise<void> {
  // No position move here. A quiz cell is deliberately not an activity, so
  // there is nothing for `goToActivityByCellId` to find — the payload's quiz
  // id is the quiz's own id, not an ipynb cell id.

  await vscode.commands.executeCommand("workbench.action.chat.open", {
    query,
    isPartialQuery: false,
  });
}
