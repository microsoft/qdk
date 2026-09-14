from pathlib import Path

from notebook_runner import run_notebook


def test_course_notebook(copied_course_notebook: tuple[Path, Path, Path]) -> None:
    notebook_path, display_path, kernel_specs_dir = copied_course_notebook

    report = run_notebook(notebook_path, display_path, kernel_specs_dir)

    assert not report.failures, report.format_failures()
