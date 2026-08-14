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

| Script               | What it does                                                                                                           |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `rst_to_notebook.py` | Converts one tutorial chapter to a unit notebook. `RECIPES` holds the per-chapter decisions a human still has to make. |
| `bake_outputs.py`    | Runs a notebook so its outputs ship with the course.                                                                   |
| `verify_course.py`   | Checks every unit loads, validates, and carries what the tree needs.                                                   |
