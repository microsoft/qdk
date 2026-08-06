# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Compare `qdk.openqasm` parsing against the reference OpenQASM 3 parser.

Run with any Python 3.10+ interpreter, from anywhere in the repository:

    python source/qdk_package/benchmarks/bench_openqasm_parsers.py

The script provisions its own virtual environment, installs the newest `qdk`
wheel found in `target/wheels` alongside pinned reference-parser dependencies,
then re-executes itself inside that environment. Build the wheel first with
`./build.py --qdk`, otherwise the run measures whatever wheel is already there.

Each cell of the matrix runs in a fresh worker process. Timing workers generate
their source before timing, validate every result, and time only the public API
call. Memory workers are spawned separately and polled externally by the
coordinator, so RSS sampling never perturbs the latency numbers.

The generated workload is a repeating mix of single- and multi-qubit gates, gate
modifiers, user-defined gates and subroutines, classical declarations and
arithmetic, control flow, aliases, timing, and measurement. Statement counts
include statements nested inside blocks, so throughput is reported per statement
as well as per byte.
"""

from __future__ import annotations

import argparse
import gc
import hashlib
import importlib
import importlib.metadata
import json
import os
import platform
import shutil
import statistics
import subprocess
import sys
import time
import traceback
import venv
from collections.abc import Callable
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Literal, NamedTuple

EXIT_SUCCESS = 0
EXIT_ERROR = 2

KIB = 1024
MIB = 1024 * KIB

SIZES = {
    "10KiB": 10 * KIB,
    "100KiB": 100 * KIB,
    "1MiB": MIB,
    "5MiB": 5 * MIB,
    "10MiB": 10 * MIB,
}
DEFAULT_ITERATIONS = {
    "10KiB": 7,
    "100KiB": 7,
    "1MiB": 5,
    "5MiB": 3,
    "10MiB": 3,
}

OPERATIONS = ("qdk_syntax", "reference_syntax", "qdk_semantic")
Operation = Literal["qdk_syntax", "reference_syntax", "qdk_semantic"]

# Pinned so repeated runs compare against an unchanging reference implementation.
REFERENCE_REQUIREMENTS = ("openqasm3[parser]==1.0.1", "antlr4-python3-runtime==4.13.2")
HARNESS_REQUIREMENTS = ("psutil==7.2.2",)

MEMORY_SAMPLE_INTERVAL_SECONDS = 0.001
MEMORY_RESULT_HOLD_SECONDS = 0.05

ROLE_COORDINATOR = "coordinator"
ROLE_TIMING = "timing-worker"
ROLE_MEMORY = "memory-worker"

STAMP_VERSION = 1


# --------------------------------------------------------------------------
# Workload
# --------------------------------------------------------------------------


class Fragment(NamedTuple):
    """One emitted chunk of QASM and the statements it contributes."""

    kind: str
    text: str
    top_level: int
    total: int


# `switch` requires 3.1; the reference parser accepts the version marker as well.
QASM_VERSION = "3.1"

# Declarations shared by every fragment, paired with their total statement counts
# (each entry is a single top-level statement; block bodies add nested ones).
HEADER_STATEMENTS: tuple[tuple[str, int], ...] = (
    ('include "stdgates.inc";', 1),
    ("const int[32] SHOTS = 128;", 1),
    ("qubit[32] q;", 1),
    ("bit[32] c;", 1),
    ("int[32] counter = 0;", 1),
    ("uint[16] accumulator = 0;", 1),
    ("float[64] theta = 0.0;", 1),
    ("angle[32] drift = 0.0;", 1),
    ("bool flag = false;", 1),
    ("array[int[32], 8] weights = {1, 2, 3, 4, 5, 6, 7, 8};", 1),
    ("gate entangle a, b { h a; cx a, b; }", 3),
    ("def accumulate(int[32] value) -> int[32] { return value + 1; }", 2),
)
HEADER = f"OPENQASM {QASM_VERSION};\n" + "".join(
    f"{text}\n" for text, _ in HEADER_STATEMENTS
)
HEADER_TOP_LEVEL = len(HEADER_STATEMENTS)
HEADER_TOTAL = sum(total for _, total in HEADER_STATEMENTS)
INCLUDE_COUNT = 1


def cycle_fragments(cycle: int) -> tuple[Fragment, ...]:
    """Build one cycle of the workload over rotating, distinct qubit operands."""
    base = (cycle * 5) % 32
    a, b, d = base, (base + 1) % 32, (base + 2) % 32
    window = base % 28

    return (
        Fragment(
            "single_qubit_gates",
            f"h q[{a}];\nrz(theta) q[{b}];\nsx q[{d}];\n",
            3,
            3,
        ),
        Fragment(
            "multi_qubit_gates",
            f"cx q[{a}], q[{b}];\ncz q[{b}], q[{d}];\n"
            f"swap q[{a}], q[{d}];\nccx q[{a}], q[{b}], q[{d}];\n",
            4,
            4,
        ),
        Fragment(
            "gate_modifiers",
            f"ctrl @ U(pi / 2, 0, pi) q[{a}], q[{b}];\n"
            f"inv @ s q[{b}];\npow(2) @ x q[{d}];\n",
            3,
            3,
        ),
        Fragment(
            "user_defined_calls",
            f"entangle q[{a}], q[{b}];\ncounter = accumulate(counter);\n",
            2,
            2,
        ),
        Fragment(
            "classical_compute",
            f"counter = counter + weights[{a % 8}];\n"
            "theta = theta * 0.5 + pi / 8;\n"
            "accumulator = (accumulator << 1) ^ 3;\n"
            "flag = counter > 16;\n",
            4,
            4,
        ),
        Fragment(
            "for_loop",
            "for int j in [0:3] {\n"
            "  int[32] step = j * 2;\n"
            "  float[64] phi = theta / 4.0;\n"
            f"  rx(phi) q[{a}];\n"
            f"  cx q[{a}], q[{b}];\n"
            "}\n",
            1,
            5,
        ),
        Fragment(
            "branch",
            f"if (flag) {{\n  x q[{a}];\n}} else {{\n  z q[{b}];\n}}\n",
            1,
            3,
        ),
        Fragment(
            "switch",
            "switch (counter) {\n"
            f"  case 1 {{ y q[{a}]; }}\n"
            f"  default {{ t q[{b}]; }}\n"
            "}\n",
            1,
            3,
        ),
        Fragment(
            "while_loop",
            "while (counter > 64) {\n  counter = counter - 16;\n}\n",
            1,
            2,
        ),
        Fragment(
            "alias_and_bit_strings",
            "for int j in [0:1] {\n"
            f"  let view = q[{window}:{window + 3}];\n"
            '  bit[8] mask = "10110010";\n'
            "  h view[j];\n"
            "  c[j] = mask[j];\n"
            "}\n",
            1,
            5,
        ),
        Fragment(
            "nested_control",
            "for int j in [0:3] {\n"
            "  if (counter > 8) {\n"
            f"    ccx q[{a}], q[{b}], q[{d}];\n"
            "  } else {\n"
            f"    swap q[{b}], q[{d}];\n"
            "  }\n"
            "}\n",
            1,
            4,
        ),
        Fragment(
            "timing_and_sync",
            f"delay[100ns] q[{b}];\nbarrier q[{a}], q[{b}], q[{d}];\n",
            2,
            2,
        ),
        Fragment(
            "measurement",
            f"c[{a}] = measure q[{a}];\nreset q[{a}];\nc = measure q;\n",
            3,
            3,
        ),
    )


def generate_qasm(target_bytes: int) -> tuple[str, dict[str, Any]]:
    """Generate a deterministic, semantically valid, exact-size QASM program.

    The workload mixes single- and multi-qubit gates, gate modifiers, user gates
    and subroutines, classical declarations and arithmetic, control flow, aliases,
    timing, and measurement, so the numbers reflect a realistic statement mix
    rather than one hot path.
    """
    source_parts = [HEADER]
    byte_count = len(HEADER)
    top_level_count = HEADER_TOP_LEVEL
    total_count = HEADER_TOTAL
    fragment_counts: dict[str, int] = {}
    cycle = 0

    emitted_in_cycle = True
    while emitted_in_cycle:
        emitted_in_cycle = False
        for fragment in cycle_fragments(cycle):
            if byte_count + len(fragment.text) > target_bytes:
                continue
            source_parts.append(fragment.text)
            byte_count += len(fragment.text)
            top_level_count += fragment.top_level
            total_count += fragment.total
            fragment_counts[fragment.kind] = fragment_counts.get(fragment.kind, 0) + 1
            emitted_in_cycle = True
        cycle += 1

    padding_bytes = target_bytes - byte_count
    source = "".join(source_parts) + " " * padding_bytes
    encoded = source.encode("ascii")
    if len(encoded) != target_bytes:
        raise AssertionError(f"generated {len(encoded)} bytes, expected {target_bytes}")

    return source, {
        "target_bytes": target_bytes,
        "actual_bytes": len(encoded),
        "sha256": hashlib.sha256(encoded).hexdigest(),
        "statement_count": total_count,
        "top_level_statement_count": top_level_count,
        "include_count": INCLUDE_COUNT,
        "padding_bytes": padding_bytes,
        "cycles": cycle,
        "fragment_counts": fragment_counts,
    }


def get_operation(operation: Operation) -> Callable[[str], Any]:
    """Import and return one public parsing entry point."""
    if operation == "qdk_syntax":
        parser = importlib.import_module("qdk.openqasm.parser")
        return lambda source: parser.parse(source, path="benchmark.qasm")
    if operation == "reference_syntax":
        return importlib.import_module("openqasm3").parse
    if operation == "qdk_semantic":
        semantic = importlib.import_module("qdk.openqasm.semantic")
        return lambda source: semantic.analyze(source, path="benchmark.qasm")
    raise ValueError(f"unknown operation: {operation}")


def validate_result(
    operation: Operation, result: Any, workload: dict[str, Any]
) -> None:
    """Reject silent parse failures and mismatched AST shapes."""
    if operation.startswith("qdk_") and result.has_errors:
        messages = [diagnostic.message for diagnostic in result.diagnostics[:5]]
        raise RuntimeError(f"{operation} returned diagnostics: {messages}")

    expected_statements = workload["top_level_statement_count"]
    if operation == "qdk_semantic":
        # Semantic analysis resolves `include` and drops it from the program.
        expected_statements -= workload["include_count"]

    program = result.program if operation.startswith("qdk_") else result
    actual_statements = len(program.statements)
    if actual_statements != expected_statements:
        raise RuntimeError(
            f"{operation} produced {actual_statements} top-level statements; "
            f"expected {expected_statements}"
        )


# --------------------------------------------------------------------------
# Statistics
# --------------------------------------------------------------------------


def percentile(samples: list[float], fraction: float) -> float:
    """Compute a linearly interpolated percentile."""
    ordered = sorted(samples)
    if len(ordered) == 1:
        return ordered[0]
    position = (len(ordered) - 1) * fraction
    lower = int(position)
    upper = min(lower + 1, len(ordered) - 1)
    weight = position - lower
    return ordered[lower] * (1.0 - weight) + ordered[upper] * weight


def summarize_samples(
    samples_ns: list[int], byte_count: int, statement_count: int
) -> dict[str, Any]:
    """Summarize raw nanosecond samples as latency and throughput statistics."""
    samples_ms = [sample / 1_000_000 for sample in samples_ns]
    median_ms = statistics.median(samples_ms)
    q1_ms = percentile(samples_ms, 0.25)
    q3_ms = percentile(samples_ms, 0.75)
    return {
        "samples_ms": samples_ms,
        "median_ms": median_ms,
        "q1_ms": q1_ms,
        "q3_ms": q3_ms,
        "iqr_ms": q3_ms - q1_ms,
        "mad_ms": statistics.median([abs(sample - median_ms) for sample in samples_ms]),
        "min_ms": min(samples_ms),
        "max_ms": max(samples_ms),
        "median_throughput_mib_s": (byte_count / MIB) / (median_ms / 1000),
        "median_throughput_statements_s": statement_count / (median_ms / 1000),
    }


# --------------------------------------------------------------------------
# Workers
# --------------------------------------------------------------------------


def timing_worker(
    operation: Operation, size_label: str, warmups: int, iterations: int
) -> dict[str, Any]:
    """Run validation warmups and latency samples in an isolated process."""
    source, workload = generate_qasm(SIZES[size_label])
    parse = get_operation(operation)

    for _ in range(warmups):
        result = parse(source)
        validate_result(operation, result, workload)
        del result
        gc.collect()

    samples_ns: list[int] = []
    for _ in range(iterations):
        start = time.perf_counter_ns()
        result = parse(source)
        elapsed = time.perf_counter_ns() - start
        validate_result(operation, result, workload)
        samples_ns.append(elapsed)
        del result
        gc.collect()

    return {
        "operation": operation,
        "size_label": size_label,
        "warmups": warmups,
        "iterations": iterations,
        "workload": workload,
        "statistics": summarize_samples(
            samples_ns, workload["actual_bytes"], workload["statement_count"]
        ),
    }


def memory_worker(operation: Operation, size_label: str) -> int:
    """Wait for the coordinator, parse once, and report retained RSS."""
    import psutil

    source, workload = generate_qasm(SIZES[size_label])
    parse = get_operation(operation)
    process = psutil.Process()
    gc.collect()

    print(
        json.dumps(
            {
                "event": "ready",
                "baseline_rss_bytes": process.memory_info().rss,
                "workload": workload,
            }
        ),
        flush=True,
    )
    if not sys.stdin.readline():
        raise RuntimeError("memory coordinator closed stdin before start")

    result = parse(source)
    validate_result(operation, result, workload)
    retained_rss = process.memory_info().rss
    # Hold the result alive so the external sampler observes the retained AST.
    time.sleep(MEMORY_RESULT_HOLD_SECONDS)
    print(
        json.dumps({"event": "complete", "retained_rss_bytes": retained_rss}),
        flush=True,
    )
    return EXIT_SUCCESS


def worker_command(
    role: str,
    operation: Operation,
    size_label: str,
    warmups: int = 0,
    iterations: int = 1,
) -> list[str]:
    """Construct an internal worker command using the active interpreter."""
    return [
        sys.executable,
        str(Path(__file__).resolve()),
        "--role",
        role,
        "--operation",
        operation,
        "--size-label",
        size_label,
        "--warmups",
        str(warmups),
        "--iterations",
        str(iterations),
    ]


def run_timing_process(
    operation: Operation, size_label: str, warmups: int, iterations: int
) -> dict[str, Any]:
    """Run and decode one timing worker."""
    completed = subprocess.run(
        worker_command(ROLE_TIMING, operation, size_label, warmups, iterations),
        check=False,
        capture_output=True,
        text=True,
    )
    if completed.returncode != EXIT_SUCCESS:
        raise RuntimeError(
            f"timing worker failed ({completed.returncode}): {completed.stderr.strip()}"
        )
    return json.loads(completed.stdout)


def run_memory_process(operation: Operation, size_label: str) -> dict[str, Any]:
    """Sample one memory worker's RSS from the coordinator process."""
    import psutil

    process = subprocess.Popen(
        worker_command(ROLE_MEMORY, operation, size_label),
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )
    if process.stdout is None or process.stdin is None or process.stderr is None:
        process.kill()
        raise RuntimeError("failed to create memory worker pipes")

    ready_line = process.stdout.readline()
    if not ready_line:
        stderr = process.stderr.read()
        process.wait()
        raise RuntimeError(f"memory worker did not become ready: {stderr.strip()}")
    ready = json.loads(ready_line)
    if ready.get("event") != "ready":
        process.kill()
        raise RuntimeError(f"unexpected memory worker event: {ready}")

    child = psutil.Process(process.pid)
    peak_rss = ready["baseline_rss_bytes"]
    process.stdin.write("start\n")
    process.stdin.flush()
    process.stdin.close()

    while process.poll() is None:
        try:
            peak_rss = max(peak_rss, child.memory_info().rss)
        except psutil.NoSuchProcess:
            break
        time.sleep(MEMORY_SAMPLE_INTERVAL_SECONDS)

    remaining_stdout = process.stdout.read()
    stderr = process.stderr.read()
    return_code = process.wait()
    if return_code != EXIT_SUCCESS:
        raise RuntimeError(f"memory worker failed ({return_code}): {stderr.strip()}")

    events = [
        json.loads(line) for line in remaining_stdout.splitlines() if line.strip()
    ]
    complete = next(
        (event for event in events if event.get("event") == "complete"), None
    )
    if complete is None:
        raise RuntimeError(
            f"memory worker omitted completion event: {remaining_stdout!r}"
        )

    baseline_rss = ready["baseline_rss_bytes"]
    retained_rss = complete["retained_rss_bytes"]
    peak_rss = max(peak_rss, retained_rss)
    return {
        "method": "fresh child process sampled externally with psutil RSS",
        "sample_interval_ms": MEMORY_SAMPLE_INTERVAL_SECONDS * 1000,
        "baseline_rss_bytes": baseline_rss,
        "peak_rss_bytes": peak_rss,
        "incremental_peak_rss_bytes": max(0, peak_rss - baseline_rss),
        "retained_rss_bytes": retained_rss,
        "incremental_retained_rss_bytes": max(0, retained_rss - baseline_rss),
    }


# --------------------------------------------------------------------------
# Environment provisioning
# --------------------------------------------------------------------------


def find_repo_root(script: Path) -> Path:
    """Find the workspace root without assuming a fixed script depth."""
    for parent in script.resolve().parents:
        if (parent / "Cargo.toml").exists() and (
            parent / "source" / "qdk_package"
        ).is_dir():
            return parent
    raise RuntimeError(f"could not locate the repository root from {script}")


def sha256_file(path: Path) -> str:
    """Return the SHA-256 digest of a file."""
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(MIB), b""):
            digest.update(chunk)
    return digest.hexdigest()


def newest_wheel(wheel_dir: Path, distribution: str) -> Path | None:
    """Return the most recently built wheel for one distribution."""
    candidates = sorted(
        wheel_dir.glob(f"{distribution}-*.whl"),
        key=lambda path: path.stat().st_mtime,
        reverse=True,
    )
    return candidates[0] if candidates else None


def venv_python(venv_dir: Path) -> Path:
    """Return the interpreter path inside a virtual environment."""
    if os.name == "nt":
        return venv_dir / "Scripts" / "python.exe"
    return venv_dir / "bin" / "python"


def environment_stamp(qdk_wheel: Path, shim_wheel: Path | None) -> dict[str, Any]:
    """Describe the inputs that determine whether the venv needs rebuilding."""
    return {
        "stamp_version": STAMP_VERSION,
        "python": f"{sys.version_info.major}.{sys.version_info.minor}",
        "qdk_wheel": qdk_wheel.name,
        "qdk_wheel_sha256": sha256_file(qdk_wheel),
        "shim_wheel": shim_wheel.name if shim_wheel else None,
        "shim_wheel_sha256": sha256_file(shim_wheel) if shim_wheel else None,
        "requirements": [*REFERENCE_REQUIREMENTS, *HARNESS_REQUIREMENTS],
    }


def provision_environment(
    venv_dir: Path, qdk_wheel: Path, shim_wheel: Path | None, recreate: bool
) -> tuple[Path, dict[str, Any]]:
    """Create or reuse the benchmark virtual environment."""
    stamp_path = venv_dir / "benchmark-stamp.json"
    desired = environment_stamp(qdk_wheel, shim_wheel)
    interpreter = venv_python(venv_dir)

    if not recreate and interpreter.exists() and stamp_path.exists():
        try:
            if json.loads(stamp_path.read_text(encoding="utf-8")) == desired:
                print(f"reusing {venv_dir}")
                return interpreter, desired
        except json.JSONDecodeError:
            pass

    if venv_dir.exists():
        shutil.rmtree(venv_dir)

    print(f"creating {venv_dir}")
    venv.create(venv_dir, with_pip=True)

    packages = [str(qdk_wheel)]
    if shim_wheel is not None:
        # The qdk wheel pins the local qsharp shim, which is not on PyPI.
        packages.append(str(shim_wheel))
    packages.extend(REFERENCE_REQUIREMENTS)
    packages.extend(HARNESS_REQUIREMENTS)

    print(f"installing {', '.join(Path(p).name for p in packages)}")
    subprocess.run(
        [
            str(interpreter),
            "-m",
            "pip",
            "install",
            "--quiet",
            "--disable-pip-version-check",
            *packages,
        ],
        check=True,
    )
    stamp_path.write_text(json.dumps(desired, indent=2) + "\n", encoding="utf-8")
    return interpreter, desired


def describe_environment(stamp: dict[str, Any]) -> dict[str, Any]:
    """Record the versions and machine details behind a set of results."""
    return {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "python_version": platform.python_version(),
        "platform": platform.platform(),
        "processor": platform.processor() or platform.machine(),
        "qdk_version": importlib.metadata.version("qdk"),
        "openqasm3_version": importlib.metadata.version("openqasm3"),
        "antlr4_version": importlib.metadata.version("antlr4-python3-runtime"),
        "wheel": stamp,
    }


# --------------------------------------------------------------------------
# Coordination and reporting
# --------------------------------------------------------------------------


def coordinate(
    sizes: list[str],
    warmups: int,
    iterations: int | None,
    measure_memory: bool,
    stamp: dict[str, Any],
) -> dict[str, Any]:
    """Run every operation and size, returning the full report."""
    results: list[dict[str, Any]] = []
    workloads: dict[str, Any] = {}

    for size_label in sizes:
        size_iterations = iterations or DEFAULT_ITERATIONS[size_label]
        for operation in OPERATIONS:
            print(
                f"timing {operation} at {size_label} ({size_iterations} iterations)",
                flush=True,
            )
            result = run_timing_process(operation, size_label, warmups, size_iterations)
            workloads[size_label] = result.pop("workload")
            if measure_memory:
                print(f"memory {operation} at {size_label}", flush=True)
                result["memory"] = run_memory_process(operation, size_label)
            results.append(result)

    return {
        "environment": describe_environment(stamp),
        "workloads": workloads,
        "results": results,
    }


def render_markdown(report: dict[str, Any]) -> str:
    """Render the comparison tables."""
    environment = report["environment"]
    workloads = report["workloads"]
    index = {(r["operation"], r["size_label"]): r for r in report["results"]}
    sizes = [label for label in SIZES if label in workloads]
    has_memory = all("memory" in r for r in report["results"])

    lines = [
        "# OpenQASM parser comparison",
        "",
        f"- qdk `{environment['qdk_version']}` from `{environment['wheel']['qdk_wheel']}`",
        f"- reference openqasm3 `{environment['openqasm3_version']}`"
        f" (antlr4 `{environment['antlr4_version']}`)",
        f"- Python {environment['python_version']} on {environment['platform']}",
        f"- generated {environment['generated_at']}",
        "",
        "## Median latency",
        "",
        "| Size | Statements | qdk syntax | reference syntax | speedup |"
        " qdk semantic | vs reference |",
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    for size in sizes:
        syntax = index[("qdk_syntax", size)]["statistics"]["median_ms"]
        reference = index[("reference_syntax", size)]["statistics"]["median_ms"]
        semantic = index[("qdk_semantic", size)]["statistics"]["median_ms"]
        lines.append(
            f"| {size} | {workloads[size]['statement_count']:,} |"
            f" {format_ms(syntax)} | {format_ms(reference)} | {reference / syntax:.0f}x |"
            f" {format_ms(semantic)} | {reference / semantic:.0f}x |"
        )

    lines += [
        "",
        "## Median throughput (MiB/s)",
        "",
        "| Size | qdk syntax | reference syntax | qdk semantic |",
        "| --- | ---: | ---: | ---: |",
    ]
    for size in sizes:
        lines.append(
            f"| {size} |"
            + "".join(
                f" {index[(op, size)]['statistics']['median_throughput_mib_s']:.1f} |"
                for op in OPERATIONS
            )
        )

    lines += [
        "",
        "## Median throughput (statements/s)",
        "",
        "| Size | Statements | qdk syntax | reference syntax | qdk semantic |",
        "| --- | ---: | ---: | ---: | ---: |",
    ]
    for size in sizes:
        statements = workloads[size]["statement_count"]
        lines.append(
            f"| {size} | {statements:,} |"
            + "".join(
                f" {index[(op, size)]['statistics']['median_throughput_statements_s']:,.0f} |"
                for op in OPERATIONS
            )
        )

    if has_memory:
        lines += [
            "",
            "## Incremental peak RSS",
            "",
            "| Size | qdk syntax | reference syntax | ratio | qdk semantic |",
            "| --- | ---: | ---: | ---: | ---: |",
        ]
        for size in sizes:
            syntax, reference, semantic = (
                index[(op, size)]["memory"]["incremental_peak_rss_bytes"]
                for op in OPERATIONS
            )
            lines.append(
                f"| {size} | {syntax / MIB:.1f} MiB | {reference / MIB:.1f} MiB |"
                f" {reference / syntax:.1f}x | {semantic / MIB:.1f} MiB |"
            )

        lines += [
            "",
            "## Peak RSS as a multiple of source size",
            "",
            "| Size | qdk syntax | reference syntax | qdk semantic |",
            "| --- | ---: | ---: | ---: |",
        ]
        for size in sizes:
            source_bytes = workloads[size]["actual_bytes"]
            lines.append(
                f"| {size} |"
                + "".join(
                    f" {index[(op, size)]['memory']['incremental_peak_rss_bytes'] / source_bytes:.0f}x |"
                    for op in OPERATIONS
                )
            )

    return "\n".join(lines) + "\n"


def format_ms(value: float) -> str:
    """Format a millisecond duration at a readable scale."""
    if value >= 10_000:
        return f"{value / 1000:.1f} s"
    if value >= 100:
        return f"{value:.0f} ms"
    return f"{value:.2f} ms"


# --------------------------------------------------------------------------
# Entry points
# --------------------------------------------------------------------------


def create_parser() -> argparse.ArgumentParser:
    """Create the command-line parser."""
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "--sizes",
        nargs="+",
        choices=list(SIZES),
        default=list(SIZES),
        help="input sizes to benchmark (use a subset for a fast loop)",
    )
    parser.add_argument(
        "--warmups",
        type=int,
        default=1,
        help="validation warmups per operation and size",
    )
    parser.add_argument(
        "--iterations",
        type=int,
        help="override the default size-dependent iteration count",
    )
    parser.add_argument(
        "--no-memory",
        dest="memory",
        action="store_false",
        help="skip memory measurement and report latency only",
    )
    parser.add_argument("--wheel", type=Path, help="qdk wheel to benchmark")
    parser.add_argument("--venv-dir", type=Path, help="virtual environment location")
    parser.add_argument(
        "--recreate-venv", action="store_true", help="rebuild the virtual environment"
    )
    parser.add_argument("--json", type=Path, help="write the full report as JSON")
    parser.add_argument("--markdown", type=Path, help="write the tables to a file")
    parser.add_argument(
        "--role",
        choices=(ROLE_COORDINATOR, ROLE_TIMING, ROLE_MEMORY),
        help=argparse.SUPPRESS,
    )
    parser.add_argument("--operation", choices=OPERATIONS, help=argparse.SUPPRESS)
    parser.add_argument("--size-label", choices=list(SIZES), help=argparse.SUPPRESS)
    parser.add_argument("--stamp", type=Path, help=argparse.SUPPRESS)
    return parser


def bootstrap(args: argparse.Namespace) -> int:
    """Provision the environment, then re-run the coordinator inside it."""
    script = Path(__file__).resolve()
    repo_root = find_repo_root(script)
    wheel_dir = repo_root / "target" / "wheels"

    qdk_wheel = args.wheel or newest_wheel(wheel_dir, "qdk")
    if qdk_wheel is None or not qdk_wheel.exists():
        raise RuntimeError(
            f"no qdk wheel in {wheel_dir}; build one with `./build.py --qdk` first"
        )
    shim_wheel = newest_wheel(wheel_dir, "qsharp")
    venv_dir = args.venv_dir or script.parent / ".venv-openqasm-bench"

    print(f"benchmarking {qdk_wheel.name}")
    interpreter, stamp = provision_environment(
        venv_dir, qdk_wheel.resolve(), shim_wheel, args.recreate_venv
    )

    stamp_path = venv_dir / "benchmark-stamp.json"
    forwarded = [
        str(interpreter),
        str(script),
        "--role",
        ROLE_COORDINATOR,
        "--sizes",
        *args.sizes,
        "--warmups",
        str(args.warmups),
        "--stamp",
        str(stamp_path),
    ]
    if args.iterations is not None:
        forwarded += ["--iterations", str(args.iterations)]
    if not args.memory:
        forwarded.append("--no-memory")
    if args.json:
        forwarded += ["--json", str(args.json.resolve())]
    if args.markdown:
        forwarded += ["--markdown", str(args.markdown.resolve())]

    return subprocess.run(forwarded, check=False).returncode


def run_coordinator(args: argparse.Namespace) -> int:
    """Run the measurement matrix and emit the report."""
    stamp = json.loads(args.stamp.read_text(encoding="utf-8")) if args.stamp else {}
    report = coordinate(args.sizes, args.warmups, args.iterations, args.memory, stamp)

    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(f"wrote {args.json}")

    tables = render_markdown(report)
    if args.markdown:
        args.markdown.parent.mkdir(parents=True, exist_ok=True)
        args.markdown.write_text(tables, encoding="utf-8")
        print(f"wrote {args.markdown}")
    print()
    print(tables)
    return EXIT_SUCCESS


def run() -> int:
    """Dispatch to the requested role."""
    args = create_parser().parse_args()
    if args.warmups < 0:
        raise ValueError("warmups must be non-negative")
    if args.iterations is not None and args.iterations < 1:
        raise ValueError("iterations must be positive")

    if args.role in (ROLE_TIMING, ROLE_MEMORY):
        if args.operation is None or args.size_label is None:
            raise ValueError("worker roles require --operation and --size-label")
        if args.role == ROLE_MEMORY:
            return memory_worker(args.operation, args.size_label)
        print(
            json.dumps(
                timing_worker(
                    args.operation, args.size_label, args.warmups, args.iterations or 1
                )
            )
        )
        return EXIT_SUCCESS

    if args.role == ROLE_COORDINATOR:
        return run_coordinator(args)
    return bootstrap(args)


def main() -> int:
    """Run the benchmark with top-level error handling."""
    try:
        return run()
    except KeyboardInterrupt:
        print("Interrupted", file=sys.stderr)
        return 130
    except Exception as error:
        print(f"Error: {error}", file=sys.stderr)
        traceback.print_exc()
        return EXIT_ERROR


if __name__ == "__main__":
    sys.exit(main())
