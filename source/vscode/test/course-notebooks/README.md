# Course notebook tests

These tests execute the source notebooks for one QDK learning course in fresh
Python kernels. The notebooks run from one shared copy of the full course
directory so relative imports work, generated files do not modify the source
tree, and later notebooks see filesystem state produced by earlier notebooks.
The suite also verifies that each notebook corresponds one-to-one with a unit
directory listed in the course's `course.json`.

Pytest creates a `.venv` inside the temporary course copy and installs the
course requirements there. Notebook kernels use that environment, while pytest
continues to use the component's test environment.

## Local setup

Run the course suite from the repository root with Python 3.11 or later:

```shell
python ./build.py --no-check --no-check-prereqs --course-notebook-tests
```

Like `--integration-tests`, `--course-notebook-tests` runs independently of
the regular `--test`/`--no-test` option.

Following the other Python test suites, `build.py` uses an active Python
environment when available. Otherwise, it creates
`source/vscode/test/course-notebooks/.venv` and installs the test requirements
there.

Run only the fast runner policy tests without creating a course environment:

```shell
source/vscode/test/course-notebooks/.venv/bin/python -m pytest source/vscode/test/course-notebooks/test_notebook_runner.py -v
```

## Cell metadata

- An `exercise` code cell must raise `ExerciseError`. Any other exception, or
  successful execution, fails the test.
- A `solution` code cell must execute without an error.
- A `skip-test` code cell is skipped only by this test suite. VS Code, Jupyter,
  and ordinary notebook execution do not interpret this custom tag. It also
  takes precedence when combined with `exercise`.

When a skipped setup cell supplies state to later cells, tag those dependent
cells with `skip-test` as well.

Each cell has a 120-second timeout. Cells taking longer than 30 seconds are
reported so expensive cells can be reviewed before adding a skip.
