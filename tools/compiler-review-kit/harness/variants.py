# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Variant matrices: one trusted file of named program variants, run at several revisions."""

from datetime import datetime, timezone
from pathlib import Path

from harness.builds import build_revision, worker_environment
from harness.common import KIT, ReviewError, isolated, preflight, write_json
from harness.execution import parse_panics


def run_variants(config, path, targets, profiles, mode, shots=None):
    path = Path(path).resolve()
    if not path.is_file():
        raise ReviewError(f"Missing variants file: {path}")
    tools = preflight(config)
    shots = shots or config["limits"]["shots"]
    if profiles is None and "profiles" in config["input"]:
        profiles = config["profiles"]
    columns = []
    for label, sha in targets:
        build = build_revision(config, sha, mode, tools)
        result = isolated(
            [str(Path(config["venv"]) / "bin/python"), str(KIT / "harness/variants_worker.py")],
            payload={"variants": str(path), "profiles": profiles, "shots": shots, "seed": config["limits"]["seed"],
                     "runner": str(Path(config["venv"]) / "bin/qir-runner")},
            env=worker_environment(config, build), cwd=str(path.parent),
            timeout=config["limits"]["compile_seconds"] * 20,
        )
        if result.get("status") != "ok":
            raise ReviewError(f"Variants worker failed at {label}: {result.get('status')} "
                              f"{(result.get('message') or result.get('stderr') or '')[-2000:]}")
        columns.append({"label": label, "sha": sha, "profiles": result["profiles"], "variants": result["variants"],
                        "panics": parse_panics(result.get("stderr", ""))})
    output = Path(config["work_dir"]) / "variants" / (
        f"{path.stem}-{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')}.json")
    write_json(output, {"variants": str(path), "mode": mode, "shots": shots, "columns": columns})
    return output, columns


def _cell(row, expected):
    if row["status"] == "ok":
        text = "/".join(row["values"])
        return f"**{text}**" if expected is not None and row["values"] != [str(expected)] else text
    return f"{row['status'].upper()}: {row['message'][:70]}"


def variant_lines(columns):
    lines, differing = [], []
    names = [variant["name"] for variant in columns[0]["variants"]]
    for index, name in enumerate(names):
        expected = columns[0]["variants"][index].get("expected")
        lines.append(f"## {name}  expected={expected if expected is not None else '?'}")
        seen = set()
        for column in columns:
            variant = column["variants"][index]
            sim = variant["simulator"]
            sim_text = "/".join(sim["values"]) if sim["status"] == "ok" else sim["status"].upper()
            cells = [f"{p}={_cell(row, expected)}" for p, row in variant["profiles"].items()]
            seen.add(tuple(cells))
            lines.append(f"   {column['label'][:14]:14} {column['sha'][:8]} sim={sim_text:8} " + "  ".join(cells))
        if len(seen) > 1:
            differing.append(name)
    lines.append("")
    lines.append(f"Variants whose outcome differs between revisions: {len(differing)}"
                 + (f" ({', '.join(differing)})" if differing else ""))
    for column in columns:
        for panic in column["panics"]:
            lines.append(f"  {column['label']}: panic at {panic['path']}:{panic['line']}: {panic['message'][:120]}")
    return lines
