// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";
import { isNotebookCourse } from "./courseLayout.js";
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
 * therefore untrusted: the renderer may only name an action id from a fixed
 * allowlist, never a prompt or command id, and any free text it contributes is
 * sanitized into an extension-owned template.
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
        await handleAction(service, message.actionId, {
          ...message.context,
        });
      } catch (e) {
        log.error(`Learning: renderer message "${message.type}" failed`, e);
      }
    }),
  );
}

async function handleAction(
  service: LearningService,
  actionId: CopilotActionId,
  context: Record<string, string>,
): Promise<void> {
  // The learner may open a course notebook before anything has started the
  // learning experience. Initialize first — the same thing the "continue"
  // command does — so the button never silently does nothing.
  if (!service.initialized) {
    await service.tryInitialize({ createIfMissing: true });
  }

  if (
    !service.initialized ||
    !isNotebookCourse(service.getActiveCourseInfo())
  ) {
    log.warn(
      "Learning: ignoring a renderer action outside an active notebook course.",
    );
    return;
  }

  await openChat(buildQuery(actionId, context));
}

/**
 * Prompt templates, owned by the extension.
 *
 * These stay as short as the queries the cell status bar sends
 * ("/qdk-learning Give me a hint"). The `qdk-learning-*` language model tools
 * already report the learner's position, progress and code on every
 * invocation, so a long prompt would be restating what the agent can look up.
 * The question and the chosen option are the exception: a quiz is not an
 * activity, so nothing the tools can read says which of a unit's questions
 * was answered or what was picked.
 */
function buildQuery(
  actionId: CopilotActionId,
  context: Record<string, string>,
): string {
  const choice = sanitize(context.choice);
  const question = sanitize(context.question);

  switch (actionId) {
    case "why-wrong":
      if (question && choice) {
        return `/qdk-learning I answered "${choice}" to: ${question} — why is that wrong?`;
      }
      return choice
        ? `/qdk-learning I picked "${choice}" and it was marked wrong. Why?`
        : `/qdk-learning I got this question wrong. Why?`;
  }
}

async function openChat(query: string): Promise<void> {
  // No position move here. A quiz cell is deliberately not an activity, so
  // there is nothing for `goToActivityByCellId` to find — the payload's
  // `cellId` is the quiz's own id, not an ipynb cell id. The question and the
  // chosen option travel in the query instead.

  await vscode.commands.executeCommand("workbench.action.chat.open", {
    query,
    isPartialQuery: false,
  });
}

/** Longest run of renderer-supplied text we'll splice into a chat prompt. */
const MAX_CONTEXT_CHARS = 300;

/**
 * Flatten renderer-supplied text so it can sit inside a quoted prompt template.
 *
 * Control characters, line separators and newlines are folded to spaces so the
 * text can't break out of its sentence and read as a fresh instruction, double
 * quotes become single quotes so they can't close the quoted span, and the
 * result is truncated. This is defence in depth: the values are authored by the
 * course or chosen by the learner, but they still reach a language model.
 */
function sanitize(value: string | undefined): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }

  let flattened = "";
  for (const char of value) {
    const code = char.codePointAt(0) ?? 0;
    const isControl =
      code < 0x20 ||
      (code >= 0x7f && code <= 0x9f) ||
      code === 0x2028 ||
      code === 0x2029;

    if (isControl) {
      flattened += " ";
    } else if (char === '"') {
      flattened += "'";
    } else {
      flattened += char;
    }
  }

  const collapsed = flattened.replace(/\s+/g, " ").trim();
  if (collapsed.length === 0) {
    return undefined;
  }

  return collapsed.length > MAX_CONTEXT_CHARS
    ? `${collapsed.slice(0, MAX_CONTEXT_CHARS - 1)}\u2026`
    : collapsed;
}
