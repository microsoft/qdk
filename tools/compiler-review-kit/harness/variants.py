# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Translate trusted variant definitions into ordinary frozen cases and run them."""

import importlib.util
from pathlib import Path
import tempfile

from harness.analysis import load_run
from harness.cases import write_project
from harness.common import ReviewError, digest, read_json, write_json
from harness.execution import parse_panics
from harness.records import outcome_key
from harness.runner import create_run
from harness.tables import cell, simulator_cell


def load_variants(path):
    spec = importlib.util.spec_from_file_location("review_variants", path)
    if spec is None or spec.loader is None:
        raise ReviewError(f"Cannot load variants file: {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    variants = getattr(module, "VARIANTS", None)
    if not isinstance(variants, list) or not variants:
        raise ReviewError("Variants file must define a non-empty VARIANTS list")
    names = [v.get("name") for v in variants]
    if any(not isinstance(n, str) or not n for n in names) or len(set(names)) != len(names):
        raise ReviewError("Every variant needs a unique non-empty name")
    return module, variants


def run_variants(config, path, targets, profiles, mode, shots=None):
    path = Path(path).resolve()
    source = path.read_text()
    work = Path(config["work_dir"])
    work.mkdir(parents=True, exist_ok=True)
    metadata, cases = [], []
    with tempfile.TemporaryDirectory(prefix="variants-", dir=work) as temporary:
        temporary = Path(temporary)
        definitions = temporary / "definitions.py"
        definitions.write_text(source)
        module, variants = load_variants(definitions)
        if profiles is None:
            profiles = config["profiles"] if "profiles" in config["input"] else (
                getattr(module, "PROFILES", None) or config["profiles"]
            )
        project = getattr(module, "PROJECT", None)
        if project:
            write_project(project, temporary / "project")
        for index, variant in enumerate(variants):
            name = variant["name"]
            adapter = temporary / f"variant-{index}.py"
            adapter.write_text(source + f'''

def prepare(ctx):
    variant = VARIANTS[{index}]
    if "build" in variant:
        return variant["build"](ctx)
    return variant["entry"], ()
''')
            case_id = "v-" + digest(name)[:12]
            case = {
                "id": case_id, "adapter": str(adapter),
                "source": getattr(module, "PRELUDE", "") + variant.get("source", variant.get("body", "")),
                "profiles": variant.get("profiles", profiles),
                "shots": shots or config["limits"]["shots"], "seed": config["limits"]["seed"],
                "oracle": {"kind": "observe"}, "origin": f"variant {name}",
            }
            if project:
                case["project_root"] = str(temporary / "project")
            cases.append(case)
            metadata.append({"id": case_id, "name": name, "expected": variant.get("expected")})
        write_json(temporary / "cases.json", cases)
        directory = create_run(config, temporary / "cases.json", mode, targets=targets)
    manifest = read_json(directory / "manifest.json")
    manifest["variants"] = metadata
    write_json(directory / "manifest.json", manifest)
    _, _, rows = load_run(directory)
    columns = []
    for label, sha in targets:
        entries, panics = [], {}
        for variant in metadata:
            if variant["id"] not in rows:
                continue  # explicit case exclusions
            result = rows[variant["id"]]["results"][label]
            entries.append({"name": variant["name"], "expected": variant["expected"], "result": result})
            stages = [result.get("simulator", {})]
            stages += [record for profile in result["profiles"].values() for record in profile.values()]
            for record in stages:
                for panic in parse_panics(record.get("stderr", "")):
                    panics[(panic["path"], panic["line"], panic["message"])] = panic
        columns.append({"label": label, "sha": sha, "variants": entries, "panics": list(panics.values())})
    return directory, columns


def variant_lines(columns):
    lines, differing = [], []
    for index, variant in enumerate(columns[0]["variants"]):
        name, expected = variant["name"], variant.get("expected")
        lines.append(f"## {name}  expected={expected if expected is not None else '?'}")
        seen = set()
        for column in columns:
            result = column["variants"][index]["result"]
            cells = [f"{p}={cell(row, str(expected) if expected is not None else None)}"
                     for p, row in result["profiles"].items()]
            seen.add(outcome_key(result))
            lines.append(f"   {column['label'][:14]:14} {column['sha'][:8]} "
                         f"sim={simulator_cell(result):8} " + "  ".join(cells))
        if len(seen) > 1:
            differing.append(name)
    lines += ["", f"Variants whose outcome differs between revisions: {len(differing)}"
              + (f" ({', '.join(differing)})" if differing else "")]
    for column in columns:
        for panic in column["panics"]:
            lines.append(f"  {column['label']}: panic at {panic['path']}:{panic['line']}: {panic['message'][:120]}")
    return lines
