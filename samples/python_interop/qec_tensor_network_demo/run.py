# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Render the QEC tensor-network results: memory and time against rounds k.

    python run.py render results/*.json --cotengra cotengra.jsonl --output report

``render`` needs no GPU: it reads GPU probe results, prints the per-k table
(Markdown) and the MPS χ sweeps, and, if matplotlib is installed, writes
<output>.qec-memory.png and <output>.qec-time.png.

A results file is a JSON object with
  "inputs": {name: {"qubits", "m", ...}}, one circuit per name ending in
            "_k<rounds>" (e.g. "fi_k3");
  "cases":  [{"input", "method" ("contraction" | "mps"), "queries", "record",
             "expected", "expected_valid", "status", "probability", "cost",
             "wall_seconds", "gpu_mem_mib_peak", "chi", "hyper_samples",
             "seed", ...}];
  "environment": {"gpu": "<name>, <driver>, <memory.total> MiB"} (optional).
P(r) is the probability of the whole measurement record r (one bit per QIR
result, SELECT acceptance included). For a valid record it is 2^-m, where m
is the number of random outcomes; a record with a flipped deterministic bit
has P(r) = 0. Every value is judged here against that reference (relative
error <= 1e-9, or |P| <= 1e-9 * 2^-m for P = 0); probe verdicts are ignored.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass, field
import json
import math
from pathlib import Path
import re
import sys
from typing import Callable, Optional, Sequence

AMPLITUDE_BYTES = 16  # complex128
# Full state-vector baselines on the memory plot: 16 B · 2^qubits.
BASELINE_QUBITS = (20, 40)
MPS_CHI = 512
# The contraction plan shown per k uses the ContractionOptions defaults.
DEFAULT_HYPER_SAMPLES, DEFAULT_SEED = 8, 17
ROUNDS = re.compile(r"_k(\d+)$")
TOLERANCE = 1e-9


@dataclass
class Rounds:
    """Everything measured for one number of rounds k."""

    k: int
    qubits: int
    m: int
    plan: Optional[dict] = None  # contraction Cost, default options preferred
    exact: Optional[dict] = None  # contraction Probability, valid record
    flipped: Optional[dict] = None  # contraction Probability, flipped record
    mps: Optional[dict] = None  # MPS at χ = MPS_CHI, valid record
    mps_sweep: list[dict] = field(default_factory=list)  # MPS, valid record, every χ
    plans: dict[tuple[int, int], dict] = field(default_factory=dict)  # (hyper_samples, seed) → Cost
    cotengra: dict[str, float] = field(default_factory=dict)  # objective → width


def load(paths: Sequence[Path]) -> tuple[dict[int, Rounds], Optional[int]]:
    """Per-k results from probe files, and the GPU memory in bytes if recorded."""
    rounds: dict[int, Rounds] = {}
    gpu_bytes = None
    for path in paths:
        document = json.loads(Path(path).read_text())
        gpu = (document.get("environment") or {}).get("gpu") or ""
        if match := re.search(r"(\d+) MiB", gpu):
            gpu_bytes = int(match.group(1)) << 20
        names = {}
        for name, entry in document["inputs"].items():
            if match := ROUNDS.search(name):
                k = int(match.group(1))
                names[name] = rounds.setdefault(k, Rounds(k, entry["qubits"], entry["m"]))
        for case in document["cases"]:
            if case["input"] in names and case.get("status") != "skipped":
                _place(names[case["input"]], case)
    for entry in rounds.values():
        entry.mps_sweep.sort(key=lambda case: case["chi"])
    return dict(sorted(rounds.items())), gpu_bytes


def _place(entry: Rounds, case: dict) -> None:
    ok = case["status"] == "ok"
    valid = case["expected"] != 0.0
    if case["method"] == "contraction":
        if ok and case.get("cost") and _prefer(case, entry.plan):
            entry.plan = case
        key = (case.get("hyper_samples"), case.get("seed"))
        # Cost-only runs time the planning alone, so they win over Probability runs.
        if ok and case.get("cost") and (key not in entry.plans or case["queries"] == ["cost"]):
            entry.plans[key] = case
        if "probability" in case["queries"]:
            if valid and (entry.exact is None or ok):
                entry.exact = case
            elif not valid and (entry.flipped is None or ok):
                entry.flipped = case
    elif case["method"] == "mps" and valid:
        entry.mps_sweep.append(case)
        if case["chi"] == MPS_CHI and (entry.mps is None or ok):
            entry.mps = case


def _prefer(case: dict, current: Optional[dict]) -> bool:
    def default(c: dict) -> bool:
        return (c.get("hyper_samples"), c.get("seed")) == (DEFAULT_HYPER_SAMPLES, DEFAULT_SEED)

    return current is None or (default(case) and not default(current))


def outcome(case: dict) -> str:
    """"exact", "wrong", "timeout" or "error", from the value and its reference."""
    if case["status"] == "timeout":
        return "timeout"
    if case["status"] != "ok":
        return "error"
    p, expected = case["probability"], case["expected"]
    if expected == 0.0:
        return "exact" if abs(p) <= TOLERANCE * case["expected_valid"] else "wrong"
    return "exact" if abs(p - expected) <= TOLERANCE * expected else "wrong"


def add_cotengra(rounds: dict[int, Rounds], path: Path) -> None:
    """Host cotengra widths per k and objective (JSON lines with k, objective, width)."""
    for line in Path(path).read_text().splitlines():
        if line.strip():
            row = json.loads(line)
            if row["k"] in rounds and row.get("width") is not None:
                rounds[row["k"]].cotengra[row["objective"]] = row["width"]


def size(value: Optional[float]) -> str:
    if value is None:
        return "—"
    for unit, shift in (("TiB", 40), ("GiB", 30), ("MiB", 20), ("KiB", 10)):
        if value >= 1 << shift:
            return f"{value / (1 << shift):.1f} {unit}"
    return f"{value:.0f} B"


def seconds(value: Optional[float]) -> str:
    if value is None:
        return "—"
    if value >= 600:
        return f"{value / 60:.0f} min"
    return f"{value:.0f} s" if value >= 10 else f"{value:.1f} s"


def power_of_two(value: float) -> str:
    exponent = math.log2(value) if value > 0 else None
    return f"2^{exponent:.0f}" if exponent is not None and exponent.is_integer() else f"{value:.3g}"


def probability_cell(case: Optional[dict], plan: Optional[dict], gpu_bytes: Optional[int]) -> str:
    """Exact contraction outcome against its reference, or why it did not run."""
    if case is None:
        workspace = ((plan or {}).get("cost") or {}).get("workspace_bytes")
        if workspace is not None and gpu_bytes is not None and workspace > gpu_bytes:
            return f"not run: {size(workspace)} > GPU"
        return "not run"
    if case["status"] != "ok":
        return outcome(case)
    mark = "✓" if outcome(case) == "exact" else "✗"
    value = f"{case['probability']:.3g}" if case["expected"] == 0.0 else power_of_two(case["probability"])
    return f"{value} {mark} ({seconds(case.get('wall_seconds'))})"


def _gpu_peak_bytes(case: Optional[dict]) -> Optional[int]:
    peak = None if case is None else case.get("gpu_mem_mib_peak")
    return None if peak is None else peak << 20


def mps_cells(case: Optional[dict]) -> list[str]:
    if case is None:
        return ["not run", "—", "—", "—"]
    peak = size(_gpu_peak_bytes(case))
    if case["status"] != "ok":
        return [outcome(case), "—", peak, seconds(case.get("wall_seconds"))]
    relative = abs(case["probability"] - case["expected"]) / case["expected"]
    mark = "✓" if outcome(case) == "exact" else "✗"
    state = size((case.get("cost") or {}).get("state_bytes"))
    return [f"{mark} rel. err. {relative:.1e}", state, peak, seconds(case.get("wall_seconds"))]


def render_table(rounds: dict[int, Rounds], gpu_bytes: Optional[int]) -> str:
    header = [
        "k", "qubits", "m", "P(r)", "contraction w", "workspace", "exact P(r)", "flipped record",
        f"MPS χ={MPS_CHI}", "MPS state", "GPU peak", "MPS time", "cotengra w (flops/size)",
    ]
    lines = ["| " + " | ".join(header) + " |", "|" + " --- |" * len(header)]
    for entry in rounds.values():
        cost = (entry.plan or {}).get("cost") or {}
        width = cost.get("width")
        cotengra = "/".join(
            f"{entry.cotengra[o]:.0f}" if o in entry.cotengra else "—" for o in ("flops", "size")
        )
        cells = [
            str(entry.k), str(entry.qubits), str(entry.m), f"2^-{entry.m}",
            "—" if width is None else f"{width:.0f}",
            size(cost.get("workspace_bytes")),
            probability_cell(entry.exact, entry.plan, gpu_bytes),
            probability_cell(entry.flipped, entry.plan, gpu_bytes),
            *mps_cells(entry.mps),
            cotengra if entry.cotengra else "—",
        ]
        lines.append("| " + " | ".join(cells) + " |")
    return "\n".join(lines)


def render_sweeps(rounds: dict[int, Rounds]) -> str:
    """MPS χ sweeps for every k measured at more than one χ."""
    blocks = []
    for entry in rounds.values():
        if len({case["chi"] for case in entry.mps_sweep}) < 2:
            continue
        lines = [
            f"MPS χ sweep, k={entry.k} (P(r) = 2^-{entry.m})",
            "",
            "| χ | realized bond | P(r) / 2^-m | result | time |",
            "| --- | --- | --- | --- | --- |",
        ]
        for case in entry.mps_sweep:
            bond = (case.get("cost") or {}).get("max_bond_dimension", "—")
            ratio = f"{case['probability'] / case['expected']:.3g}" if case["status"] == "ok" else case["status"]
            lines.append(
                f"| {case['chi']} | {bond} | {ratio} | {outcome(case)} | {seconds(case.get('wall_seconds'))} |"
            )
        blocks.append("\n".join(lines))
    return "\n\n".join(blocks)


def render_search(rounds: dict[int, Rounds]) -> str:
    """cuTensorNet path-search effort: one row per hyper_samples, one column per seed."""
    blocks = []
    for entry in rounds.values():
        if len(entry.plans) < 2:
            continue
        samples = sorted({hyper for hyper, _ in entry.plans})
        seeds = list(dict.fromkeys(seed for _, seed in entry.plans))
        lines = [
            f"Contraction path search, k={entry.k}: width w, flops, planning time",
            "",
            "| hyper_samples | " + " | ".join(f"seed {seed}" for seed in seeds) + " |",
            "| --- |" + " --- |" * len(seeds),
        ]
        for hyper in samples:
            cells = []
            for seed in seeds:
                case = entry.plans.get((hyper, seed))
                if case is None:
                    cells.append("—")
                    continue
                cost = case["cost"]
                cells.append(f"w {cost['width']:.0f}, {cost['flops']:.1e} flops, {seconds(case.get('wall_seconds'))}")
            lines.append(f"| {hyper} | " + " | ".join(cells) + " |")
        blocks.append("\n".join(lines))
    return "\n\n".join(blocks)


def render(rounds: dict[int, Rounds], gpu_bytes: Optional[int]) -> str:
    parts = [render_table(rounds, gpu_bytes)]
    for extra in (render_search(rounds), render_sweeps(rounds)):
        if extra:
            parts.append(extra)
    return "\n\n".join(parts)


def _ok(case: Optional[dict]) -> Optional[dict]:
    return case if case is not None and case["status"] == "ok" else None


def _series(rounds: dict[int, Rounds], pick: Callable[[Rounds], Optional[float]]) -> tuple[list, list]:
    points = [(k, value) for k, entry in rounds.items() if (value := pick(entry)) is not None]
    return [k for k, _ in points], [value for _, value in points]


def _cost(case: Optional[dict], key: str) -> Optional[float]:
    return ((case or {}).get("cost") or {}).get(key)


def plot(rounds: dict[int, Rounds], gpu_bytes: Optional[int], stem: Path) -> list[Path]:
    """Write <stem>.qec-memory.png and <stem>.qec-time.png."""
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    figure, axes = plt.subplots(figsize=(7, 4.5))
    axes.plot(*_series(rounds, lambda e: _cost(e.plan, "workspace_bytes")), "o-",
              label="exact contraction: workspace (Cost)")
    axes.plot(*_series(rounds, lambda e: _cost(_ok(e.exact), "workspace_bytes")), "o", markersize=11,
              fillstyle="none", color="C0", label="exact contraction: ran on the GPU")
    axes.plot(*_series(rounds, lambda e: _cost(_ok(e.mps), "state_bytes")), "s-",
              label=f"MPS χ={MPS_CHI}: state")
    axes.plot(*_series(rounds, lambda e: _gpu_peak_bytes(_ok(e.mps))), "s--",
              label=f"MPS χ={MPS_CHI}: GPU peak")
    for qubits, style in zip(BASELINE_QUBITS, (":", "-.")):
        baseline = AMPLITUDE_BYTES * 2.0**qubits
        axes.axhline(baseline, color="gray", linestyle=style,
                     label=f"{qubits}-qubit state vector ({size(baseline)})")
    if gpu_bytes is not None:
        axes.axhline(gpu_bytes, color="red", linewidth=0.8, label=f"GPU memory ({size(gpu_bytes)})")
    axes.set_yscale("log", base=2)
    axes.set_xlabel("rounds k")
    axes.set_ylabel("bytes")
    axes.set_title("Memory to compute P(r) exactly")
    axes.set_xticks(list(rounds))
    axes.legend(fontsize=7)
    figure.tight_layout()
    memory_path = stem.with_name(stem.name + ".qec-memory.png")
    figure.savefig(memory_path, dpi=150)
    plt.close(figure)

    figure, axes = plt.subplots(figsize=(7, 4.5))
    axes.plot(*_series(rounds, lambda e: (_ok(e.exact) or {}).get("wall_seconds")), "o-",
              label="exact contraction (plan + contract)")
    axes.plot(*_series(rounds, lambda e: None if _ok(e.exact) else (e.plan or {}).get("wall_seconds")), "o",
              fillstyle="none", color="C0", label="contraction plan only (Cost)")
    axes.plot(*_series(rounds, lambda e: (_ok(e.mps) or {}).get("wall_seconds")), "s-",
              label=f"MPS χ={MPS_CHI}")
    axes.set_yscale("log")
    axes.set_xlabel("rounds k")
    axes.set_ylabel("wall time (s)")
    axes.set_title("Time to compute P(r) on the GPU")
    axes.set_xticks(list(rounds))
    axes.legend(fontsize=7)
    figure.tight_layout()
    time_path = stem.with_name(stem.name + ".qec-time.png")
    figure.savefig(time_path, dpi=150)
    plt.close(figure)
    return [memory_path, time_path]


def parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    rendering = commands.add_parser("render", help="print the tables and write the plots (no GPU)")
    rendering.add_argument("results", type=Path, nargs="+", help="GPU probe results (JSON)")
    rendering.add_argument("--cotengra", type=Path, help="host cotengra widths (JSON lines)")
    rendering.add_argument("--output", type=Path, default=Path("qec"), help="plot path stem")
    rendering.add_argument("--no-plots", action="store_true")
    return parser


def main(argv: Optional[Sequence[str]] = None) -> int:
    args = parser().parse_args(argv)
    rounds, gpu_bytes = load(args.results)
    if args.cotengra is not None:
        add_cotengra(rounds, args.cotengra)
    print(render(rounds, gpu_bytes))
    if not args.no_plots:
        try:
            paths = plot(rounds, gpu_bytes, args.output)
        except ImportError:
            print("\nmatplotlib is not installed; skipping plots", file=sys.stderr)
        else:
            print("\nplots: " + ", ".join(str(path) for path in paths))
    return 0


if __name__ == "__main__":
    sys.exit(main())
