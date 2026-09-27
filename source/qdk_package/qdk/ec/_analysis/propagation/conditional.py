"""Choi-prepared exact propagation with outcome-conditioned frames."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Mapping, Sequence

from paulimer import OutcomeCompleteSimulation
from qodec.gadgets import Circuit

from ..._layout import ProgramLayout
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
    parameter_outcome_rows: Mapping[str, int]


def conditional_choi_state(
    program: Circuit,
    *,
    input_qubits: Sequence[int],
    codespace_projector: Sequence[Pauli] = (),
    aux_origin: int | None = None,
    parameters: Mapping[str, str] | None = None,
) -> ConditionalChoiResult:
    from .interpreter import walk_program

    relevant_qubits: set[int] = set(range(ProgramLayout.of(program).total_qubits))
    relevant_qubits.update(input_qubits)
    for stabilizer in codespace_projector:
        relevant_qubits.update(stabilizer.support)
    if aux_origin is None:
        aux_origin = max(relevant_qubits) + 1 if relevant_qubits else 0

    total_qubits = aux_origin + len(input_qubits)
    parameters = {} if parameters is None else parameters
    capacity = total_qubits + bool(parameters)
    simulation = OutcomeCompleteSimulation.with_capacity(capacity, 100, 64)
    simulation.reserve_qubits(capacity)
    simulation.reserve_outcomes(100, 64)

    for offset, qubit in enumerate(input_qubits):
        auxiliary = aux_origin + offset
        # Measuring XX then ZZ is a Bell preparation with random signs: the pair
        # ends up in one of the four Bell states, and the frames carry which.
        simulation.measure(Pauli({qubit: "X", auxiliary: "X"}))
        simulation.measure(Pauli({qubit: "Z", auxiliary: "Z"}))

    projector_rows = []
    for stabilizer in codespace_projector:
        projector_rows.append(simulation.outcome_count)
        simulation.measure(stabilizer)

    parameter_rows = {}
    for name in sorted(set(parameters.values())):
        # Measuring X on a fresh Z eigenstate makes one independent symbolic bit.
        parameter_rows[name] = simulation.outcome_count
        simulation.measure(Pauli.x(total_qubits))
        simulation.measure(Pauli.z(total_qubits))
    walk = walk_program(
        program,
        simulation=simulation,
        parameter_rows={
            alias: parameter_rows[name] for alias, name in parameters.items()
        },
    )
    group = frame_group_of(simulation)
    if parameters:
        group, _, _ = group.partition(over=range(total_qubits))
    return ConditionalChoiResult(
        group=group,
        simulation=simulation,
        projector_outcome_rows=tuple(projector_rows),
        observe_outcome_rows=walk.observe_outcomes,
        aux_origin=aux_origin,
        parameter_outcome_rows=parameter_rows,
    )


__all__ = ["ConditionalChoiResult", "conditional_choi_state"]
