# Course notebook tests

These tests execute the source notebooks for one QDK learning course in fresh
Python kernels. Each notebook runs from an isolated copy of its full course
directory so relative imports work and generated files do not modify the source
tree.

## Local setup

Create a Python 3.11 environment and install the test harness plus the selected
course's dependencies:

```shell
python3.11 -m venv .venv-course-notebooks
source .venv-course-notebooks/bin/activate
python -m pip install -r source/vscode/test/course-notebooks/requirements.txt
python -m pip install -r source/vscode/resources/qdk-learning/courses/chemistry-qpe/requirements.txt
```

Run the course suite from the repository root:

```shell
python -m pytest source/vscode/test/course-notebooks -v -s --course chemistry-qpe
```

Run only the fast runner policy tests without installing course dependencies:

```shell
python -m pytest source/vscode/test/course-notebooks/test_notebook_runner.py -v
```

## Cell metadata

- An `exercise` code cell must raise `ExerciseError`. Any other exception, or
  successful execution, fails the test.
- A `solution` code cell must execute without an error.
- A `skip-test` code cell is skipped only by this test suite. VS Code, Jupyter,
  and ordinary notebook execution do not interpret this custom tag.

Do not combine `exercise` and `skip-test`. When a skipped setup cell supplies
state to later cells, tag those dependent cells with `skip-test` as well.

Each cell has a 120-second timeout. Cells taking longer than 30 seconds are
reported so expensive cells can be reviewed before adding a skip.
