# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""End-to-end pass throughput, measured rather than asserted.

A design review recorded two workloads through the delivered Python surface with
the default pipeline, and concluded from them that the rewrite driver is
quadratic in the number of applications. It recorded no harness, so neither the
quadratic finding nor any later claim of improvement could be reproduced. This
is that harness.

The two workloads, kept as the review defined them:

- ``cancelling``: ``h`` repeated on one qubit. Every adjacent pair is an
  inverse pair, so the number of rewrite applications is proportional to the
  number of gates. This is the workload the quadratic finding rests on.
- ``non-cancelling``: ``t`` and ``h`` alternating on one qubit. No pair is an
  inverse pair, so the pass applies nothing and the time is lowering, one full
  rule scan, verification, and the raise.

Method, stated here rather than left to be inferred:

- ``time.perf_counter`` around ``PassPipeline.run`` only. Circuit construction
  is excluded because it is measured by a separate harness and is two orders of
  magnitude cheaper.
- A fresh circuit per sample. ``run`` does not mutate its argument, but a reused
  circuit would still share whatever the first run warmed.
- The minimum of several samples, not the mean. Measurement noise here is
  one-directional: unrelated system activity can only inflate a duration.
- One qubit for both workloads, because an inverse pair must be adjacent on the
  same qubit for the built-in pass to match it.
- Applications are derived from the gate count the pipeline returns, since the
  surface reports which passes ran but not how often each applied. Each
  cancellation removes two gates.

The default sizes are the review's. The cancelling workload is quadratic, so
raising its sizes costs roughly four times as much per doubling.

Run ``python bench_pass_throughput.py --help`` for options.
"""

from __future__ import annotations

import argparse
import json
import platform
import sys
import time
import warnings

DEFAULT_CANCELLING = (256, 512, 1024)
DEFAULT_NON_CANCELLING = (256, 1024, 4096, 16384)
DEFAULT_SAMPLES = 3

# Recorded in the design review this harness exists to make reproducible.
REVIEW_BASELINE = {
    "cancelling": {256: 0.2216, 512: 0.8756, 1024: 3.5386},
    "non-cancelling": {256: 0.0017, 1024: 0.0068, 4096: 0.0300, 16384: 0.1241},
}


def build(workload: str, gates: int):
    from qdk.circuit import QuantumCircuit

    circuit = QuantumCircuit(1)
    for index in range(gates):
        if workload == "cancelling" or index % 2 == 1:
            circuit.h(0)
        else:
            circuit.t(0)
    return circuit


def measure(workload: str, gates: int, samples: int) -> dict:
    warnings.filterwarnings("ignore", message="qdk.circuit is experimental")
    from qdk.circuit import PassPipeline

    best = None
    remaining = None
    for _ in range(samples):
        circuit = build(workload, gates)
        pipeline = PassPipeline()
        start = time.perf_counter()
        result = pipeline.run(circuit)
        elapsed = time.perf_counter() - start
        remaining = len(result.circuit)
        best = elapsed if best is None else min(best, elapsed)

    assert best is not None and remaining is not None
    return {
        "workload": workload,
        "gates": gates,
        "seconds": best,
        "microseconds_per_gate": best * 1e6 / gates,
        "remaining_gates": remaining,
        "applications": (gates - remaining) // 2,
        "samples": samples,
        "review_seconds": REVIEW_BASELINE[workload].get(gates),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--samples", type=int, default=DEFAULT_SAMPLES)
    parser.add_argument(
        "--cancelling",
        type=str,
        default=",".join(str(size) for size in DEFAULT_CANCELLING),
        help="comma-separated gate counts, or empty to skip",
    )
    parser.add_argument(
        "--non-cancelling",
        type=str,
        default=",".join(str(size) for size in DEFAULT_NON_CANCELLING),
        help="comma-separated gate counts, or empty to skip",
    )
    parser.add_argument("--json", action="store_true", help="emit JSON only")
    args = parser.parse_args()

    results = []
    for workload, sizes in (
        ("cancelling", args.cancelling),
        ("non-cancelling", args.non_cancelling),
    ):
        for gates in [int(size) for size in sizes.split(",") if size]:
            results.append(measure(workload, gates, args.samples))

    report = {
        "platform": platform.platform(),
        "python": platform.python_version(),
        "method": "perf_counter around PassPipeline.run, fresh circuit per sample, minimum of samples",
        "results": results,
    }
    if args.json:
        print(json.dumps(report, indent=2))
        return 0

    header = (
        f"{'workload':>14}  {'gates':>6}  {'applies':>7}  {'seconds':>9}  "
        f"{'us/gate':>8}  {'review s':>9}  {'change':>8}"
    )
    print(header)
    for result in results:
        review = result["review_seconds"]
        change = f"{(result['seconds'] / review - 1) * 100:+.1f}%" if review else "n/a"
        print(
            f"{result['workload']:>14}  {result['gates']:>6}  "
            f"{result['applications']:>7}  {result['seconds']:>9.4f}  "
            f"{result['microseconds_per_gate']:>8.2f}  "
            f"{review if review else '-':>9}  {change:>8}"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
