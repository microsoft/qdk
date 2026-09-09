#!/usr/bin/env python3
"""Build a *measured* QIR circuit for the 2D Ising Trotter case from
qdk-chemistry's estimation_ising_2d notebook recipe.

qdk_chemistry's Circuit.get_qir() for this recipe is built purely for
resource estimation (a static, symbolic gate-counting pass): the underlying
Q# primitive (QDKChemistry.Utils.PauliExp's RepPauliExp) returns Unit, so the
exported QIR has zero measurements and is tagged qir_profiles="adaptive_profile".
It cannot be run through qdk.simulation.run_qir as-is.

This script does not modify qdk_chemistry's QIR. It parses the ordered
sequence of __quantum__qis__rx__body / __quantum__qis__rzz__body calls out of
that QIR (the physical Trotter circuit qdk_chemistry already computed), writes
an equivalent Q# program that applies the same gates in the same order and
appends a real terminal measurement (Std.Measurement.MResetEachZ), and
compiles that with qdk.qsharp under TargetProfile.Base. The result is
base_profile QIR with required_num_results == num_qubits, runnable through
qdk.simulation.run_qir(..., type="cpu"/"gpu") or (once available) MPS.

Requires `pip install qdk-chemistry qdk` (see Ising2D.md for the exact
recipe this mirrors).
"""

from __future__ import annotations

import argparse
import re
from dataclasses import dataclass


@dataclass(frozen=True)
class ParsedCircuit:
    num_qubits: int
    # Each gate is ("rx", angle, qubit) or ("rzz", angle, q1, q2), in program order.
    gates: list[tuple]


def build_ising_2d_qir(nx: int, ny: int, total_time: float, order: int, num_divisions: int) -> str:
    """Reproduce the estimation_ising_2d.ipynb recipe and return its QIR (unmeasured)."""
    from qdk_chemistry.algorithms import create
    from qdk_chemistry.algorithms.state_preparation import identity_state_prep
    from qdk_chemistry.data import AlgorithmRef, DrivenQubitHamiltonian, LatticeGraph
    from qdk_chemistry.utils import Logger
    from qdk_chemistry.utils.model_hamiltonians import create_ising_hamiltonian

    Logger.set_global_level(Logger.LogLevel.off)

    lattice = LatticeGraph.square(nx, ny)
    h0 = create_ising_hamiltonian(lattice, j=1.0, h=0.0)  # ZZ interaction (with term grouping)
    h1 = create_ising_hamiltonian(lattice, j=0.0, h=0.5)  # Transverse X field
    td_hamiltonian = DrivenQubitHamiltonian(h0, h1, drive=lambda t: 1.0)

    evolution_builder = AlgorithmRef("hamiltonian_unitary_builder", "trotter", order=order, num_divisions=num_divisions)
    propagator = AlgorithmRef("propagator", "magnus", order=1)
    circuit_mapper = AlgorithmRef("circuit_mapper", "pauli_sequence")
    circuit_builder = create(
        "evolution_circuit_builder",
        "euler",
        evolution_builder=evolution_builder,
        propagator=propagator,
        circuit_mapper=circuit_mapper,
        total_time=total_time,
        dt=total_time,  # single Euler step = full time
    )

    state_prep = identity_state_prep(num_qubits=td_hamiltonian.num_qubits)
    circuit = circuit_builder.run(td_hamiltonian, state_prep)
    return str(circuit.get_qir())


_CALL_PATTERN = re.compile(
    r"call void @__quantum__qis__(rx|rzz)__body\("
    r"double ([^,]+), %Qubit\* inttoptr \(i64 (\d+) to %Qubit\*\)"
    r"(?:, %Qubit\* inttoptr \(i64 (\d+) to %Qubit\*\))?\)"
)
_NUM_QUBITS_PATTERN = re.compile(r'required_num_qubits"="(\d+)"')


def parse_gates(qir_text: str) -> ParsedCircuit:
    """Extract the ordered rx/rzz gate sequence from qdk_chemistry's (unmeasured) QIR.

    Raises ValueError if the QIR contains any intrinsic other than rx/rzz, since
    that would mean the gate set assumption behind this script no longer holds.
    """
    gates: list[tuple] = []
    for match in _CALL_PATTERN.finditer(qir_text):
        kind, angle, q1, q2 = match.groups()
        if kind == "rx":
            gates.append(("rx", float(angle), int(q1)))
        else:
            gates.append(("rzz", float(angle), int(q1), int(q2)))

    other_intrinsics = set(re.findall(r"call void @__quantum__qis__(\w+)__body\(", qir_text)) - {"rx", "rzz"}
    if other_intrinsics:
        raise ValueError(
            f"Unexpected intrinsics in qdk_chemistry QIR: {sorted(other_intrinsics)}; "
            "this script only knows how to replay rx/rzz."
        )

    num_qubits_match = _NUM_QUBITS_PATTERN.search(qir_text)
    if num_qubits_match is None:
        raise ValueError("Could not find required_num_qubits attribute in QIR")
    return ParsedCircuit(num_qubits=int(num_qubits_match.group(1)), gates=gates)


def qsharp_source(parsed: ParsedCircuit, operation_name: str = "Ising2DTrotter") -> str:
    """Emit a Q# operation that replays `parsed.gates` and ends in a real measurement."""
    lines = [
        f"operation {operation_name}() : Result[] {{",
        f"    use qs = Qubit[{parsed.num_qubits}];",
    ]
    for gate in parsed.gates:
        if gate[0] == "rx":
            _, angle, q = gate
            lines.append(f"    Rx({angle!r}, qs[{q}]);")
        else:
            _, angle, q1, q2 = gate
            lines.append(f"    Rzz({angle!r}, qs[{q1}], qs[{q2}]);")
    lines.append("    return Std.Measurement.MResetEachZ(qs);")
    lines.append("}")
    return "\n".join(lines)


def build_measured_qir(nx: int, ny: int, total_time: float, order: int, num_divisions: int) -> str:
    """Full pipeline: qdk_chemistry recipe -> parsed gates -> our own measured Q# -> base_profile QIR."""
    from qdk.qsharp import TargetProfile, compile as qsharp_compile, eval as qsharp_eval, init as qsharp_init

    unmeasured_qir = build_ising_2d_qir(nx, ny, total_time, order, num_divisions)
    parsed = parse_gates(unmeasured_qir)

    qsharp_init(target_profile=TargetProfile.Base)
    qsharp_eval(qsharp_source(parsed))
    program = qsharp_compile("Ising2DTrotter()")
    return str(program)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--nx", type=int, default=2, help="Lattice width")
    parser.add_argument("--ny", type=int, default=2, help="Lattice height")
    parser.add_argument("--time", type=float, default=1.0, help="Total simulation time")
    parser.add_argument("--order", type=int, default=4, help="Trotter-Suzuki product formula order")
    parser.add_argument("--num-divisions", type=int, default=2, help="Number of Trotter sub-divisions")
    parser.add_argument("--output", required=True, help="Path to write the resulting base_profile QIR")
    args = parser.parse_args()

    qir = build_measured_qir(args.nx, args.ny, args.time, args.order, args.num_divisions)
    with open(args.output, "w") as f:
        f.write(qir)

    num_qubits = int(_NUM_QUBITS_PATTERN.search(qir).group(1))
    num_results_match = re.search(r'required_num_results"="(\d+)"', qir)
    num_results = int(num_results_match.group(1)) if num_results_match else 0
    print(f"wrote {args.output}: {num_qubits} qubits, {num_results} measured results")


if __name__ == "__main__":
    main()
