// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { assert } from "chai";
import {
  parseNotebookActivities,
  stripAuthoringCells,
} from "../../../src/learning/notebookExercises.js";
import {
  isCorrectQuizAnswer,
  isRendererToExtensionMessage,
  MIME_TYPE,
  parseLearningPayload,
  RENDERER_ID,
  type MultipleChoicePayload,
} from "../../../src/notebookRenderer/schema.js";

const authorCode = {
  id: "author-check",
  cell_type: "code",
  metadata: { tags: ["author-only"] },
  source: "validate_quizzes()",
  outputs: [{ output_type: "stream", name: "stdout", text: "Validated." }],
};

const authorNote = {
  id: "author-note",
  cell_type: "markdown",
  metadata: { tags: ["author-only"] },
  source: "## Private author heading",
};

suite("Learning notebook author-only cells", () => {
  test("are excluded from activities and learner copies without orphaning hints", () => {
    const text = JSON.stringify({
      cells: [
        { id: "heading", cell_type: "markdown", source: "## Practice" },
        {
          id: "exercise",
          cell_type: "code",
          metadata: { tags: ["exercise"] },
          source: "@exercise\ndef answer(): return 0",
          outputs: [],
        },
        authorCode,
        authorNote,
        {
          id: "hint",
          cell_type: "markdown",
          metadata: { tags: ["hint"] },
          source: "Think about the return value.",
        },
      ],
    });

    const activities = parseNotebookActivities(text, "test-unit");
    assert.lengthOf(activities, 1);
    const exercise = activities[0];
    assert(exercise.type === "exercise");
    assert.equal(exercise.id, "exercise");
    assert.deepEqual(exercise.hints, ["Think about the return value."]);

    const stripped = stripAuthoringCells(text, "test-unit");
    assert(stripped !== undefined);
    const learner: { cells: { id: string }[] } = JSON.parse(stripped);
    assert.deepEqual(
      learner.cells.map((cell) => cell.id),
      ["heading", "exercise"],
    );
  });

  test("do not replace or hide the learner cell's section heading", () => {
    const text = JSON.stringify({
      cells: [
        { id: "heading", cell_type: "markdown", source: "## Preparation" },
        authorCode,
        authorNote,
        {
          id: "learner-code",
          cell_type: "code",
          source: "value = 42",
          outputs: [],
        },
      ],
    });

    const activities = parseNotebookActivities(text, "test-unit");
    assert.lengthOf(activities, 1);
    assert.equal(activities[0].id, "learner-code");
    assert.equal(activities[0].title, "Preparation");
  });
});

const quiz: MultipleChoicePayload = {
  schemaVersion: 1,
  kind: "multiple-choice",
  payloadId: "first-question",
  prompt: "Which choice is correct?",
  options: [
    { id: "right", text: "Right", correct: true },
    { id: "wrong", text: "Wrong", correct: false },
  ],
};

suite("Learning notebook quiz activities", () => {
  test("keep separate question identities while sharing one notebook cell", () => {
    const second = {
      ...quiz,
      payloadId: "second-question",
      prompt: "A second question",
    };
    const activities = parseNotebookActivities(
      JSON.stringify({
        cells: [
          { id: "heading", cell_type: "markdown", source: "## Section" },
          {
            id: "quiz-cell",
            cell_type: "code",
            metadata: { tags: ["quiz"] },
            outputs: [quiz, second].map((payload) => ({
              output_type: "display_data",
              data: { [MIME_TYPE]: payload },
            })),
          },
          { id: "next-code", cell_type: "code", source: "value = 1" },
        ],
      }),
      "test-unit",
    );

    assert.deepEqual(
      activities.map(({ id, type, title }) => ({ id, type, title })),
      [
        { id: "quiz:first-question", type: "quiz", title: quiz.prompt },
        { id: "quiz:second-question", type: "quiz", title: second.prompt },
        { id: "next-code", type: "code-cell", title: "Section" },
      ],
    );
    const [first, nextQuestion] = activities;
    assert(first.type === "quiz" && nextQuestion.type === "quiz");
    assert.equal(first.cellId, "quiz-cell");
    assert.equal(nextQuestion.cellId, "quiz-cell");
  });

  test("grade exact answer sets without accepting unknown or duplicate choices", () => {
    assert.isTrue(isCorrectQuizAnswer(quiz.options, ["right"]));
    for (const selection of [
      [],
      ["wrong"],
      ["right", "unknown"],
      ["right", "right"],
    ]) {
      assert.isFalse(isCorrectQuizAnswer(quiz.options, selection));
    }

    const multiple = [
      ...quiz.options,
      { id: "also-right", text: "Also right", correct: true },
    ];
    assert.isFalse(isCorrectQuizAnswer(multiple, ["right"]));
    assert.isFalse(
      isCorrectQuizAnswer(multiple, ["right", "also-right", "wrong"]),
    );
    assert.isTrue(isCorrectQuizAnswer(multiple, ["also-right", "right"]));
  });

  test("only accept bounded checked-answer messages with distinct option IDs", () => {
    const message = {
      type: "qdk-learning/quiz-answered",
      rendererId: RENDERER_ID,
      quizId: "first-question",
      optionIds: ["right"],
    };
    assert.isTrue(isRendererToExtensionMessage(message));
    assert.isFalse(
      isRendererToExtensionMessage({ ...message, quizId: undefined }),
    );
    assert.isFalse(isRendererToExtensionMessage({ ...message, optionIds: [] }));
    assert.isFalse(
      isRendererToExtensionMessage({
        ...message,
        optionIds: ["right", "right"],
      }),
    );
    assert.isFalse(
      isRendererToExtensionMessage({ ...message, optionIds: ["not an id"] }),
    );
    assert.isFalse(
      isRendererToExtensionMessage({ ...message, quizId: "first-question\n" }),
    );
  });

  test("reject malformed saved grading data before it enters the catalog", () => {
    assert.throws(
      () =>
        parseLearningPayload({
          ...quiz,
          options: [{ id: "right", text: "Right", correct: "false" }],
        }),
      /does not say whether it is correct/,
    );
  });
});
