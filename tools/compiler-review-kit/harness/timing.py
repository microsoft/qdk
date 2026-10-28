# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Isolated, interleaved, paired compile timing across revisions."""

from datetime import datetime, timezone
import fnmatch
from pathlib import Path
import statistics
import tempfile

from harness.builds import build_revision, worker_environment
from harness.cases import load_cases
from harness.common import KIT, ReviewError, isolated, preflight, write_json

PHASES = ("context", "load", "compile")


def aggregate(sweeps, labels):
    """sweeps: {label: [worker results]} -> per case/profile medians per label, plus failures."""
    table = {}
    for label in labels:
        for sweep in sweeps[label]:
            for case_id, profiles in sweep["results"].items():
                for profile, row in profiles.items():
                    entry = table.setdefault((case_id, profile), {}).setdefault(label, {"samples": [], "status": None})
                    if row["status"] == "ok":
                        entry["samples"].append(row)
                    else:
                        entry["status"] = row["status"]
                        entry["message"] = row.get("message", "")
    rows = []
    for (case_id, profile), by_label in sorted(table.items()):
        row = {"case": case_id, "profile": profile, "revisions": {}}
        for label in labels:
            entry = by_label.get(label, {"samples": [], "status": "missing"})
            if entry["samples"] and not entry["status"]:
                row["revisions"][label] = {
                    phase: round(statistics.median(s[phase] for s in entry["samples"]), 4) for phase in PHASES
                }
            else:
                row["revisions"][label] = {"status": entry["status"] or "missing", "message": entry.get("message", "")}
        rows.append(row)
    return rows


def ratios(rows, baseline, label, phase="compile"):
    """Per-row ratio label/baseline for rows where both revisions measured the phase."""
    out = []
    for row in rows:
        a, b = row["revisions"].get(baseline, {}), row["revisions"].get(label, {})
        if phase in a and phase in b and a[phase] > 0:
            out.append({"case": row["case"], "profile": row["profile"], "baseline": a[phase], "value": b[phase],
                        "ratio": round(b[phase] / a[phase], 3)})
    return out


def run_timing(config, cases_path, targets, profiles, repeats, mode, patterns=()):
    if repeats < 1:
        raise ReviewError("--repeats must be positive")
    tools = preflight(config)
    root = Path(config["work_dir"]) / "timing"
    root.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix=datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ-"), dir=root))
    cases, skipped = load_cases(cases_path, config, directory / "inputs")
    if patterns:
        cases = [case for case in cases if any(fnmatch.fnmatch(case["id"], p) for p in patterns)]
    if not cases:
        raise ReviewError("No cases selected")
    profiles = profiles or config["profiles"]
    builds = [(label, build_revision(config, sha, mode, tools)) for label, sha in targets]
    labels = [label for label, _ in builds]
    sweeps = {label: [] for label in labels}
    limit = config["limits"]["compile_seconds"] * max(1, len(cases)) * len(profiles)
    for repeat in range(repeats):
        # Rotate the order so no revision always runs first (warm caches) or last (load spikes).
        order = builds[repeat % len(builds):] + builds[:repeat % len(builds)]
        for label, build in order:
            result = isolated(
                [str(Path(config["venv"]) / "bin/python"), str(KIT / "harness/timing_worker.py")],
                payload={"cases": cases, "profiles": profiles},
                env=worker_environment(config, build), cwd=str(directory), timeout=limit,
            )
            if result.get("status") != "ok":
                raise ReviewError(f"Timing worker failed at {label}: {result.get('status')} "
                                  f"{(result.get('message') or result.get('stderr') or '')[-2000:]}")
            if Path(result["qdk"]).resolve() != (Path(build["overlay"]) / "qdk/__init__.py").resolve():
                raise ReviewError(f"Timing worker imported the wrong qdk: {result['qdk']}")
            sweeps[label].append(result)
            print(f"[{repeat + 1}/{repeats}] {label}: {len(cases)} cases", flush=True)
    rows = aggregate(sweeps, labels)
    summary = {
        "targets": [{"label": label, "sha": build["sha"]} for label, build in builds],
        "mode": mode, "repeats": repeats, "profiles": profiles, "cases": len(cases), "skipped": skipped,
        "rows": rows,
    }
    write_json(directory / "timing.json", summary)
    return directory, summary


def print_timing(summary, show=15, outlier=1.5):
    labels = [target["label"] for target in summary["targets"]]
    baseline = labels[0]
    print(f"{summary['cases']} cases x {len(summary['profiles'])} profiles, {summary['repeats']} interleaved repeats; "
          f"medians; baseline {baseline}")
    for label in labels[1:]:
        for phase in PHASES:
            pairs = ratios(summary["rows"], baseline, label, phase)
            if pairs:
                values = [pair["ratio"] for pair in pairs]
                print(f"  {label} / {baseline} {phase:7}: median ratio {statistics.median(values):.2f} "
                      f"(min {min(values):.2f}, max {max(values):.2f}, n={len(values)})")
        worst = sorted((p for p in ratios(summary["rows"], baseline, label) if p["ratio"] >= outlier),
                       key=lambda p: -p["ratio"])
        print(f"  {label}: {len(worst)} compile outliers >= {outlier}x")
        for pair in worst[:show]:
            print(f"    {pair['ratio']:6.2f}x {pair['case']} [{pair['profile']}] {pair['baseline']}s -> {pair['value']}s")
    failures = [(row, label) for row in summary["rows"] for label in labels if "status" in row["revisions"][label]]
    if failures:
        print(f"  {len(failures)} case/profile/revision combinations did not compile (excluded from ratios)")
