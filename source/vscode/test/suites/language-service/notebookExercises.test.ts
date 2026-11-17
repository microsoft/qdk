// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { assert } from "chai";
import {
  parseNotebookActivities,
  stripAuthoringCells,
} from "../../../src/learning/notebookExercises.js";

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
