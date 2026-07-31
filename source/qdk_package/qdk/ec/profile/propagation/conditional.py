"""Choi-prepared exact propagation with outcome-conditioned frames."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Sequence

from paulimer import OutcomeCompleteSimulation
from qodec.circuits import Program

from .frames import FrameGroup
from .pauli import Pauli
from .stabilizer import frame_group_of


@dataclass(frozen=True)
class ConditionalChoiResult:
    group: FrameGroup
    simulation: OutcomeCompleteSimulation
    projector_outcome_rows: tuple[int, ...]
    observe_outcome_rows: tuple[int, ...]
    aux_origin: int


def conditional_choi_state(
    program: Program,
    *,
    input_qubits: Sequence[int],
    codespace_projector: Sequence[Pauli] = (),
    aux_origin: int | None = None,
) -> ConditionalChoiResult:
    from ..check_discovery import simulate_program

    relevant_qubits: set[int] = set(range(program.qubit_count))
    relevant_qubits.update(input_qubits)
    for stabilizer in codespace_projector:
        relevant_qubits.update(stabilizer.support)
    if aux_origin is None:
        aux_origin = max(relevant_qubits) + 1 if relevant_qubits else 0

    total_qubits = aux_origin + len(input_qubits)
    simulation = OutcomeCompleteSimulation.with_capacity(total_qubits, 100, 64)
    simulation.reserve_qubits(total_qubits)
    simulation.reserve_outcomes(100, 64)

    for offset, qubit in enumerate(input_qubits):
        auxiliary = aux_origin + offset
        simulation.measure(Pauli({qubit: "X", auxiliary: "X"}))
        simulation.measure(Pauli({qubit: "Z", auxiliary: "Z"}))

    projector_rows = []
    for stabilizer in codespace_projector:
        projector_rows.append(simulation.outcome_count)
        simulation.measure(stabilizer)

    walk = simulate_program(program, simulation=simulation)
    return ConditionalChoiResult(
        group=frame_group_of(simulation),
        simulation=simulation,
        projector_outcome_rows=tuple(projector_rows),
        observe_outcome_rows=tuple(walk.observe_outcomes),
        aux_origin=aux_origin,
    )


__all__ = ["ConditionalChoiResult", "conditional_choi_state"]
