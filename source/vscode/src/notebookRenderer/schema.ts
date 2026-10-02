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
   * Identifies the payload. For a quiz this is its registered id, which keeps
   * radio groups unique and names the question when the learner asks about it.
   * Deliberately not an ipynb cell id: a quiz cell is not an activity, so this
   * must never reach anything that resolves one.
   */
  payloadId?: string;
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
  /** Ids of the options the learner had selected when they asked. */
  optionIds?: string[];
};

export type RendererToExtensionMessage = RendererActionMessage;

/**
 * The shape every id crossing the bridge must have.
 *
 * A budget, not a guarantee. Hyphens are word separators, so a 64-character id
 * can still read as a short sentence — the pattern removes punctuation,
 * newlines and length, not meaning. What actually limits an attacker is that
 * the extension owns every word around these ids; see `buildQuery` in
 * `notebookRendererMessaging.ts`, and keep any new template as narrow.
 */
const ID_PATTERN = /^[a-z0-9][a-z0-9-]{0,63}$/;

/**
 * Caps how much attacker-controlled text one action can carry, and so how many
 * options a question may have — the emitter refuses more, so the limit lands on
 * the author. Keep in step with `_MAX_OPTIONS` in `_learning_output.py`.
 */
const MAX_OPTION_IDS = 8;

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
    x.quizId !== undefined &&
    !(typeof x.quizId === "string" && ID_PATTERN.test(x.quizId))
  ) {
    return false;
  }

  return (
    x.optionIds === undefined ||
    (Array.isArray(x.optionIds) &&
      x.optionIds.length <= MAX_OPTION_IDS &&
      x.optionIds.every((id) => typeof id === "string" && ID_PATTERN.test(id)))
  );
}

function isCopilotActionId(value: unknown): value is CopilotActionId {
  return (
    typeof value === "string" &&
    COPILOT_ACTION_IDS.includes(value as CopilotActionId)
  );
}
