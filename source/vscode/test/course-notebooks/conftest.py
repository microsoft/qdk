from __future__ import annotations

import shutil
from pathlib import Path

import pytest

from notebook_runner import discover_notebooks

REPO_ROOT = Path(__file__).resolve().parents[4]
COURSES_ROOT = REPO_ROOT / "source/vscode/resources/qdk-learning/courses"


def pytest_addoption(parser: pytest.Parser) -> None:
    parser.addoption(
        "--course",
        help="Immediate child of the QDK learning courses directory to test",
    )


def pytest_generate_tests(metafunc: pytest.Metafunc) -> None:
    if "course_notebook" not in metafunc.fixturenames:
        return

    course_dir = _selected_course_dir(metafunc.config)
    notebooks = discover_notebooks(course_dir)
    if not notebooks:
        raise pytest.UsageError(f"no source notebooks found under {course_dir}")
    metafunc.parametrize(
        "course_notebook",
        notebooks,
        ids=[str(path.relative_to(course_dir)) for path in notebooks],
    )


@pytest.fixture
def isolated_course_notebook(
    course_notebook: Path,
    tmp_path: Path,
) -> tuple[Path, Path]:
    course_dir = course_notebook.parents[1]
    copied_course_dir = tmp_path / course_dir.name
    shutil.copytree(course_dir, copied_course_dir)
    copied_notebook = copied_course_dir / course_notebook.relative_to(course_dir)
    return copied_notebook, course_notebook.relative_to(REPO_ROOT)


def _selected_course_dir(config: pytest.Config) -> Path:
    course_name = config.getoption("course")
    if not course_name:
        raise pytest.UsageError(
            "course notebook tests require --course, for example "
            "--course chemistry-qpe"
        )
    if Path(course_name).name != course_name or "\\" in course_name:
        raise pytest.UsageError(
            f"--course must name one immediate child of {COURSES_ROOT}"
        )

    course_dir = COURSES_ROOT / course_name
    if not course_dir.is_dir():
        raise pytest.UsageError(f"course directory does not exist: {course_dir}")
    return course_dir