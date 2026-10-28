# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Generate an evidence draft, not an automatically adjudicated review."""

import json
from pathlib import Path

from harness.common import ReviewError, read_json
from harness.cases import read_run_cases
from harness.repros import render_repro
from harness.tables import format_value


def source_repro(case, limits=None):
    expected = case["oracle"].get("expected")
    text = format_value(expected) if expected is not None and expected != {"type": "unit"} else None
    return render_repro([case], text, ["simulator", *case["profiles"]], case["id"], limits=limits)


def generate_report(run, output, overwrite=False):
    run = Path(run)
    output = Path(output)
    if output.exists() and not overwrite:
        raise ReviewError(f"Report exists: {output}; use --overwrite only for an owned generated draft.")
    manifest = read_json(run / "manifest.json")
    cases = {case["id"]: case for case in read_run_cases(run)}
    rows = [json.loads(line) for line in (run / "results.jsonl").read_text().splitlines() if line]
    refs = manifest["config"]["refs"]
    lines = [
        "# Compiler review evidence draft",
        "",
        "**This is harness evidence, not a completed review.** Confirm intent, minimize cases, "
        "identify responsible PRs in stacks, match issues, and fill in root causes/client impact before publishing. "
        "Individual-commit attribution and debug builds are only needed if the user requests them.",
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
                "**Attribution:** identify the responsible PR for stacks; individual commits only if requested.  ",
                "**Client impact:** not yet established.  ",
                "**Root cause / relationship to existing issues:** not yet established.",
                "",
                "```json",
                json.dumps(row["comparison"], indent=2),
                "```",
                "",
            ]
            if "source" in case:
                lines += [
                    "```qsharp",
                    case["source"].rstrip(),
                    "```",
                    "",
                ]
            expected = case["oracle"].get("expected")
            if case["oracle"]["kind"] == "deterministic" and expected is not None and expected != {"type": "unit"}:
                lines += [
                    "Run with the selected revision's pinned overlay using `review.py exec`:",
                    "", "```python", source_repro(case, manifest["config"].get("limits")).rstrip(), "```", "",
                ]
            else:
                lines += ["Add an explicit non-Unit deterministic expectation before generating a self-checking repro. "
                          "The frozen case bundle remains available for ordinary replay.", ""]
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
        "Native compiler suites, issue coverage, requested debug coverage, generated variants, and client "
        "coverage must be described by the reviewer. `observe`, `compile_only`, and `Unit` output "
        "are not semantic validation. Compiler/runner failures and timeouts require investigation.",
        "",
    ]
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text("\n".join(lines))
