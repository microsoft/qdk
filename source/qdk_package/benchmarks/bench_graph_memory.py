# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Per-gate memory of the graph representation, measured rather than asserted.

The number this produces is a threshold's evidence. A design review recorded
about 1.4 KB per gate at three sizes, through the Python surface, holding
graphs live and excluding view materialization, but recorded no harness, so the
figure could not be reproduced or falsified. This is that harness.

Method, stated here rather than left to be inferred:

- One size per subprocess. Resident memory does not fall back reliably after a
  large allocation, so measuring several sizes in one process would charge each
  size for its predecessors.
- Resident set size is read from ``ps``, which reports the current value rather
  than the peak. The peak would include whatever the lowering allocated
  transiently and freed, which is not what a live graph costs.
- The baseline is taken after the circuit is built and after a collection, so
  the reported figure is the graph's cost and not the circuit's.
- The graph is held in a local for the duration of the measurement, and no view
  is materialized. Touching ``graph.nodes`` builds a Python list of node views,
  which measures the view rather than the graph.
- Each size is sampled several times and the minimum is reported. Measurement
  noise is one-directional here: unrelated allocation can only inflate the
  figure, never deflate it.
- Two workloads are reported. ``gates`` is the review's all-gate program and is
  the only one its baseline can be compared against. ``measured`` ends with one
  measurement per qubit, because an observable node carries a named effect
  resource and an all-gate program has none, so the first workload cannot see
  that cost at all.

Run ``python bench_graph_memory.py --help`` for options.
"""

from __future__ import annotations

import argparse
import gc
import json
import os
import platform
import subprocess
import sys
import warnings

DEFAULT_SIZES = (4_000, 16_000, 64_000)
DEFAULT_SAMPLES = 3
DEFAULT_QUBITS = 32

# Recorded in the design review this harness exists to make reproducible.
REVIEW_BASELINE = {4_000: 1180, 16_000: 1352, 64_000: 1380}


def resident_bytes() -> int:
    """Current resident set size of this process, in bytes."""
    output = subprocess.run(
        ["ps", "-o", "rss=", "-p", str(os.getpid())],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    # ps reports kilobytes on both macOS and Linux.
    return int(output) * 1024


def measure(gates: int, qubits: int, measured: bool) -> dict:
    warnings.filterwarnings("ignore", message="qdk.circuit is experimental")
    from qdk.circuit import QuantumCircuit

    circuit = QuantumCircuit(qubits, qubits if measured else 0)
    for index in range(gates):
        circuit.h(index % qubits)
    if measured:
        for qubit in range(qubits):
            circuit.measure(qubit, qubit)

    gc.collect()
    before = resident_bytes()
    graph = circuit.to_graph()
    gc.collect()
    after = resident_bytes()

    # Keeping the graph reachable is the point: a dropped graph measures the
    # allocator's behavior rather than the representation's cost.
    assert graph is not None
    return {
        "gates": gates,
        "qubits": qubits,
        "measured": measured,
        "before_bytes": before,
        "after_bytes": after,
        "graph_bytes": after - before,
        "bytes_per_gate": (after - before) / gates,
    }


def run_child(gates: int, qubits: int, measured: bool) -> dict:
    command = [sys.executable, __file__, "--child", str(gates), "--qubits", str(qubits)]
    if measured:
        command.append("--measured")
    result = subprocess.run(command, capture_output=True, text=True, check=True)
    return json.loads(result.stdout)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--child", type=int, help=argparse.SUPPRESS)
    parser.add_argument("--measured", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--qubits", type=int, default=DEFAULT_QUBITS)
    parser.add_argument("--samples", type=int, default=DEFAULT_SAMPLES)
    parser.add_argument(
        "--sizes",
        type=str,
        default=",".join(str(size) for size in DEFAULT_SIZES),
        help="comma-separated gate counts",
    )
    parser.add_argument("--json", action="store_true", help="emit JSON only")
    args = parser.parse_args()

    if args.child is not None:
        print(json.dumps(measure(args.child, args.qubits, args.measured)))
        return 0

    sizes = [int(size) for size in args.sizes.split(",")]
    results = []
    for measured in (False, True):
        for gates in sizes:
            samples = [
                run_child(gates, args.qubits, measured) for _ in range(args.samples)
            ]
            best = min(samples, key=lambda sample: sample["bytes_per_gate"])
            best["samples"] = args.samples
            # Only the all-gate workload is comparable to the review's figure.
            best["review_bytes_per_gate"] = (
                None if measured else REVIEW_BASELINE.get(gates)
            )
            results.append(best)

    report = {
        "platform": platform.platform(),
        "python": platform.python_version(),
        "method": "current RSS via ps, one size per subprocess, minimum of samples",
        "results": results,
    }
    if args.json:
        print(json.dumps(report, indent=2))
        return 0

    print(
        f"{'workload':>9}  {'gates':>8}  {'bytes/gate':>11}  {'review':>8}  {'change':>8}"
    )
    for result in results:
        review = result["review_bytes_per_gate"]
        change = (
            f"{(result['bytes_per_gate'] / review - 1) * 100:+.1f}%"
            if review
            else "n/a"
        )
        workload = "measured" if result["measured"] else "gates"
        print(
            f"{workload:>9}  {result['gates']:>8}  {result['bytes_per_gate']:>11.1f}  "
            f"{review if review else '-':>8}  {change:>8}"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
