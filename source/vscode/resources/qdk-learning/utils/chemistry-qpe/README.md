# Chemistry course tools

Scripts used to generate the QDK/Chemistry ground-state QPE tutorial as the
notebook course at `source/vscode/resources/qdk-learning/courses/chemistry-qpe`.

These are authoring tools, not product code, and are excluded from the extension
package by `.vscodeignore`. They locate the course relative to this folder.

They also need a built copy of the qdk-chemistry documentation, because the
notebooks are generated from the tutorial's own source rather than from its
rendered pages:

    <docs>/_sources/tutorials/...rst.txt   the tutorial source text
    <docs>/_static/examples/python/*.py    the example scripts the cells quote

Both are published as part of the Sphinx site at
<https://microsoft.github.io/qdk-chemistry/>, so a local build or a copy of the
published site works. The default location is an `html` directory beside this
repo; pass `--docs` to point somewhere else, and `--course` to write elsewhere.

Image assets stored beside `rst_to_notebook.py` are used in place of the ones
from the built documentation, and the converter embeds them in the notebooks.
Diagrams are SVG, embedded inline in the Markdown so they follow the active
theme. `tutorial_qpe_atomic_basis_functions.png` and
`tutorial_qpe_example_molecular_orbitals.png` stay PNG attachments. Every
figure needs an `:alt:`, which becomes the SVG's accessible name.

Each notebook ends with links to the neighbouring units. Unit order and titles
come from `course.json` and the notebook names from `RECIPES`, so the two must
list the same units; the converter stops if they disagree. The links point at
the learner's `*.workbook.ipynb` copies, which the extension materializes beside
the authored notebooks, so they only resolve inside a learner's workspace.

| Script               | What it does                                                                                                                                  |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `rst_to_notebook.py` | Converts one tutorial chapter to a unit notebook. `RECIPES` holds the per-chapter decisions a human still has to make.                        |
| `details_to_quiz.py` | Bakes a chapter's self-check questions into answerable quizzes. The quiz counterpart of `bake_outputs.py`; run it after `rst_to_notebook.py`. |
| `bake_outputs.py`    | Runs a notebook so its outputs ship with the course.                                                                                          |
| `verify_course.py`   | Checks every unit loads, validates, and carries what the tree needs. Pass `--allow-outputs` when reviewing baked notebooks.                   |

## Self-check questions

`details_to_quiz.py` bakes a chapter's self-check questions into the notebook,
so a learner sees them on opening the file and can answer without a kernel. It
is the quiz counterpart of `bake_outputs.py`, but a quiz is pure data, so it
calls the emitter directly instead of starting a kernel.

The first run on a chapter also creates the cells to bake. A chapter's
`quiz-question` admonitions arrive as `<details>` disclosures whose only
interaction is revealing the answer, and those blocks are replaced with
`quiz("id")` calls. Afterwards every run is only a rebake.

The choices themselves live in the unit's `_unit.py`, registered by id, so the
notebook cell a learner reads is just `quiz("id")` rather than the answer key.
Write them by hand: which wrong answers are worth offering is a judgement about
what a learner is likely to believe, and the converter never touches `_unit.py`.
The answers do travel to the browser inside the baked cell output, because the
renderer grades without a kernel — this raises the effort of looking them up
rather than making it impossible, and it is no weaker than the collapsible
answers it replaces.

Regenerating a chapter puts the `<details>` back, so re-run this afterwards.
Both steps are idempotent and safe to run either way:

    python rst_to_notebook.py 06_iterative_phase_estimation
    python details_to_quiz.py 06-iterative-phase-estimation --ids <ids in document order>
    python verify_course.py

After that first conversion the notebook names its own ids, so rebaking edited
questions, or checking for drift against `_unit.py`, is just:

    python details_to_quiz.py 06-iterative-phase-estimation
    python details_to_quiz.py 06-iterative-phase-estimation --check
