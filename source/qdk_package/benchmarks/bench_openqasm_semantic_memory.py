#!/usr/bin/env python3

"""Measure retained memory for expanded Python OpenQASM semantic projections."""

from __future__ import annotations

import argparse
import gc
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import tracemalloc

import qdk.openqasm.semantic as semantic


def create_parser() -> argparse.ArgumentParser:
    """Create the command-line parser."""
    parser = argparse.ArgumentParser(
        description="Measure retained Python memory for OpenQASM gate broadcasts."
    )
    parser.add_argument("--repetitions", type=int, default=256)
    parser.add_argument("--register-width", type=int, default=32)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--child", action="store_true", help=argparse.SUPPRESS)
    return parser


def build_source(repetitions: int, register_width: int) -> str:
    """Build a broadcast-heavy OpenQASM source program."""
    lines = [
        "OPENQASM 3.0;",
        'include "stdgates.inc";',
        f"qubit[{register_width}] left;",
        f"qubit[{register_width}] right;",
    ]
    for _ in range(repetitions):
        lines.extend(("h left;", "cx left, right;", "rz(0.25) right;"))
    return "\n".join(lines)


def current_rss_bytes() -> int:
    """Return the current resident set size in bytes."""
    result = subprocess.run(
        ["ps", "-o", "rss=", "-p", str(os.getpid())],
        check=True,
        capture_output=True,
        text=True,
    )
    return int(result.stdout.strip()) * 1024


def measure_once(repetitions: int, register_width: int) -> dict[str, int]:
    """Analyze and retain one expanded semantic projection."""
    source = build_source(repetitions, register_width)
    source_gate_calls = repetitions * 3
    expected_projected_gate_calls = source_gate_calls * register_width
    gc.collect()
    rss_before = current_rss_bytes()
    tracemalloc.start()

    result = semantic.analyze(source, path="broadcast.qasm")
    if result.has_errors:
        raise RuntimeError(
            f"broadcast semantic analysis produced {len(result.errors)} errors"
        )
    statements = result.program.statements
    projected_gate_calls = sum(
        type(statement).__name__ == "QuantumGate" for statement in statements
    )
    if projected_gate_calls != expected_projected_gate_calls:
        raise RuntimeError(
            "expected "
            f"{expected_projected_gate_calls} projected gates, found {projected_gate_calls}"
        )
    del statements
    gc.collect()

    python_heap_live_bytes, python_heap_peak_bytes = tracemalloc.get_traced_memory()
    rss_after = current_rss_bytes()
    tracemalloc.stop()
    return {
        "source_gate_calls": source_gate_calls,
        "projected_gate_calls": projected_gate_calls,
        "python_heap_live_bytes": python_heap_live_bytes,
        "python_heap_peak_bytes": python_heap_peak_bytes,
        "rss_before_bytes": rss_before,
        "rss_after_bytes": rss_after,
        "rss_delta_bytes": rss_after - rss_before,
    }


def run_child(args: argparse.Namespace) -> dict[str, int]:
    """Run one measurement in a fresh Python process."""
    command = [
        sys.executable,
        str(Path(__file__).resolve()),
        "--repetitions",
        str(args.repetitions),
        "--register-width",
        str(args.register_width),
        "--iterations",
        "1",
        "--child",
    ]
    completed = subprocess.run(command, check=True, capture_output=True, text=True)
    return json.loads(completed.stdout)


def validate_args(args: argparse.Namespace) -> None:
    """Validate positive benchmark dimensions."""
    for name in ("repetitions", "register_width", "iterations"):
        if getattr(args, name) <= 0:
            raise ValueError(f"--{name.replace('_', '-')} must be greater than zero")


def print_summary(args: argparse.Namespace, measurements: list[dict[str, int]]) -> None:
    """Print median values across fresh-process measurements."""
    print(f"repetitions: {args.repetitions}")
    print(f"register_width: {args.register_width}")
    print(f"iterations: {args.iterations}")
    for key in measurements[0]:
        values = [measurement[key] for measurement in measurements]
        print(f"{key}: {int(statistics.median(values))}")


def main() -> int:
    """Run the retained-memory benchmark."""
    args = create_parser().parse_args()
    try:
        validate_args(args)
        if args.child:
            print(json.dumps(measure_once(args.repetitions, args.register_width)))
            return 0
        measurements = [run_child(args) for _ in range(args.iterations)]
        print_summary(args, measurements)
        return 0
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(f"Error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
