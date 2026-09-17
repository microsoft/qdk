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
qdk.simulation.run_qir(..., type="cpu"/"gpu"/"mps").

Requires `pip install qdk-chemistry qdk` (see Ising2D.md for the exact
recipe this mirrors).
"""

from __future__ import annotations

import argparse
import math
import re
from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

if TYPE_CHECKING:
    from qdk_chemistry.data import LatticeGraph


Gate = tuple[Literal["rx"], float, int] | tuple[Literal["rzz"], float, int, int]


@dataclass(frozen=True)
class ParsedCircuit:
    num_qubits: int
    gates: list[Gate]

    def __post_init__(self) -> None:
        if self.num_qubits < 1:
            raise ValueError("The circuit must allocate at least one qubit")
        for gate in self.gates:
            if not gate or gate[0] not in ("rx", "rzz"):
                raise ValueError(f"Unsupported gate: {gate!r}")
            if len(gate) != (3 if gate[0] == "rx" else 4):
                raise ValueError(f"Incorrect gate arity: {gate!r}")
            if not math.isfinite(gate[1]):
                raise ValueError(f"Non-finite gate angle: {gate!r}")
            if any(not 0 <= qubit < self.num_qubits for qubit in gate[2:]):
                raise ValueError(f"Qubit outside the allocated register: {gate!r}")
            if gate[0] == "rzz" and gate[2] == gate[3]:
                raise ValueError(f"Rzz requires distinct qubits: {gate!r}")


def ising_lattice(nx: int, ny: int) -> LatticeGraph:
    from qdk_chemistry.data import LatticeGraph

    return LatticeGraph.square(nx, ny)


def build_ising_2d_qir(nx: int, ny: int, total_time: float, order: int, num_divisions: int) -> str:
    """Reproduce the estimation_ising_2d.ipynb recipe and return its QIR (unmeasured)."""
    from qdk_chemistry.algorithms import create
    from qdk_chemistry.algorithms.state_preparation import identity_state_prep
    from qdk_chemistry.data import AlgorithmRef, DrivenQubitHamiltonian
    from qdk_chemistry.utils import Logger
    from qdk_chemistry.utils.model_hamiltonians import create_ising_hamiltonian

    Logger.set_global_level(Logger.LogLevel.off)

    lattice = ising_lattice(nx, ny)
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

    Every QIS call must match in full. This deliberately rejects unsupported
    syntax rather than silently dropping a gate; it is not a general QIR parser.
    """
    gates: list[Gate] = []
    for line in qir_text.splitlines():
        instruction = line.partition(";")[0].strip()
        if "@__quantum__qis__" not in instruction or instruction.startswith("declare "):
            continue
        match = _CALL_PATTERN.fullmatch(instruction)
        if match is None:
            raise ValueError(f"Unsupported or malformed QIS call: {instruction}")
        kind, angle, q1, q2 = match.groups()
        if kind == "rx" and q2 is None:
            gates.append(("rx", float(angle), int(q1)))
        elif kind == "rzz" and q2 is not None:
            gates.append(("rzz", float(angle), int(q1), int(q2)))
        else:
            raise ValueError(f"Incorrect gate arity: {instruction}")

    widths = _NUM_QUBITS_PATTERN.findall(qir_text)
    if len(widths) != 1:
        raise ValueError("Expected exactly one required_num_qubits attribute in QIR")
    return ParsedCircuit(num_qubits=int(widths[0]), gates=gates)


def qsharp_source(
    parsed: ParsedCircuit,
    operation_name: str = "Ising2DTrotter",
    *,
    dump_state: bool = False,
) -> str:
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
    if dump_state:
        lines.append("    Std.Diagnostics.DumpMachine();")
    lines.append("    return Std.Measurement.MResetEachZ(qs);")
    lines.append("}")
    return "\n".join(lines)


def compile_measured_qir(source: str) -> str:
    """Compile the sample's Ising2DTrotter entry point with Base profile."""
    from qdk.qsharp import TargetProfile, compile as qsharp_compile, eval as qsharp_eval, init as qsharp_init

    qsharp_init(target_profile=TargetProfile.Base)
    qsharp_eval(source)
    program = qsharp_compile("Ising2DTrotter()")
    return str(program)


def build_measured_qir(nx: int, ny: int, total_time: float, order: int, num_divisions: int) -> str:
    """Full pipeline: chemistry recipe -> parsed gates -> measured Q# -> Base QIR."""
    unmeasured_qir = build_ising_2d_qir(nx, ny, total_time, order, num_divisions)
    return compile_measured_qir(qsharp_source(parse_gates(unmeasured_qir)))


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
