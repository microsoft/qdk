# Authoring a QDK Learning course

A QDK Learning course is a collection of Jupyter notebooks arranged in a particular folder structure with a few support files.

The structure described below is what we've implemented in our prototype but can easily be updated based on feedback.

For a concrete example, see the
[sample course](source/vscode/test/suites/learning/test-workspace/qdk-learning/courses/circuit-diagrams-new).

## Structure

```
qdk-learning/courses/
└── circuit-diagrams-new/    
    ├── course.json          course metadata
    ├── README.md            course landing page
    ├── requirements.txt     python requirements for the notebooks
    ├── _course_lib.py       utilities for validating exercises
    ├── _check_env.py        utilities for checking the learner's setup (provided)
    ├── 01-intro/
    │   ├── _unit.py         unit-specific helpers (mostly exercise registration)
    │   └── intro.ipynb      unit content (author copy)
    └── 02-circuits/
        ├── _unit.py
        └── circuits.ipynb
```

_Note_: The folders in this example are numbered, but that's not required - the order comes from `course.json`

## course.json

```json
{
  "schemaVersion": 1,
  "id": "circuit-diagrams",
  "title": "Generating Circuit Diagrams",
  "shortDescription": "Build and visualize quantum circuits with the QDK in Python notebooks.",
  "units": [
    { "id": "intro", "title": "Getting Started", "dir": "01-intro" },
    { "id": "circuits", "title": "Circuit Diagrams", "dir": "02-circuits" }
  ]
}
```

- `schemaVersion`: set to `1`.
- `id`: A unique identifier for your course.  No whitespace.
- `title`: Display name of the course.
- `shortDescription`: Simple help/alt text about the course.
- `units`: An ordered list of units and where to find each.

## Exercises

Exercises are one of the main value-adds of the QDK Learning courses.
You can tag individual code cells as exercises and associate hints, solutions, and explanations with them.
Each exercise also has associated validation logic to tell a learner whether their solution is correct and course progress is updated as they complete exercises successfully.

An exercise code cell is preceded by a markdown cell giving the name of the exercise (as a header) and a brief description of what's to be accomplished.
The exercise code cell itself is tagged `exercise` using the `Add Cell Tag` functionality in VS Code (or by editing the JSON directly).

<div align="center">
  <img src="media/add-cell-tag.png" alt="Adding a cell tag from the notebook cell context menu">
</div>

The exercise code cell may optionally be followed by additional cells tagged `hint`, `solution`, or `explanation`.
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

The validation function should raise an exception if the solution is incorrect so that running the cell fails.
Once the cell runs successfully, the exercise will be considered to be complete.

## Environment

Setting up a Python environment can be tricky for new users and we want the focus to be on the course content, so we've added some helper functionality to both prepare and validate the environment.

- `requirements.txt` lists course dependencies and will be installed in a per-course virtual environment
- `course.json` lets you list imports you expect to work so they can be checked before the student starts the unit (e.g. in case they've selected the wrong notebook kernel)
- The course infrastructure depends on the Python and Jupyter VS Code extensions, so they'll be prompted if those are absent

## Trying it out

Open the `qdk-learning` folder in VS Code and navigate to the Microsoft Quantum extension panel (indicated by a moebius strip).
If you've never used it before, it'll offer you a `Start Learning` button.
The Microsoft Quantum Katas are the default course, so you'll need to explicitly Switch Course to your new content in the tree view.

When you switch to your course, temporary working copies of all the notebooks will be created (indicated by the `.workbook.ipynb` file extension).
These copies omit all the exercise hints, solutions, and explanations and give the learner a notebook they can edit freely without worrying about overwriting anything important.
An existing working copy is never overwritten, so if you based your course on a sample you'd previously run, `git clean` it first to clear out any working copies left behind.

There are buttons and context menu items throughout the UI that connect the experience to the Copilot chat.
Exercises, in particular, offer hints and explanations.
Copilot's responses will be informed by the hints, solutions, and exercises provided in the original notebook.
