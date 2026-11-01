"""Execute and bake outputs across all QDK-EC course unit notebooks."""

from pathlib import Path
import json
import nbformat
from nbclient import NotebookClient

COURSE = Path(__file__).resolve().parents[2] / "courses" / "qdk-ec"
manifest = json.loads((COURSE / "course.json").read_text(encoding="utf-8"))

for unit in manifest["units"]:
    unit_dir = COURSE / unit["dir"]
    nb_path = next(p for p in unit_dir.glob("*.ipynb") if not p.name.endswith(".workbook.ipynb"))
    nb = nbformat.read(nb_path, as_version=4)

    orig_sources = {}
    for idx, cell in enumerate(nb.cells):
        tags = set(cell.get("metadata", {}).get("tags", []))
        if "exercise" in tags:
            orig_sources[idx] = cell.source
            for j in range(idx + 1, min(idx + 5, len(nb.cells))):
                if "solution" in set(nb.cells[j].get("metadata", {}).get("tags", [])):
                    cell.source = nb.cells[j].source
                    break

    client = NotebookClient(
        nb,
        timeout=300,
        kernel_name="python3",
        resources={"metadata": {"path": str(unit_dir)}},
    )
    client.execute()

    for idx, src in orig_sources.items():
        nb.cells[idx].source = src
    for cell in nb.cells:
        if cell.cell_type != "code":
            continue
        tags = set(cell.get("metadata", {}).get("tags", []))
        if "check_env" in cell.source or "exercise" in tags or "solution" in tags:
            cell.outputs = []
            cell.execution_count = None

    nbformat.write(nb, nb_path)
    print(f"[BAKED OK] {unit['dir']}/{nb_path.name}", flush=True)
