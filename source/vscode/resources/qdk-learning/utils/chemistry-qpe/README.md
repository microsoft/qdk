# Chemistry course tools

Scripts used to generate the QDK/Chemistry ground-state QPE tutorial as the
notebook course at `source/vscode/resources/qdk-learning/courses/chemistry-qpe`.

The course supports `qdk-chemistry>=2.2.0`. Its source tutorial and
companion Python files come from QDK/Chemistry tag `v2.2.0`, commit
`f28a4f87a0fc0aa0308b67b1f507c9692cc655df`.

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

`rst_to_notebook.py` reconstructs each selected notebook and replaces its file;
it does not merge hand edits or retain outputs. When the generated cell sequence
and tags are unchanged, it preserves the existing cell IDs so learner progress
continues to refer to the same activities. It refuses a structural change unless
`--allow-cell-id-changes` is passed after reviewing the effect on learner
progress.

Use `--check` before write mode. It builds every selected notebook in memory,
reports changed fields by one-based cell number, and does not write files:

    python rst_to_notebook.py --docs <docs> --check

For review, generate into a temporary course directory containing a copy of
`course.json`, then compare the result before replacing the tracked notebooks:

    python rst_to_notebook.py --docs <docs> --course <temporary-course>
    python verify_course.py <temporary-course>

After intentional recipe changes, run the converter in write mode and then use
`bake_outputs.py` only for notebooks whose outputs should ship with the course.

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

| Script               | What it does                                                                                                                                        |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `rst_to_notebook.py` | Reconstructs selected unit notebooks. `RECIPES` holds the per-chapter decisions a human still has to make; `--check` detects drift without writing. |
| `details_to_quiz.py` | Refreshes saved quiz outputs from the author notebook's literal definitions; `--check` reports drift without writing.                               |
| `bake_outputs.py`    | Runs a notebook so its outputs ship with the course.                                                                                                |
| `verify_course.py`   | Checks every unit loads, validates, and carries what the tree needs. Pass `--allow-outputs` when reviewing baked notebooks.                         |

## Self-check questions

Write each question's full Python definition in a dedicated author notebook
code cell tagged `quiz`, following the [authoring guide](../../../../authoring-courses.md).
The author notebook is the source of truth; `_unit.py` binds the helper to that
notebook once, without registering individual questions. Convert `<details>`
questions by hand so their placement, correct answers, and new distractors can
be reviewed for scientific accuracy.

Run the final `author-only` validation cell to check the saved definitions.
Validation does not bake outputs. Publishing first-open interaction is a
separate, explicit step:

    python details_to_quiz.py 01-energy-and-accuracy
    python details_to_quiz.py 01-energy-and-accuracy --check
    python verify_course.py

The baker calls the quiz emitter directly, without starting a kernel or executing
notebook cells. It refreshes only quiz outputs, preserving source and cell IDs.
`--check` reports stale outputs without writing. Materialization never invokes
the baker: it preserves available outputs and replaces definitions with named
calls such as `quiz("energy-total-components")`. Learner reruns resolve those
calls against the saved author source, not its outputs.

`rst_to_notebook.py` reconstructs notebooks from the upstream tutorial and does
not retain handwritten quiz definitions. Do not regenerate a converted chapter
merely to rebake it; preserve and reapply its reviewed definitions when updating
from upstream.

Answers travel to the browser in the saved MIME output because the renderer
grades without a kernel. This is self-paced learning, not a secure assessment.
