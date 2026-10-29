# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Independent stages and conservative endpoint classifications."""

from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone
import json
from pathlib import Path
import tempfile

from harness.builds import build_revision, worker_environment
from harness.cases import case_identity, load_cases, write_case_manifest
from harness.common import KIT, ReviewError, digest, file_digest, isolated, preflight, select_roles, write_json


def stage(config, build, case, name, profile=None, qir_path=None):
    job = {
        "stage": name,
        "build": build,
        "case": case,
        "profile": profile,
        "memory_mb": config["limits"]["memory_mb"],
        "shots": case["shots"],
        "seed": case["seed"],
        "runner": str(Path(config["venv"]) / "bin/qir-runner"),
        "qir_path": str(qir_path) if qir_path else None,
    }
    with tempfile.TemporaryDirectory(prefix="review-worker-") as cwd:
        return isolated(
            config.get("_worker_command", [
                str(Path(config["venv"]) / "bin/python"), str(KIT / "harness/worker.py"),
            ]),
            payload=job,
            env=worker_environment(config, build),
            cwd=cwd,
            timeout=config["limits"][f"{name}_seconds"],
        )


def run_case(config, build, case, directory):
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    result = {"sha": build["sha"], "mode": build["mode"], "profiles": {}}
    if case["oracle"]["kind"] != "compile_only":
        result["simulator"] = stage(config, build, case, "simulate")
    for profile in case["profiles"]:
        compiled = stage(config, build, case, "compile", profile)
        row = {"compile": compiled}
        if compiled["status"] == "ok":
            qir = compiled.pop("qir")
            qir_path = directory / f"{profile}.ll"
            qir_path.write_text(qir)
            compiled.update(qir_digest=file_digest(qir_path), qir_bytes=len(qir.encode()), qir_path=str(qir_path))
            if case["oracle"]["kind"] != "compile_only":
                row["runner"] = stage(config, build, case, "runner", profile, qir_path)
        result["profiles"][profile] = row
    return result


def endpoint(case, result, profile):
    row = result["profiles"][profile]
    compiled = row["compile"]["status"]
    if compiled != "ok":
        return {
            "diagnostic": "rejected",
            "panic": "panic",
            "crash": "compiler_crash",
            "timeout": "compile_timeout",
        }.get(compiled, compiled)
    if case["oracle"]["kind"] == "compile_only":
        return "accepted_unvalidated"
    ran = row.get("runner", {})
    if ran.get("status") != "ok":
        return {
            "runner_error": "execution_failure",
            "runner_crash": "execution_failure",
            "timeout": "execution_timeout",
            "parser_error": "parser_error",
            "resource_exhausted": "resource_exhausted",
        }.get(ran.get("status"), "infrastructure_error")
    oracle = case["oracle"]
    if oracle["kind"] == "observe":
        return "observed_unvalidated"
    sim = result.get("simulator", {})
    expected = oracle.get("expected")
    if expected is None:
        if sim.get("status") != "ok":
            return "oracle_unavailable"
        values = sim["values"]
        if not values or any(v != values[0] for v in values):
            return "oracle_disagreement"
        expected = values[0]
    if expected == {"type": "unit"}:
        return "unit_unvalidated"
    if any(value != expected for value in ran["values"]):
        return "wrong_result"
    if sim.get("status") != "ok":
        return "oracle_unavailable"
    if any(value != expected for value in sim["values"]):
        return "oracle_disagreement"
    return "validated"


FAILURES = {
    "rejected",
    "panic",
    "compiler_crash",
    "compile_timeout",
    "execution_failure",
    "execution_timeout",
    "wrong_result",
}


def simulator_endpoint(case, result):
    sim = result["simulator"]
    expected = case["oracle"].get("expected")
    if expected is not None and expected != {"type": "unit"} and sim["status"] == "ok":
        if any(value != expected for value in sim["values"]):
            return "wrong_result"
    return sim["status"]


def compare(case, reference, candidate):
    rows = []
    for profile in case["profiles"]:
        old, new = endpoint(case, reference, profile), endpoint(case, candidate, profile)
        if case["oracle"]["kind"] == "deterministic" and "expected" not in case["oracle"]:
            a, b = reference.get("simulator", {}), candidate.get("simulator", {})
            if a.get("status") == b.get("status") == "ok" and a["values"][0] != b["values"][0]:
                old = new = "oracle_disagreement"
        if old == "validated" and new in FAILURES:
            verdict = "regression_candidate"
        elif old in FAILURES and new == "validated":
            verdict = "fix_candidate"
        elif old in FAILURES and new in FAILURES:
            verdict = "pre_existing_or_changed_failure"
        elif old == new == "validated":
            verdict = "validated_unchanged"
        elif old in FAILURES and new.endswith("_unvalidated"):
            verdict = "newly_accepted_unvalidated"
        elif old.endswith("_unvalidated") and new in FAILURES:
            verdict = "new_failure_unvalidated"
        else:
            verdict = "inconclusive"
        rows.append({"profile": profile, "reference": old, "candidate": new, "verdict": verdict})
    if case["oracle"]["kind"] != "compile_only":
        old, new = simulator_endpoint(case, reference), simulator_endpoint(case, candidate)
        expected = case["oracle"].get("expected")
        failures = {"panic", "crash", "diagnostic", "runtime_error", "timeout", "wrong_result"}
        if old == "ok" and new in failures:
            verdict = "simulator_regression_candidate"
        elif old in failures and new == "ok":
            verdict = "simulator_fix_candidate" if expected is not None else "newly_accepted_unvalidated"
        elif old in failures and new in failures:
            verdict = "pre_existing_or_changed_failure"
        elif old == new == "ok":
            verdict = "simulator_observed"
        else:
            verdict = "inconclusive"
        rows.append({"profile": "Unrestricted", "reference": old, "candidate": new, "verdict": verdict})
    return rows


def prune_qir(config, case, results):
    if config["keep_qir"]:
        return
    uninteresting = {
        "validated",
        "observed_unvalidated",
        "unit_unvalidated",
        "accepted_unvalidated",
    }
    good = {
        profile
        for profile in case["profiles"]
        if all(endpoint(case, result, profile) in uninteresting for result in results.values())
    }
    for result in results.values():
        for profile in good:
            compiled = result["profiles"][profile]["compile"]
            path = compiled.pop("qir_path", None)
            if path:
                Path(path).unlink()


def create_run(config, cases_path, mode, roles=None, targets=None):
    tools = preflight(config)
    if targets is None:
        roles = select_roles(config, roles)
        targets = [(role, config["refs"][role]) for role in roles]
    else:
        roles = [role for role, _ in targets]
        if len(set(roles)) != len(roles):
            raise ReviewError("Execution target labels must be unique")
        config = config | {"refs": dict(targets)}
    root = Path(config["work_dir"]) / "runs"
    root.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix=datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ-"), dir=root))
    cases, skipped = load_cases(cases_path, config, directory / "inputs")
    projects = {}
    identities = {case["id"]: case_identity(case, projects) for case in cases}
    manifest = {
        "config": config,
        "tools": tools,
        "mode": mode,
        "cases_path": str(Path(cases_path).resolve()),
        "case_digest": digest(identities),
        "case_identities": identities,
        "harness_digest": digest(
            {str(p.relative_to(KIT)): p.read_text() for p in sorted((KIT / "harness").glob("*.py"))}
        ),
        "skipped": skipped,
        "started": datetime.now(timezone.utc).isoformat(),
        "status": "running",
    }
    write_json(directory / "manifest.json", manifest)
    write_case_manifest(cases, directory / "cases.json")
    (directory / "inputs/cases.json").unlink()
    try:
        by_sha, builds = {}, {}
        for role, sha in targets if cases else []:
            if sha not in by_sha:
                by_sha[sha] = build_revision(config, sha, mode, tools)
            builds[role] = by_sha[sha]
        manifest["builds"] = builds
        manifest["roles"] = roles
        write_json(directory / "manifest.json", manifest)

        def execute(case):
            results = {
                role: run_case(config, build, case, directory / case["id"] / role)
                for role, build in builds.items()
            }
            comparison = (
                compare(case, results["reference"], results["candidate"])
                if "reference" in results and "candidate" in results else []
            )
            historical = (
                compare(case, results["historical_reference"], results["candidate"])
                if "historical_reference" in results and "candidate" in results
                else None
            )
            # Lost fixes: what the previously reviewed candidate did versus the current candidate.
            previous = (
                compare(case, results["previously_reviewed"], results["candidate"])
                if "previously_reviewed" in results and "candidate" in results
                else None
            )
            layers = {}
            for role, result in results.items():
                if role.startswith("layer"):
                    layers[role] = {profile: endpoint(case, result, profile) for profile in case["profiles"]}
                    if "simulator" in result:
                        layers[role]["Unrestricted"] = simulator_endpoint(case, result)
            prune_qir(config, case, results)
            return {"case": case["id"], "results": results, "comparison": comparison, "historical": historical,
                    "previous": previous, "layers": layers or None}

        # Rows are appended as cases finish so slow cases never hide progress; the file
        # is rewritten in manifest order once every case has completed.
        infrastructure_failures, finished = [], {}
        with (directory / "results.jsonl").open("w") as output:
            with ThreadPoolExecutor(config["limits"]["jobs"]) as executor:
                futures = [executor.submit(execute, case) for case in cases]
                for future in as_completed(futures):
                    row = future.result()
                    finished[row["case"]] = row
                    output.write(json.dumps(row, allow_nan=False) + "\n")
                    output.flush()
                    print(
                        f"[{len(finished)}/{len(cases)}] {row['case']}: "
                        + ", ".join(f"{r['profile']}={r['verdict']}" for r in row["comparison"]),
                        flush=True,
                    )
                    for role, result in row["results"].items():
                        stages = [("Unrestricted", "simulate", result.get("simulator", {}))]
                        stages += [
                            (profile, name, details)
                            for profile, values in result["profiles"].items()
                            for name, details in values.items()
                        ]
                        for profile, name, details in stages:
                            if details.get("status") in (
                                "infrastructure_error", "parser_error", "unsupported_profile", "resource_exhausted",
                            ):
                                failure = {
                                    "case": row["case"], "role": role, "profile": profile,
                                    "stage": name, "status": details["status"],
                                    "message": details.get("message", details.get("stderr", "")),
                                }
                                infrastructure_failures.append(failure)
                                print(f"WARNING: {failure}", flush=True)
        ordered = directory / "results.jsonl.tmp"
        ordered.write_text("".join(json.dumps(finished[case["id"]], allow_nan=False) + "\n" for case in cases))
        ordered.replace(directory / "results.jsonl")
        manifest["infrastructure_failures"] = infrastructure_failures
        manifest["status"] = "complete_with_infrastructure_failures" if infrastructure_failures else "complete"
    except BaseException:
        manifest["status"] = "failed"
        raise
    finally:
        manifest["finished"] = datetime.now(timezone.utc).isoformat()
        write_json(directory / "manifest.json", manifest)
    return directory


def oracle_check(config, cases_path, role="reference", mode="release"):
    """Simulate every case with a hand-written expectation at one revision; report disagreements.

    Use before a large differential run: a generator or curation mistake otherwise shows
    up as shared "wrong results" at both endpoints.
    """
    tools = preflight(config)
    if role not in config["refs"]:
        raise ReviewError(f"Configuration has no {role}")
    root = Path(config["work_dir"]) / "oracle-checks"
    root.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix=datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ-"), dir=root))
    cases, skipped = load_cases(cases_path, config, directory / "inputs")
    checked = [case for case in cases if case["oracle"]["kind"] == "deterministic" and "expected" in case["oracle"]]
    build = build_revision(config, config["refs"][role], mode, tools)

    def check(case):
        simulated = stage(config, build, case, "simulate")
        expected = case["oracle"]["expected"]
        if simulated["status"] != "ok":
            return {"case": case["id"], "status": simulated["status"],
                    "message": simulated.get("message", simulated.get("stderr", ""))[:2000]}
        wrong = [value for value in simulated["values"] if value != expected]
        return {"case": case["id"], "status": "mismatch" if wrong else "ok",
                "expected": expected, "observed": wrong[0] if wrong else expected}

    with ThreadPoolExecutor(config["limits"]["jobs"]) as executor:
        results = list(executor.map(check, checked))
    write_json(directory / "oracle-check.json", {
        "revision": config["refs"][role], "role": role, "mode": mode, "cases_path": str(Path(cases_path).resolve()),
        "skipped": skipped, "unchecked": len(cases) - len(checked), "results": results,
    })
    return directory, results, len(cases) - len(checked)
