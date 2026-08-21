"""Verify the generated Chemistry QPE course and its exercise wiring.

Usage:  python verify_course.py [course-directory] [--allow-outputs]
"""

import argparse
import ast
import json
import sys
from collections import Counter
from pathlib import Path

import nbformat

DEFAULT_COURSE = Path(__file__).resolve().parents[2] / "courses/chemistry-qpe"
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("course", nargs="?", type=Path, default=DEFAULT_COURSE)
parser.add_argument(
    "--allow-outputs",
    action="store_true",
    help="allow outputs and execution counts created by bake_outputs.py",
)
args = parser.parse_args()
COURSE = args.course
EXPECTED_TOTALS = Counter(
    {
        "units": 7,
        "cells": 174,
        "exercises": 6,
        "hints": 6,
        "solutions": 6,
        "explanations": 6,
        "quizzes": 38,
        "attachments": 11,
    }
)
AUTHORING_KINDS = {
    "exercise": "code",
    "hint": "markdown",
    "solution": "code",
    "explanation": "markdown",
}
REGISTER_CALLS = {"register_exercise", "register_value_exercise"}


def exercise_names(source: str) -> list[str]:
    """Return functions decorated with ``@exercise`` in one cell."""
    try:
        tree = ast.parse(source)
    except SyntaxError:
        return []
    return [
        node.name
        for node in tree.body
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
        and any(
            isinstance(decorator, ast.Name) and decorator.id == "exercise"
            for decorator in node.decorator_list
        )
    ]


def registered_names(path: Path) -> set[str]:
    """Return exercise names registered by a unit helper."""
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    names = set()
    for node in ast.walk(tree):
        if (
            isinstance(node, ast.Call)
            and isinstance(node.func, ast.Name)
            and node.func.id in REGISTER_CALLS
            and node.args
            and isinstance(node.args[0], ast.Constant)
            and isinstance(node.args[0].value, str)
        ):
            names.add(node.args[0].value)
    return names


manifest_path = COURSE / "course.json"
if not manifest_path.is_file():
    sys.exit(f"no course.json under {COURSE}")

manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
print(f"{manifest['title']}  ({len(manifest['units'])} units)")

failed = False
totals = Counter(units=len(manifest["units"]))
if "id" in manifest:
    print("  [FAIL] course.json must derive its ID from the directory name")
    failed = True

for unit in manifest["units"]:
    directory = COURSE / unit["dir"]
    notebooks = [
        path
        for path in directory.glob("*.ipynb")
        if not path.name.endswith(".workbook.ipynb")
    ]
    problems = []
    counts = Counter()
    exercise_authoring: dict[str, Counter] = {}
    exercise_ids: dict[str, str] = {}
    current_exercise: str | None = None
    uses_unit_module = False

    if len(notebooks) != 1:
        problems.append(f"expected 1 notebook, found {len(notebooks)}")

    if len(notebooks) == 1:
        notebook = nbformat.read(notebooks[0], as_version=4)
        try:
            nbformat.validate(notebook)
        except nbformat.ValidationError as error:
            problems.append(f"invalid notebook: {error}")

        counts["cells"] = len(notebook.cells)
        if notebook.metadata.get("widgets"):
            problems.append("notebook ships widget state")

        cell_ids = [cell.get("id") for cell in notebook.cells]
        missing_ids = sum(cell_id is None for cell_id in cell_ids)
        if missing_ids:
            problems.append(f"{missing_ids} cells have no stable ID")
        duplicate_ids = sorted(
            cell_id
            for cell_id, count in Counter(cell_ids).items()
            if cell_id is not None and count > 1
        )
        if duplicate_ids:
            problems.append(f"duplicate cell IDs: {', '.join(duplicate_ids)}")

        for cell in notebook.cells:
            tags = set(cell.get("metadata", {}).get("tags", []))
            source = cell.source
            authoring_tag = next(
                (
                    tag
                    for tag in ("hint", "solution", "explanation")
                    if tag in tags
                ),
                None,
            )
            if cell.cell_type == "code" and authoring_tag is None:
                current_exercise = None
            counts["sections"] += any(tag.startswith("section:") for tag in tags)
            counts["quizzes"] += source.count("<details>")

            for tag, expected_kind in AUTHORING_KINDS.items():
                if tag in tags and cell.cell_type != expected_kind:
                    problems.append(
                        f'{cell.id}: "{tag}" must be a {expected_kind} cell, '
                        f"found {cell.cell_type}"
                    )

            if "exercise" in tags:
                counts["exercises"] += 1
                names = exercise_names(source)
                if len(names) != 1:
                    problems.append(
                        f"{cell.id}: exercise cell must define exactly one "
                        "@exercise function"
                    )
                    current_exercise = None
                else:
                    current_exercise = names[0]
                    exercise_ids[current_exercise] = cell.id
                    exercise_authoring[current_exercise] = Counter()

            for tag, total_name in (
                ("hint", "hints"),
                ("solution", "solutions"),
                ("explanation", "explanations"),
            ):
                if tag not in tags:
                    continue
                counts[total_name] += 1
                if current_exercise is None:
                    problems.append(
                        f'{cell.id}: "{tag}" cell does not follow an exercise'
                    )
                else:
                    exercise_authoring[current_exercise][tag] += 1

            for name, payload in cell.get("attachments", {}).items():
                counts["attachments"] += 1
                if f"attachment:{name}" not in source:
                    problems.append(f"{cell.id}: orphan attachment {name}")
                if not payload:
                    problems.append(f"{cell.id}: empty attachment {name}")

            if cell.cell_type == "code":
                counts["code_cells"] += 1
                uses_unit_module |= "_unit" in source
                allow_cell_output = args.allow_outputs and not (
                    tags & AUTHORING_KINDS.keys()
                )
                if cell.get("outputs") and not allow_cell_output:
                    problems.append(f"{cell.id}: code cell ships output")
                if (
                    cell.get("execution_count") is not None
                    and not allow_cell_output
                ):
                    problems.append(f"{cell.id}: code cell has an execution count")

        unit_helper = directory / "_unit.py"
        if uses_unit_module and not unit_helper.is_file():
            problems.append("missing _unit.py")
        elif unit_helper.is_file():
            registered = registered_names(unit_helper)
            defined = set(exercise_authoring)
            if registered != defined:
                problems.append(
                    "checker mismatch: "
                    f"registered={sorted(registered)}, exercises={sorted(defined)}"
                )

        for name, authoring in exercise_authoring.items():
            for tag in ("hint", "solution", "explanation"):
                if authoring[tag] != 1:
                    problems.append(
                        f"{exercise_ids[name]}: {name} has "
                        f"{authoring[tag]} {tag} cells, expected 1"
                    )

    totals.update(
        {
            name: counts[name]
            for name in (
                "cells",
                "exercises",
                "hints",
                "solutions",
                "explanations",
                "quizzes",
                "attachments",
            )
        }
    )
    status = "FAIL" if problems else "ok"
    print(
        f"  [{status}] {unit['title']:<36} "
        f"{counts['cells']:>2} cells  {counts['sections']:>2} sections  "
        f"{counts['exercises']} exercises"
    )
    for problem in problems:
        print(f"         {problem}")
        failed = True

for name, expected in EXPECTED_TOTALS.items():
    if totals[name] != expected:
        print(f"  [FAIL] total {name}: found {totals[name]}, expected {expected}")
        failed = True

if not failed:
    print(
        "Verified "
        f"{totals['cells']} cells, {totals['exercises']} exercises, "
        f"{totals['quizzes']} quizzes, and {totals['attachments']} attachments."
    )

raise SystemExit(1 if failed else 0)
