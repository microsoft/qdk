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
        // only as trustworthy as the file it came from. This is the whole
        // authorization: a workbook this workspace materialized is a course
        // file whichever course the learner last navigated to.
        if (
          !service.initialized ||
          !service.isCourseWorkbookUri(event.editor.notebook.uri)
        ) {
          log.warn(
            "Learning: ignoring a renderer message from a notebook this workspace did not create.",
          );
          return;
        }

        await openChat(buildQuery(message.actionId, message.quizId));
      } catch (e) {
        log.error(`Learning: renderer message "${message.type}" failed`, e);
      }
    }),
  );
}

/**
 * Prompt templates, owned by the extension.
 *
 * These stay as short as the queries the cell status bar sends
 * ("/qdk-learning Give me a hint"). The `qdk-learning-*` language model tools
 * already report the learner's position, progress and code on every
 * invocation, so a long prompt would be restating what the agent can look up.
 *
 * Nothing the renderer wrote is quoted here. An earlier version spliced in the
 * question and the chosen option, which are notebook content and therefore
 * attacker-supplied prose in a file that only has to sit at a workbook's path.
 * The quiz id is enough for the agent to find the question in the open
 * notebook, and its shape leaves no room for an instruction.
 */
function buildQuery(actionId: CopilotActionId, quizId?: string): string {
  switch (actionId) {
    case "why-wrong":
      return quizId
        ? `/qdk-learning I answered the quiz "${quizId}" in this notebook incorrectly. Why is my answer wrong?`
        : `/qdk-learning I got this question wrong. Why?`;
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
