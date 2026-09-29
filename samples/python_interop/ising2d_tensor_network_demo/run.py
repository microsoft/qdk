# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Measure and render the 2D Ising MPS χ sweep.

    python run.py measure --size 4 --field 3.03 --chi 2 4 8 16 32 64 128 --output results.json
    python run.py render results.json

``measure`` needs the preview ``qdk`` build and an NVIDIA GPU. It evaluates
m_z and C_ZZ on the frozen circuits in fixtures/ exactly (method="contraction")
and with MPS per χ (method="mps", plus Cost), and writes them to the
"ising2d" section of the results file, keeping any other section. ``render``
needs neither: it prints the tables and, if matplotlib is installed, writes
the plots next to the results file.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
from importlib import metadata
import json
import os
from pathlib import Path
import platform
import sys
import time
from typing import Any, Callable, Iterator, Optional, Sequence

from ising import (
    DIRECTORY,
    FIELDS,
    J,
    SIZES,
    circuit_path,
    correlation_terms,
    cpu_reference,
    field_label,
    magnetization_terms,
)


SECTION = "ising2d"
SCHEMA = 1
THRESHOLD = 1e-3
# The I1 limit for later tensor-network comparisons with the CPU reference.
CPU_CHECK_LIMIT = 1e-8
DEFAULT_CHIS = (2, 4, 8, 16, 32, 64, 128)


def measure(
    cases: Sequence[tuple[int, float]],
    chis: Sequence[int],
    *,
    exact: bool = True,
    tensornetwork_qir: Optional[Callable[..., list]] = None,
    clock: Callable[[], float] = time.perf_counter,
    progress: Callable[[str], None] = lambda message: None,
) -> Iterator[dict]:
    """Yield one run record per (size, field): exact reference, then MPS per χ.

    Out-of-resource failures (OSError, MemoryError) are recorded in the run,
    so a reference that does not fit leaves the rest of the sweep intact.
    Program errors (ValueError) propagate.
    """
    from qdk.simulation import Cost, Expectation, MpsOptions

    if tensornetwork_qir is None:
        from qdk.simulation import tensornetwork_qir
    environment = {
        "qdk": metadata.version("qdk"),
        "python": platform.python_version(),
        "machine": platform.machine(),
    }
    for size, field in cases:
        path = circuit_path(size, field)
        qir = path.read_text()
        observables = [Expectation(magnetization_terms(size)), Expectation(correlation_terms(size))]
        cpu = cpu_reference(size, field)
        run: dict[str, Any] = {
            "size": size,
            "field": field,
            "J": J,
            "qubits": size * size,
            "circuit": path.relative_to(DIRECTORY).as_posix(),
            "cpu_reference": None if cpu is None else {name: cpu[name] for name in ("m_z", "c_zz")},
            "exact": None,
            "mps": [],
            "environment": {**environment, "measured_at_utc": datetime.now(timezone.utc).isoformat()},
        }
        if exact:
            run["exact"] = _timed(lambda: tensornetwork_qir(qir, observables, method="contraction"), clock)
            progress(f"size={size} h={field_label(field)} exact: {_summary(run['exact'])}")
        for chi in chis:
            options = MpsOptions(max_bond_dimension=chi)
            entry = _timed(
                lambda: tensornetwork_qir(qir, [*observables, Cost()], method="mps", options=options),
                clock,
            )
            run["mps"].append({"chi": chi, **entry})
            progress(f"size={size} h={field_label(field)} χ={chi}: {_summary(entry)}")
        yield run


def _timed(call: Callable[[], list], clock: Callable[[], float]) -> dict:
    started = clock()
    try:
        results = call()
    except (OSError, MemoryError) as error:
        return {"error": f"{type(error).__name__}: {error}", "seconds": clock() - started}
    seconds = clock() - started
    m_z, c_zz = complex(results[0]), complex(results[1])
    entry: dict[str, Any] = {
        "m_z": m_z.real,
        "c_zz": c_zz.real,
        "max_imag": max(abs(m_z.imag), abs(c_zz.imag)),
        "seconds": seconds,
    }
    if len(results) > 2:
        entry["cost"] = dict(results[2])
    return entry


def _summary(entry: dict) -> str:
    if "error" in entry:
        return entry["error"]
    return f"m_z={entry['m_z']:.7f} C_ZZ={entry['c_zz']:.7f} ({entry['seconds']:.2f} s)"


def load_results(path: Path) -> dict:
    if not path.exists():
        return {}
    document = json.loads(path.read_text())
    if not isinstance(document, dict):
        raise ValueError(f"{path} is not a results file: expected a JSON object")
    section = document.get(SECTION)
    if section is not None and section.get("schema") != SCHEMA:
        raise ValueError(f"{path} has an unsupported '{SECTION}' schema: {section.get('schema')!r}")
    return document


def merge(document: dict, run: dict) -> dict:
    """Replace the run with the same (size, field) in this section; keep everything else."""
    section = document.setdefault(SECTION, {"schema": SCHEMA, "runs": []})
    runs = [old for old in section["runs"] if (old["size"], old["field"]) != (run["size"], run["field"])]
    section["runs"] = sorted([*runs, run], key=lambda r: (r["size"], r["field"]))
    return document


def save_results(path: Path, document: dict) -> None:
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(json.dumps(document, indent=2, sort_keys=True, allow_nan=False) + "\n")
    os.replace(temporary, path)


def reference(run: dict) -> tuple[str, Optional[dict]]:
    """The exact result if it fitted, else the largest-χ MPS result, never called exact."""
    exact = run.get("exact")
    if exact is not None and "error" not in exact:
        return "exact", exact
    measured = [entry for entry in run["mps"] if "error" not in entry]
    if not measured:
        return "none", None
    largest = max(measured, key=lambda entry: entry["chi"])
    return f"MPS χ={largest['chi']}", largest


def errors(run: dict) -> list[dict]:
    """Per-χ errors against the run's reference; the reference χ itself is marked."""
    _, ref = reference(run)
    rows = []
    for entry in sorted(run["mps"], key=lambda entry: entry["chi"]):
        row = dict(entry)
        if "error" not in entry and ref is not None:
            row["is_reference"] = entry is ref
            row["eps_m"] = abs(entry["m_z"] - ref["m_z"])
            row["eps_c"] = abs(entry["c_zz"] - ref["c_zz"])
        rows.append(row)
    return rows


def _compared(run: dict) -> list[dict]:
    return [row for row in errors(run) if "eps_m" in row and not row["is_reference"]]


def chi_needed(run: dict, threshold: float = THRESHOLD) -> Optional[int]:
    """Smallest χ with ε_m and ε_C ≤ threshold, or None if no compared χ reaches it."""
    for row in _compared(run):
        if max(row["eps_m"], row["eps_c"]) <= threshold:
            return row["chi"]
    return None


def _bytes(value: Optional[int]) -> str:
    if value is None:
        return "—"
    for unit, scale in (("GiB", 1 << 30), ("MiB", 1 << 20), ("KiB", 1 << 10)):
        if value >= scale:
            return f"{value / scale:.1f} {unit}"
    return f"{value} B"


def _chi_needed_cell(run: Optional[dict]) -> str:
    if run is None or not _compared(run):
        return "—"
    needed = chi_needed(run)
    return str(needed) if needed is not None else f"> {max(row['chi'] for row in _compared(run))}"


def render_run(run: dict) -> str:
    label, ref = reference(run)
    timing = f" ({ref['seconds']:.2f} s)" if label == "exact" and ref is not None else ""
    lines = [f"ising2d | size={run['size']} J={run['J']:g} h={field_label(run['field'])} | reference: {label}{timing}"]
    exact = run.get("exact")
    if exact is not None and "error" in exact:
        lines.append(f"exact reference did not fit: {exact['error']}")
    cpu = run.get("cpu_reference")
    if cpu is not None and label == "exact" and ref is not None:
        differences = abs(ref["m_z"] - cpu["m_z"]), abs(ref["c_zz"] - cpu["c_zz"])
        status = "ok" if max(differences) <= CPU_CHECK_LIMIT else "MISMATCH"
        lines.append(
            f"CPU check: |exact − CPU| m_z {differences[0]:.1e}, C_ZZ {differences[1]:.1e} "
            f"(limit {CPU_CHECK_LIMIT:g}): {status}"
        )
    lines.append(f"{'χ':<6}{'ε_m':<10}{'ε_C':<10}{'max bond':<10}{'time':<10}{'state':<12}workspace")
    for row in errors(run):
        if "error" in row:
            lines.append(f"{row['chi']:<6}error: {row['error']}")
            continue
        if row.get("is_reference"):
            eps_m = eps_c = "ref"
        elif "eps_m" in row:
            eps_m, eps_c = f"{row['eps_m']:.1e}", f"{row['eps_c']:.1e}"
        else:
            eps_m = eps_c = "—"
        cost = row.get("cost", {})
        bond = cost.get("max_bond_dimension")
        time_cell = f"{row['seconds']:.2f} s"
        lines.append(
            f"{row['chi']:<6}{eps_m:<10}{eps_c:<10}{'—' if bond is None else str(bond):<10}{time_cell:<10}"
            f"{_bytes(cost.get('state_bytes')):<12}{_bytes(cost.get('workspace_bytes'))}"
        )
    return "\n".join(lines)


def render_summary(runs: Sequence[dict]) -> str:
    by_key = {(run["size"], run["field"]): run for run in runs}
    columns = "".join(f"{'χ needed (h=' + field_label(field) + ')':<21}" for field in FIELDS)
    lines = [
        f"χ needed: smallest χ with ε_m and ε_C ≤ {THRESHOLD:g}",
        f"{'N':<5}{'qubits':<9}{'reference':<22}{columns}".rstrip(),
    ]
    for size in sorted({size for size, _ in by_key}):
        labels: list[str] = []
        for field in FIELDS:
            run = by_key.get((size, field))
            if run is not None and reference(run)[0] not in labels:
                labels.append(reference(run)[0])
        cells = "".join(f"{_chi_needed_cell(by_key.get((size, field))):<21}" for field in FIELDS)
        lines.append(f"{size:<5}{size * size:<9}{', '.join(labels):<22}{cells}".rstrip())
    return "\n".join(lines)


def render(document: dict) -> str:
    section = document.get(SECTION)
    if section is None or not section["runs"]:
        raise ValueError(f"No '{SECTION}' results to render")
    return "\n\n".join([*(render_run(run) for run in section["runs"]), render_summary(section["runs"])])


def plot(document: dict, stem: Path) -> list[Path]:
    """Write <stem>.ising2d-error.png and <stem>.ising2d-chi-needed.png."""
    import matplotlib

    matplotlib.use("Agg")
    from matplotlib import pyplot

    runs = document[SECTION]["runs"]
    figure, axes = pyplot.subplots(1, 3, figsize=(15, 4.5))
    for run in runs:
        label, _ = reference(run)
        name = f"N={run['size']} h={field_label(run['field'])}" + ("" if label == "exact" else f" (vs {label})")
        for axis, key in zip(axes[:2], ("eps_m", "eps_c")):
            # A log axis cannot show an exact zero error.
            points = [(row["chi"], row[key]) for row in _compared(run) if row[key] > 0]
            if points:
                axis.plot(*zip(*points), marker="o", label=name)
        timed = [(row["chi"], row["seconds"]) for row in errors(run) if "error" not in row]
        if timed:
            axes[2].plot(*zip(*timed), marker="o", label=name)
    titles = ("ε_m = |m_z(χ) − m_z(ref)|", "ε_C = |C_ZZ(χ) − C_ZZ(ref)|", "MPS wall time [s]")
    for axis, title in zip(axes, titles):
        axis.set_xscale("log", base=2)
        axis.set_yscale("log")
        axis.set_xlabel("χ")
        axis.set_title(title)
    for axis in axes[:2]:
        axis.axhline(THRESHOLD, color="grey", linestyle="--", linewidth=1)
    if axes[2].get_legend_handles_labels()[0]:
        axes[2].legend(fontsize="small")
    figure.tight_layout()
    error_path = stem.with_name(stem.name + ".ising2d-error.png")
    figure.savefig(error_path)
    pyplot.close(figure)

    figure, axis = pyplot.subplots(figsize=(6, 4.5))
    for field in FIELDS:
        points = sorted(
            (run["size"], needed) for run in runs
            if run["field"] == field and (needed := chi_needed(run)) is not None
        )
        if points:
            axis.plot(*zip(*points), marker="o", label=f"h={field_label(field)}")
    axis.set_yscale("log", base=2)
    axis.set_xticks(sorted({run["size"] for run in runs}))
    axis.set_xlabel("N (N×N lattice)")
    axis.set_ylabel(f"χ needed (ε ≤ {THRESHOLD:g})")
    if axis.get_legend_handles_labels()[0]:
        axis.legend()
    figure.tight_layout()
    chi_path = stem.with_name(stem.name + ".ising2d-chi-needed.png")
    figure.savefig(chi_path)
    pyplot.close(figure)
    return [error_path, chi_path]


def _chi(text: str) -> int:
    try:
        value = int(text)
    except ValueError:
        raise argparse.ArgumentTypeError(f"χ must be a positive integer: {text!r}") from None
    if value < 1:
        raise argparse.ArgumentTypeError(f"χ must be a positive integer: {text!r}")
    return value


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = result.add_subparsers(dest="command", required=True)
    measuring = commands.add_parser("measure", help="run the sweep (GPU)")
    measuring.add_argument("--size", type=int, nargs="+", required=True, help=f"lattice sizes N, from {list(SIZES)}")
    measuring.add_argument(
        "--field", type=float, nargs="+", required=True, help=f"fields h, from {[field_label(f) for f in FIELDS]}"
    )
    measuring.add_argument("--chi", type=_chi, nargs="+", default=list(DEFAULT_CHIS), help="bond-dimension caps χ")
    measuring.add_argument("--no-exact", action="store_true", help="skip the exact contraction reference")
    measuring.add_argument("--output", type=Path, required=True, help="results file (other sections are kept)")
    rendering = commands.add_parser("render", help="print tables and write plots (no GPU)")
    rendering.add_argument("results", type=Path)
    rendering.add_argument("--no-plots", action="store_true")
    return result


def main(argv: Optional[Sequence[str]] = None, *, tensornetwork_qir: Optional[Callable[..., list]] = None) -> int:
    arguments = parser()
    args = arguments.parse_args(argv)
    if args.command == "render":
        document = load_results(args.results)
        print(render(document))
        if not args.no_plots:
            try:
                paths = plot(document, args.results.with_suffix(""))
            except ImportError:
                print("\nmatplotlib is not installed; skipping plots", file=sys.stderr)
            else:
                print("\nplots: " + ", ".join(str(path) for path in paths))
        return 0
    for size in args.size:
        if size not in SIZES:
            arguments.error(f"no frozen circuit for --size {size}; choose from {list(SIZES)}")
    for field in args.field:
        if field not in FIELDS:
            arguments.error(f"no frozen circuit for --field {field:g}; choose from {[field_label(f) for f in FIELDS]}")
    # Fail before any GPU time is spent if the results file cannot be extended.
    document = load_results(args.output)
    cases = [(size, field) for size in dict.fromkeys(args.size) for field in dict.fromkeys(args.field)]
    runs = measure(
        cases,
        sorted(set(args.chi)),
        exact=not args.no_exact,
        tensornetwork_qir=tensornetwork_qir,
        progress=lambda message: print(message, file=sys.stderr, flush=True),
    )
    for run in runs:
        document = merge(document, run)
        save_results(args.output, document)
    print(f"wrote {len(cases)} run(s) to {args.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
