# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Summaries over completed runs: transitions, failure signatures, run diffs, and coverage shape."""

from collections import Counter, defaultdict
import json
from pathlib import Path
import re
import statistics

from harness.common import PROFILES, ReviewError, read_json
from harness.cases import case_identity, read_run_cases

# Base ⊂ Adaptive_RI ⊂ Adaptive_RIF ⊂ Adaptive: a program accepted by a lower
# profile is expected to remain valid on every higher one.
HIERARCHY = PROFILES
BAD_VERDICTS = {"regression_candidate", "simulator_regression_candidate", "new_failure_unvalidated"}
GOOD_VERDICTS = {"fix_candidate", "simulator_fix_candidate", "newly_accepted_unvalidated"}
# Failures that announce themselves; a change from one of these to wrong_result hides a bug.
LOUD = {"panic", "rejected", "compiler_crash", "compile_timeout", "execution_failure", "execution_timeout",
        "crash", "diagnostic", "runtime_error", "timeout"}
FAILED = LOUD | {"wrong_result"}
# Defaults for per-case compile-time outliers (whole compile stage, including process start).
OUTLIER_RATIO = 2.0
OUTLIER_SECONDS = 0.25
EXTRACTED = re.compile(r"^([0-9a-f]{12}) (\S+) test (\S+)")


def load_run(run):
    run = Path(run)
    manifest = read_json(run / "manifest.json")
    cases = {case["id"]: case for case in read_run_cases(run)} if (run / "cases.json").exists() else {}
    rows = {}
    results = run / "results.jsonl"
    if results.exists():
        for line in results.read_text().splitlines():
            if line:
                row = json.loads(line)
                rows[row["case"]] = row
    return manifest, cases, rows


def normalize_signature(message, limit=160):
    """A stable grouping key: first non-empty line, numbers replaced by N, whitespace collapsed."""
    text = re.sub(r"\x1b\[[0-9;]*m", "", message or "")
    line = next((part.strip() for part in text.splitlines() if part.strip()), "")
    line = re.sub(r"0x[0-9a-fA-F]+|\d+", "N", line)
    line = re.sub(r"\s+", " ", line)
    return line[:limit]


def outcomes(row, side):
    return {comparison["profile"]: comparison[side] for comparison in row["comparison"]}


def stage_message(result, profile):
    """The most specific message for a non-validated endpoint, for signature grouping."""
    if profile == "Unrestricted":
        sim = result.get("simulator", {})
        if sim.get("status") == "ok":
            return ""
        return sim.get("message") or sim.get("stderr") or sim.get("status", "")
    record = result["profiles"].get(profile, {})
    compiled = record.get("compile", {})
    if compiled.get("status") != "ok":
        return compiled.get("message") or compiled.get("stderr") or compiled.get("status", "")
    ran = record.get("runner", {})
    if ran.get("status") not in (None, "ok"):
        return ran.get("message") or ran.get("runner_stderr") or ran.get("status", "")
    return ""


def hierarchy_violations(statuses):
    """Profiles that fail although a lower profile in the hierarchy validated the same program."""
    present = [profile for profile in HIERARCHY if profile in statuses]
    violations = []
    for index, lower in enumerate(present):
        if statuses[lower] != "validated":
            continue
        for higher in present[index + 1 :]:
            if statuses[higher] != "validated":
                violations.append((lower, higher, statuses[higher]))
    return violations


def _compile_seconds(result, profile):
    compiled = result.get("profiles", {}).get(profile, {}).get("compile", {})
    return compiled.get("seconds") if compiled.get("status") == "ok" else None


def summarize_run(run, examples=5, outlier_ratio=OUTLIER_RATIO, outlier_seconds=OUTLIER_SECONDS):
    manifest, cases, rows = load_run(run)
    if not {"reference", "candidate"} <= manifest["config"]["refs"].keys():
        raise ReviewError("Summary requires reference/candidate targets; use table for other target labels.")
    candidate_sha = manifest["config"]["refs"]["candidate"]
    verdicts = Counter()
    transitions = defaultdict(Counter)
    signatures = defaultdict(lambda: {"count": 0, "reference": Counter(), "examples": []})
    flagged, silenced, lost, outliers, pipeline = [], [], [], [], defaultdict(list)
    hierarchy = {"reference": [], "candidate": []}
    hierarchy_groups = defaultdict(lambda: {"count": 0, "reference": Counter(), "examples": []})
    layer_hazards = defaultdict(lambda: defaultdict(list))
    qir = Counter()
    timings = defaultdict(list)
    for case_id, row in rows.items():
        candidate = row["results"]["candidate"]
        for comparison in row["comparison"]:
            profile, old, new, verdict = (
                comparison["profile"], comparison["reference"], comparison["candidate"], comparison["verdict"]
            )
            verdicts[verdict] += 1
            transitions[profile][(old, new)] += 1
            if verdict in BAD_VERDICTS:
                flagged.append({
                    "case": case_id, "profile": profile, "reference": old, "candidate": new,
                    "signature": normalize_signature(stage_message(candidate, profile)),
                })
            if old in LOUD and new == "wrong_result":
                silenced.append({
                    "case": case_id, "profile": profile, "reference": old,
                    "signature": normalize_signature(stage_message(row["results"]["reference"], profile)),
                })
            if new not in ("validated", "validated_unchanged", "ok") and not new.endswith("_unvalidated"):
                key = (profile, new, normalize_signature(stage_message(candidate, profile)))
                group = signatures[key]
                group["count"] += 1
                group["reference"][old] += 1
                if len(group["examples"]) < examples:
                    group["examples"].append(case_id)
            if profile != "Unrestricted":
                a, b = (row["results"]["reference"].get("profiles", {}).get(profile, {}).get("compile", {}),
                        candidate.get("profiles", {}).get(profile, {}).get("compile", {}))
                if a.get("qir_digest") and b.get("qir_digest"):
                    kind = cases.get(case_id, {}).get("oracle", {}).get("kind", "deterministic")
                    same = a["qir_digest"] == b["qir_digest"]
                    qir["identical" if same else "different"] += 1
                    if kind != "deterministic":
                        qir["unvalidated_identical" if same else "unvalidated_different"] += 1
                old_s, new_s = _compile_seconds(row["results"]["reference"], profile), _compile_seconds(candidate, profile)
                if old_s and new_s and new_s / old_s >= outlier_ratio and new_s - old_s >= outlier_seconds:
                    outliers.append({"case": case_id, "profile": profile, "reference": round(old_s, 3),
                                     "candidate": round(new_s, 3), "ratio": round(new_s / old_s, 2)})
        for comparison in row.get("previous") or []:
            old, new = comparison["reference"], comparison["candidate"]
            if comparison["profile"] != "Unrestricted" and (
                (old == "validated" and new in FAILED) or (old in LOUD and new == "wrong_result")
            ):
                lost.append({"case": case_id, "profile": comparison["profile"], "previous": old, "candidate": new,
                             "signature": normalize_signature(stage_message(candidate, comparison["profile"]))})
        reference_status = outcomes(row, "reference")
        candidate_status = outcomes(row, "candidate")
        for role, statuses in (row.get("layers") or {}).items():
            for profile, status in statuses.items():
                # A layer that breaks what both ends get right, or silences a loud reference failure.
                correct = "ok" if profile == "Unrestricted" else "validated"
                if status in FAILED and candidate_status.get(profile) == correct and (
                    reference_status.get(profile) == correct or status == "wrong_result"
                ):
                    layer_hazards[role][(profile, reference_status.get(profile), status)].append(case_id)
        for side in ("reference", "candidate"):
            highest_lower = {}
            for lower, higher, status in hierarchy_violations(outcomes(row, side)):
                hierarchy[side].append({"case": case_id, "lower": lower, "higher": higher, "status": status})
                highest_lower[(higher, status)] = lower  # violations are ordered from the lowest profile up
            if side == "candidate":
                # One entry per program and failing profile, naming the highest profile that validated.
                for (higher, status), lower in highest_lower.items():
                    signature = normalize_signature(stage_message(candidate, higher))
                    group = hierarchy_groups[(lower, higher, status, signature)]
                    group["count"] += 1
                    group["reference"][reference_status.get(higher)] += 1
                    if len(group["examples"]) < examples:
                        group["examples"].append(case_id)
        origin = EXTRACTED.match(cases.get(case_id, {}).get("origin", ""))
        if origin and candidate_sha.startswith(origin.group(1)):
            bad = sorted({status for profile, status in candidate_status.items()
                          if profile != "Unrestricted" and status in FAILED - {"rejected"}})
            if bad:
                pipeline[origin.group(3)].append({"case": case_id, "statuses": bad})
        for role, result in row["results"].items():
            for profile, record in result.get("profiles", {}).items():
                seconds = record.get("compile", {}).get("seconds")
                if seconds is not None:
                    timings[(role, profile)].append(seconds)
    reference_keys = {(v["case"], v["lower"], v["higher"]) for v in hierarchy["reference"]}
    new_violations = [v for v in hierarchy["candidate"] if (v["case"], v["lower"], v["higher"]) not in reference_keys]
    roles = manifest.get("roles") or list(dict.fromkeys(role for row in rows.values() for role in row["results"]))
    return {
        "run": str(run),
        "reference": manifest["config"]["refs"]["reference"],
        "candidate": manifest["config"]["refs"]["candidate"],
        "roles": {role: manifest["config"]["refs"].get(role) for role in roles},
        "status": manifest.get("status"),
        "cases": len(rows),
        "verdicts": dict(verdicts),
        "transitions": {
            profile: [{"reference": old, "candidate": new, "count": n} for (old, new), n in counter.most_common()]
            for profile, counter in transitions.items()
        },
        "flagged": flagged,
        "silenced": silenced,
        "lost_fixes": lost,
        "layer_hazards": {
            role: [{"profile": p, "reference": old, "layer": status, "count": len(ids), "examples": ids[:examples]}
                   for (p, old, status), ids in sorted(groups.items(), key=lambda item: -len(item[1]))]
            for role, groups in layer_hazards.items()
        },
        "candidate_failures": [
            {"profile": p, "status": s, "signature": sig, "count": g["count"],
             "reference": dict(g["reference"]), "examples": g["examples"]}
            for (p, s, sig), g in sorted(signatures.items(), key=lambda item: (-item[1]["count"], item[0]))
        ],
        "hierarchy": {
            "reference": len(hierarchy["reference"]),
            "candidate": len(hierarchy["candidate"]),
            "new_at_candidate": new_violations,
            "candidate_groups": [
                {"lower": lo, "higher": hi, "status": st, "signature": sig, "count": g["count"],
                 "reference": dict(g["reference"]), "examples": g["examples"]}
                for (lo, hi, st, sig), g in sorted(hierarchy_groups.items(), key=lambda item: -item[1]["count"])
            ],
        },
        "qir_identity": dict(qir),
        "pipeline_gaps": [
            {"test": test, "count": len(items), "statuses": sorted({s for item in items for s in item["statuses"]}),
             "examples": [item["case"] for item in items[:examples]]}
            for test, items in sorted(pipeline.items(), key=lambda item: -len(item[1]))
        ],
        "compile_outliers": sorted(outliers, key=lambda item: -item["ratio"]),
        "compile_outlier_thresholds": {"ratio": outlier_ratio, "seconds": outlier_seconds},
        "compile_seconds_median": {
            f"{role}/{profile}": round(statistics.median(values), 3) for (role, profile), values in sorted(timings.items())
        },
    }


def print_summary(summary, show=10):
    print(f"Run {summary['run']} ({summary['status']}): {summary['cases']} cases")
    print(f"Reference {summary['reference'][:12]}  candidate {summary['candidate'][:12]}")
    extra = {role: sha for role, sha in summary.get("roles", {}).items() if role not in ("reference", "candidate")}
    if extra:
        print("Other columns: " + ", ".join(f"{role} {sha[:12]}" for role, sha in extra.items()))
    print("Verdicts:", ", ".join(f"{k}={v}" for k, v in sorted(summary["verdicts"].items())))
    print("\nReference -> candidate outcomes per profile:")
    for profile, rows in summary["transitions"].items():
        cells = ", ".join(f"{r['reference']}->{r['candidate']}: {r['count']}" for r in rows)
        print(f"  {profile}: {cells}")
    print(f"\nFlagged regressions ({len(summary['flagged'])}):")
    for item in summary["flagged"][:show]:
        print(f"  {item['case']} [{item['profile']}] {item['reference']} -> {item['candidate']}: {item['signature']}")
    silenced = summary.get("silenced", [])
    print(f"\nLoud -> silent (reference panicked or rejected; candidate returns a wrong result): {len(silenced)}")
    for item in silenced[:show]:
        print(f"  {item['case']} [{item['profile']}] {item['reference']} -> wrong_result; reference: {item['signature']}")
    if "previously_reviewed" in summary.get("roles", {}):
        lost = summary.get("lost_fixes", [])
        print(f"\nLost fixes (previously reviewed candidate validated or failed loudly; candidate fails/is wrong): {len(lost)}")
        for item in lost[:show]:
            print(f"  {item['case']} [{item['profile']}] {item['previous']} -> {item['candidate']}: {item['signature']}")
    for role, groups in summary.get("layer_hazards", {}).items():
        sha = summary.get("roles", {}).get(role, "")
        print(f"\nIntermediate-layer hazards at {role} {sha[:12]} (broken there, correct at the candidate): "
              f"{sum(g['count'] for g in groups)}")
        for group in groups[:show]:
            print(f"  {group['count']:5} [{group['profile']}] reference {group['reference']}, layer {group['layer']}; "
                  f"e.g. {', '.join(group['examples'])}")
    print(f"\nCandidate failures by signature ({len(summary['candidate_failures'])} groups):")
    for group in summary["candidate_failures"][:show]:
        print(f"  {group['count']:5} [{group['profile']}] {group['status']}: {group['signature']}")
        print(f"        reference {group['reference']}; e.g. {', '.join(group['examples'])}")
    hierarchy = summary["hierarchy"]
    print(
        f"\nProfile-hierarchy violations (validates on a lower profile, fails on a higher one): "
        f"reference {hierarchy['reference']}, candidate {hierarchy['candidate']}, "
        f"new at candidate {len(hierarchy['new_at_candidate'])}"
    )
    for item in hierarchy["new_at_candidate"][:show]:
        print(f"  {item['case']}: {item['lower']} validated, {item['higher']} {item['status']}")
    if hierarchy.get("candidate_groups"):
        print("  Candidate violations by higher-profile signature:")
        for group in hierarchy["candidate_groups"][:show]:
            print(f"  {group['count']:5} {group['lower']} validated, {group['higher']} {group['status']}: "
                  f"{group['signature']}; reference {group['reference']}; e.g. {', '.join(group['examples'])}")
    gaps = summary.get("pipeline_gaps", [])
    if gaps:
        print(f"\nCandidate's own extracted test programs that fail end to end ({sum(g['count'] for g in gaps)} programs; "
              "the cargo tests pass, so their helpers skip part of the user pipeline):")
        for gap in gaps[:show]:
            print(f"  {gap['count']:5} {gap['test']}: {', '.join(gap['statuses'])}; e.g. {', '.join(gap['examples'])}")
    identity = summary.get("qir_identity", {})
    if identity:
        total = identity.get("identical", 0) + identity.get("different", 0)
        print(f"\nIdentical QIR at both endpoints: {identity.get('identical', 0)}/{total} compilations"
              + (f" ({identity.get('unvalidated_identical', 0)}/"
                 f"{identity.get('unvalidated_identical', 0) + identity.get('unvalidated_different', 0)} "
                 "for observe/compile-only cases, where identity is the only evidence)"
                 if identity.get("unvalidated_identical", 0) + identity.get("unvalidated_different", 0) else ""))
    outliers = summary.get("compile_outliers", [])
    limits = summary.get("compile_outlier_thresholds", {"ratio": OUTLIER_RATIO, "seconds": OUTLIER_SECONDS})
    print(f"\nCompile-time outliers (candidate >= {limits['ratio']}x and +{limits['seconds']}s over the reference, "
          f"same case and profile): {len(outliers)}")
    for item in outliers[:show]:
        print(f"  {item['ratio']:6.2f}x {item['case']} [{item['profile']}] {item['reference']}s -> {item['candidate']}s")
    if outliers:
        print("  Concurrent sweep timings; confirm with `review.py timing` before reporting.")
    print("\nMedian compile seconds (concurrent sweep; not a performance measurement):")
    for key, value in summary["compile_seconds_median"].items():
        print(f"  {key}: {value}")


def _run_refs(manifest):
    return manifest["config"]["refs"]


def _check_inputs(manifest_a, cases_a, manifest_b, cases_b, ids):
    if manifest_a["mode"] != manifest_b["mode"]:
        raise ReviewError("Cannot compare runs with different build modes")
    projects = {}
    for case_id in ids:
        if manifest_a["case_identities"][case_id] != manifest_b["case_identities"][case_id]:
            # Coverage can differ; comparisons already select shared profile columns.
            a = {key: value for key, value in cases_a[case_id].items() if key != "profiles"}
            b = {key: value for key, value in cases_b[case_id].items() if key != "profiles"}
            if case_identity(a, projects) != case_identity(b, projects):
                raise ReviewError(f"Cannot compare {case_id}: the frozen inputs or execution settings differ")


def compare_runs(run_a, run_b, side="candidate", examples=5):
    """Case-by-case status changes between two runs that share case IDs."""
    manifest_a, cases_a, rows_a = load_run(run_a)
    manifest_b, cases_b, rows_b = load_run(run_b)
    common = sorted(set(rows_a) & set(rows_b))
    _check_inputs(manifest_a, cases_a, manifest_b, cases_b, common)
    changes = defaultdict(list)
    reference_changes = []
    for case_id in common:
        a = {c["profile"]: c for c in rows_a[case_id]["comparison"]}
        b = {c["profile"]: c for c in rows_b[case_id]["comparison"]}
        for profile in sorted(set(a) & set(b), key=lambda p: (p == "Unrestricted", p)):
            if side == "candidate" and a[profile]["reference"] != b[profile]["reference"]:
                reference_changes.append((case_id, profile, a[profile]["reference"], b[profile]["reference"]))
            old, new = a[profile][side], b[profile][side]
            if old != new:
                changes[(profile, old, new)].append(case_id)
    return {
        "a": {"run": str(run_a), "refs": _run_refs(manifest_a), "cases": len(rows_a)},
        "b": {"run": str(run_b), "refs": _run_refs(manifest_b), "cases": len(rows_b)},
        "side": side,
        "common": len(common),
        "only_a": sorted(set(rows_a) - set(rows_b)),
        "only_b": sorted(set(rows_b) - set(rows_a)),
        "reference_changes": reference_changes,
        "changes": [
            {"profile": p, "a": old, "b": new, "count": len(ids), "examples": ids[:examples],
             "direction": "better" if new == "validated" else "worse" if old == "validated"
             else "worse (silent)" if old in LOUD and new == "wrong_result" else "changed"}
            for (p, old, new), ids in sorted(changes.items(), key=lambda item: (-len(item[1]), item[0]))
        ],
    }


def print_comparison(result, show=20):
    a, b = result["a"], result["b"]
    print(f"A {a['run']}: reference {a['refs']['reference'][:12]} candidate {a['refs']['candidate'][:12]} ({a['cases']} cases)")
    print(f"B {b['run']}: reference {b['refs']['reference'][:12]} candidate {b['refs']['candidate'][:12]} ({b['cases']} cases)")
    print(f"Common cases {result['common']}; only in A {len(result['only_a'])}; only in B {len(result['only_b'])}")
    if result["side"] == "candidate" and a["refs"]["reference"] == b["refs"]["reference"]:
        print(f"Reference outcome changes (should be 0 for identical reference builds): {len(result['reference_changes'])}")
        for item in result["reference_changes"][:show]:
            print("  ", item)
    print(f"\n{result['side'].capitalize()} outcome changes A -> B:")
    if not result["changes"]:
        print("  none")
    for change in result["changes"][:show]:
        print(f"  {change['count']:5} [{change['profile']}] {change['a']} -> {change['b']} ({change['direction']}); "
              f"e.g. {', '.join(change['examples'])}")


def parse_selection(text):
    """'a=validated,b!=validated' -> list of (run, negated, status)."""
    terms = []
    for part in filter(None, (piece.strip() for piece in text.split(","))):
        match = re.fullmatch(r"([ab])\s*(!?=)\s*([A-Za-z_]+)", part)
        if not match:
            raise ReviewError(f"Invalid selection term: {part!r}; use a=STATUS, b!=STATUS, ...")
        terms.append((match.group(1), match.group(2) == "!=", match.group(3)))
    if not terms:
        raise ReviewError("Empty selection")
    return terms


def select_cases(run_a, run_b, selection, side="candidate", profiles=None):
    """Case IDs for which every term holds for at least one shared profile."""
    terms = parse_selection(selection)
    manifest_a, cases_a, rows_a = load_run(run_a)
    manifest_b, cases_b, rows_b = load_run(run_b)
    _check_inputs(manifest_a, cases_a, manifest_b, cases_b, set(rows_a) & set(rows_b))
    chosen = []
    for case_id in sorted(set(rows_a) & set(rows_b)):
        a, b = outcomes(rows_a[case_id], side), outcomes(rows_b[case_id], side)
        for profile in set(a) & set(b):
            if profiles and profile not in profiles:
                continue
            values = {"a": a[profile], "b": b[profile]}
            if all((values[run] == status) != negated for run, negated, status in terms):
                chosen.append(case_id)
                break
    return chosen


_STAGE_PATTERNS = {
    "closure": r"(\(\s*\)|\b[a-z_]\w*|\([a-z_][\w ,]*\))\s*(=>|->)\s*[^\sA-Z(]|[(,]\s*_\s*[,)]",
    "record": r"struct\s+\w+\s*\{[^}]*(=>|->)[^}]*\}|newtype\s+\w+\s*=[^;]*(=>|->)[^;]*;",
    "selection": r"\b(let|set|mutable)\s[^;]*=\s*if\b|\]\s*\(|return\s+if\b|\bif\b[^{;]*\{\s*[A-Z]\w*\s*\}\s*else",
    "library": r"\b(ApplyToEach\w*|Mapped|ForEach|Fold|Sorted|Filtered|ApplyControlledOn\w+|ApplyIfElse\w*)\s*\(|\bwithin\b",
}
CHAIN_STAGES = ("factory", "closure", "record", "selection", "library")


def _returns_callable(source):
    for match in re.finditer(r"\b(function|operation)\s+\w+\s*(<[^>]*>)?\s*\(", source):
        index, depth = match.end() - 1, 0
        while index < len(source):
            depth += source[index] == "("
            depth -= source[index] == ")"
            index += 1
            if depth == 0:
                break
        tail = source[index : index + 200].split("{")[0].split(" is ")[0]
        if tail.strip().startswith(":") and ("=>" in tail or "->" in tail):
            return True
    return False


def callable_stages(source):
    """Heuristic: which stages of a callable chain a Q# program combines."""
    stages = {name for name, pattern in _STAGE_PATTERNS.items() if re.search(pattern, source, re.S)}
    if _returns_callable(source):
        stages.add("factory")
    if re.search(r"\b(Adjoint|Controlled)\s+[A-Za-z(]", source):
        stages.add("functor")
    return stages


def stage_histogram(cases):
    histogram, combinations, inline = Counter(), Counter(), 0
    for case in cases:
        source = case.get("source")
        if not source:
            continue
        inline += 1
        stages = callable_stages(source)
        depth = sum(stage in stages for stage in CHAIN_STAGES)
        histogram[depth] += 1
        if depth >= 4:
            origin = re.sub(r"\s*\(.*", "", case.get("origin", "")).split("::")[-1] or case["id"]
            combinations[origin] += 1
    return {"inline": inline, "depths": dict(sorted(histogram.items())), "deep_origins": combinations.most_common(10)}


def match_issues(snapshot, needles, limit=10):
    """Issues whose normalized title/body contain a normalized needle (a hint, not a duplicate proof)."""
    issues = snapshot["issues"]
    normalized = [
        (issue, normalize_signature(issue.get("title", ""), 10**9) + " " +
         re.sub(r"\s+", " ", re.sub(r"\d+", "N", issue.get("body") or "")))
        for issue in issues
    ]
    results = {}
    for needle in needles:
        key = normalize_signature(needle, 10**9)
        # Match on the signature's stable prefix: the text before the first variable detail.
        prefix = re.split(r"\bExpr N\b|\bPat N\b|: ", key)[0].strip() or key
        if len(prefix) < 12:
            prefix = key[:60]
        hits = [issue for issue, text in normalized if prefix.lower() in text.lower()]
        results[needle] = {"key": prefix, "issues": [
            {"number": i["number"], "state": i.get("state"), "title": i.get("title")} for i in hits[:limit]
        ], "total": len(hits)}
    return results
