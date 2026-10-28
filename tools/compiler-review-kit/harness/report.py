# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Generate an evidence draft, not an automatically adjudicated review."""

import json
from pathlib import Path

from harness.common import ReviewError, read_json


def source_repro(case):
    return f'''import pathlib
import subprocess
import sys

SOURCE = {case["source"]!r}
ENTRY = {case["entry"]!r}
FEATURES = {case.get("language_features")!r}
SHOTS = {case["shots"]}
SEED = {case["seed"]}
PROFILES = {case["profiles"]!r}
CHILD = """
import pathlib, subprocess, sys
from qdk import Context, TargetProfile
profile = sys.argv[1]
ctx = Context(target_profile=getattr(TargetProfile, profile), language_features=FEATURES)
ctx.set_classical_seed(SEED)
ctx.set_quantum_seed(SEED)
ctx.eval(SOURCE)
if profile == "Unrestricted":
    print(ctx.run(ENTRY, SHOTS, seed=SEED))
else:
    qir = str(ctx.compile(ENTRY))
    runner = pathlib.Path(sys.executable).parent / "qir-runner"
    result = subprocess.run([str(runner), "-s", str(SHOTS), "-r", str(SEED)],
                            input=qir, text=True, capture_output=True, timeout=120)
    print(result.stdout, end="")
    print(result.stderr, end="", file=sys.stderr)
    sys.exit(result.returncode)
"""
prefix = f"SOURCE={{SOURCE!r}}\\nENTRY={{ENTRY!r}}\\nFEATURES={{FEATURES!r}}\\nSHOTS={{SHOTS}}\\nSEED={{SEED}}\\n"
for profile in ["Unrestricted"] + PROFILES:
    print(f"=== {{profile}} ===", flush=True)
    try:
        result = subprocess.run([sys.executable, "-c", prefix + CHILD, profile],
                                capture_output=True, text=True, timeout=300)
    except subprocess.TimeoutExpired:
        print("TIMEOUT")
        continue
    print(result.stdout, end="")
    print(result.stderr, end="", file=sys.stderr)
    print(f"exit={{result.returncode}}")
'''


def generate_report(run, output, overwrite=False):
    run = Path(run)
    output = Path(output)
    if output.exists() and not overwrite:
        raise ReviewError(f"Report exists: {output}; use --overwrite only for an owned generated draft.")
    manifest = read_json(run / "manifest.json")
    cases = {case["id"]: case for case in read_json(run / "cases.json")}
    rows = [json.loads(line) for line in (run / "results.jsonl").read_text().splitlines() if line]
    refs = manifest["config"]["refs"]
    lines = [
        "# Compiler review evidence draft",
        "",
        "**This is harness evidence, not a completed review.** Confirm intent, minimize cases, "
        "attribute commits, match issues, and fill in root causes/client impact before publishing.",
        "",
        f"Reference: `{refs['reference']}`  ",
        f"Candidate: `{refs['candidate']}`  ",
        f"Mode: `{manifest['mode']}`; run status: `{manifest['status']}`",
        "",
        "## Summary",
        "",
        "| Case | Profile | Reference | Candidate | Classification |",
        "|---|---|---|---|---|",
    ]
    for row in rows:
        for result in row["comparison"]:
            lines.append(
                f"| {row['case']} | {result['profile']} | {result['reference']} | "
                f"{result['candidate']} | {result['verdict']} |"
            )
    if "historical_reference" in refs:
        lines += [
            "",
            "### Additional historical comparison",
            "",
            f"Historical baseline: `{refs['historical_reference']}`. These transitions are "
            "not automatically attributed to the reviewed reference-to-candidate range.",
            "",
            "| Case | Profile | Historical baseline | Candidate | Classification |",
            "|---|---|---|---|---|",
        ]
        for row in rows:
            for result in row.get("historical") or []:
                lines.append(
                    f"| {row['case']} | {result['profile']} | {result['reference']} | "
                    f"{result['candidate']} | {result['verdict']} |"
                )
    groups = [
        ("1. Regression candidates", {"regression_candidate", "simulator_regression_candidate", "new_failure_unvalidated"}),
        ("2. Fix candidates", {"fix_candidate", "simulator_fix_candidate", "newly_accepted_unvalidated"}),
        ("3. Non-regression failures and inconclusive cases", {"pre_existing_or_changed_failure", "inconclusive"}),
    ]
    for title, classifications in groups:
        lines += ["", f"## {title}", ""]
        matching = [row for row in rows if any(r["verdict"] in classifications for r in row["comparison"])]
        if not matching:
            lines.append("None flagged in this run; this is not proof of absence.")
        for row in matching:
            case = cases[row["case"]]
            lines += [
                f"### {case['id']}",
                "",
                f"Origin: {case.get('origin', 'curated case')}",
                f"Related issues: {case.get('issues', [])}",
                f"Notes: {case.get('notes', '')}",
                f"Oracle: `{json.dumps(case['oracle'])}`",
                "",
                "**Commit attribution:** not yet established.  ",
                "**Client impact:** not yet established.  ",
                "**Root cause / relationship to existing issues:** not yet established.",
                "",
                "```json",
                json.dumps(row["comparison"], indent=2),
                "```",
                "",
            ]
            if "source" in case and "adapter" not in case and "project_root" not in case:
                lines += [
                    "```qsharp",
                    case["source"].rstrip(),
                    "```",
                    "",
                    "Run with the selected revision's pinned overlay using `review.py exec`:",
                    "",
                    "```python",
                    source_repro(case).rstrip(),
                    "```",
                    "",
                ]
            else:
                lines += [
                    "**Repro incomplete for publication:** this case needs its project/adapter "
                    "bundle. Minimize to standalone Q# plus inline Python, or explicitly document "
                    "and bundle every dependency.",
                    "",
                ]
            lines += ["Observed evidence:", "", "```json", json.dumps(row["results"], indent=2), "```", ""]
            if row.get("historical") is not None:
                lines += [
                    "Additional comparison against the historical baseline:",
                    "",
                    "```json",
                    json.dumps(row["historical"], indent=2),
                    "```",
                    "",
                ]
    lines += ["", "## Coverage and limitations", "", f"Run manifest: `{run / 'manifest.json'}`", ""]
    for skipped in manifest["skipped"]:
        lines.append(f"- Excluded `{skipped['id']}`: {skipped['reason']}")
    lines += [
        "",
        "Native compiler suites, issue coverage, debug coverage, generated variants, and client "
        "coverage must be described by the reviewer. `observe`, `compile_only`, and `Unit` output "
        "are not semantic validation. Compiler/runner failures and timeouts require investigation.",
        "",
    ]
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text("\n".join(lines))
