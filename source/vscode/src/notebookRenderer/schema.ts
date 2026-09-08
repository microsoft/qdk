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
 * Security boundary: output may name an id from this list, and nothing else
 * except a quiz id of a fixed shape. It may never send free text, a prompt or
 * a command identifier across the renderer bridge — the wording lives in the
 * extension, so a notebook cannot script the chat panel.
 */
export const COPILOT_ACTION_IDS = ["why-wrong"] as const;

export type CopilotActionId = (typeof COPILOT_ACTION_IDS)[number];

type RendererActionMessage = {
  type: "qdk-learning/action";
  rendererId: typeof RENDERER_ID;
  actionId: CopilotActionId;
  quizId?: string;
};

export type RendererToExtensionMessage = RendererActionMessage;

/**
 * A quiz id is the only thing a renderer may contribute to a chat prompt.
 *
 * Everything a notebook carries is untrusted: an output of this MIME type can
 * be hand-written, and a file at a workbook's path can be shipped by whatever
 * produced the workspace. Free prose from such a payload reaching a prompt is
 * an injection, and no amount of quote-stripping changes that, because the
 * payload is a sentence either way. Constraining the one value that does cross
 * to this shape leaves no room for an instruction.
 */
const QUIZ_ID_PATTERN = /^[a-z0-9][a-z0-9-]{0,63}$/;

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

  return (
    x.quizId === undefined ||
    (typeof x.quizId === "string" && QUIZ_ID_PATTERN.test(x.quizId))
  );
}

function isCopilotActionId(value: unknown): value is CopilotActionId {
  return (
    typeof value === "string" &&
    COPILOT_ACTION_IDS.includes(value as CopilotActionId)
  );
}
