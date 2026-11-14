from collections.abc import Iterable

import nbformat
from nbformat import NotebookNode

from notebook_runner import collect_cell_failures


def _notebook(*cells: NotebookNode) -> NotebookNode:
    return nbformat.v4.new_notebook(cells=list(cells))


def _code_cell(
    *,
    tags: Iterable[str] = (),
) -> NotebookNode:
    return nbformat.v4.new_code_cell(
        "answer = 42",
        metadata={"tags": list(tags)},
    )


def _execution_error(name: str, value: str) -> NotebookNode:
    return NotebookNode(
        {
            "ename": name,
            "evalue": value,
            "traceback": [],
        }
    )


def test_hidden_exercise_error_satisfies_policy() -> None:
    cell = _code_cell(tags=["exercise"])
    cell.outputs = [
        nbformat.v4.new_output(
            "display_data",
            data={"text/html": "<strong>try again</strong>"},
            metadata={},
        )
    ]
    notebook = _notebook(cell)

    failures = collect_cell_failures(
        notebook,
        {0: _execution_error("ExerciseError", "try again")},
    )

    assert failures == []
    assert [output.output_type for output in cell.outputs] == ["display_data"]


def test_displayed_exercise_error_fails_policy() -> None:
    cell = _code_cell(tags=["exercise"])
    cell.outputs = [
        nbformat.v4.new_output(
            "display_data",
            data={"text/html": "<strong>try again</strong>"},
            metadata={},
        ),
        nbformat.v4.new_output(
            "error",
            ename="ExerciseError",
            evalue="<strong>try again</strong>",
            traceback=[],
        ),
    ]
    notebook = _notebook(cell)

    failures = collect_cell_failures(
        notebook,
        {0: _execution_error("ExerciseError", "try again")},
    )

    assert len(failures) == 1
    assert failures[0].message == "exercise cell displayed duplicate error output"


def test_exercise_that_succeeds_fails_policy() -> None:
    notebook = _notebook(_code_cell(tags=["exercise"]))

    failures = collect_cell_failures(notebook, {})

    assert len(failures) == 1
    assert failures[0].message == "exercise cell did not raise ExerciseError"


def test_exercise_with_wrong_error_fails_policy() -> None:
    notebook = _notebook(_code_cell(tags=["exercise"]))

    failures = collect_cell_failures(
        notebook,
        {0: _execution_error("ValueError", "bad value")},
    )

    assert len(failures) == 1
    assert failures[0].message == "exercise cell raised ValueError: bad value"


def test_ordinary_cell_error_fails_policy() -> None:
    notebook = _notebook(_code_cell())

    failures = collect_cell_failures(
        notebook,
        {0: _execution_error("RuntimeError", "broken")},
    )

    assert len(failures) == 1
    assert failures[0].message == "unexpected error: RuntimeError: broken"


def test_skip_test_cell_is_not_evaluated() -> None:
    notebook = _notebook(_code_cell(tags=["skip-test"]))

    failures = collect_cell_failures(
        notebook,
        {0: _execution_error("RuntimeError", "ignored")},
    )

    assert failures == []


def test_skipped_exercise_is_not_evaluated() -> None:
    notebook = _notebook(_code_cell(tags=["exercise", "skip-test"]))

    assert collect_cell_failures(notebook, {}) == []
