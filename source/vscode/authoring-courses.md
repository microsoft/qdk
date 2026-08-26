# Authoring a QDK Learning course

A QDK Learning course is a collection of Jupyter notebooks arranged in a particular folder structure with a few support files.

The structure described below is what we've implemented in our prototype but can easily be updated based on feedback.

For a concrete example, see the
[Chemistry QPE course](resources/qdk-learning/courses/chemistry-qpe).

## Structure

```
resources/qdk-learning/courses/
└── chemistry-qpe/           folder name is the course ID
    ├── course.json          course metadata
    ├── requirements.txt     python requirements for the notebooks
    ├── _course_lib.py       utilities for validating exercises
    ├── _check_env.py        checks the learner's setup against `environment.importChecks`
    ├── 00-overview/
    │   ├── _unit.py         unit-specific helpers (mostly exercise registration)
    │   └── overview.ipynb   unit content (author copy)
    └── 01-energy-and-accuracy/
        ├── _unit.py
        └── energy_and_accuracy.ipynb
```

Bundled courses live under the extension's `resources` folder.
A course can also be authored in a workspace under `qdk-learning/courses/`, which is where the extension copies bundled courses the first time a learner opens one.

_Note_: The folders in this example are numbered, but that's not required - the order comes from `course.json`

## course.json

```json
{
  "schemaVersion": 1,
  "title": "Ground-State Molecular Energies with QPE",
  "shortDescription": "Estimate a molecule's ground-state energy using quantum phase estimation.",
  "units": [
    { "id": "overview", "title": "Tutorial Overview", "dir": "00-overview" },
    {
      "id": "energy-and-accuracy",
      "title": "Energy and Accuracy",
      "dir": "01-energy-and-accuracy"
    }
  ],
  "environment": {
    "importChecks": ["qdk_chemistry", "pyscf"]
  }
}
```

- `schemaVersion`: set to `1`.
- `title`: Display name of the course.
- `shortDescription`: Simple help/alt text about the course.
- `units`: An ordered list of units and where to find each.
- `environment.importChecks`: Optional. Imports the course's `_check_env.py` verifies before a unit starts.

The course ID comes from the folder name, so there is no `id` field.

## Exercises

Exercises are one of the main value-adds of the QDK Learning courses.
You can tag individual code cells as exercises and associate hints, solutions, and explanations with them.
Each exercise also has associated validation logic to tell a learner whether their solution is correct and course progress is updated as they complete exercises successfully.

An exercise code cell is preceded by a markdown cell giving the name of the exercise (as a header) and a brief description of what's to be accomplished.
The exercise code cell itself is tagged `exercise` using the `Add Cell Tag` functionality in VS Code (or by editing the JSON directly).

<div align="center">
  <img src="../../media/add-cell-tag.png" alt="Adding a cell tag from the notebook cell context menu">
</div>

The exercise code cell may optionally be followed by additional cells tagged `hint`, `solution`, or `explanation`.
`solution` goes on a code cell; `hint` and `explanation` go on markdown cells.
A tag on the wrong kind of cell is ignored with only a log warning, so the cell simply never reaches Copilot.
The contents of these cells will be available to the Copilot Agent guiding the learner, but not to the learner themselves.

## Exercise Validation

An exercise will generally take the form of an empty function with an `@exercise` decorator.
It will be the learner's job to fill in the function body.

```python
from _unit import exercise


@exercise
def forty_two():
    # ========================================================================
    # YOUR TASK: change the expression below so forty_two() returns 42.
    # ========================================================================
    return qsharp.eval("0")  # <-- edit this expression
```

In `_unit.py`, each exercise in the unit is "registered" by name, along with an associated validation function.
(There are some pre-rolled validation functions for things like asserting the output has a particular scalar value.)
When the cell is executed, the decorator will look up the validation function, run the student's implementation, and apply the validation logic.

```python
import sys
from pathlib import Path

_course_root = str(Path(__file__).resolve().parent.parent)
if _course_root not in sys.path:
    sys.path.insert(0, _course_root)

from _check_env import check as check_env  # noqa: E402, F401
from _course_lib import exercise, register_value_exercise  # noqa: E402, F401

ANSWER = register_value_exercise("forty_two", expected=42)
```

The `sys.path` preamble is what lets the notebook import `_course_lib.py` from the course root.
`register_value_exercise` covers the common case of checking the return value.
Use `register_exercise(name, validate, ...)` when a unit needs its own checking; `validate` returns an error message when the answer is wrong and `None` when it's right.

The validation function should raise an exception if the solution is incorrect so that running the cell fails.
Once the cell runs successfully, the exercise will be considered to be complete.

## Editing a published course

Progress is tracked per cell, using the notebook's nbformat cell IDs.
Those IDs are opaque to the learning system, so editing a cell's text is safe, but deleting a cell and adding a replacement gives it a new ID and the old completion no longer matches it.

Existing learners are insulated from this: a `*.workbook.ipynb` is never overwritten once it exists, so they keep working in the copy they already have.
The change reaches them only when they reset the unit, and it reaches anyone starting the course after the update.

Images are worth a note too.
An attachment is rendered as an `<img>`, which cannot read the notebook's stylesheet, so a light-background diagram stays light in a dark theme.
Putting the SVG markup directly in a Markdown cell keeps it in the notebook's own DOM, where it can pick up `var(--vscode-*)` colours and follow the active theme.
Attachments that stay attachments must be base64-encoded.

## Environment

Setting up a Python environment can be tricky for new users and we want the focus to be on the course content, so we've added some helper functionality around installing and validating dependencies.

- `requirements.txt` lists course dependencies; each unit's first cell has a commented-out `%pip install -r ../requirements.txt` for the learner to run
- The `QDK: Create a Microsoft Quantum Python virtual environment` command sets up an environment and prompts for the packages to install
- `course.json` lets you list imports you expect to work so they can be checked before the student starts the unit (e.g. in case they've selected the wrong notebook kernel), which the course's `_check_env.py` reads
- The course infrastructure depends on the Python and Jupyter VS Code extensions, so they'll be prompted if those are absent

## Trying it out

Open a workspace folder in VS Code and navigate to the Microsoft Quantum extension panel (indicated by a moebius strip).
If you've never used it before, it'll offer you a `Start Learning` button.
The Microsoft Quantum Katas are the default course, so you'll need to explicitly Switch Course to your new content in the tree view.

When you switch to your course, temporary working copies of all the notebooks will be created (indicated by the `.workbook.ipynb` file extension).
These copies omit all the exercise hints, solutions, and explanations and give the learner a notebook they can edit freely without worrying about overwriting anything important.
An existing working copy is never overwritten, so if you based your course on a sample you'd previously run, `git clean` it first to clear out any working copies left behind.

There are buttons and context menu items throughout the UI that connect the experience to the Copilot chat.
Exercises, in particular, offer hints and explanations.
Copilot's responses will be informed by the hints, solutions, and exercises provided in the original notebook.
