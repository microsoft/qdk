"""Execute an authored course notebook so its outputs ship with the course.

Run with the course venv's python. The exercise cell is cleared afterwards:
that one is the learner's to run.

Usage:  python bake_outputs.py 02
"""

import json
import sys
from pathlib import Path

import nbformat
from nbclient import NotebookClient

COURSE = Path.home() / "qdk-chem/qdk-learning/courses/chemistry-active-space"
NOTEBOOKS = {
    "02": "01-describe-molecule/describe_molecule.ipynb",
    "03": "02-active-space/active_space.ipynb",
    "05": "04-trial-state/trial_state.ipynb",
}
NOTEBOOK = COURSE / NOTEBOOKS[sys.argv[1] if len(sys.argv) > 1 else "03"]

nb = nbformat.read(NOTEBOOK, as_version=4)
NotebookClient(
    nb,
    timeout=1800,
    allow_errors=True,
    kernel_name="qdk-chem-course",
    resources={"metadata": {"path": str(NOTEBOOK.parent)}},
).execute()

raw = json.loads(nbformat.writes(nb))
# Widget state is several megabytes and the viewer rebuilds it when the cell runs.
raw["metadata"].pop("widgets", None)
errors = []
for cell in raw["cells"]:
    if cell["cell_type"] != "code":
        continue
    if "exercise" in cell.get("metadata", {}).get("tags", []):
        cell["outputs"] = []
        cell["execution_count"] = None
        continue
    for out in cell["outputs"]:
        if out.get("output_type") == "error":
            errors.append((cell.get("id"), out.get("ename"), out.get("evalue")))

NOTEBOOK.write_text(
    json.dumps(raw, indent=1, ensure_ascii=False) + "\n", encoding="utf-8"
)

code = [c for c in raw["cells"] if c["cell_type"] == "code"]
print(f"{sum(1 for c in code if c['outputs'])}/{len(code)} code cells carry outputs")
for cell_id, name, value in errors:
    print(f"ERROR in {cell_id}: {name}: {value}")
