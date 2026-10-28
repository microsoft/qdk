# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Load curated cases and snapshot projects without flattening package boundaries."""

import fnmatch
from pathlib import Path
import re
import shutil

from harness.common import KIT, ReviewError, digest, file_digest, read_json, validate, write_json
from harness.records import validate_value


def snapshot_project(root, destination, seen):
    root = root.resolve()
    if root in seen:
        return seen[root]
    if not (root / "qsharp.json").is_file():
        raise ReviewError(f"Project has no qsharp.json: {root}")
    target = destination / digest(str(root))[:16]
    seen[root] = target
    target.mkdir(parents=True, exist_ok=True)
    manifest = read_json(root / "qsharp.json")
    for source in sorted((root / "src").rglob("*")):
        if source.is_symlink():
            raise ReviewError(f"Snapshot does not support source symlinks: {source}")
        if source.is_file():
            copied = target / source.relative_to(root)
            copied.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, copied)
    (target / "src").mkdir(exist_ok=True)
    for dependency in manifest.get("dependencies", {}).values():
        if "path" in dependency:
            child = snapshot_project(root / dependency["path"], destination, seen)
            dependency["path"] = str(child)
    write_json(target / "qsharp.json", manifest)
    return target


def load_cases(path, config, snapshot_dir):
    path = Path(path).resolve()
    cases = read_json(path)
    validate(cases, read_json(KIT / "cases.schema.json"))
    ids, loaded, skipped = set(), [], []
    for item in cases:
        case = dict(item)
        name = case["id"]
        if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]*", name) or name in ids:
            raise ReviewError(f"Invalid or duplicate case ID: {name!r}")
        ids.add(name)
        excluded = next(
            (
                rule
                for rule in config["input"].get("exclusions", [])
                if any(
                    fnmatch.fnmatch(str(case.get(field, "")), rule["match"])
                    for field in ("id", "origin", "source_file", "project_root")
                )
            ),
            None,
        )
        if excluded:
            skipped.append({"id": name, "reason": excluded["reason"]})
            continue
        if "source" in case and "source_file" in case:
            raise ReviewError(f"{name}: choose source or source_file, not both")
        if not any(k in case for k in ("source", "source_file", "project_root", "adapter")):
            raise ReviewError(f"{name}: no source, project, or adapter")
        if "adapter" not in case and "entry" not in case:
            raise ReviewError(f"{name}: entry is required without an adapter")
        if "source_file" in case:
            case["source"] = (path.parent / case.pop("source_file")).read_text()
        if "expected" in case["oracle"]:
            if case["oracle"]["kind"] != "deterministic":
                raise ReviewError(f"{name}: expected requires a deterministic oracle")
            validate_value(case["oracle"]["expected"])
        if "project_root" in case:
            original = (path.parent / case["project_root"]).resolve()
            case["project_origin"] = str(original)
            case["project_root"] = str(
                snapshot_project(original, Path(snapshot_dir) / name / "projects", {})
            )
        if "adapter" in case:
            adapter = (path.parent / case["adapter"]).resolve()
            if not adapter.is_file():
                raise ReviewError(f"{name}: missing adapter {adapter}")
            copied = Path(snapshot_dir) / name / "adapter.py"
            copied.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(adapter, copied)
            case["adapter"] = str(copied)
            case["adapter_digest"] = file_digest(copied)
        case["profiles"] = case.get("profiles", config["profiles"])
        case["shots"] = case.get("shots", config["limits"]["shots"])
        case["seed"] = case.get("seed", config["limits"]["seed"])
        loaded.append(case)
    return loaded, skipped
