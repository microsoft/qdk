"""Execute and bake outputs across all QDK-EC course unit notebooks."""

import asyncio
import json
from pathlib import Path
import subprocess
import sys

import nbformat
from nbclient import NotebookClient

if sys.platform == "win32":
    asyncio.set_event_loop_policy(asyncio.WindowsSelectorEventLoopPolicy())

COURSE = Path(__file__).resolve().parents[2] / "courses" / "qdk-ec"


def _bake_single_unit(unit_dir_name: str, *, check_only: bool = False) -> None:
    unit_dir = COURSE / unit_dir_name
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
    if check_only:
        print(f"[EXEC CHECK OK] {unit_dir_name}/{nb_path.name}", flush=True)
        return

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
    print(f"[BAKED OK] {unit_dir_name}/{nb_path.name}", flush=True)


if __name__ == "__main__":
    check_only = "--check-only" in sys.argv
    clean_args = [a for a in sys.argv[1:] if a != "--check-only"]
    if len(clean_args) == 2 and clean_args[0] == "--unit":
        _bake_single_unit(clean_args[1], check_only=check_only)
    else:
        manifest = json.loads((COURSE / "course.json").read_text(encoding="utf-8"))
        for unit in manifest["units"]:
            cmd = [sys.executable, str(Path(__file__).resolve()), "--unit", unit["dir"]]
            if check_only:
                cmd.append("--check-only")
            subprocess.run(cmd, check=True)
