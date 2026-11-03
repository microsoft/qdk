# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Construction-throughput reference points for a Python quantum circuit API.

This harness measures systems that already exist so that a construction target
can be derived from observed behavior rather than asserted in advance. It does
not measure a QDK dense circuit store, because none exists yet.

Reference points are grouped by category, and only ``construction`` points are
comparable to each other. Mixing categories produces a meaningless ratio.

``construction`` — building an in-memory circuit object, nothing more:

- ``qiskit-fluent``: Qiskit's per-gate builder methods, the idiom a migrating
  user writes today.
- ``qiskit-append``: Qiskit's lower-level append with a cached gate instance,
  which approximates its best case.
- ``qdk-circuit``: the new ``qdk.circuit.QuantumCircuit`` fluent surface.

``ingest-and-compile`` — everything the existing QDK entry points do, which is
strictly more than construction:

- ``qdk-openqasm-import``: parses OpenQASM, runs semantic validation, converts
  to the Q# AST, and compiles the result into the running interpreter context.
- ``qdk-qiskit-interop``: the same, preceded by serializing a Qiskit circuit to
  OpenQASM 3 text. This is what moving a Qiskit circuit into QDK costs today.

The ``ingest-and-compile`` numbers are dated rather than durable: no
parse-only entry point is exposed to Python, and a separate change to add
high-speed OpenQASM parsing is expected to move the front-end component. Treat
them as a snapshot of today's behavior, not a baseline to regress against.

Workloads:

- ``single_qubit``: one-qubit gates spread across the register.
- ``repeated_two_qubit``: a two-qubit gate applied repeatedly to the same
  operand pair, which is the pattern operand interning is meant to collapse.

Run ``python bench_circuit_construction.py --help`` for options.
"""

from __future__ import annotations

import argparse
import json
import math
import platform
import statistics
import sys
import time
from dataclasses import dataclass, field
from typing import Callable, Optional

DEFAULT_GATES = 200_000
DEFAULT_SAMPLES = 7
DEFAULT_QUBITS = 32

# Text ingestion is orders of magnitude slower, so it gets its own gate count.
DEFAULT_TEXT_GATES = 20_000


def nearest_rank_percentile(values: list[float], percentile: float) -> float:
    """Nearest-rank percentile, which stays meaningful at small sample counts."""
    if not values:
        raise ValueError("no values")
    ordered = sorted(values)
    rank = math.ceil(percentile / 100.0 * len(ordered))
    return ordered[max(1, rank) - 1]


CONSTRUCTION = "construction"
INGEST_AND_COMPILE = "ingest-and-compile"


@dataclass
class Measurement:
    reference_point: str
    workload: str
    gates: int
    category: str = CONSTRUCTION
    seconds: list[float] = field(default_factory=list)
    skipped_reason: Optional[str] = None
    # Only pre-existing systems set the bar. Including the system under
    # measurement would make the target self-referential.
    is_baseline: bool = True

    @property
    def samples(self) -> int:
        return len(self.seconds)

    @property
    def median_seconds(self) -> float:
        return statistics.median(self.seconds)

    @property
    def p95_seconds(self) -> float:
        return nearest_rank_percentile(self.seconds, 95)

    @property
    def median_gates_per_second(self) -> float:
        return self.gates / self.median_seconds

    @property
    def p95_gates_per_second(self) -> float:
        return self.gates / self.p95_seconds

    @property
    def best_gates_per_second(self) -> float:
        """Throughput at the fastest observed time.

        Benchmark noise is one-directional: interference can only make a run
        slower, never faster. The best sample is therefore the most stable
        estimator of what the system can do, and the median moves around with
        unrelated machine load.
        """
        return self.gates / min(self.seconds)

    def as_dict(self) -> dict:
        if self.skipped_reason is not None:
            return {
                "reference_point": self.reference_point,
                "workload": self.workload,
                "category": self.category,
                "status": "skipped",
                "reason": self.skipped_reason,
            }
        return {
            "reference_point": self.reference_point,
            "workload": self.workload,
            "category": self.category,
            "status": "measured",
            "gates": self.gates,
            "samples": self.samples,
            "median_seconds": self.median_seconds,
            "p95_seconds": self.p95_seconds,
            "best_seconds": min(self.seconds),
            "median_gates_per_second": self.median_gates_per_second,
            "p95_gates_per_second": self.p95_gates_per_second,
            "best_gates_per_second": self.best_gates_per_second,
        }


def measure(
    reference_point: str,
    workload: str,
    gates: int,
    samples: int,
    build: Callable[[], object],
    category: str = CONSTRUCTION,
    is_baseline: bool = True,
) -> Measurement:
    result = Measurement(
        reference_point, workload, gates, category, is_baseline=is_baseline
    )
    build()  # Warm up import-time and first-call costs out of the samples.
    for _ in range(samples):
        start = time.perf_counter()
        build()
        result.seconds.append(time.perf_counter() - start)
    return result


def qiskit_measurements(gates: int, samples: int, qubits: int) -> list[Measurement]:
    try:
        from qiskit.circuit import QuantumCircuit
        from qiskit.circuit.library import CXGate, HGate
    except ImportError as exc:
        reason = f"qiskit is not installed ({exc.name}); install the qiskit extra"
        return [
            Measurement("qiskit-fluent", "single_qubit", gates, skipped_reason=reason),
            Measurement(
                "qiskit-fluent", "repeated_two_qubit", gates, skipped_reason=reason
            ),
            Measurement("qiskit-append", "single_qubit", gates, skipped_reason=reason),
            Measurement(
                "qiskit-append", "repeated_two_qubit", gates, skipped_reason=reason
            ),
        ]

    def fluent_single() -> object:
        qc = QuantumCircuit(qubits)
        for i in range(gates):
            qc.h(i % qubits)
        return qc

    def fluent_repeated_pair() -> object:
        qc = QuantumCircuit(qubits)
        for _ in range(gates):
            qc.cx(0, 1)
        return qc

    def append_single() -> object:
        qc = QuantumCircuit(qubits)
        gate = HGate()
        for i in range(gates):
            qc.append(gate, (i % qubits,), ())
        return qc

    def append_repeated_pair() -> object:
        qc = QuantumCircuit(qubits)
        gate = CXGate()
        operands = (0, 1)
        for _ in range(gates):
            qc.append(gate, operands, ())
        return qc

    return [
        measure("qiskit-fluent", "single_qubit", gates, samples, fluent_single),
        measure(
            "qiskit-fluent",
            "repeated_two_qubit",
            gates,
            samples,
            fluent_repeated_pair,
        ),
        measure("qiskit-append", "single_qubit", gates, samples, append_single),
        measure(
            "qiskit-append",
            "repeated_two_qubit",
            gates,
            samples,
            append_repeated_pair,
        ),
    ]


def openqasm_text_source(workload: str, gates: int, qubits: int) -> str:
    header = ["OPENQASM 3.0;", 'include "stdgates.inc";', f"qubit[{qubits}] q;"]
    if workload == "single_qubit":
        body = [f"h q[{i % qubits}];" for i in range(gates)]
    else:
        body = ["cx q[0], q[1];"] * gates
    return "\n".join(header + body)


def qdk_import_measurements(gates: int, samples: int, qubits: int) -> list[Measurement]:
    """Measure the existing OpenQASM entry point.

    This is not a construction measurement. ``import_openqasm`` parses, runs
    semantic validation, converts to the Q# AST, and compiles into the running
    interpreter context.
    """
    try:
        from qdk.openqasm import import_openqasm
        from qdk._native import ProgramType  # type: ignore
    except ImportError as exc:
        reason = f"qdk is not importable ({exc.name}); build the qdk package"
        return [
            Measurement(
                "qdk-openqasm-import",
                workload,
                gates,
                INGEST_AND_COMPILE,
                skipped_reason=reason,
            )
            for workload in ("single_qubit", "repeated_two_qubit")
        ]

    results = []
    for index, workload in enumerate(("single_qubit", "repeated_two_qubit")):
        source = openqasm_text_source(workload, gates, qubits)
        counter = [index * 1_000_000]

        def build() -> object:
            # Each import needs a distinct entry point name in the interpreter.
            counter[0] += 1
            return import_openqasm(
                source,
                name=f"bench_{counter[0]}",
                program_type=ProgramType.File,
            )

        results.append(
            measure(
                "qdk-openqasm-import",
                workload,
                gates,
                samples,
                build,
                INGEST_AND_COMPILE,
            )
        )
    return results


def qdk_circuit_measurements(
    gates: int, samples: int, qubits: int
) -> list[Measurement]:
    workloads = ("single_qubit", "repeated_two_qubit")
    try:
        import warnings

        warnings.filterwarnings("ignore", message="qdk.circuit is experimental")
        # Stubs for the native classes arrive with the typing task.
        from qdk.circuit import QuantumCircuit  # type: ignore[attr-defined]
    except ImportError as exc:
        reason = f"qdk.circuit is unavailable ({exc.name}); build the qdk package"
        return [
            Measurement("qdk-circuit", w, gates, skipped_reason=reason)
            for w in workloads
        ]

    def fluent_single() -> object:
        circuit = QuantumCircuit(qubits)
        circuit.reserve(gates)
        for i in range(gates):
            circuit.h(i % qubits)
        return circuit

    def fluent_repeated_pair() -> object:
        circuit = QuantumCircuit(qubits)
        circuit.reserve(gates)
        for _ in range(gates):
            circuit.cx(0, 1)
        return circuit

    return [
        measure(
            "qdk-circuit",
            "single_qubit",
            gates,
            samples,
            fluent_single,
            is_baseline=False,
        ),
        measure(
            "qdk-circuit",
            "repeated_two_qubit",
            gates,
            samples,
            fluent_repeated_pair,
            is_baseline=False,
        ),
    ]


def qdk_interop_measurements(
    gates: int, samples: int, qubits: int
) -> list[Measurement]:
    """Measure the full Qiskit-to-QDK path, including text serialization.

    Measuring only the QDK side would understate what interop actually costs,
    because the caller must first serialize the circuit to text.
    """
    workloads = ("single_qubit", "repeated_two_qubit")
    try:
        from qiskit.circuit import QuantumCircuit
        from qiskit.qasm3 import dumps

        from qdk.openqasm import import_openqasm
        from qdk._native import ProgramType  # type: ignore
    except ImportError as exc:
        reason = f"qiskit or qdk is unavailable ({exc.name})"
        return [
            Measurement(
                "qdk-qiskit-interop",
                w,
                gates,
                INGEST_AND_COMPILE,
                skipped_reason=reason,
            )
            for w in workloads
        ]

    def make_build(source: object, start: int) -> Callable[[], object]:
        counter = [start]

        def build() -> object:
            counter[0] += 1
            return import_openqasm(
                dumps(source),
                name=f"interop_{counter[0]}",
                program_type=ProgramType.File,
            )

        return build

    results = []
    for index, workload in enumerate(workloads):
        source = QuantumCircuit(qubits)
        if workload == "single_qubit":
            for i in range(gates):
                source.h(i % qubits)
        else:
            for _ in range(gates):
                source.cx(0, 1)

        results.append(
            measure(
                "qdk-qiskit-interop",
                workload,
                gates,
                samples,
                make_build(source, index * 1_000_000),
                INGEST_AND_COMPILE,
            )
        )
    return results


def qhdg_measurements(gates: int) -> list[Measurement]:
    reason = (
        "the QHDG crates are not in the working tree; this reference point "
        "becomes available after the incremental port"
    )
    return [
        Measurement("qhdg-builder", "single_qubit", gates, skipped_reason=reason),
        Measurement("qhdg-builder", "repeated_two_qubit", gates, skipped_reason=reason),
    ]


def environment() -> dict:
    return {
        "python": sys.version.split()[0],
        "implementation": platform.python_implementation(),
        "platform": platform.platform(),
        "machine": platform.machine(),
        "processor": platform.processor() or "unknown",
    }


def derive_target(measurements: list[Measurement]) -> Optional[dict]:
    """Derive a construction target from the fastest pre-existing system.

    Only ``construction`` points marked as baselines are eligible. The
    ingest-and-compile points do strictly more work, and the system under
    measurement is excluded because a target it sets for itself says nothing.
    """
    measured = [
        m
        for m in measurements
        if m.skipped_reason is None and m.category == CONSTRUCTION and m.is_baseline
    ]
    if not measured:
        return None
    fastest = max(measured, key=lambda m: m.best_gates_per_second)
    return {
        "basis_reference_point": fastest.reference_point,
        "basis_workload": fastest.workload,
        "basis_best_gates_per_second": fastest.best_gates_per_second,
        "target_gates_per_second": fastest.best_gates_per_second,
        "rationale": (
            "The target is the best observed throughput across pre-existing "
            "construction systems. Ingest-and-compile paths are excluded "
            "because they do strictly more work, and the system under "
            "measurement is excluded because a target it sets for itself is "
            "self-referential. Best rather than median is used because "
            "benchmark noise only ever slows a run down, so the median drifts "
            "with unrelated machine load while the best sample stays stable."
        ),
    }


def report(measurements: list[Measurement], target: Optional[dict]) -> None:
    print()
    print("Construction throughput reference points")
    print()
    header = f"{'reference point':<22} {'category':<20} {'workload':<20} {'gates':>9} {'samples':>8} {'best g/s':>14} {'median g/s':>14} {'p95 g/s':>14}"
    print(header)
    print("-" * len(header))
    for m in measurements:
        if m.skipped_reason is not None:
            print(
                f"{m.reference_point:<22} {m.category:<20} {m.workload:<20} {'skipped':>9}   {m.skipped_reason}"
            )
            continue
        print(
            f"{m.reference_point:<22} {m.category:<20} {m.workload:<20} {m.gates:>9} {m.samples:>8} "
            f"{m.best_gates_per_second:>14,.0f} {m.median_gates_per_second:>14,.0f} {m.p95_gates_per_second:>14,.0f}"
        )
    print()
    print(
        "Categories are not comparable. An ingest-and-compile point parses, \n"
        "validates, lowers to the Q# AST, and compiles into the interpreter \n"
        "context, which is strictly more than building a circuit object. Those \n"
        "points are also dated: high-speed OpenQASM parsing is expected to \n"
        "change the front-end component."
    )
    print()
    if target is None:
        print("No target derived: no construction reference point could be measured.")
        return
    print(
        f"Derived target: {target['target_gates_per_second']:,.0f} gates/second, "
        f"from {target['basis_reference_point']} on {target['basis_workload']}."
    )
    print(f"Rationale: {target['rationale']}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gates", type=int, default=DEFAULT_GATES)
    parser.add_argument("--text-gates", type=int, default=DEFAULT_TEXT_GATES)
    parser.add_argument("--samples", type=int, default=DEFAULT_SAMPLES)
    parser.add_argument("--qubits", type=int, default=DEFAULT_QUBITS)
    parser.add_argument(
        "--json", action="store_true", help="emit JSON instead of a table"
    )
    parser.add_argument(
        "--skip-text",
        action="store_true",
        help="skip the ingest-and-compile paths, which are slow",
    )
    args = parser.parse_args()

    measurements = qiskit_measurements(args.gates, args.samples, args.qubits)
    measurements += qdk_circuit_measurements(args.gates, args.samples, args.qubits)
    if args.skip_text:
        measurements += [
            Measurement(
                reference_point,
                workload,
                args.text_gates,
                INGEST_AND_COMPILE,
                skipped_reason="skipped by --skip-text",
            )
            for reference_point in ("qdk-openqasm-import", "qdk-qiskit-interop")
            for workload in ("single_qubit", "repeated_two_qubit")
        ]
    else:
        measurements += qdk_import_measurements(
            args.text_gates, args.samples, args.qubits
        )
        measurements += qdk_interop_measurements(
            args.text_gates, args.samples, args.qubits
        )
    measurements += qhdg_measurements(args.gates)

    target = derive_target(measurements)

    if args.json:
        print(
            json.dumps(
                {
                    "environment": environment(),
                    "measurements": [m.as_dict() for m in measurements],
                    "derived_target": target,
                },
                indent=2,
            )
        )
    else:
        env = environment()
        print(f"Host: {env['platform']} / {env['machine']}")
        print(f"Python: {env['python']} ({env['implementation']})")
        report(measurements, target)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
