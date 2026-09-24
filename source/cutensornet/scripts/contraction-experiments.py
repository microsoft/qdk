#!/usr/bin/env python3
# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Sequential, source-built cuTensorNet experiments; no Nsight dependency."""

import argparse
import csv
import hashlib
import itertools
import json
import math
import os
from pathlib import Path
import platform
import signal
import statistics
import subprocess
import sys
import time


ROOT = Path(__file__).resolve().parents[3]
TEST = (
    "simulation::contraction::execution::qualification::experiment::native::"
    "parameterized_trial"
)
CONTINUE = {"passed", "workspace_limit", "allocation_failed", "timeout"}
WORKSPACE_BYTES = 32 * 1024**3
ERRORS = (OSError, ValueError, RuntimeError, subprocess.SubprocessError)


def write_json(path, value):
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2, allow_nan=False)
        stream.write("\n")


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def configurations(samples, reconfigurations, seeds, repeats):
    yield {"plan_source": "chronological", "repeats": repeats}
    for sample, reconfiguration, disabled, seed in itertools.product(
        samples, reconfigurations, [True, False], seeds
    ):
        yield {
            "plan_source": "optimizer",
            "repeats": repeats,
            "hyper_samples": sample,
            "reconfiguration_iterations": reconfiguration,
            "disable_rank_simplification": disabled,
            "seed": seed,
        }


def finite_float(value):
    number = float(value)
    require(math.isfinite(number), "nonfinite JSON number")
    return number


def read_events(path, allow_partial=False):
    if not path.exists():
        return []
    events = []
    lines = path.read_bytes().splitlines(keepends=True)
    for index, line in enumerate(lines):
        if not line.endswith(b"\n") and index == len(lines) - 1:
            if allow_partial:
                break
            raise ValueError("incomplete final journal event")
        event = json.loads(line, parse_float=finite_float, parse_constant=finite_float)
        if not isinstance(event, dict) or not isinstance(event.get("event"), str):
            raise ValueError("journal event must be an object with an event name")
        events.append(event)
    return events


def require(condition, message):
    if not condition:
        raise ValueError(message)


def nonnegative(value):
    return (
        type(value) is int and value >= 0
        or type(value) is float and math.isfinite(value) and value >= 0
    )


def validate_success(by_type, config, timings):
    def single(name):
        events = by_type.get(name, [])
        require(len(events) == 1, f"expected one {name} event")
        return events[0]

    environment = single("environment")
    require(
        set(environment["versions"])
        == {"cutensornet", "cutensornet_cuda_runtime", "cuda_runtime", "cuda_driver"}
        and all(type(value) is int and value > 0 for value in environment["versions"].values())
        and set(environment["libraries"]) == {"cutensornet", "cuda_runtime"},
        "incomplete native environment",
    )
    require(
        environment["device_scratch_ceiling"] == WORKSPACE_BYTES
        and environment["host_scratch_ceiling"] is None
        and environment["optimizer_workspace_constraint"]
        == (WORKSPACE_BYTES if config["plan_source"] == "optimizer" else None)
        and environment["limit"] == 1e-8
        and environment["precision"] == "CUDA_C_64F/COMPUTE_64F"
        and environment["device"] == 0
        and environment["threads"] == 1
        and all(environment[key] is False for key in ("slicing", "cache", "autotuning")),
        "inconsistent native execution policy",
    )
    plan = single("plan")
    require(plan["slicing"] == [] and plan["num_slices"] == 1, "unexpected slicing")
    require(len(plan["path"]) == len(plan["intermediate_modes"]) == 447, "incomplete plan")
    for remaining, pair in zip(range(448, 1, -1), plan["path"]):
        require(
            len(pair) == 2
            and all(type(i) is int and 0 <= i < remaining for i in pair)
            and pair[0] != pair[1],
            "invalid positional path",
        )
    snapshots = by_type.get("memory", [])
    require(len(snapshots) == 2, "expected registration and final memory snapshots")
    registered, final = snapshots
    fields = (
        "resident_input_bytes", "resident_input_count", "output_bytes",
        "device_scratch_minimum", "device_scratch_recommended", "device_scratch_allocated",
        "host_scratch_minimum", "host_scratch_recommended", "host_scratch_allocated",
        "device_cache_recommended", "host_cache_recommended", "owned_device_bytes",
    )
    require(
        all(
            type(memory[key]) is int and memory[key] >= 0
            for memory in snapshots for key in fields
        ),
        "invalid memory measurements",
    )
    require(
        registered["selected_input_bytes"] is None
        and registered["selected_input_count"] is None
        and type(final["selected_input_bytes"]) is int
        and type(final["selected_input_count"]) is int
        and final["selected_input_bytes"] == 352
        and final["selected_input_count"] == 6,
        "inconsistent selected-input evidence",
    )
    require(
        all(registered[key] == final[key] for key in fields),
        "resident resources changed during fixed-input execution",
    )
    memory = final
    require(
        memory["resident_input_bytes"] == 352
        and memory["resident_input_count"] == 6
        and memory["output_bytes"] == 65536 * 16
        and memory["device_scratch_allocated"] == max(256, memory["device_scratch_minimum"])
        and memory["device_scratch_allocated"] <= WORKSPACE_BYTES
        and memory["host_scratch_allocated"] == memory["host_scratch_minimum"]
        and memory["owned_device_bytes"]
        == memory["resident_input_bytes"] + memory["output_bytes"] + memory["device_scratch_allocated"]
        and all(
            memory[f"{space}_scratch_recommended"] >= memory[f"{space}_scratch_minimum"]
            for space in ("host", "device")
        ),
        "inconsistent allocation accounting",
    )
    estimates = single("estimates")
    for key in ("flops", "largest_intermediate_elements"):
        require(
            estimates[key] is None if config["plan_source"] == "chronological"
            else nonnegative(estimates[key]),
            "invalid or fabricated optimizer estimate",
        )
    comparisons = by_type.get("comparison", [])
    expected = set(range(config["repeats"] + 1))
    require(
        len(comparisons) == len(expected)
        and {event["iteration"] for event in comparisons} == expected
        and all(type(event["iteration"]) is int for event in comparisons)
        and all(type(event["bitwise_equal_to_first"]) is bool for event in comparisons)
        and all(
            nonnegative(event[key]) and event[key] <= 1e-8
            for event in comparisons
            for key in ("amplitude_error", "probability_tv", "squared_norm_error")
        ),
        "incomplete or invalid numerical evidence",
    )
    phases = {"export", "prepare_host_call"} | {
        f"contract_readback_{i}" for i in expected
    }
    phases.add("optimize" if config["plan_source"] == "optimizer" else "construct_control")
    require(set(timings) == phases, "missing or unexpected phase timings")
    cleanup = by_type.get("cleanup", [])
    require(
        len(cleanup) == 4
        and {event["owner"] for event in cleanup}
        == {"source_topology", "source_session", "execution", "fresh_session"}
        and all(event["error"] is None for event in cleanup),
        "incomplete or failed cleanup",
    )


def summarize(events, config, returncode, timed_out):
    by_type = {}
    for event in events:
        by_type.setdefault(event["event"], []).append(event)
    results = by_type.get("result", [])
    summary = dict(results[-1]) if results else {"status": "failed"}
    summary.pop("event", None)
    summary["returncode"] = returncode
    timings = {}
    comparisons = by_type.get("comparison", [])
    cleanup = by_type.get("cleanup", [])
    try:
        known = {
            "environment", "device_memory_before_search", "begin", "timing",
            "plan", "estimates", "memory", "comparison", "cleanup", "operation_error", "result",
        }
        require(not (set(by_type) - known), "unknown journal event")
        require(len(results) <= 1, "duplicate terminal outcome")
        for event in by_type.get("timing", []):
            require(event["phase"] not in timings, "duplicate phase timing")
            require(nonnegative(event["seconds"]), "invalid phase timing")
            timings[event["phase"]] = event["seconds"]
        require(not any(event["error"] is not None for event in cleanup), "native cleanup failed")
        failures = by_type.get("operation_error", [])
        require(
            all(event["status"] in {"workspace_limit", "allocation_failed"} for event in failures),
            "fatal native operation error",
        )
        require(not results or results[0]["status"] != "failed", "native trial failed")
        if timed_out:
            summary.update(status="timeout", error="trial deadline exceeded")
        else:
            require(len(results) == 1 and events[-1]["event"] == "result", "missing terminal outcome")
            require(summary["status"] in CONTINUE - {"timeout"}, "unknown terminal status")
            require(
                (returncode == 0) == (summary["status"] == "passed"),
                "inconsistent process exit and terminal outcome",
            )
            if summary["status"] == "passed":
                require(not failures and summary["error"] is None, "success after operation failure")
                validate_success(by_type, config, timings)
            else:
                require(bool(summary["error"]), "resource rejection without error")
                if summary["status"] == "workspace_limit":
                    rejection = summary["workspace_rejection"]
                    require(
                        type(rejection["required"]) is int
                        and rejection["maximum"] == WORKSPACE_BYTES
                        and rejection["required"] > rejection["maximum"],
                        "invalid workspace rejection",
                    )
    except (KeyError, TypeError, ValueError) as error:
        summary.update(status="failed", evidence_error=str(error))
    summary["timings_seconds"] = timings
    snapshots = by_type.get("memory", [])
    summary["memory"] = snapshots[-1] if snapshots else None
    summary["estimates"] = next(iter(by_type.get("estimates", [])), None)
    summary["environment"] = next(iter(by_type.get("environment", [])), None)
    summary["comparisons"] = comparisons
    summary["cleanup"] = cleanup
    if summary["status"] == "passed":
        repeated = [timings[f"contract_readback_{i}"] for i in range(1, config["repeats"] + 1)]
        summary["repeated_readback_median_seconds"] = statistics.median(repeated)
        summary["repeated_readback_min_seconds"] = min(repeated)
        summary["repeated_readback_max_seconds"] = max(repeated)
    return summary


def sample_memory(pid):
    query = subprocess.run(
        [
            "nvidia-smi",
            "--query-compute-apps=pid,used_gpu_memory",
            "--format=csv,noheader,nounits",
        ],
        capture_output=True, text=True, check=True, timeout=5,
    )
    gpu = []
    for row in csv.reader(query.stdout.splitlines()):
        if len(row) != 2:
            raise ValueError(f"unexpected nvidia-smi row: {row}")
        if row[0].strip() == str(pid):
            gpu.append(int(row[1].strip()) * 1024 * 1024)
    try:
        status = Path(f"/proc/{pid}/status").read_text()
    except (FileNotFoundError, ProcessLookupError):
        status = ""
    hwm = next(
        (int(line.split()[1]) * 1024 for line in status.splitlines() if line.startswith("VmHWM:")),
        None,
    )
    return {"gpu_process_bytes": sum(gpu) if gpu else None, "host_peak_rss_bytes": hwm}


def terminate(process):
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        process.wait()
        return
    try:
        process.wait(timeout=2)
    except subprocess.TimeoutExpired:
        pass
    # A descendant can outlive a leader that exited on SIGTERM.
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait()


def run_trial(command, directory, config, seconds, sampler=sample_memory):
    directory.mkdir()
    write_json(directory / "config.json", config)
    environment = os.environ.copy()
    environment["QDK_CONTRACTION_EVIDENCE_DIR"] = str(directory.resolve())
    observations = []
    timed_out = False
    start = time.monotonic()
    process = None
    driver_error = None
    interrupted = False
    try:
        with (directory / "trial.log").open("x") as log, (
            directory / "memory-observations.jsonl"
        ).open("x") as samples:
            process = subprocess.Popen(
                command, stdout=log, stderr=subprocess.STDOUT,
                env=environment, start_new_session=True,
            )
            while True:
                if time.monotonic() - start >= seconds:
                    timed_out = True
                    break
                if process.poll() is not None:
                    break
                observation = {
                    "elapsed_seconds": time.monotonic() - start,
                    **sampler(process.pid),
                }
                require(
                    all(
                        observation[key] is None
                        or (type(observation[key]) is int and observation[key] >= 0)
                        for key in ("gpu_process_bytes", "host_peak_rss_bytes")
                    ),
                    "invalid memory sample",
                )
                observations.append(observation)
                samples.write(json.dumps(observation, allow_nan=False) + "\n")
                samples.flush()
                remaining = seconds - (time.monotonic() - start)
                if remaining > 0:
                    try:
                        process.wait(timeout=min(0.1, remaining))
                    except subprocess.TimeoutExpired:
                        pass
    except (*ERRORS, KeyError, TypeError, KeyboardInterrupt) as error:
        driver_error = f"{type(error).__name__}: {error}"
        interrupted = isinstance(error, KeyboardInterrupt)
    finally:
        if process is not None:
            terminate(process)
    returncode = process.returncode if process is not None else None
    try:
        summary = summarize(
            read_events(directory / "events.jsonl", allow_partial=timed_out),
            config, returncode, timed_out,
        )
    except (OSError, ValueError) as error:
        summary = summarize([], config, returncode, False)
        summary.update(status="failed", evidence_error=f"{type(error).__name__}: {error}")
    if driver_error:
        summary.update(
            status="interrupted" if interrupted else "failed", driver_error=driver_error
        )
    summary["wall_seconds"] = time.monotonic() - start
    summary["pid"] = process.pid if process is not None else None
    summary["observed_gpu_process_peak_bytes"] = max(
        (sample["gpu_process_bytes"] for sample in observations if sample["gpu_process_bytes"] is not None),
        default=None,
    )
    summary["observed_host_peak_rss_bytes"] = max(
        (sample["host_peak_rss_bytes"] for sample in observations if sample["host_peak_rss_bytes"] is not None),
        default=None,
    )
    summary["memory_sampling_note"] = (
        "Observed process peaks, not exact peak accounting; sampling may miss transient allocations."
    )
    try:
        readbacks = sorted(directory.glob("*.complex64le"))
        summary["readbacks_sha256"] = {path.name: sha256(path) for path in readbacks}
        if summary["status"] == "passed":
            require(
                len(readbacks) == 1
                and readbacks[0].name == "case_a_4x4-0.complex64le"
                and readbacks[0].stat().st_size == 65536 * 16,
                "missing or invalid retained first readback",
            )
        if summary["environment"] is not None:
            require(
                summary["environment"]["pid"] == summary["pid"],
                "native environment PID does not match the trial",
            )
            require(
                all(Path(path).is_absolute() for path in summary["environment"]["libraries"].values()),
                "library hashing requires absolute paths; set QDK_CUTENSORNET_LIBRARY and QDK_CUDART_LIBRARY",
            )
            summary["native_libraries_sha256"] = {
                name: {"path": str(Path(path).resolve()), "sha256": sha256(Path(path))}
                for name, path in summary["environment"]["libraries"].items()
            }
    except (OSError, ValueError, KeyError, TypeError) as error:
        summary.update(status="failed", evidence_error=str(error))
    write_json(directory / "summary.json", summary)
    return summary


def campaign(command, output, configs, trial_seconds, campaign_seconds, sampler=sample_memory):
    start = time.monotonic()
    rows = []
    with (output / "results.jsonl").open("x") as journal, (output / "results.csv").open("x", newline="") as table:
        fields = [
            "trial", "plan_source", "hyper_samples", "reconfiguration_iterations",
            "disable_rank_simplification", "seed", "status", "optimize_seconds",
            "construct_control_seconds", "export_seconds",
            "prepare_host_call_seconds", "first_readback_seconds", "repeated_median_seconds",
            "flops_estimate", "largest_intermediate_elements", "scratch_minimum_bytes",
            "scratch_recommended_bytes", "scratch_allocated_bytes", "host_scratch_allocated_bytes",
            "owned_device_bytes", "observed_gpu_process_peak_bytes", "observed_host_peak_rss_bytes",
        ]
        writer = csv.DictWriter(table, fieldnames=fields)
        writer.writeheader()
        table.flush()
        reason = "completed"
        error = None
        for index, config in enumerate(configs):
            remaining = campaign_seconds - (time.monotonic() - start)
            if remaining <= 0:
                reason = "campaign_deadline"
                break
            name = f"trial-{index:03d}"
            print(f"{name}: {json.dumps(config, sort_keys=True)}", flush=True)
            try:
                result = run_trial(
                    command, output / name, config, min(trial_seconds, remaining), sampler
                )
            except (*ERRORS, KeyboardInterrupt) as failure:
                reason = "interrupted" if isinstance(failure, KeyboardInterrupt) else "driver_failure"
                error = f"{type(failure).__name__}: {failure}"
                break
            record = {"trial": name, "config": config, **result}
            rows.append(record)
            journal.write(json.dumps(record, allow_nan=False) + "\n")
            journal.flush()
            memory = result["memory"] or {}
            estimates = result["estimates"] or {}
            timings = result["timings_seconds"]
            writer.writerow({
                **{key: config.get(key) for key in fields if key in config},
                "trial": name, "status": result["status"],
                "optimize_seconds": timings.get("optimize"),
                "construct_control_seconds": timings.get("construct_control"),
                "export_seconds": timings.get("export"),
                "prepare_host_call_seconds": timings.get("prepare_host_call"),
                "first_readback_seconds": timings.get("contract_readback_0"),
                "repeated_median_seconds": result.get("repeated_readback_median_seconds"),
                "flops_estimate": estimates.get("flops"),
                "largest_intermediate_elements": estimates.get("largest_intermediate_elements"),
                "scratch_minimum_bytes": memory.get("device_scratch_minimum"),
                "scratch_recommended_bytes": memory.get("device_scratch_recommended"),
                "scratch_allocated_bytes": memory.get("device_scratch_allocated"),
                "host_scratch_allocated_bytes": memory.get("host_scratch_allocated"),
                "owned_device_bytes": memory.get("owned_device_bytes"),
                "observed_gpu_process_peak_bytes": result["observed_gpu_process_peak_bytes"],
                "observed_host_peak_rss_bytes": result["observed_host_peak_rss_bytes"],
            })
            table.flush()
            print(f"{name}: {result['status']}", flush=True)
            if result["status"] not in CONTINUE:
                reason = "interrupted" if result["status"] == "interrupted" else "trial_failure"
                break
            if time.monotonic() - start >= campaign_seconds:
                reason = "campaign_deadline"
                break
        outcome = {
            "reason": reason, "trials_completed": len(rows), "trials_requested": len(configs),
            "error": error,
            "wall_seconds": time.monotonic() - start,
            "ranked_successful_trials": [
                row["trial"] for row in sorted(
                    (row for row in rows if row["status"] == "passed"),
                    key=lambda row: row["repeated_readback_median_seconds"],
                )
            ],
            "nonpassing_trials": [
                {"trial": row["trial"], "status": row["status"]}
                for row in rows if row["status"] != "passed"
            ],
        }
        write_json(output / "campaign.json", outcome)
        return outcome


def integers(value):
    values = [int(item) for item in value.split(",")]
    if not values or len(set(values)) != len(values) or min(values) < 0:
        raise argparse.ArgumentTypeError("expected distinct nonnegative integers")
    if max(values) > 2**31 - 1:
        raise argparse.ArgumentTypeError("values must fit in i32")
    return values


def build_binary(output, target_dir):
    command = [
        "cargo", "test", "--locked", "--release", "-p", "qdk_cutensornet",
        "--lib", "--no-run", "--message-format=json", "--target-dir", str(target_dir),
    ]
    with (output / "build.jsonl").open("x") as messages, (output / "build.log").open("x") as log:
        subprocess.run(command, cwd=ROOT, stdout=messages, stderr=log, check=True)
    binaries = {
        event["executable"]
        for line in (output / "build.jsonl").read_text().splitlines()
        if (event := json.loads(line)).get("reason") == "compiler-artifact"
        and event.get("executable")
        and event["target"]["name"] == "qdk_cutensornet"
        and event["profile"]["test"]
    }
    if len(binaries) != 1:
        raise RuntimeError(f"expected one native library test binary, got {binaries}")
    return Path(binaries.pop()).resolve()


def check_selector(command, selector):
    listing = subprocess.check_output(
        [*command, "--list", "--ignored", "--exact", selector], text=True, timeout=30
    )
    require(
        [line for line in listing.splitlines() if line.endswith(": test")] == [f"{selector}: test"],
        f"missing or non-exact selector {selector}; refusing zero-test success",
    )


def preflight(command, output):
    check_selector(command, TEST)
    for name, fixture, count in [
        ("a_asymmetric_diagnostic", "diagnostic", 8),
        ("b_case_a_2x2", "case_a_2x2", 16),
        ("c_case_a_4x4", "case_a_4x4", 65536),
    ]:
        selector = "simulation::contraction::execution::qualification::native::" + name
        check_selector(command, selector)
        environment = os.environ.copy()
        directory = output / name
        directory.mkdir()
        environment["QDK_CONTRACTION_EVIDENCE_DIR"] = str(directory.resolve())
        with (directory / "qualification.log").open("x") as log:
            subprocess.run(
                [*command, "--exact", selector, "--ignored", "--nocapture", "--test-threads=1"],
                env=environment, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=300,
            )
        text = (directory / "qualification.log").read_text()
        require(
            "test result: ok. 1 passed; 0 failed; 0 ignored;" in text,
            f"qualification did not execute exactly one passing case: {name}",
        )
        for iteration in range(2):
            path = directory / f"{fixture}-{iteration}.complex64le"
            require(path.stat().st_size == count * 16, f"invalid qualification readback: {path}")


def source_snapshot(root):
    def git(*args):
        return subprocess.check_output(["git", *args], cwd=root, text=True)

    fixture_root = root / "samples/python_interop/ising2d_tensor_network_demo/fixtures"
    paths = sorted(
        path for name in ("i3a_numerical", "case_a_4x4")
        for path in (fixture_root / name).rglob("*") if path.is_file()
    )
    return {
        "head": git("rev-parse", "HEAD").strip(),
        "source_status": git("status", "--porcelain", "--untracked-files=all"),
        "branch": git("symbolic-ref", "--quiet", "--short", "HEAD").strip(),
        "branch_refs": git("for-each-ref", "--format=%(refname) %(objectname)", "refs/heads/"),
        "fixtures_sha256": {str(path.relative_to(root)): sha256(path) for path in paths},
    }


def verify_unchanged(before, after, binary_before, binary_after):
    require(not before["source_status"], "source checkout must be clean")
    require(before == after, "source, fixtures, or branch refs changed during the run")
    require(binary_before == binary_after, "test binary changed during the run")


def stop_on_signal(signum, _frame):
    raise KeyboardInterrupt(f"received signal {signum}")


def argument_parser():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="new evidence directory; never overwritten")
    parser.add_argument("--expected-head", required=True, help="reviewed full source commit SHA")
    parser.add_argument("--target-dir", type=Path, help="Cargo target directory (default: target/contraction-experiments)")
    parser.add_argument("--hyper-samples", type=integers, default=[1, 64], help="default: 1,64")
    parser.add_argument("--reconfigurations", type=integers, default=[0, 500], help="default: 0,500")
    parser.add_argument("--seeds", type=integers, default=[17], help="default: 17")
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--trial-seconds", type=float, default=300)
    parser.add_argument("--campaign-seconds", type=float, default=6 * 3600)
    return parser


def main():
    parser = argument_parser()
    args = parser.parse_args()
    if len(args.expected_head) != 40 or any(c not in "0123456789abcdef" for c in args.expected_head):
        parser.error("expected-head must be a full lowercase commit SHA")
    if args.repeats <= 0 or args.repeats > 2**31 - 1:
        parser.error("repeats must be a positive i32")
    if any(not math.isfinite(value) or value <= 0 for value in [args.trial_seconds, args.campaign_seconds]):
        parser.error("deadlines must be positive finite seconds")
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        parser.error("native experiments require Linux x86_64")
    configs = list(configurations(args.hyper_samples, args.reconfigurations, args.seeds, args.repeats))
    args.output = args.output.resolve()
    try:
        args.output.mkdir(parents=True, exist_ok=False)
    except OSError as error:
        parser.error(str(error))
    source = None
    binary = None
    binary_hash = None
    exitcode = 1
    signal.signal(signal.SIGTERM, stop_on_signal)
    try:
        source = source_snapshot(ROOT)
        write_json(args.output / "source-before.json", source)
        require(source["head"] == args.expected_head, "unexpected source HEAD")
        require(not source["source_status"], "source checkout must be clean")
        provenance = {
            "expected_head": args.expected_head,
            "gpu": subprocess.check_output(["nvidia-smi", "--query-gpu=index,name,memory.total,memory.free,driver_version", "--format=csv"], text=True),
            "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
            "cargo": subprocess.check_output(["cargo", "--version"], text=True).strip(),
            "host": platform.platform(), "python": sys.version,
            "native_library_overrides": {
                key: os.environ.get(key) for key in ("QDK_CUTENSORNET_LIBRARY", "QDK_CUDART_LIBRARY")
            },
            "script_sha256": sha256(Path(__file__)),
            "trial_seconds": args.trial_seconds, "campaign_seconds": args.campaign_seconds,
            "workspace_bytes": WORKSPACE_BYTES, "host_scratch_ceiling": None,
            "timing_scope": "wall-clock contract + synchronization + readback/host conversion, not GPU-only",
            "preparation_timing_scope": "host call only; first contraction can include queued preparation work",
            "sampling_scope": "nvidia-smi and /proc approximately every 100 ms; overhead affects timing",
        }
        write_json(args.output / "provenance.json", provenance)
        write_json(args.output / "manifest.json", configs)
        target_dir = (args.target_dir or ROOT / "target/contraction-experiments").resolve()
        target_dir.mkdir(parents=True, exist_ok=False)
        binary = build_binary(args.output, target_dir)
        binary_hash = sha256(binary)
        write_json(args.output / "binary.json", {"path": str(binary), "sha256": binary_hash, "profile": "release"})
        preflight([str(binary)], args.output)
        verify_unchanged(source, source_snapshot(ROOT), binary_hash, sha256(binary))
        command = [str(binary), "--exact", TEST, "--ignored", "--nocapture", "--test-threads=1"]
        outcome = campaign(command, args.output, configs, args.trial_seconds, args.campaign_seconds)
        print(json.dumps(outcome, indent=2), flush=True)
        exitcode = 0 if outcome["reason"] == "completed" and outcome["ranked_successful_trials"] else 1
    except (*ERRORS, KeyboardInterrupt) as error:
        write_json(args.output / "failure.json", {"error": f"{type(error).__name__}: {error}"})
        print(f"FAIL: {error}; evidence retained in {args.output}", file=sys.stderr)
    finally:
        final = {"exitcode": exitcode}
        try:
            after = source_snapshot(ROOT)
            final["source"] = after
            final["binary_sha256"] = sha256(binary) if binary is not None else None
            if source is not None:
                verify_unchanged(source, after, binary_hash, final["binary_sha256"])
            final["source_unchanged"] = source is not None
        except ERRORS as error:
            exitcode = 1
            final.update(exitcode=1, source_unchanged=False, error=f"{type(error).__name__}: {error}")
            print(f"FAIL final provenance: {error}", file=sys.stderr)
        write_json(args.output / "final.json", final)
    return exitcode


if __name__ == "__main__":
    sys.exit(main())
