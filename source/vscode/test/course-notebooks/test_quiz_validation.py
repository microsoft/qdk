import json
import runpy
from pathlib import Path

import pytest

from notebook_runner import discover_notebooks

_helper = (
    Path(__file__).resolve().parents[2]
    / "resources/qdk-learning/courses/chemistry-qpe/_learning_output.py"
)
_helpers = runpy.run_path(str(_helper))
validate_quizzes = _helpers["validate_quizzes"]
validate_cell_roles = _helpers["validate_cell_roles"]
MIME_TYPE = _helpers["MIME_TYPE"]
_baker = _helper.parents[2] / "utils" / "chemistry-qpe" / "details_to_quiz.py"
bake_quizzes = runpy.run_path(str(_baker))["bake_quizzes"]


def _quiz_cell(*, tags: tuple[str, ...] = ("quiz",)) -> dict:
    return {
        "id": "question-cell",
        "cell_type": "code",
        "metadata": {"tags": list(tags)},
        "source": (
            'quiz("example", prompt="Pick one", options=['
            '("yes", "Yes", True, "Right"), ("no", "No", False, "Wrong")])'
        ),
        "outputs": [],
    }


def _write_notebook(tmp_path: Path, *cells: dict) -> Path:
    path = tmp_path / "author.ipynb"
    path.write_text(json.dumps({"cells": list(cells)}), encoding="utf-8")
    return path


@pytest.mark.parametrize(
    ("field", "malformed_value"),
    [("prompt", ["Pick one"]), ("schemaVersion", True)],
)
def test_baker_repairs_payload_types_without_rewriting_text_fallbacks(
    field: str, malformed_value: list[str] | bool
) -> None:
    cell = _quiz_cell()
    notebook = {"cells": [cell]}
    assert bake_quizzes(notebook) == ["question-cell"]
    bundle = cell["outputs"][0]["data"]
    expected = bundle[MIME_TYPE].copy()
    for mime in ("text/plain", "text/html"):
        bundle[mime] = bundle[mime].splitlines(keepends=True)
    assert bake_quizzes(notebook) == []

    bundle[MIME_TYPE][field] = malformed_value
    assert bake_quizzes(notebook) == ["question-cell"]
    payload = cell["outputs"][0]["data"][MIME_TYPE]
    assert payload == expected
    assert type(payload[field]) is type(expected[field])
    assert bake_quizzes(notebook) == []


def test_validation_reads_source_without_requiring_or_writing_outputs(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    note = {
        "id": "note",
        "cell_type": "markdown",
        "source": 'Example: quiz("not-a-definition")',
    }
    example = {
        "id": "example",
        "cell_type": "code",
        "source": '# quiz("comment")\nexample = \'quiz("string")\'',
    }
    notebook = _write_notebook(tmp_path, note, example, _quiz_cell())
    before = notebook.read_bytes()

    validate_quizzes(notebook)

    assert capsys.readouterr().out == "Validated 1 quiz in author.ipynb.\n"
    assert notebook.read_bytes() == before


@pytest.mark.parametrize(
    ("tags", "kind", "message"),
    [
        ((), "code", "add the 'quiz' tag to this code cell"),
        (("quiz",), "markdown", "the 'quiz' tag belongs on a code cell"),
    ],
)
def test_validation_rejects_misclassified_quiz_cells(
    tmp_path: Path, tags: tuple[str, ...], kind: str, message: str
) -> None:
    cell = _quiz_cell(tags=tags)
    cell["cell_type"] = kind
    notebook = _write_notebook(tmp_path, cell)

    with pytest.raises(ValueError) as error:
        validate_quizzes(notebook)

    assert "cell 1" in str(error.value).lower()
    assert "question-cell" not in str(error.value)
    if kind == "code":
        assert "Quiz 'example'" in str(error.value)
    assert message in str(error.value)


@pytest.mark.parametrize(
    ("prompt", "message"),
    [
        ('" "', "requires question text"),
        ("None", "requires question text"),
        ("123", "requires question text"),
        ("question_text", "prompt must be literal data"),
    ],
)
def test_validation_identifies_the_cell_for_bad_definitions(
    tmp_path: Path, prompt: str, message: str
) -> None:
    cell = _quiz_cell()
    cell["source"] = cell["source"].replace('prompt="Pick one"', f"prompt={prompt}")
    notebook = _write_notebook(
        tmp_path, {"cell_type": "markdown", "source": "Introduction"}, cell
    )

    with pytest.raises(ValueError) as error:
        validate_quizzes(notebook)

    assert "Quiz 'example' (cell 2)" in str(error.value)
    assert "question-cell" not in str(error.value)
    assert message in str(error.value)


def test_validation_rejects_duplicate_quiz_ids_across_cells(tmp_path: Path) -> None:
    duplicate = _quiz_cell()
    duplicate["id"] = "duplicate-cell"
    notebook = _write_notebook(tmp_path, _quiz_cell(), duplicate)

    with pytest.raises(ValueError, match="Quiz 'example' .*duplicate quiz ID") as error:
        validate_quizzes(notebook)

    assert "(cell 2)" in str(error.value)
    assert "duplicate-cell" not in str(error.value)


@pytest.mark.parametrize(
    "source", ["quiz(", 'quiz(question_id, prompt="Pick one", options=[])']
)
def test_unidentifiable_quizzes_use_the_notebook_cell_number(
    tmp_path: Path, source: str
) -> None:
    cell = _quiz_cell()
    cell["source"] = source
    notebook = _write_notebook(tmp_path, cell)

    with pytest.raises(ValueError, match="Cell 1:") as error:
        validate_quizzes(notebook)

    assert "question-cell" not in str(error.value)


def test_multiple_quizzes_must_use_separate_cells(tmp_path: Path) -> None:
    cell = _quiz_cell()
    bad_source = cell["source"].replace('"example"', '"second-question"').replace(
        'prompt="Pick one"', 'prompt=" "'
    )
    cell["source"] += "\n" + bad_source
    notebook = _write_notebook(tmp_path, cell)

    with pytest.raises(ValueError, match="exactly one quiz") as error:
        validate_quizzes(notebook)

    assert "Cell 1" in str(error.value)


def test_untagged_invalid_python_reports_the_cell_number(tmp_path: Path) -> None:
    cell = _quiz_cell(tags=())
    cell["source"] = "quiz("
    notebook = _write_notebook(tmp_path, cell)

    with pytest.raises(
        ValueError, match="Cell 1: cannot check quiz calls in invalid Python"
    ):
        validate_quizzes(notebook)


def test_keyword_unpacking_explains_the_supported_authoring_form(tmp_path: Path) -> None:
    cell = _quiz_cell()
    cell["source"] = 'quiz("example", **kwargs)'
    notebook = _write_notebook(tmp_path, cell)

    with pytest.raises(ValueError) as error:
        validate_quizzes(notebook)

    assert str(error.value) == (
        "Quiz 'example' (cell 1): **kwargs are not supported; "
        "write quiz arguments explicitly"
    )


@pytest.mark.parametrize(
    "other_role", ["exercise", "hint", "solution", "explanation", "author-only"]
)
def test_quiz_role_conflicts_are_reported_before_source_errors(
    tmp_path: Path, other_role: str
) -> None:
    cell = _quiz_cell(tags=("quiz", other_role))
    cell["source"] = "quiz("
    notebook = _write_notebook(tmp_path, cell)

    with pytest.raises(ValueError) as error:
        validate_quizzes(notebook)

    message = str(error.value)
    assert message.startswith("Cell 1: conflicting content roles")
    assert "'quiz'" in message and repr(other_role) in message


def test_exercise_role_conflicts_are_rejected() -> None:
    cell = {"metadata": {"tags": ["exercise", "solution"]}}

    with pytest.raises(ValueError, match="Cell 2: conflicting content roles"):
        validate_cell_roles(cell, cell_number=2)


def test_modifier_tags_do_not_conflict_with_cell_roles() -> None:
    for role in ("exercise", "quiz"):
        cell = {"metadata": {"tags": [role, "skip-test", "section:example"]}}
        validate_cell_roles(cell, cell_number=1)


def test_course_cells_have_no_conflicting_roles(course_dir: Path) -> None:
    for path in discover_notebooks(course_dir):
        notebook = json.loads(path.read_text(encoding="utf-8"))
        for cell_number, cell in enumerate(notebook["cells"], start=1):
            try:
                validate_cell_roles(cell, cell_number=cell_number)
            except ValueError as error:
                pytest.fail(f"{path}: {error}")


@pytest.mark.parametrize(
    "spelling",
    [r'r"example"', r'"ex" "ample"', r'"\u0065xample"', '"""example"""', '("example")'],
)
def test_validation_rejects_id_spellings_the_header_reader_does_not_support(
    tmp_path: Path, spelling: str,
) -> None:
    cell = _quiz_cell()
    cell["source"] = cell["source"].replace('"example"', spelling, 1)
    with pytest.raises(ValueError, match="plain quoted string"):
        validate_quizzes(_write_notebook(tmp_path, cell))


def test_validation_accepts_the_documented_multiline_header(tmp_path: Path) -> None:
    cell = _quiz_cell()
    cell["source"] = (
        '# quiz("not-the-id", ...)\n'
        'quiz(\n# ID comment\n"example"\n# separator comment\n,'
        + cell["source"].split(",", 1)[1]
    )
    validate_quizzes(_write_notebook(tmp_path, cell))
