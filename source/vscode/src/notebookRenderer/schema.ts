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
export const SUPPORTED_SCHEMA_VERSION = 1;
export const SUPPORTED_KINDS = ["multiple-choice"] as const;

type LearningPayloadBase = {
  schemaVersion: 1;
  kind: string;
  /**
   * Identifies the payload. For a quiz this is its authored id, which keeps
   * radio groups unique and names the question when the learner asks about it.
   * Deliberately distinct from the stable notebook cell ID used for progress.
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

type QuizAnsweredMessage = {
  type: "qdk-learning/quiz-answered";
  rendererId: typeof RENDERER_ID;
  quizId: string;
  optionIds: string[];
};

type QuizStateRequest = {
  type: "qdk-learning/quiz-state-request";
  rendererId: typeof RENDERER_ID;
  quizId: string;
  requestId: string;
};

export type QuizAnswerState = {
  quizId: string;
  optionIds: string[];
};

export type QuizStateMessage = {
  type: "qdk-learning/quiz-state";
  rendererId: typeof RENDERER_ID;
  quizId: string;
  requestId: string;
  optionIds?: string[];
};

export type RendererToExtensionMessage =
  | RendererActionMessage
  | QuizAnsweredMessage
  | QuizStateRequest;

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

  if (x.type === "qdk-learning/quiz-answered") {
    return isQuizAnswerState(x);
  }
  if (x.type === "qdk-learning/quiz-state-request") {
    return isLearningId(x.quizId) && isLearningId(x.requestId);
  }

  if (x.type !== "qdk-learning/action" || !isCopilotActionId(x.actionId)) {
    return false;
  }

  if (x.quizId !== undefined && !isLearningId(x.quizId)) {
    return false;
  }

  return x.optionIds === undefined || isQuizOptionIds(x.optionIds);
}

export function isQuizAnswerState(value: unknown): value is QuizAnswerState {
  return (
    isRecord(value) &&
    isLearningId(value.quizId) &&
    isQuizOptionIds(value.optionIds) &&
    value.optionIds.length > 0 &&
    new Set(value.optionIds).size === value.optionIds.length
  );
}

export function isQuizStateMessage(value: unknown): value is QuizStateMessage {
  return (
    isRecord(value) &&
    value.type === "qdk-learning/quiz-state" &&
    value.rendererId === RENDERER_ID &&
    isLearningId(value.quizId) &&
    isLearningId(value.requestId) &&
    (value.optionIds === undefined || isQuizAnswerState(value))
  );
}

export function canRestoreQuizAnswer(
  payload: MultipleChoicePayload,
  optionIds: readonly string[],
): boolean {
  return (
    isQuizOptionIds(optionIds) &&
    optionIds.length > 0 &&
    new Set(optionIds).size === optionIds.length &&
    (payload.multiSelect === true || optionIds.length === 1) &&
    optionIds.every((id) => payload.options.some((option) => option.id === id))
  );
}

export function isLearningId(value: unknown): value is string {
  return (
    typeof value === "string" &&
    value === value.trim() &&
    ID_PATTERN.test(value)
  );
}

export function isQuizOptionIds(value: unknown): value is string[] {
  return (
    Array.isArray(value) &&
    value.length <= MAX_OPTION_IDS &&
    value.every(isLearningId)
  );
}

export function isCorrectQuizAnswer(
  options: MultipleChoicePayload["options"],
  optionIds: readonly string[],
): boolean {
  const selected = new Set(optionIds);
  return (
    selected.size > 0 &&
    selected.size === optionIds.length &&
    optionIds.every((id) => options.some((option) => option.id === id)) &&
    options.every((option) => selected.has(option.id) === option.correct)
  );
}

/** Validate payloads identically in the renderer and checked-answer handler. */
export function parseLearningPayload(value: unknown): LearningPayload {
  if (!isRecord(value)) {
    throw new Error("Expected a QDK learning payload object.");
  }
  const payload = value;
  if (payload.schemaVersion !== SUPPORTED_SCHEMA_VERSION) {
    const ahead =
      typeof payload.schemaVersion === "number" &&
      payload.schemaVersion > SUPPORTED_SCHEMA_VERSION;
    throw new Error(
      `This output uses QDK learning payload version ${String(payload.schemaVersion)}, ` +
        `but this renderer supports version ${SUPPORTED_SCHEMA_VERSION}. ` +
        (ahead
          ? "Update the QDK extension to view it."
          : "Use Reset Unit on this notebook's toolbar to get an up-to-date copy."),
    );
  }
  if (!SUPPORTED_KINDS.some((kind) => kind === payload.kind)) {
    throw new Error(
      `Unknown QDK learning output kind "${String(payload.kind)}". ` +
        `This renderer knows: ${SUPPORTED_KINDS.join(", ")}.`,
    );
  }
  if (
    payload.payloadId !== undefined &&
    typeof payload.payloadId !== "string"
  ) {
    throw new Error("QDK learning payload has a non-string payloadId.");
  }
  if (typeof payload.prompt !== "string" || payload.prompt.length === 0) {
    throw new Error("QDK learning payload has no question text.");
  }
  if (
    payload.multiSelect !== undefined &&
    typeof payload.multiSelect !== "boolean"
  ) {
    throw new Error("QDK learning payload has a non-boolean multiSelect.");
  }
  if (!Array.isArray(payload.options) || payload.options.length === 0) {
    throw new Error("QDK learning payload has no options.");
  }

  const options: MultipleChoicePayload["options"] = [];
  const ids = new Set<string>();
  for (const option of payload.options) {
    if (!isRecord(option)) {
      throw new Error("QDK learning payload has a malformed option.");
    }
    if (typeof option.id !== "string" || option.id.length === 0) {
      throw new Error("QDK learning payload has an option with no id.");
    }
    if (ids.has(option.id)) {
      throw new Error(
        `QDK learning payload reuses the option id "${option.id}".`,
      );
    }
    ids.add(option.id);
    if (typeof option.text !== "string" || option.text.length === 0) {
      throw new Error(
        `QDK learning option "${option.id}" has no text to show.`,
      );
    }
    if (typeof option.correct !== "boolean") {
      throw new Error(
        `QDK learning option "${option.id}" does not say whether it is correct.`,
      );
    }
    if (
      option.explanation !== undefined &&
      typeof option.explanation !== "string"
    ) {
      throw new Error(
        `QDK learning option "${option.id}" has a non-string explanation.`,
      );
    }
    options.push({
      id: option.id,
      text: option.text,
      correct: option.correct,
      ...(option.explanation !== undefined
        ? { explanation: option.explanation }
        : {}),
    });
  }
  const correct = options.filter((option) => option.correct).length;
  if (correct === 0) {
    throw new Error("QDK learning payload has no correct option.");
  }
  if (payload.multiSelect === true) {
    if (correct < 2) {
      throw new Error(
        "QDK learning payload says select all that apply but marks one option correct.",
      );
    }
    if (correct === options.length) {
      throw new Error(
        "QDK learning payload marks every option correct, so it cannot be answered wrongly.",
      );
    }
  } else if (correct > 1) {
    throw new Error(
      `QDK learning payload marks ${correct} options correct but is not multi-select.`,
    );
  }
  return {
    schemaVersion: SUPPORTED_SCHEMA_VERSION,
    kind: "multiple-choice",
    prompt: payload.prompt,
    options,
    ...(payload.payloadId !== undefined
      ? { payloadId: payload.payloadId }
      : {}),
    ...(payload.multiSelect !== undefined
      ? { multiSelect: payload.multiSelect }
      : {}),
  };
}

function isCopilotActionId(value: unknown): value is CopilotActionId {
  return (
    typeof value === "string" &&
    COPILOT_ACTION_IDS.includes(value as CopilotActionId)
  );
}
