# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Live demo: P(r) of QEC measurement records, computed on the GPU.

    python live.py --circuit n.ll --records host.json
    python live.py --circuit n.ll --records host.json --method mps --chi 512

--circuit is the adaptive-profile QIR of the QEC circuit (from qdk.stim.compile).
--records is a JSON file {"m": ..., "records": {name: bits}, "expected": {name: P}}
with one bit per QIR result, in result order.

P(r) is the probability of the full measurement record r (one bit per QIR
result, SELECT acceptance included): 2^-m for a valid record with m random
outcomes, 0 for a record with a flipped deterministic bit. Each record is one
tensornetwork_qir call, so the time shown includes path finding.
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
import sys
import time
from typing import Callable, Optional, Sequence

from run import outcome, power_of_two, seconds, size


def default_records(records: dict) -> list[str]:
    """The first valid record and the first flipped one, if present."""
    valid = [name for name in records if "flip" not in name]
    flipped = [name for name in records if "flip" in name]
    return valid[:1] + flipped[:1]


def value(p: float) -> str:
    if p > 0 and math.isfinite(p):
        return f"{p:.6e} = 2^{math.log2(p):.2f}"
    return f"{p:.6g}"


def cost_text(method: str, cost: dict) -> str:
    if method == "mps":
        return f"bond {cost.get('max_bond_dimension', '—')}, state {size(cost.get('state_bytes'))}"
    return f"width {cost.get('width', 0):.0f}, workspace {size(cost.get('workspace_bytes'))}"


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    result.add_argument("--circuit", type=Path, required=True, help="QIR text (.ll)")
    result.add_argument("--records", type=Path, required=True, help="records JSON")
    result.add_argument("--record", nargs="+", help="record names (default: r0 and its first flipped copy)")
    result.add_argument("--method", choices=("contraction", "mps"), default="contraction")
    result.add_argument("--chi", type=int, default=512, help="MPS bond-dimension cap χ")
    result.add_argument("--hyper-samples", type=int, default=8, help="contraction path-finder samples")
    result.add_argument("--seed", type=int, default=17, help="contraction path-finder seed")
    return result


def main(
    argv: Optional[Sequence[str]] = None,
    *,
    tensornetwork_qir: Optional[Callable[..., list]] = None,
    clock: Callable[[], float] = time.perf_counter,
) -> int:
    """Print P(r) against its reference for each record; exit 1 if any is not exact."""
    arguments = parser()
    args = arguments.parse_args(argv)
    from qdk.simulation import ContractionOptions, Cost, MpsOptions, Probability

    if tensornetwork_qir is None:
        from qdk.simulation import tensornetwork_qir
    qir = args.circuit.read_text()
    document = json.loads(args.records.read_text())
    records, expected, m = document["records"], document["expected"], document["m"]
    names = args.record or default_records(records)
    for name in names:
        if name not in records:
            arguments.error(f"no record {name!r} in {args.records}; choose from {sorted(records)}")
    if args.method == "mps":
        options = MpsOptions(max_bond_dimension=args.chi)
        setting = f"χ = {args.chi}"
    else:
        options = ContractionOptions(hyper_samples=args.hyper_samples, seed=args.seed)
        setting = f"hyper_samples = {args.hyper_samples}, seed = {args.seed}"
    print(f"{args.circuit.name}: {len(records[names[0]])} results, m = {m} random outcomes, valid P(r) = 2^-{m}")
    print(f"method {args.method} ({setting})\n")

    width = max(len(name) for name in names)
    exact = True
    for name in names:
        reference = expected[name]
        outcomes = [bit == "1" for bit in records[name]]
        print(f"{name:<{width}}  expected {power_of_two(reference)}", flush=True)
        started = clock()
        try:
            p, cost = tensornetwork_qir(qir, [Probability(), Cost()], method=args.method, options=options, outcomes=outcomes)
        except Exception as error:  # a live demo reports the failure and moves on
            exact = False
            print(f"{'':<{width}}  failed after {seconds(clock() - started)}: {type(error).__name__}: {error}\n")
            continue
        elapsed = clock() - started
        verdict = outcome({"status": "ok", "probability": p, "expected": reference, "expected_valid": 2.0**-m})
        exact = exact and verdict == "exact"
        mark = "✓" if verdict == "exact" else "✗"
        print(f"{'':<{width}}  P(r) = {value(p)} {mark}   {cost_text(args.method, cost)}   {seconds(elapsed)}\n")
    return 0 if exact else 1


if __name__ == "__main__":
    sys.exit(main())
