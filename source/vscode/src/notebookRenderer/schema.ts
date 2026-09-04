// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * The contract between a learning notebook's output and the renderer.
 *
 * Written twice — here, and as the dicts `_learning_output.py` builds — so
 * `checkRendererContract()` in `build.mjs` fails the build when they drift.
 */

export const MIME_TYPE = "application/vnd.qdk.learning+json" as const;
export const RENDERER_ID = "qsharp-vscode.qdkLearningRenderer" as const;

type LearningPayloadBase = {
  schemaVersion: 1;
  kind: string;
  /**
   * Identifies the payload, not the notebook cell holding it: for a quiz this
   * is its registered id, which keeps radio groups unique. Deliberately not an
   * ipynb cell id, so don't pass it to anything that resolves activities.
   */
  cellId?: string;
};

export type MultipleChoicePayload = LearningPayloadBase & {
  kind: "multiple-choice";
  prompt: string;
  options: Array<{
    id: string;
    text: string;
    correct: boolean;
    explanation?: string;
  }>;
  /**
   * More than one option is correct, and the learner must find all of them.
   *
   * Drives checkboxes rather than radios, and an explicit instruction — a
   * learner who assumes one answer would stop at the first correct option and
   * be marked wrong for a question they understood.
   */
  multiSelect?: boolean;
};

export type LearningPayload = MultipleChoicePayload;

/**
 * Copilot actions a notebook output is allowed to request.
 *
 * Security boundary: output may name an id from this list and attach small
 * structured string context. It may never send a free-form prompt string or a
 * command identifier across the renderer bridge — the wording lives in the
 * extension, so a notebook cannot script the chat panel.
 */
export const COPILOT_ACTION_IDS = ["why-wrong"] as const;

export type CopilotActionId = (typeof COPILOT_ACTION_IDS)[number];

type RendererActionMessage = {
  type: "qdk-learning/action";
  rendererId: typeof RENDERER_ID;
  actionId: CopilotActionId;
  cellId?: string;
  context?: Record<string, string>;
};

export type RendererToExtensionMessage = RendererActionMessage;

/**
 * Bounds on renderer-supplied strings.
 *
 * Only the value and cell-id limits can be reached by a payload today — the
 * key set is built here in the renderer. They are enforced anyway because this
 * validator runs on the extension host, where the message is untrusted input
 * rather than something this code produced.
 */
const MAX_CONTEXT_ENTRIES = 20;
const MAX_CONTEXT_KEY_LENGTH = 64;
const MAX_CONTEXT_VALUE_LENGTH = 4096;
const MAX_CELL_ID_LENGTH = 256;

export function isRecord(value: unknown): value is Record<string, unknown> {
  return (
    typeof value === "object" &&
    value !== null &&
    !Array.isArray(value) &&
    Object.getPrototypeOf(value) === Object.prototype
  );
}

export function isRendererToExtensionMessage(
  x: unknown,
): x is RendererToExtensionMessage {
  if (!isRecord(x) || x.rendererId !== RENDERER_ID) {
    return false;
  }

  if (x.type !== "qdk-learning/action" || !isCopilotActionId(x.actionId)) {
    return false;
  }

  if (
    x.cellId !== undefined &&
    !isNonEmptyShortString(x.cellId, MAX_CELL_ID_LENGTH)
  ) {
    return false;
  }

  return x.context === undefined || isContextRecord(x.context);
}

function isCopilotActionId(value: unknown): value is CopilotActionId {
  return (
    typeof value === "string" &&
    COPILOT_ACTION_IDS.includes(value as CopilotActionId)
  );
}

function isContextRecord(value: unknown): value is Record<string, string> {
  if (!isRecord(value)) {
    return false;
  }

  const entries = Object.entries(value);
  return (
    entries.length <= MAX_CONTEXT_ENTRIES &&
    entries.every(
      ([key, entryValue]) =>
        isShortString(key, MAX_CONTEXT_KEY_LENGTH) &&
        key.length > 0 &&
        isShortString(entryValue, MAX_CONTEXT_VALUE_LENGTH),
    )
  );
}

function isShortString(value: unknown, maxLength: number): value is string {
  return typeof value === "string" && value.length <= maxLength;
}

function isNonEmptyShortString(
  value: unknown,
  maxLength: number,
): value is string {
  return isShortString(value, maxLength) && value.length > 0;
}
