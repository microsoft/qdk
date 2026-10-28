# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Isolated, interleaved, paired compile timing across revisions."""

from datetime import datetime, timezone
import fnmatch
from pathlib import Path
import statistics
import tempfile

from harness.builds import worker_environment
from harness.cases import load_cases
from harness.common import KIT, ReviewError, isolated, preflight, write_json
from harness.execution import prepare_builds

PHASES = ("context", "load", "compile")


def aggregate(sweeps, labels):
    """Keep every repeat and compute unrounded medians/spread for successful series."""
    table = {}
    for label in labels:
        for repeat, sweep in enumerate(sweeps[label]):
            for case_id, profiles in sweep.get("results", {}).items():
                for profile, row in profiles.items():
                    table.setdefault((case_id, profile), {}).setdefault(label, []).append(
                        {"repeat": repeat, **row}
                    )
    rows = []
    for (case_id, profile), by_label in sorted(table.items()):
        row = {"case": case_id, "profile": profile, "revisions": {}}
        for label in labels:
            samples = by_label.get(label, [])
            entry = row["revisions"][label] = {"samples": samples}
            failed = next((sample for sample in samples if sample["status"] != "ok"), None)
            if samples and failed is None:
                entry.update({phase: statistics.median(sample[phase] for sample in samples) for phase in PHASES})
                entry["spread"] = {}
                for phase in PHASES:
                    values = [sample[phase] for sample in samples]
                    entry["spread"][phase] = {
                        "min": min(values), "max": max(values),
                        "stdev": statistics.stdev(values) if len(values) > 1 else None,
                    }
            else:
                entry.update(status=failed["status"] if failed else "missing",
                             message=failed.get("message", "") if failed else "")
        rows.append(row)
    return rows


def ratios(rows, baseline, label, phase="compile"):
    """Median of paired repeat ratios, not a ratio of rounded medians."""
    out = []
    for row in rows:
        a, b = row["revisions"].get(baseline, {}), row["revisions"].get(label, {})
        if phase in a and phase in b and a[phase] > 0:
            if "samples" not in a or "samples" not in b:
                raise ReviewError("Timing data has no raw repeats; rerun timing to obtain paired ratios.")
            reference = {sample["repeat"]: sample[phase] for sample in a["samples"]}
            candidate = {sample["repeat"]: sample[phase] for sample in b["samples"]}
            paired = [candidate[repeat] / reference[repeat] for repeat in sorted(reference.keys() & candidate.keys())
                      if reference[repeat] > 0]
            if not paired:
                continue
            out.append({"case": row["case"], "profile": row["profile"], "baseline": a[phase], "value": b[phase],
                        "ratio": statistics.median(paired), "pairs": len(paired)})
    return out


def run_timing(config, cases_path, targets, profiles, repeats, mode, patterns=()):
    if repeats < 1:
        raise ReviewError("--repeats must be positive")
    labels = [label for label, _ in targets]
    if not labels or any(not label for label in labels) or len(set(labels)) != len(labels):
        raise ReviewError("Timing targets must have unique, non-empty labels")
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
    prepared = prepare_builds(config, targets, mode, tools=tools)
    builds = [(label, prepared[sha]) for label, sha in targets]
    sweeps = {label: [] for label in labels}
    limit = config["limits"]["compile_seconds"] * max(1, len(cases)) * len(profiles)
    summary = {
        "targets": [{"label": label, "sha": build["sha"]} for label, build in builds],
        "mode": mode, "repeats": repeats, "profiles": profiles, "cases": len(cases), "skipped": skipped,
        "tools": tools, "status": "running", "workers": [], "ratio_method": "median_of_paired_ratios",
    }

    def save():
        summary["rows"] = aggregate(sweeps, labels)
        write_json(directory / "timing.json", summary)

    save()
    try:
        for repeat in range(repeats):
            # Rotate so no revision always runs first (warm caches) or last (load spikes).
            order = builds[repeat % len(builds):] + builds[:repeat % len(builds)]
            for label, build in order:
                result = isolated(
                    [str(Path(config["venv"]) / "bin/python"), str(KIT / "harness/timing_worker.py")],
                    payload={"cases": cases, "profiles": profiles},
                    env=worker_environment(config, build), cwd=str(directory), timeout=limit,
                )
                sweeps[label].append(result)
                summary["workers"].append({"label": label, "repeat": repeat,
                                           **{key: value for key, value in result.items() if key != "results"}})
                save()
                if result.get("status") != "ok":
                    raise ReviewError(f"Timing worker failed at {label}: {result.get('status')} "
                                      f"{(result.get('message') or result.get('stderr') or '')[-2000:]}")
                if Path(result["qdk"]).resolve() != (Path(build["overlay"]) / "qdk/__init__.py").resolve():
                    raise ReviewError(f"Timing worker imported the wrong qdk: {result['qdk']}")
                print(f"[{repeat + 1}/{repeats}] {label}: {len(cases)} cases", flush=True)
        summary["status"] = "complete"
    except BaseException:
        summary["status"] = "failed"
        raise
    finally:
        save()
    return directory, summary


def print_timing(summary, show=15, outlier=1.5):
    if summary.get("status", "complete") != "complete":
        print(f"Timing run {summary['status']}; partial samples retained, no performance ratios reported.")
        return
    labels = [target["label"] for target in summary["targets"]]
    baseline = labels[0]
    print(f"{summary['cases']} cases x {len(summary['profiles'])} profiles, {summary['repeats']} interleaved repeats; "
          f"unrounded medians and paired ratios; baseline {baseline}")
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
            row = next(row for row in summary["rows"] if (row["case"], row["profile"]) == (pair["case"], pair["profile"]))
            ranges = [row["revisions"][revision]["spread"]["compile"] for revision in (baseline, label)]
            spread = " -> ".join(f"[{value['min']:.6g}, {value['max']:.6g}]" for value in ranges)
            print(f"    {pair['ratio']:6.2f}x {pair['case']} [{pair['profile']}] "
                  f"{pair['baseline']:.6g}s -> {pair['value']:.6g}s; n={pair['pairs']}; ranges {spread}s")
    failures = [(row, label) for row in summary["rows"] for label in labels if "status" in row["revisions"][label]]
    if failures:
        print(f"  {len(failures)} case/profile/revision combinations did not compile (excluded from ratios)")
