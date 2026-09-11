import nbformat

from notebook_runner import collect_cell_failures


def _notebook(*cells):
    return nbformat.v4.new_notebook(cells=list(cells))


def _code_cell(*, tags=(), error=None):
    cell = nbformat.v4.new_code_cell("answer = 42", metadata={"tags": list(tags)})
    if error is not None:
        name, value = error
        cell.outputs = [
            nbformat.v4.new_output(
                "error",
                ename=name,
                evalue=value,
                traceback=[],
            )
        ]
    return cell


def test_exercise_requires_exercise_error():
    notebook = _notebook(_code_cell(tags=["exercise"], error=("ExerciseError", "try again")))

    assert collect_cell_failures(notebook) == []


def test_exercise_that_succeeds_fails_policy():
    notebook = _notebook(_code_cell(tags=["exercise"]))

    failures = collect_cell_failures(notebook)

    assert len(failures) == 1
    assert failures[0].message == "exercise cell did not raise ExerciseError"


def test_exercise_with_wrong_error_fails_policy():
    notebook = _notebook(_code_cell(tags=["exercise"], error=("ValueError", "bad value")))

    failures = collect_cell_failures(notebook)

    assert len(failures) == 1
    assert failures[0].message == "exercise cell raised ValueError: bad value"


def test_ordinary_cell_error_fails_policy():
    notebook = _notebook(_code_cell(error=("RuntimeError", "broken")))

    failures = collect_cell_failures(notebook)

    assert len(failures) == 1
    assert failures[0].message == "unexpected error: RuntimeError: broken"


def test_skip_test_cell_is_not_evaluated():
    notebook = _notebook(_code_cell(tags=["skip-test"], error=("RuntimeError", "ignored")))

    assert collect_cell_failures(notebook) == []


def test_exercise_cannot_be_skipped():
    notebook = _notebook(_code_cell(tags=["exercise", "skip-test"]))

    failures = collect_cell_failures(notebook)

    assert len(failures) == 1
    assert failures[0].message == "a cell cannot have both 'exercise' and 'skip-test'"