"""Regenerate the DEMO.md figures from the reduced measurement files.

Reads ``measurements.csv`` and ``frequencies.csv`` in this directory and writes
four SVG files. No network access and no GPU are required: the figures are
rendered from retained evidence, not from a live run.

Usage (from the repository root)::

    ./source/qdk_package/.venv/bin/python \\
        samples/python_interop/mps_trotter_quench_demo/figures/make_figures.py

matplotlib is already a QDK development dependency (``check_requirements.txt``),
so the qdk virtual environment satisfies this script without extra installs.
"""

from __future__ import annotations

import csv
import math
from pathlib import Path

import matplotlib

matplotlib.use("Agg")

import matplotlib.pyplot as plt  # noqa: E402

HERE = Path(__file__).parent

# Deterministic SVG output so regenerated figures diff cleanly.
matplotlib.rcParams["svg.hashsalt"] = "mps-trotter-quench-demo"
matplotlib.rcParams["svg.fonttype"] = "none"
matplotlib.rcParams["figure.dpi"] = 100
matplotlib.rcParams["font.size"] = 9
matplotlib.rcParams["axes.grid"] = True
matplotlib.rcParams["grid.alpha"] = 0.3

CPU_COLOR = "#c0392b"
MPS_COLOR = "#1f77b4"
REFERENCE_COLOR = "#7f8c8d"

# Fitted on the two ladder endpoints (512 and 8192); see DEMO.md section 3.
SECONDS_PER_QUBIT = 0.0348

# Dense statevector amplitudes are complex128: 16 bytes each.
BYTES_PER_AMPLITUDE = 16

# nvidia-smi reports the A100 80GB PCIe as 81920 MiB of usable device memory.
A100_CAPACITY_MIB = 81920


def _float(value: str) -> float | None:
    return float(value) if value else None


def _int(value: str) -> int | None:
    return int(value) if value else None


def load_measurements() -> list[dict]:
    rows = []
    with (HERE / "measurements.csv").open(newline="") as handle:
        for row in csv.DictReader(line for line in handle if not line.startswith("#")):
            row["width"] = int(row["width"])
            row["shots"] = int(row["shots"])
            row["total_seconds"] = _float(row["total_seconds"])
            row["per_shot_seconds"] = _float(row["per_shot_seconds"])
            row["peak_gpu_mib"] = _int(row["peak_gpu_mib"])
            row["peak_host_bytes"] = _int(row["peak_host_bytes"])
            rows.append(row)
    return rows


def load_frequencies() -> tuple[list[int], list[float], list[float]]:
    qubits, cpu, mps = [], [], []
    with (HERE / "frequencies.csv").open(newline="") as handle:
        for row in csv.DictReader(line for line in handle if not line.startswith("#")):
            qubits.append(int(row["qubit"]))
            cpu.append(float(row["cpu"]))
            mps.append(float(row["mps"]))
    return qubits, cpu, mps


def select(rows, dataset, backend, *, label=None, outcome="success"):
    chosen = [
        row
        for row in rows
        if row["dataset"] == dataset
        and row["backend"] == backend
        and (label is None or row["label"] == label)
        and (outcome is None or row["outcome"] == outcome)
    ]
    chosen.sort(key=lambda row: row["width"])
    return chosen


def save(figure, name: str) -> None:
    path = HERE / name
    figure.savefig(path, format="svg", bbox_inches="tight", metadata={"Date": None})
    plt.close(figure)
    print(f"wrote {path.relative_to(HERE.parent)}")


def figure_ladder(rows) -> None:
    """MPS wall time against width, log-log, with a linear reference."""
    points = select(rows, "ladder", "mps", label="headline-1-shot")
    widths = [row["width"] for row in points]
    times = [row["total_seconds"] for row in points]

    figure, axes = plt.subplots(figsize=(6.0, 4.0))
    reference = [SECONDS_PER_QUBIT * width for width in widths]
    axes.plot(
        widths,
        reference,
        "--",
        color=REFERENCE_COLOR,
        linewidth=1.2,
        label=f"linear reference: {SECONDS_PER_QUBIT} s x width",
    )
    axes.plot(
        widths,
        times,
        "o-",
        color=MPS_COLOR,
        linewidth=1.6,
        markersize=5,
        label="measured, 1 shot",
    )

    for width, seconds in zip(widths, times):
        axes.annotate(
            f"{seconds:.1f} s",
            (width, seconds),
            textcoords="offset points",
            xytext=(6, -10),
            fontsize=8,
        )

    axes.set_xscale("log", base=2)
    axes.set_yscale("log", base=10)
    axes.set_xlabel("qubits (width)")
    axes.set_ylabel("wall time (s)")
    axes.set_title("MPS scaling, depth 8 — linear in width")
    axes.set_xticks(widths)
    axes.set_xticklabels([str(width) for width in widths])
    axes.legend(loc="upper left", frameon=False)
    save(figure, "ladder-scaling.svg")


def figure_crossover(rows) -> None:
    """CPU against MPS across the crossover band, including CPU timeouts."""
    cpu_ok = select(rows, "crossover", "cpu")
    mps_ok = select(rows, "crossover", "mps")
    cpu_timeout = select(rows, "crossover", "cpu", outcome="timeout")

    figure, axes = plt.subplots(figsize=(6.0, 4.0))
    axes.plot(
        [row["width"] for row in cpu_ok],
        [row["total_seconds"] for row in cpu_ok],
        "o-",
        color=CPU_COLOR,
        linewidth=1.6,
        markersize=5,
        label='dense statevector (type="cpu")',
    )
    axes.plot(
        [row["width"] for row in mps_ok],
        [row["total_seconds"] for row in mps_ok],
        "o-",
        color=MPS_COLOR,
        linewidth=1.6,
        markersize=5,
        label='cuTensorNet MPS (type="mps")',
    )
    axes.plot(
        [row["width"] for row in cpu_timeout],
        [row["total_seconds"] for row in cpu_timeout],
        "x",
        color=CPU_COLOR,
        markersize=9,
        markeredgewidth=2,
        label="CPU timeout (budget exhausted)",
    )

    for row in cpu_timeout:
        axes.annotate(
            "timeout",
            (row["width"], row["total_seconds"]),
            textcoords="offset points",
            xytext=(-14, 10),
            fontsize=8,
            color=CPU_COLOR,
        )

    axes.set_yscale("log", base=10)
    axes.set_xlabel("qubits (width)")
    axes.set_ylabel("wall time for 10 shots (s)")
    axes.set_title("Crossover, depth 8 — CPU and MPS, 10 shots")
    axes.set_xticks([row["width"] for row in cpu_ok + cpu_timeout])
    axes.legend(loc="center left", frameon=False)

    note = "MPS not measured at 26 and 28:\nthe CPU run consumed the budget"
    axes.annotate(
        note,
        xy=(0.97, 0.06),
        xycoords="axes fraction",
        ha="right",
        fontsize=7.5,
        color="#555555",
    )
    save(figure, "crossover.svg")


def figure_memory(rows) -> None:
    """Measured memory against the dense statevector requirement.

    The dense curve is drawn over the widths where it is still physically
    meaningful and is allowed to leave the top of the axes, because clipping it
    would hide the exponential that is the whole point of the comparison.

    Both measured series are linear in width. Host RSS is plotted alongside the
    GPU peak because above width ~2048 the host, not the device, is the larger
    consumer -- a production-relevant fact that a GPU-only chart would hide.
    """
    points = select(rows, "ladder", "mps", label="headline-1-shot")
    widths = [row["width"] for row in points]
    gpu_mib = [row["peak_gpu_mib"] for row in points]
    host_mib = [row["peak_host_bytes"] / (1024 * 1024) for row in points]

    figure, axes = plt.subplots(figsize=(6.0, 4.0))

    dense_widths = list(range(16, 45))
    dense_mib = [
        (BYTES_PER_AMPLITUDE / (1024 * 1024)) * math.pow(2.0, width)
        for width in dense_widths
    ]
    axes.plot(
        dense_widths,
        dense_mib,
        "-",
        color=CPU_COLOR,
        linewidth=1.8,
        label="dense statevector requirement",
    )
    axes.plot(
        widths,
        host_mib,
        "^--",
        color=REFERENCE_COLOR,
        linewidth=1.5,
        markersize=4,
        label="measured peak host RSS",
    )
    axes.plot(
        widths,
        gpu_mib,
        "o-",
        color=MPS_COLOR,
        linewidth=1.8,
        markersize=5,
        label="measured MPS peak GPU memory",
    )
    axes.axhline(
        A100_CAPACITY_MIB,
        color=REFERENCE_COLOR,
        linewidth=1.2,
        linestyle=":",
        label="A100 80GB capacity",
    )

    axes.annotate(
        "dense exceeds\nan A100 above\nwidth 32",
        xy=(33, A100_CAPACITY_MIB),
        xytext=(48, 3.0e6),
        fontsize=7.5,
        color=CPU_COLOR,
        arrowprops={"arrowstyle": "->", "color": CPU_COLOR, "linewidth": 0.9},
    )
    axes.annotate(
        "8192 qubits in\n4942 MiB on device",
        xy=(8192, 4942),
        xytext=(600, 40),
        fontsize=7.5,
        color=MPS_COLOR,
        arrowprops={"arrowstyle": "->", "color": MPS_COLOR, "linewidth": 0.9},
    )

    axes.set_xscale("log", base=2)
    axes.set_yscale("log", base=10)
    axes.set_ylim(10, 1.0e8)
    axes.set_xlim(14, 12000)
    axes.set_xlabel("qubits (width)")
    axes.set_ylabel("memory (MiB)")
    axes.set_title("Memory: MPS is linear in width, dense is exponential")
    axes.set_xticks([16, 32, 64, 128, 512, 2048, 8192])
    axes.set_xticklabels(["16", "32", "64", "128", "512", "2048", "8192"])
    axes.legend(loc="upper right", frameon=False, fontsize=8)
    save(figure, "memory.svg")


def figure_correctness() -> None:
    """Per-qubit One-frequencies, CPU against MPS, width 16."""
    qubits, cpu, mps = load_frequencies()
    deviations = [abs(a - b) for a, b in zip(cpu, mps)]

    figure, (upper, lower) = plt.subplots(
        2, 1, figsize=(6.0, 4.6), sharex=True, height_ratios=[3, 1]
    )
    upper.plot(qubits, cpu, "o-", color=CPU_COLOR, markersize=4, label="cpu (exact)")
    upper.plot(qubits, mps, "s--", color=MPS_COLOR, markersize=4, label="mps")
    upper.set_ylabel("P(measure One)")
    upper.set_title("Correctness at width 16, 1000 shots, seed 42")
    upper.legend(loc="upper left", frameon=False)

    lower.bar(qubits, deviations, color="#7f8c8d", width=0.6)
    lower.axhline(
        0.0224,
        color=REFERENCE_COLOR,
        linestyle=":",
        linewidth=1.0,
        label="1 sigma sampling noise (1000 shots)",
    )
    lower.set_xlabel("qubit index")
    lower.set_ylabel("|deviation|")
    lower.set_xticks(qubits)
    lower.legend(loc="upper right", frameon=False, fontsize=7.5)

    save(figure, "correctness-w16.svg")


def main() -> int:
    rows = load_measurements()
    figure_ladder(rows)
    figure_crossover(rows)
    figure_memory(rows)
    figure_correctness()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
