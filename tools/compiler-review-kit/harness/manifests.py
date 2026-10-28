# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Build case-manifest subsets without breaking manifest-relative paths."""

import fnmatch
import os
from pathlib import Path

from harness.common import KIT, ReviewError, read_json, validate, write_json

PATH_FIELDS = ("source_file", "project_root", "adapter")


def rebase_paths(case, source_manifest, output):
    """Rewrite manifest-relative paths so they resolve from the output manifest's directory."""
    case = dict(case)
    source_dir = Path(source_manifest).resolve().parent
    output_dir = Path(output).resolve().parent
    for field in PATH_FIELDS:
        if field in case:
            absolute = (source_dir / case[field]).resolve()
            if not absolute.exists():
                raise ReviewError(f"{case['id']}: {field} does not exist: {absolute}")
            case[field] = os.path.relpath(absolute, output_dir)
    return case


def _content(case):
    """A case's identity for de-duplication: everything except provenance notes."""
    return {key: value for key, value in case.items() if key not in ("origin", "notes")}


def subset_manifests(manifests, output, ids=(), patterns=()):
    """Select cases by exact ID or glob across manifests.

    The same ID with the same content (provenance aside) is merged, keeping the first: extracted
    test programs hash their source, so manifests from several revisions overlap. The same ID
    with different content is an error.
    """
    if not ids and not patterns:
        raise ReviewError("Select cases with --ids and/or --match")
    schema = read_json(KIT / "cases.schema.json")
    wanted, selected, seen = set(ids), [], {}
    for manifest in manifests:
        cases = read_json(manifest)
        validate(cases, schema, str(manifest))
        for case in cases:
            if case["id"] in wanted or any(fnmatch.fnmatch(case["id"], p) for p in patterns):
                rebased = rebase_paths(case, manifest, output)
                if case["id"] in seen:
                    first, index = seen[case["id"]]
                    if _content(selected[index]) != _content(rebased):
                        raise ReviewError(f"Case {case['id']} appears in both {first} and {manifest} with different content")
                    continue
                seen[case["id"]] = (manifest, len(selected))
                selected.append(rebased)
    missing = sorted(wanted - set(seen))
    if missing:
        raise ReviewError(f"Case IDs not found: {', '.join(missing[:20])}")
    if not selected:
        raise ReviewError("No cases matched")
    validate(selected, schema, "subset")
    write_json(output, selected)
    return len(selected)


def export_run_cases(run, ids, output):
    """Copy the original (pre-snapshot) case entries of a run into a new manifest."""
    manifest = read_json(Path(run) / "manifest.json")
    source = Path(manifest["cases_path"])
    if not source.is_file():
        raise ReviewError(f"Original case manifest is gone: {source}")
    return subset_manifests([source], output, ids=ids)
