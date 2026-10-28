# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Per-case, per-revision outcome tables over one or more runs."""

from collections import Counter
import fnmatch
import re

from harness.analysis import load_run
from harness.common import PROFILES, ordered_roles


def format_value(value):
    """Typed record value -> compact text: results as 0/1, arrays as [..], tuples as (..)."""
    kind = value.get("type")
    if kind in ("result", "int"):
        return str(value["value"])
    if kind == "bool":
        return str(value["value"]).lower()
    if kind == "double":
        return repr(float.fromhex(value["value"]))
    if kind == "array":
        return "[" + ", ".join(format_value(item) for item in value["items"]) + "]"
    if kind == "tuple":
        return "(" + ", ".join(format_value(item) for item in value["items"]) + ")"
    if kind == "unit":
        return "()"
    return str(value.get("value"))


def _first_line(text, width):
    line = next((part.strip() for part in (text or "").splitlines() if part.strip()), "")
    line = re.sub(r"\s*\(https://aka\.ms[^)]*\)", "", line)
    return line if len(line) <= width else line[: width - 3] + "..."


def _values(values):
    counts = Counter(format_value(value) for value in values)
    if len(counts) == 1:
        return next(iter(counts))
    return "/".join(f"{text}x{count}" for text, count in sorted(counts.items()))


def cell(record, expected, width=60):
    """One profile's outcome: values (marked **wrong**), or a failure kind with its message."""
    if record is None:
        return "-"
    compiled = record.get("compile", {})
    status = compiled.get("status")
    if status != "ok":
        message = compiled.get("message") or compiled.get("stderr") or ""
        if status == "diagnostic":
            code = re.search(r"Qdk\.Qsc\.[\w.]+", message)
            message = code.group(0) if code else message
        return f"{(status or 'missing').upper()}: {_first_line(message, width)}"
    ran = record.get("runner")
    if ran is None:
        return "compiled"
    if ran.get("status") != "ok":
        return f"RUN {ran.get('status')}: {_first_line(ran.get('message') or ran.get('runner_stderr'), width)}"
    text = _values(ran.get("values", []))
    if expected is not None and expected != {"type": "unit"} and any(v != expected for v in ran.get("values", [])):
        return f"**{text}**"
    return text


def simulator_cell(result):
    sim = result.get("simulator")
    if not sim:
        return "-"
    if sim.get("status") != "ok":
        return f"{sim.get('status', 'missing').upper()}: {_first_line(sim.get('message') or sim.get('stderr'), 40)}"
    return _values(sim.get("values", []))


def run_table(run, profiles=None, roles=None, only_diff=False, patterns=(), width=60):
    """Lines describing every case of a run: one header per case, one line per revision."""
    manifest, cases, rows = load_run(run)
    refs = manifest["config"]["refs"]
    lines = []
    for case_id, row in rows.items():
        if patterns and not any(fnmatch.fnmatch(case_id, pattern) for pattern in patterns):
            continue
        case = cases.get(case_id, {})
        expected = case.get("oracle", {}).get("expected")
        shown = [role for role in ordered_roles(row["results"]) if not roles or role in roles]
        chosen = [p for p in (profiles or PROFILES) if any(p in row["results"][r].get("profiles", {}) for r in shown)]
        body = []
        for role in shown:
            result = row["results"][role]
            cells = [f"{p}={cell(result.get('profiles', {}).get(p), expected, width)}" for p in chosen]
            body.append((role, refs.get(role, result.get("sha", ""))[:8], simulator_cell(result), cells))
        if only_diff and len({tuple(cells) for *_, cells in body}) <= 1:
            continue
        lines.append(f"## {case_id}  expected={format_value(expected) if expected else '?'}")
        for role, sha, sim, cells in body:
            lines.append(f"   {role[:20]:20} {sha} sim={sim:10} " + "  ".join(cells))
    return lines
