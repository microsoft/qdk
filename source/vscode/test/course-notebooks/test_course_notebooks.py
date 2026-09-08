from pathlib import Path

from notebook_runner import run_notebook


def test_course_notebook(isolated_course_notebook: tuple[Path, Path]) -> None:
    notebook_path, display_path = isolated_course_notebook

    report = run_notebook(notebook_path, display_path)

    assert not report.failures, report.format_failures()