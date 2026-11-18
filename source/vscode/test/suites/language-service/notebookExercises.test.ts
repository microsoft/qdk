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
  test("discover one stable activity per quiz cell, independently of saved outputs", () => {
    const activities = parseNotebookActivities(
      JSON.stringify({
        cells: [
          { id: "heading", cell_type: "markdown", source: "## Section" },
          {
            id: "quiz-cell",
            cell_type: "code",
            metadata: { tags: ["quiz"] },
            source: 'quiz("first-question", prompt="Question", options=[])',
            outputs: [quiz].map((payload) => ({
              output_type: "display_data",
              data: { [MIME_TYPE]: payload },
            })),
          },
          {
            id: "unrun-quiz",
            cell_type: "code",
            metadata: { tags: ["quiz"] },
            source: 'quiz("second-question", prompt="Question", options=[])',
            outputs: [],
          },
          { id: "next-code", cell_type: "code", source: "value = 1" },
        ],
      }),
      "test-unit",
      6,
    );

    assert.deepEqual(
      activities.map(({ id, type, title }) => ({ id, type, title })),
      [
        { id: "quiz-cell", type: "quiz", title: "Quiz 6.1" },
        { id: "unrun-quiz", type: "quiz", title: "Quiz 6.2" },
        { id: "next-code", type: "code-cell", title: "Section" },
      ],
    );
  });

  test("materialize named callers without changing cell IDs or saved outputs", () => {
    for (const outputs of [
      [],
      [{ output_type: "display_data", data: { [MIME_TYPE]: quiz } }],
    ]) {
      const original = {
        id: "quiz-cell",
        cell_type: "code",
        metadata: { tags: ["quiz"] },
        source: ['quiz("first-question", prompt="Private answer definition")'],
        outputs,
      };
      const text = stripAuthoringCells(
        JSON.stringify({ cells: [original] }),
        "test-unit",
      );
      assert(text !== undefined);
      const learner: { cells: (typeof original)[] } = JSON.parse(text);
      assert.deepEqual(learner.cells[0], {
        ...original,
        source: ['quiz("first-question")\n'],
      });
    }
  });

  test("extract only the leading quoted name with multiline calls and comments", () => {
    for (const source of [
      'quiz("first-question", prompt="Question", options=[])',
      "quiz('first-question', prompt='Question', options=[])",
      '# quiz("false-name", ...)\nquiz(\n# ID comment\n"first-question"\n# separator comment\n, prompt="Question", options=[])',
      "quiz(\"first-question\", prompt='''Example:\nquiz(\"false-name\", ...)\n''', options=[])",
    ]) {
      for (const text of [source, source.split(/(?<=\n)/)]) {
        const notebook = JSON.stringify({
          cells: [
            {
              id: "stable-cell",
              cell_type: "code",
              metadata: { tags: ["quiz"] },
              source: text,
              outputs: [],
            },
          ],
        });
        assert.equal(
          parseNotebookActivities(notebook, "test-unit")[0].title,
          "Quiz 1.1",
        );
        const learner = stripAuthoringCells(notebook, "test-unit");
        assert(learner !== undefined);
        assert.include(learner, 'quiz(\\"first-question\\")');
      }
    }
  });

  test("reject unsupported names without falling back to an output payload", () => {
    for (const source of [
      '# quiz("false-name", ...)',
      '"""Example:\nquiz("false-name", ...)\n"""',
      'quiz(name, prompt="Question", options=[])',
      'quiz("first-" "question", prompt="Question", options=[])',
      'quiz(r"first-question", prompt="Question", options=[])',
      'quiz("""first-question""", prompt="Question", options=[])',
      'quiz("\\u0066irst-question", prompt="Question", options=[])',
      '_unit.quiz("first-question", prompt="Question", options=[])',
    ]) {
      const notebook = JSON.stringify({
        cells: [
          {
            id: "stable-cell",
            cell_type: "code",
            metadata: { tags: ["quiz"] },
            source,
            outputs: [
              { output_type: "display_data", data: { [MIME_TYPE]: quiz } },
            ],
          },
        ],
      });
      assert.deepEqual(parseNotebookActivities(notebook, "test-unit"), []);
      assert.throws(
        () => stripAuthoringCells(notebook, "test-unit"),
        /plain quoted ID/,
      );
    }
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
