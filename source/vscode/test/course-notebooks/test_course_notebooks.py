import ast
import json
from collections import Counter
from pathlib import Path
from textwrap import dedent

import nbformat

from notebook_runner import discover_notebooks, run_notebook


def test_discovered_notebooks_match_course_manifest(course_dir: Path) -> None:
    manifest = json.loads((course_dir / "course.json").read_text(encoding="utf-8"))
    manifest_unit_dirs = [Path(unit["dir"]) for unit in manifest["units"]]
    discovered_notebooks = discover_notebooks(course_dir)
    discovered_unit_dirs = [
        notebook.relative_to(course_dir).parent for notebook in discovered_notebooks
    ]

    manifest_counts = Counter(manifest_unit_dirs)
    discovered_counts = Counter(discovered_unit_dirs)
    missing = list((manifest_counts - discovered_counts).elements())
    unmatched = [
        notebook.relative_to(course_dir)
        for notebook in discovered_notebooks
        if discovered_counts[notebook.relative_to(course_dir).parent]
        > manifest_counts[notebook.relative_to(course_dir).parent]
    ]

    assert manifest_counts == discovered_counts, (
        f"{course_dir / 'course.json'} does not match discovered notebooks; "
        f"missing notebooks for units: {missing}; "
        f"unmatched notebooks: {unmatched}"
    )


def test_course_notebook(copied_course_notebook: tuple[Path, Path, Path]) -> None:
    notebook_path, display_path, kernel_specs_dir = copied_course_notebook

    report = run_notebook(notebook_path, display_path, kernel_specs_dir)

    assert not report.failures, report.format_failures()


def test_chemistry_quiz_named_callers(
    copied_course_dirs: dict[Path, Path],
) -> None:
    source_course = (
        Path(__file__).resolve().parents[2]
        / "resources"
        / "qdk-learning"
        / "courses"
        / "chemistry-qpe"
    )
    course_dir = copied_course_dirs[source_course]
    author = nbformat.v4.new_notebook(
        cells=[
            nbformat.v4.new_code_cell(
                'quiz("example", prompt="Pick one", options=['
                '("yes", "Yes", True, "Right"), ("no", "No", False, "Wrong")])',
                metadata={"tags": ["quiz"]},
            )
        ]
    )
    (course_dir / "quiz_source.ipynb").write_text(
        json.dumps(author), encoding="utf-8"
    )
    notebook = nbformat.v4.new_notebook(
        cells=[
            nbformat.v4.new_code_cell(dedent(source))
            for source in (
                """\
                import json
                from pathlib import Path
                from unittest import TestCase

                from IPython.utils.capture import capture_output
                from _learning_output import MIME_TYPE, quiz_for_notebook

                author = Path("quiz_source.ipynb").resolve()
                before = author.read_bytes()
                assert json.loads(before)["cells"][0]["outputs"] == []
                bound = quiz_for_notebook(author)
                """,
                """\
                with capture_output() as captured:
                    bound("example")

                assert len(captured.outputs) == 1
                payload = captured.outputs[0].data[MIME_TYPE]
                assert payload["payloadId"] == "example"
                assert payload["prompt"] == "Pick one"
                assert author.read_bytes() == before
                """,
                """\
                notebook = json.loads(before)
                cell = notebook["cells"][0]
                cell["source"] = cell["source"].replace("Pick one", "Updated question")
                author.write_text(json.dumps(notebook), encoding="utf-8")
                updated = author.read_bytes()

                with capture_output() as captured:
                    bound("example")

                assert len(captured.outputs) == 1
                assert captured.outputs[0].data[MIME_TYPE]["prompt"] == "Updated question"
                assert author.read_bytes() == updated
                """,
                """\
                checks = TestCase()
                with checks.assertRaisesRegex(ValueError, "no quiz 'missing'"):
                    bound("missing")
                with checks.assertRaises(TypeError):
                    bound("example", "second")
                assert author.read_bytes() == updated
                """,
            )
        ]
    )
    notebook_path = course_dir / "quiz_callers.ipynb"
    nbformat.write(notebook, notebook_path)
    kernel_specs_dir = course_dir / ".venv" / "share" / "jupyter" / "kernels"

    report = run_notebook(
        notebook_path,
        Path("chemistry-qpe") / notebook_path.name,
        kernel_specs_dir,
    )

    assert not report.failures, report.format_failures()


def test_quiz_notebooks_have_author_validation(course_dir: Path) -> None:
    for path in discover_notebooks(course_dir):
        notebook = json.loads(path.read_text(encoding="utf-8"))
        if not any(
            "quiz" in cell.get("metadata", {}).get("tags", [])
            for cell in notebook["cells"]
        ):
            continue
        last = notebook["cells"][-1]
        tags = last.get("metadata", {}).get("tags", [])
        assert last["cell_type"] == "code" and "author-only" in tags, (
            f"{path}: end quiz notebooks with an author-only validation code cell"
        )
        assert "skip-test" not in tags, f"{path}: author validation must run in CI"
        source = last["source"]
        if isinstance(source, list):
            assert all(isinstance(line, str) for line in source), (
                f"{path}: validation cell source must contain only text lines"
            )
            source = "".join(source)
        assert isinstance(source, str), f"{path}: validation cell source must be text"
        try:
            statements = ast.parse(source).body
        except SyntaxError as error:
            raise AssertionError(
                f"{path}: final validation cell has invalid Python "
                f"(line {error.lineno}): {error.msg}"
            ) from error
        assert statements and isinstance(statements[-1], ast.Expr), (
            f"{path}: the final cell must call validate_quizzes()"
        )
        call = statements[-1].value
        assert (
            isinstance(call, ast.Call)
            and isinstance(call.func, ast.Name)
            and call.func.id == "validate_quizzes"
            and not call.args
            and not call.keywords
        ), f"{path}: the final cell must call the notebook-bound validate_quizzes()"
