from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from time import perf_counter
from typing import Any

import nbformat
from nbclient import NotebookClient

CELL_TIMEOUT_SECONDS = 120
SLOW_CELL_SECONDS = 30
SKIP_TEST_TAG = "skip-test"
EXERCISE_TAG = "exercise"


@dataclass(frozen=True)
class CellFailure:
    cell_number: int
    source_line: str
    message: str


@dataclass(frozen=True)
class SlowCell:
    cell_number: int
    source_line: str
    duration_seconds: float


@dataclass(frozen=True)
class NotebookRunReport:
    notebook_path: Path
    elapsed_seconds: float
    executed_cells: int
    skipped_cells: tuple[int, ...]
    slow_cells: tuple[SlowCell, ...]
    failures: tuple[CellFailure, ...]

    def format_failures(self) -> str:
        return "\n".join(
            f"{self.notebook_path}: cell {failure.cell_number} "
            f"({failure.source_line}): {failure.message}"
            for failure in self.failures
        )


def discover_notebooks(course_dir: Path) -> list[Path]:
    return sorted(
        path
        for path in course_dir.rglob("*.ipynb")
        if not path.name.endswith(".workbook.ipynb")
    )


def clear_notebook_outputs(notebook: Any) -> None:
    for cell in notebook.cells:
        if cell.cell_type != "code":
            continue
        cell.outputs = []
        cell.execution_count = None
        cell.metadata.pop("execution", None)


def collect_cell_failures(notebook: Any) -> list[CellFailure]:
    failures: list[CellFailure] = []
    for cell_number, cell in enumerate(notebook.cells, start=1):
        if cell.cell_type != "code":
            continue

        tags = set(cell.metadata.get("tags", []))
        source_line = _first_source_line(cell.source)
        if EXERCISE_TAG in tags and SKIP_TEST_TAG in tags:
            failures.append(
                CellFailure(
                    cell_number,
                    source_line,
                    f"a cell cannot have both {EXERCISE_TAG!r} and {SKIP_TEST_TAG!r}",
                )
            )
            continue
        if SKIP_TEST_TAG in tags:
            continue

        errors = [
            output
            for output in cell.get("outputs", [])
            if output.get("output_type") == "error"
        ]
        if EXERCISE_TAG in tags:
            if not errors:
                failures.append(
                    CellFailure(
                        cell_number,
                        source_line,
                        "exercise cell did not raise ExerciseError",
                    )
                )
            elif errors[0].get("ename") != "ExerciseError":
                failures.append(
                    CellFailure(
                        cell_number,
                        source_line,
                        "exercise cell raised " + _format_error(errors[0]),
                    )
                )
            continue

        failures.extend(
            CellFailure(
                cell_number,
                source_line,
                "unexpected error: " + _format_error(error),
            )
            for error in errors
        )
    return failures


def run_notebook(notebook_path: Path, display_path: Path | None = None) -> NotebookRunReport:
    notebook = nbformat.read(notebook_path, as_version=4)
    clear_notebook_outputs(notebook)

    metadata_failures = [
        failure
        for failure in collect_cell_failures(notebook)
        if "cannot have both" in failure.message
    ]
    if metadata_failures:
        return NotebookRunReport(
            display_path or notebook_path,
            0.0,
            0,
            (),
            (),
            tuple(metadata_failures),
        )

    started = perf_counter()
    NotebookClient(
        notebook,
        timeout=CELL_TIMEOUT_SECONDS,
        allow_errors=True,
        kernel_name="python3",
        resources={"metadata": {"path": str(notebook_path.parent)}},
        skip_cells_with_tag=SKIP_TEST_TAG,
        store_widget_state=False,
    ).execute()
    elapsed_seconds = perf_counter() - started

    skipped_cells = tuple(
        cell_number
        for cell_number, cell in enumerate(notebook.cells, start=1)
        if cell.cell_type == "code" and SKIP_TEST_TAG in cell.metadata.get("tags", [])
    )
    slow_cells = tuple(
        slow_cell
        for cell_number, cell in enumerate(notebook.cells, start=1)
        if (slow_cell := _slow_cell(cell_number, cell)) is not None
    )
    executed_cells = sum(
        1
        for cell in notebook.cells
        if cell.cell_type == "code" and SKIP_TEST_TAG not in cell.metadata.get("tags", [])
    )
    report = NotebookRunReport(
        display_path or notebook_path,
        elapsed_seconds,
        executed_cells,
        skipped_cells,
        slow_cells,
        tuple(collect_cell_failures(notebook)),
    )
    print_notebook_report(report)
    return report


def print_notebook_report(report: NotebookRunReport) -> None:
    print(
        f"{report.notebook_path}: {report.elapsed_seconds:.1f}s, "
        f"{report.executed_cells} executed, {len(report.skipped_cells)} skipped"
    )
    for cell_number in report.skipped_cells:
        print(f"  skipped cell {cell_number} ({SKIP_TEST_TAG})")
    for cell in report.slow_cells:
        print(
            f"  slow cell {cell.cell_number}: {cell.duration_seconds:.1f}s "
            f"({cell.source_line})"
        )


def _first_source_line(source: str) -> str:
    for line in source.splitlines():
        if stripped := line.strip():
            return stripped
    return "<empty>"


def _format_error(error: Any) -> str:
    name = error.get("ename", "Error")
    value = error.get("evalue", "")
    return f"{name}: {value}" if value else name


def _slow_cell(cell_number: int, cell: Any) -> SlowCell | None:
    if cell.cell_type != "code":
        return None
    execution = cell.metadata.get("execution", {})
    started = execution.get("iopub.status.busy")
    finished = execution.get("iopub.status.idle")
    if not started or not finished:
        return None
    duration = _parse_timestamp(finished) - _parse_timestamp(started)
    duration_seconds = duration.total_seconds()
    if duration_seconds <= SLOW_CELL_SECONDS:
        return None
    return SlowCell(
        cell_number,
        _first_source_line(cell.source),
        duration_seconds,
    )


def _parse_timestamp(value: str) -> datetime:
    return datetime.fromisoformat(value.replace("Z", "+00:00"))