#!/usr/bin/env python3
# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Entry point for the review kit; no third-party Python dependencies."""

import argparse
import json
from pathlib import Path
import sys

from harness.analysis import (
    compare_runs,
    match_issues,
    print_comparison,
    print_summary,
    select_cases,
    stage_histogram,
    summarize_run,
)
from harness.builds import build_revision
from harness.common import (
    ReviewError, is_ancestor, load_config, ordered_roles, preflight, read_json, resolve_ref, write_json,
)
from harness.diffscan import print_scan, scan
from harness.discovery import discover_samples, snapshot_all_issues, snapshot_issues
from harness.evidence import (
    merged_manifest, print_recheck, recheck_inputs, recheck_scripts, save_evidence,
)
from harness.execution import exec_script, print_exec, resolve_targets
from harness.extraction import extract_tests
from harness.investigate import attribute, reduce_adapter, reduce_case
from harness.issue_drafts import draft_issues
from harness.manifests import export_run_cases, subset_manifests
from harness.report import generate_report
from harness.runner import create_run, oracle_check
from harness.statecheck import check as statecheck
from harness.tables import run_table
from harness.timing import print_timing, run_timing
from harness.variants import run_variants, variant_lines

PROFILE_CHOICES = ("Base", "Adaptive_RI", "Adaptive_RIF", "Adaptive")
KINDS = ("panic", "rejected", "wrong_result", "compiler_crash")


def parser():
    cli = argparse.ArgumentParser(description=__doc__)
    commands = cli.add_subparsers(dest="command", required=True)
    for name in ("check", "build", "run", "samples", "issues", "attribute", "reduce", "reduce-adapter"):
        sub = commands.add_parser(name)
        sub.add_argument("config", type=Path)
        if name in ("run", "attribute", "reduce", "reduce-adapter"):
            sub.add_argument("cases", type=Path)
        if name in ("build", "run", "attribute"):
            sub.add_argument("--mode", choices=("release", "debug"), default="release")
        if name == "run":
            sub.add_argument("--roles", nargs="*", metavar="ROLE",
                             help="extra configured roles to run besides reference and candidate (default: all)")
        if name in ("samples", "issues", "attribute", "reduce", "reduce-adapter"):
            sub.add_argument("--output", type=Path, required=True)
        if name in ("attribute", "reduce", "reduce-adapter"):
            sub.add_argument("--case", required=True)
        if name == "samples":
            sub.add_argument("--revision", choices=("candidate", "reference"), default="candidate")
        if name == "issues":
            sub.add_argument("--all", action="store_true", help="snapshot every repository issue (no comments)")
        if name == "attribute":
            sub.add_argument("--refs", nargs="+")
        if name in ("reduce", "reduce-adapter"):
            sub.add_argument("--kind", choices=KINDS, required=True)
            sub.add_argument("--profile", choices=PROFILE_CHOICES, required=True)
            sub.add_argument("--needle", default="")
            sub.add_argument("--attempts", type=int, default=100)
            sub.add_argument("--allow-preexisting", action="store_true")
    report = commands.add_parser("report")
    report.add_argument("run", type=Path)
    report.add_argument("--output", type=Path)
    report.add_argument("--overwrite", action="store_true")

    summarize = commands.add_parser("summarize", help="transitions, failure signatures, profile-hierarchy checks")
    summarize.add_argument("run", type=Path)
    summarize.add_argument("--show", type=int, default=10)
    summarize.add_argument("--json", type=Path)

    compare = commands.add_parser("compare", help="case-by-case outcome changes between two runs")
    compare.add_argument("run_a", type=Path)
    compare.add_argument("run_b", type=Path)
    compare.add_argument("--side", choices=("candidate", "reference"), default="candidate")
    compare.add_argument("--show", type=int, default=20)
    compare.add_argument("--json", type=Path)
    compare.add_argument("--select", help="e.g. 'a=validated,b!=validated' (statuses on --side)")
    compare.add_argument("--profiles", nargs="+", choices=PROFILE_CHOICES + ("Unrestricted",))
    compare.add_argument("--output", type=Path, help="write selected cases (from run B's manifest) here")

    subset = commands.add_parser("subset", help="select cases across manifests, fixing relative paths")
    subset.add_argument("manifests", type=Path, nargs="+")
    subset.add_argument("--output", type=Path, required=True)
    subset.add_argument("--ids", nargs="+", default=[])
    subset.add_argument("--match", nargs="+", default=[], help="case-ID globs")

    execute = commands.add_parser("exec", help="run a standalone repro script under each revision's build")
    execute.add_argument("config", type=Path)
    execute.add_argument("script", type=Path)
    execute.add_argument("--roles", nargs="+", metavar="ROLE", help="configured roles (default: all, oldest first)")
    execute.add_argument("--refs", nargs="+", help="explicit refs instead of configured roles")
    execute.add_argument("--mode", choices=("release", "debug"), default="release")
    execute.add_argument("--output", type=Path, help="defaults to <work_dir>/exec")
    execute.add_argument("--timeout", type=int, default=600)
    execute.add_argument("--args", nargs=argparse.REMAINDER, default=[], help="arguments passed to the script")

    extract = commands.add_parser("extract-tests", help="dump test programs with explicit expected values")
    extract.add_argument("config", type=Path)
    extract.add_argument("--crate", default="qsc_fir_transforms")
    extract.add_argument("--revision", default="candidate", metavar="ROLE",
                         help="configured role: candidate, reference, historical_reference, previously_reviewed, layerN")
    extract.add_argument("--ref", help="explicit ref instead of a configured role")
    extract.add_argument("--hooks", type=Path, help="JSON hook specification overriding the bundled one")
    extract.add_argument("--filter", help="cargo test name filter")
    extract.add_argument("--output", type=Path, required=True)

    matches = commands.add_parser("match-issues", help="suggest existing issues sharing failure signatures")
    matches.add_argument("snapshot", type=Path)
    matches.add_argument("--run", type=Path, help="use the run's candidate failure signatures")
    matches.add_argument("--text", nargs="+", default=[], help="explicit signatures or phrases")
    matches.add_argument("--limit", type=int, default=10)

    oracle = commands.add_parser("oracle-check", help="simulate hand-written expectations at one revision")
    oracle.add_argument("config", type=Path)
    oracle.add_argument("cases", type=Path)
    oracle.add_argument("--role", default="reference", metavar="ROLE")
    oracle.add_argument("--mode", choices=("release", "debug"), default="release")

    state = commands.add_parser("statecheck", help="statevector replay of straight-line QIR files")
    state.add_argument("qir", type=Path, nargs="+")
    state.add_argument("--max-qubits", type=int, default=16)

    stages = commands.add_parser("stages", help="how many callable-chain stages manifest programs combine")
    stages.add_argument("manifests", type=Path, nargs="+")

    table = commands.add_parser("table", help="per-case outcomes and values for every revision of one or more runs")
    table.add_argument("runs", type=Path, nargs="+")
    table.add_argument("--profiles", nargs="+", choices=PROFILE_CHOICES)
    table.add_argument("--roles", nargs="+", metavar="ROLE")
    table.add_argument("--cases", nargs="+", default=[], help="case-ID globs")
    table.add_argument("--only-diff", action="store_true", help="only cases whose revisions differ")

    timing = commands.add_parser("timing", help="isolated, interleaved, paired compile timing across revisions")
    timing.add_argument("config", type=Path)
    timing.add_argument("cases", type=Path)
    timing.add_argument("--roles", nargs="+", metavar="ROLE", help="first is the baseline (default: reference candidate)")
    timing.add_argument("--refs", nargs="+", help="explicit refs instead of roles; first is the baseline")
    timing.add_argument("--profiles", nargs="+", choices=PROFILE_CHOICES)
    timing.add_argument("--cases-match", nargs="+", default=[], help="case-ID globs")
    timing.add_argument("--repeats", type=int, default=5)
    timing.add_argument("--outlier", type=float, default=1.5, help="ratio reported as an outlier")
    timing.add_argument("--mode", choices=("release", "debug"), default="release")

    variants = commands.add_parser("variants", help="a matrix of named program variants across revisions and profiles")
    variants.add_argument("config", type=Path)
    variants.add_argument("variants", type=Path)
    variants.add_argument("--roles", nargs="+", metavar="ROLE")
    variants.add_argument("--refs", nargs="+")
    variants.add_argument("--profiles", nargs="+", choices=PROFILE_CHOICES)
    variants.add_argument("--shots", type=int)
    variants.add_argument("--mode", choices=("release", "debug"), default="release")

    drafts = commands.add_parser("draft-issues", help="draft issues with executed per-profile output tables")
    drafts.add_argument("config", type=Path)
    drafts.add_argument("definitions", type=Path)
    drafts.add_argument("--output", type=Path, required=True)
    drafts.add_argument("--revisions", nargs="+", metavar="ROLE_OR_REF", help="table columns (default: reference candidate)")
    drafts.add_argument("--only", nargs="+", default=[], help="rerun only these slugs; reuse cached results for others")
    drafts.add_argument("--cached", action="store_true", help="reuse cached results whenever the script is unchanged")
    drafts.add_argument("--mode", choices=("release", "debug"), default="release")

    save = commands.add_parser("save-evidence", help="copy findings, interesting run cases and repros to the cache")
    save.add_argument("config", type=Path)
    save.add_argument("--findings", type=Path, nargs="+", default=[], help="case manifests to bundle")
    save.add_argument("--runs", type=Path, nargs="+", default=[], help="runs whose non-quiet cases to bundle")
    save.add_argument("--files", type=Path, nargs="+", default=[], help="repro/variant scripts, generator.json, ...")
    save.add_argument("--name", help="review name (default: config name)")
    save.add_argument("--note")

    recheck = commands.add_parser("recheck", help="rerun a review's saved findings and repro scripts")
    recheck.add_argument("config", type=Path)
    recheck.add_argument("--name", help="cached review to recheck (default: config name)")
    recheck.add_argument("--report", type=Path, help="fallback: a report whose embedded repro scripts to run")
    recheck.add_argument("--roles", nargs="+", metavar="ROLE")
    recheck.add_argument("--refs", nargs="+")
    recheck.add_argument("--mode", choices=("release", "debug"), default="release")

    diagnostics = commands.add_parser("new-diagnostics", help="added/removed diagnostics and panic sites in the diff")
    diagnostics.add_argument("config", type=Path)
    diagnostics.add_argument("--paths", nargs="+", default=["source"])
    diagnostics.add_argument("--show", type=int, default=40)
    diagnostics.add_argument("--json", type=Path)
    return cli


def run_command(args):
    if args.command == "report":
        manifest = read_json(args.run / "manifest.json")
        destination = args.output or Path(manifest["config"]["report"])
        generate_report(args.run, destination, args.overwrite)
        print(destination)
        try:  # the cache is a convenience: never fail the report because of it
            config = load_config(manifest["config"]["config_path"])
            saved, size, total, removed = save_evidence(config, runs=[args.run])
            print(f"Evidence cached in {saved} ({size // 1024} KiB; cache {total // 1024} KiB"
                  + (f"; pruned {', '.join(removed)}" if removed else "") + ")")
        except (ReviewError, OSError, KeyError) as error:
            print(f"WARNING: evidence not cached: {error}", file=sys.stderr)
        return
    if args.command == "table":
        for run in args.runs:
            for line in run_table(run, args.profiles, args.roles, args.only_diff, args.cases):
                print(line)
        return
    if args.command == "summarize":
        summary = summarize_run(args.run)
        print_summary(summary, args.show)
        if args.json:
            write_json(args.json, summary)
        return
    if args.command == "compare":
        result = compare_runs(args.run_a, args.run_b, args.side)
        print_comparison(result, args.show)
        if args.json:
            write_json(args.json, result)
        if args.select:
            chosen = select_cases(args.run_a, args.run_b, args.select, args.side, args.profiles)
            print(f"\nSelected {len(chosen)} cases for {args.select!r}")
            if args.output and chosen:
                print(f"Wrote {export_run_cases(args.run_b, chosen, args.output)} cases to {args.output}")
        elif args.output:
            raise ReviewError("--output requires --select")
        return
    if args.command == "subset":
        print(f"Wrote {subset_manifests(args.manifests, args.output, args.ids, args.match)} cases to {args.output}")
        return
    if args.command == "match-issues":
        snapshot = read_json(args.snapshot)
        needles = list(args.text)
        if args.run:
            needles += [group["signature"] for group in summarize_run(args.run)["candidate_failures"]]
        if not needles:
            raise ReviewError("Supply --run and/or --text")
        for needle, result in match_issues(snapshot, list(dict.fromkeys(needles)), args.limit).items():
            print(f"{needle}\n  key: {result['key']!r}; {result['total']} matching issue(s)")
            for issue in result["issues"]:
                print(f"    #{issue['number']} [{issue['state']}] {issue['title']}")
        print("\nMatches share text, not necessarily a root cause; read each issue before linking it.")
        return
    if args.command == "statecheck":
        for path in args.qir:
            print(path, json.dumps(statecheck(path.read_text(), args.max_qubits)))
        return
    if args.command == "stages":
        for path in args.manifests:
            result = stage_histogram(read_json(path))
            print(f"{path}: {result['inline']} inline programs; chain stages per program {result['depths']}")
            for origin, count in result["deep_origins"]:
                print(f"    {count:5} with >=4 stages from {origin}")
        return

    config = load_config(args.config)
    if args.command == "check":
        print(json.dumps({"config": config, "tools": preflight(config)}, indent=2))
        for older in ("reference", "previously_reviewed"):
            if older in config["refs"] and not is_ancestor(
                config["repository"], config["refs"][older], config["refs"]["candidate"]
            ):
                print(f"WARNING: {older} is not an ancestor of candidate; inspect the rebase/comparison.", file=sys.stderr)
        if config["refs"]["reference"] == config["refs"]["candidate"]:
            print("WARNING: identical endpoints; this is a smoke test, not a regression review.", file=sys.stderr)
        if config["checkout_status"]:
            print("WARNING: main checkout is dirty; pinned builds exclude those changes.", file=sys.stderr)
        for client in config["clients"]:
            if client["status"]:
                print(f"WARNING: client {client['name']} is dirty; case snapshots include local edits.", file=sys.stderr)
    elif args.command == "build":
        tools = preflight(config)
        for role in ordered_roles(config["refs"]):
            build = build_revision(config, config["refs"][role], args.mode, tools)
            print(f"{role}: {build['overlay']}")
    elif args.command == "run":
        if config["debug_validation"] == "all" and args.mode == "release":
            release = create_run(config, args.cases, "release", args.roles)
            debug = create_run(config, args.cases, "debug", args.roles)
            print(f"Release: {release}\nDebug: {debug}")
            runs = [release, debug]
        else:
            run = create_run(config, args.cases, args.mode, args.roles)
            print(run)
            runs = [run]
        if any(read_json(run / "manifest.json")["status"] != "complete" for run in runs):
            sys.exit(2)
    elif args.command == "samples":
        found, pending = discover_samples(config, args.output, args.revision)
        print(f"{found} candidates, {pending} pending manual curation; {args.output}")
    elif args.command == "issues":
        if args.all:
            print(f"{snapshot_all_issues(config, args.output)} issues snapshotted (all); {args.output}")
        else:
            print(f"{snapshot_issues(config, args.output)} issues snapshotted; {args.output}")
    elif args.command == "attribute":
        attribute(config, args.cases, args.case, args.output, args.refs, args.mode)
    elif args.command in ("reduce", "reduce-adapter"):
        if args.attempts < 1:
            raise ReviewError("--attempts must be positive")
        if args.command == "reduce":
            tested, lines = reduce_case(
                config, args.cases, args.case, args.output, args.kind, args.profile,
                args.needle, args.attempts, not args.allow_preexisting,
            )
            print(f"{tested} predicate attempts; {lines} lines; {args.output}")
        else:
            tested, adapter = reduce_adapter(
                config, args.cases, args.case, args.output, args.kind, args.profile,
                args.needle, args.attempts, not args.allow_preexisting,
            )
            print(f"{tested} predicate attempts; reduced adapter {adapter}; manifest {args.output}")
    elif args.command == "exec":
        targets = resolve_targets(config, args.roles, args.refs)
        output = args.output or Path(config["work_dir"]) / "exec"
        print_exec(exec_script(config, args.script, targets, args.mode, output, args.timeout, args.args))
    elif args.command == "extract-tests":
        sha = resolve_ref(config["repository"], args.ref) if args.ref else config["refs"].get(args.revision)
        if sha is None:
            raise ReviewError(f"Configuration has no {args.revision}")
        count, stats, status = extract_tests(config, sha, args.crate, args.output, args.hooks, args.filter)
        print(f"{count} cases from {stats['records']} records; skipped {stats}; cargo exit {status}; {args.output}")
        if status:
            print("WARNING: some tests failed; their programs are still extracted. Inspect the log.", file=sys.stderr)
    elif args.command == "timing":
        targets = resolve_targets(config, args.roles or (None if args.refs else ["reference", "candidate"]), args.refs)
        directory, summary = run_timing(config, args.cases, targets, args.profiles, args.repeats, args.mode,
                                        args.cases_match)
        print_timing(summary, outlier=args.outlier)
        print(directory)
    elif args.command == "variants":
        targets = resolve_targets(config, args.roles, args.refs)
        output, columns = run_variants(config, args.variants, targets, args.profiles, args.mode, args.shots)
        for line in variant_lines(columns):
            print(line)
        print(output)
    elif args.command == "draft-issues":
        output, status = draft_issues(config, args.definitions, args.output, args.revisions, args.only, args.cached,
                                      args.mode)
        for slug, columns in status.items():
            print(f"{slug}: " + ", ".join(f"{label} {result}" for label, result in columns.items()))
        print(output)
    elif args.command == "save-evidence":
        saved, size, total, removed = save_evidence(config, args.findings, args.runs, args.files, args.name, args.note)
        print(f"Saved {saved} ({size // 1024} KiB); cache {total // 1024} KiB of {config['cache_budget_mb']} MiB"
              + (f"; pruned {', '.join(removed)}" if removed else ""))
        print("The cache may be wiped at any time; keep repro scripts embedded in reports.")
    elif args.command == "recheck":
        manifests, scripts, source = recheck_inputs(config, args.name, args.report or config["report"])
        print(f"Rechecking from {source}")
        targets = resolve_targets(config, args.roles, args.refs)
        if manifests:
            work = Path(config["work_dir"]) / "recheck"
            work.mkdir(parents=True, exist_ok=True)
            merged = work / f"{args.name or config['name']}-cases.json"
            print(f"{merged_manifest(manifests, merged)} saved cases")
            # Saved cases run at configured roles; explicit --refs apply to the repro scripts only.
            roles = None if args.refs else [label for label, _ in targets if label not in ("reference", "candidate")]
            run = create_run(config, merged, args.mode, roles)
            print(run)
            for line in run_table(run):
                print(line)
        if scripts:
            table = recheck_scripts(config, scripts, targets, args.mode, Path(config["work_dir"]) / "recheck" / "exec")
            print_recheck(table, [label for label, _ in targets])
    elif args.command == "new-diagnostics":
        found = scan(config["repository"], config["refs"]["reference"], config["refs"]["candidate"], args.paths)
        print_scan(found, args.show)
        if args.json:
            write_json(args.json, found)
    elif args.command == "oracle-check":
        directory, results, unchecked = oracle_check(config, args.cases, args.role, args.mode)
        bad = [result for result in results if result["status"] != "ok"]
        for result in bad:
            print(f"{result['case']}: {result['status']} {json.dumps(result.get('observed', result.get('message')))[:300]}")
        print(f"{len(results) - len(bad)}/{len(results)} expectations confirmed by the simulator; "
              f"{unchecked} cases without an explicit expectation; {directory}")
        if bad:
            sys.exit(1)


def main():
    try:
        run_command(parser().parse_args())
    except (ReviewError, OSError, json.JSONDecodeError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        sys.exit(2)


if __name__ == "__main__":
    main()
