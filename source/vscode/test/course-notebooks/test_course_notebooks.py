import json
from collections import Counter
from pathlib import Path

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
