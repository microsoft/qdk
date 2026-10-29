# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Load curated cases and snapshot projects without flattening package boundaries."""

import fnmatch
import os
from pathlib import Path
import re
import shutil

from harness.common import KIT, ReviewError, digest, file_digest, read_json, validate, write_json
from harness.records import validate_value


def write_project(files, root):
    root = Path(root)
    root.mkdir(parents=True, exist_ok=True)
    if "qsharp.json" not in files:
        write_json(root / "qsharp.json", {})
    for name, text in files.items():
        relative = Path(name)
        if relative.is_absolute() or ".." in relative.parts:
            raise ReviewError(f"Project file must stay inside its bundle: {name}")
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
    return root


def project_digest(root):
    """Hash a local package graph by content, not its absolute paths."""
    packages, seen = [], {}

    def visit(path):
        path = path.resolve()
        if path in seen:
            return seen[path]
        if not (path / "qsharp.json").is_file():
            raise ReviewError(f"Project has no qsharp.json: {path}")
        index = seen[path] = len(packages)
        packages.append(None)
        manifest = read_json(path / "qsharp.json")
        for _, dependency in sorted(manifest.get("dependencies", {}).items()):
            if "path" in dependency:
                dependency["path"] = visit(path / dependency["path"])
        sources = []
        for source in sorted((path / "src").rglob("*")):
            if source.is_symlink():
                raise ReviewError(f"Snapshot does not support source symlinks: {source}")
            if source.is_file():
                sources.append((str(source.relative_to(path)), file_digest(source)))
        packages[index] = {"manifest": manifest, "sources": sources}
        return index

    visit(Path(root))
    return digest(packages)


def snapshot_project(root, destination, seen, hashes=None):
    root = root.resolve()
    if root in seen:
        return seen[root]
    hashes = {} if hashes is None else hashes
    if root not in hashes:
        hashes[root] = project_digest(root)
    target = destination / f"{hashes[root]}-{len(seen)}"
    seen[root] = target
    if target.exists():
        if project_digest(target) != hashes[root]:
            raise ReviewError(f"Project snapshot changed: {target}")
        return target
    target.mkdir(parents=True)
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
            child = snapshot_project(root / dependency["path"], destination, seen, hashes)
            dependency["path"] = os.path.relpath(child, target)
    write_json(target / "qsharp.json", manifest)
    return target


def read_case_manifest(path):
    """Resolve a portable manifest without resnapshotting its inputs."""
    path = Path(path).resolve()
    cases = read_json(path)
    for case in cases:
        for field in ("source_file", "adapter", "project_root"):
            if field in case:
                case[field] = str((path.parent / case[field]).resolve())
    return cases


def write_case_manifest(cases, path):
    """Write public case entries, keeping frozen inputs relative to the manifest."""
    path = Path(path).resolve()
    schema = read_json(KIT / "cases.schema.json")
    entries = []
    for case in cases:
        entry = {key: value for key, value in case.items() if key not in ("adapter_digest", "project_origin")}
        for field in ("source_file", "adapter", "project_root"):
            if field in entry:
                entry[field] = os.path.relpath(entry[field], path.parent)
        entries.append(entry)
    if entries:
        validate(entries, schema)
    write_json(path, entries)


def bundle_cases(cases, base, target):
    """Freeze a portable manifest, sharing adapters and complete local package graphs."""
    target, base = Path(target).resolve(), Path(base).resolve()
    bundled, projects, hashes = [], {}, {}
    for original in cases:
        case = dict(original)
        name = case["id"]
        if "source" in case and "source_file" in case:
            raise ReviewError(f"{name}: choose source or source_file, not both")
        if not any(key in case for key in ("source", "source_file", "project_root", "adapter")):
            raise ReviewError(f"{name}: no source, project, or adapter")
        if "adapter" not in case and "entry" not in case:
            raise ReviewError(f"{name}: entry is required without an adapter")
        if "expected" in case["oracle"]:
            if case["oracle"]["kind"] != "deterministic":
                raise ReviewError(f"{name}: expected requires a deterministic oracle")
            validate_value(case["oracle"]["expected"])
        if "source_file" in case:
            case["source"] = (base / case.pop("source_file")).read_text()
        if "adapter" in case:
            source = (base / case["adapter"]).resolve()
            checksum = file_digest(source)
            copied = target / "adapters" / f"{checksum}.py"
            copied.parent.mkdir(parents=True, exist_ok=True)
            if copied.exists() and file_digest(copied) != checksum:
                raise ReviewError(f"Adapter snapshot changed: {copied}")
            if not copied.exists():
                shutil.copy2(source, copied)
            case["adapter"] = str(copied)
        if "project_root" in case:
            case["project_root"] = str(snapshot_project(
                (base / case["project_root"]).resolve(), target / "projects", projects, hashes,
            ))
        bundled.append(case)
    write_case_manifest(bundled, target / "cases.json")
    return len(bundled)


def export_cases(cases, output):
    output = Path(output).resolve()
    bundle = output.parent / f"{output.stem}.inputs"
    count = bundle_cases(cases, output.parent, bundle)
    write_case_manifest(read_case_manifest(bundle / "cases.json"), output)
    (bundle / "cases.json").unlink()
    return count


def case_identity(case, projects=None):
    """Execution identity; labels, provenance, and temporary paths are not inputs."""
    projects = {} if projects is None else projects
    fields = read_json(KIT / "cases.schema.json")["items"]["properties"]
    content = {key: value for key, value in case.items() if key in fields and key not in ("id", "origin", "notes")}
    if "source_file" in content:
        content["source"] = Path(content.pop("source_file")).read_text()
    if "adapter" in content:
        content["adapter"] = file_digest(content["adapter"])
    if "project_root" in content:
        root = Path(content["project_root"]).resolve()
        if root not in projects:
            projects[root] = project_digest(root)
        content["project_root"] = projects[root]
    return digest(content)


def read_run_cases(run):
    run = Path(run)
    cases = read_case_manifest(run / "cases.json")
    expected = read_json(run / "manifest.json")["case_identities"]
    if set(expected) != {case["id"] for case in cases}:
        raise ReviewError(f"Frozen case manifest changed: {run}")
    projects = {}
    for case in cases:
        if case_identity(case, projects) != expected[case["id"]]:
            raise ReviewError(f"Frozen inputs changed for {case['id']}: {run}")
    return cases


def load_cases(path, config, snapshot_dir):
    path = Path(path).resolve()
    cases = read_json(path)
    validate(cases, read_json(KIT / "cases.schema.json"))
    ids, selected, skipped, origins = set(), [], [], {}
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
        if "project_root" in case:
            origins[name] = str((path.parent / case["project_root"]).resolve())
        case["profiles"] = case.get("profiles", config["profiles"])
        case["shots"] = case.get("shots", config["limits"]["shots"])
        case["seed"] = case.get("seed", config["limits"]["seed"])
        selected.append(case)
    bundle_cases(selected, path.parent, snapshot_dir)
    loaded = read_case_manifest(Path(snapshot_dir) / "cases.json")
    for case in loaded:
        if case["id"] in origins:
            case["project_origin"] = origins[case["id"]]
        if "adapter" in case:
            case["adapter_digest"] = file_digest(case["adapter"])
    return loaded, skipped
