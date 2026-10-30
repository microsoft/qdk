# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Select and export frozen case manifests with content-based conflict checks."""

import fnmatch
from harness.cases import case_identity, export_cases, read_case_manifest, read_run_cases
from harness.common import KIT, ReviewError, read_json, validate


def merge_manifests(manifests, ids=(), patterns=()):
    if not ids and not patterns:
        raise ReviewError("Select cases with --ids and/or --match")
    schema = read_json(KIT / "cases.schema.json")
    wanted, selected, seen, projects = set(ids), [], {}, {}
    for manifest in manifests:
        validate(read_json(manifest), schema, str(manifest))
        for case in read_case_manifest(manifest):
            if case["id"] in wanted or any(fnmatch.fnmatch(case["id"], p) for p in patterns):
                identity = case_identity(case, projects)
                if case["id"] in seen:
                    first, previous = seen[case["id"]]
                    if previous != identity:
                        raise ReviewError(f"Case {case['id']} appears in both {first} and {manifest} with different content")
                    continue
                seen[case["id"]] = (manifest, identity)
                selected.append(case)
    missing = sorted(wanted - set(seen))
    if missing:
        raise ReviewError(f"Case IDs not found: {', '.join(missing[:20])}")
    if not selected:
        raise ReviewError("No cases matched")
    return selected


def subset_manifests(manifests, output, ids=(), patterns=()):
    return export_cases(merge_manifests(manifests, ids, patterns), output)


def export_run_cases(run, ids, output):
    cases = read_run_cases(run)
    selected = [case for case in cases if case["id"] in ids]
    missing = set(ids) - {case["id"] for case in selected}
    if missing:
        raise ReviewError(f"Case IDs not found: {', '.join(sorted(missing))}")
    return export_cases(selected, output)
