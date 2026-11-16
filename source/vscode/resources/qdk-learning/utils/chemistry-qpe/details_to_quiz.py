"""Refresh saved quiz outputs from the author notebook's literal definitions.

Only quiz outputs are regenerated; no notebook cells are executed. Source,
cell IDs, author-only cells and unrelated outputs are preserved. Materialization
copies these outputs into the learner workbook and replaces quiz definitions
with ID-only calls. Learner reruns read author source, not these saved outputs.

    python details_to_quiz.py 06-iterative-phase-estimation
    python details_to_quiz.py 06-iterative-phase-estimation --check
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

COURSE = Path(__file__).resolve().parents[2] / "courses" / "chemistry-qpe"
sys.path.insert(0, str(COURSE))

from _learning_output import (  # noqa: E402
    MIME_TYPE,
    notebook_quiz_outputs,
    read_quiz_notebook,
)


def _normalize_bundle(value: Any) -> Any:
    if isinstance(value, list):
        if all(isinstance(line, str) for line in value):
            return "".join(value)
        return [_normalize_bundle(item) for item in value]
    if isinstance(value, dict):
        return {key: _normalize_bundle(item) for key, item in value.items()}
    return value


def bake_quizzes(notebook: dict[str, Any]) -> list[str]:
    outputs = notebook_quiz_outputs(notebook)
    changed = []
    for cell in notebook["cells"]:
        if cell.get("cell_type") != "code":
            continue
        rendered = outputs.get(cell.get("id"))
        if rendered is None:
            expected = [
                output for output in cell.get("outputs", [])
                if MIME_TYPE not in output.get("data", {})
            ]
        else:
            expected = [
                {
                    "output_type": "display_data",
                    "data": output._repr_mimebundle_(),
                    "metadata": {},
                }
                for output in rendered
            ]
        if _normalize_bundle(cell.get("outputs", [])) == _normalize_bundle(expected):
            continue
        cell["outputs"] = expected
        cell["execution_count"] = None
        cell.get("metadata", {}).pop("execution", None)
        changed.append(cell["id"])
    return changed


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("unit", help="unit folder name, e.g. 06-iterative-phase-estimation")
    parser.add_argument("--course", type=Path, default=COURSE, help="course root")
    parser.add_argument(
        "--check", action="store_true", help="check saved quiz outputs without writing"
    )
    args = parser.parse_args()

    unit_dir = args.course / args.unit
    notebooks = sorted(
        path
        for path in unit_dir.glob("*.ipynb")
        if not path.name.endswith(".workbook.ipynb")
    )
    if len(notebooks) != 1:
        parser.error(
            f"expected exactly one author notebook in {unit_dir}, found {len(notebooks)}"
        )
    source = notebooks[0]
    notebook = read_quiz_notebook(source)
    if not any(
        "quiz" in cell.get("metadata", {}).get("tags", [])
        or any(MIME_TYPE in output.get("data", {}) for output in cell.get("outputs", []))
        for cell in notebook["cells"]
    ):
        parser.error(f"{source} has no quiz definitions or saved quiz outputs")
    changed = bake_quizzes(notebook)
    if not changed:
        print(f"{source.name}: quiz outputs are up to date")
        return 0
    if args.check:
        print(f"{source.name}: stale quiz outputs in {', '.join(changed)}")
        return 1
    source.write_text(
        json.dumps(notebook, indent=1, ensure_ascii=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    print(f"{source.name}: refreshed quiz outputs in {', '.join(changed)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
