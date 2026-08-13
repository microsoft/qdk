"""Check every unit in the course loads, validates, and carries what the tree needs.

Usage:  python verify_course.py [course-directory]
"""

import json
import sys
from pathlib import Path

import nbformat

DEFAULT_COURSE = (
    Path(__file__).resolve().parent.parent
    / "source/vscode/test/suites/learning/test-workspace"
    / "qdk-learning/courses/chemistry-active-space"
)
COURSE = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_COURSE
if not (COURSE / "course.json").is_file():
    sys.exit(f"no course.json under {COURSE}")

manifest = json.loads((COURSE / "course.json").read_text())
print(f"{manifest['title']}  ({len(manifest['units'])} units)")

failed = False
for unit in manifest["units"]:
    directory = COURSE / unit["dir"]
    notebooks = [
        p for p in directory.glob("*.ipynb") if not p.name.endswith(".workbook.ipynb")
    ]
    problems = []
    if len(notebooks) != 1:
        problems.append(f"expected 1 notebook, found {len(notebooks)}")

    sections = exercises = with_outputs = code_cells = 0
    uses_unit_module = False
    if len(notebooks) == 1:
        nb = nbformat.read(notebooks[0], as_version=4)
        try:
            nbformat.validate(nb)
        except nbformat.ValidationError as error:
            problems.append(f"invalid: {error}")
        for cell in nb.cells:
            tags = cell.get("metadata", {}).get("tags", [])
            sections += any(t.startswith("section:") for t in tags)
            exercises += "exercise" in tags
            if cell.cell_type == "code":
                code_cells += 1
                with_outputs += bool(cell.get("outputs"))
                uses_unit_module |= "_unit" in cell.source
                if "exercise" in tags and cell.get("outputs"):
                    problems.append("exercise cell ships an output")

    if uses_unit_module and not (directory / "_unit.py").exists():
        problems.append("missing _unit.py")

    status = "FAIL" if problems else "ok"
    print(
        f"  [{status}] {unit['title']:<32} {sections:>2} sections  "
        f"{exercises} exercise  {with_outputs}/{code_cells} code cells with outputs"
    )
    for problem in problems:
        print(f"         {problem}")
        failed = True

raise SystemExit(1 if failed else 0)
