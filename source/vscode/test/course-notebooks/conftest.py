from __future__ import annotations

import json
import os
import shutil
import subprocess
import venv
from pathlib import Path

import pytest

from notebook_runner import discover_notebooks

REPO_ROOT = Path(__file__).resolve().parents[4]
COURSES_ROOT = REPO_ROOT / "source/vscode/resources/qdk-learning/courses"


def pytest_generate_tests(metafunc: pytest.Metafunc) -> None:
    course_dirs = _course_dirs()
    if "course_dir" in metafunc.fixturenames:
        metafunc.parametrize(
            "course_dir",
            course_dirs,
            ids=[path.name for path in course_dirs],
        )

    if "course_notebook" not in metafunc.fixturenames:
        return

    notebooks = [
        notebook
        for course_dir in course_dirs
        for notebook in discover_notebooks(course_dir)
    ]
    if not notebooks:
        raise pytest.UsageError(f"no source notebooks found under {COURSES_ROOT}")
    metafunc.parametrize(
        "course_notebook",
        notebooks,
        ids=[str(path.relative_to(COURSES_ROOT)) for path in notebooks],
    )


@pytest.fixture(scope="session")
def copied_course_dirs(
    tmp_path_factory: pytest.TempPathFactory,
) -> dict[Path, Path]:
    copies_root = tmp_path_factory.mktemp("course-notebooks")
    copied_course_dirs = {}
    for course_dir in _course_dirs():
        copied_course_dir = copies_root / course_dir.name
        shutil.copytree(course_dir, copied_course_dir)
        _create_notebook_environment(copied_course_dir)
        copied_course_dirs[course_dir] = copied_course_dir
    return copied_course_dirs


@pytest.fixture
def copied_course_notebook(
    course_notebook: Path,
    copied_course_dirs: dict[Path, Path],
) -> tuple[Path, Path, Path]:
    course_name = course_notebook.relative_to(COURSES_ROOT).parts[0]
    course_dir = COURSES_ROOT / course_name
    copied_course_dir = copied_course_dirs[course_dir]
    copied_notebook = copied_course_dir / course_notebook.relative_to(course_dir)
    kernel_specs_dir = copied_course_dir / ".venv" / "share" / "jupyter" / "kernels"
    return (
        copied_notebook,
        course_notebook.relative_to(REPO_ROOT),
        kernel_specs_dir,
    )


def _create_notebook_environment(course_dir: Path) -> None:
    venv_dir = course_dir / ".venv"
    venv.create(venv_dir, with_pip=True)
    python = venv_dir / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
    subprocess.run(
        [
            python,
            "-m",
            "pip",
            "install",
            "--quiet",
            "-r",
            course_dir / "requirements.txt",
        ],
        check=True,
    )

    kernel_dir = venv_dir / "share/jupyter/kernels/python3"
    kernel_dir.mkdir(parents=True, exist_ok=True)
    kernel_spec = {
        "argv": [str(python), "-m", "ipykernel_launcher", "-f", "{connection_file}"],
        "display_name": "Course notebook tests",
        "language": "python",
    }
    (kernel_dir / "kernel.json").write_text(
        json.dumps(kernel_spec, indent=2),
        encoding="utf-8",
    )


def _course_dirs() -> list[Path]:
    return sorted(path for path in COURSES_ROOT.iterdir() if path.is_dir())
