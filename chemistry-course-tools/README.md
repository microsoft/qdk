# Chemistry course tools

Scripts used to port the QDK/Chemistry ground-state QPE tutorial into the
notebook course at
`source/vscode/test/suites/learning/test-workspace/qdk-learning/courses/chemistry-active-space`.

These are authoring tools, not product code, and are not packaged with the extension.
Paths at the top of each script point at a local docs build and a local course
checkout, so they need editing before they run anywhere else.

| Script               | What it does                                                                                                           |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `rst_to_notebook.py` | Converts one tutorial chapter to a unit notebook. `RECIPES` holds the per-chapter decisions a human still has to make. |
| `bake_outputs.py`    | Runs a notebook so its outputs ship with the course.                                                                   |
| `verify_course.py`   | Checks every unit loads, validates, and carries what the tree needs.                                                   |
