"""Stabilizer-state evaluation through qodec programs."""

from __future__ import annotations

from paulimer import OutcomeCompleteSimulation, PauliGroup
from qodec.circuits import Program

from .frames import FrameGroup, PauliFrame
from .interpreter import walk_for_outcome_code
from .pauli import Pauli


def stabilizer_group_of(program: Program) -> PauliGroup:
    evolved = evolution_of(PauliGroup([], all_commute=True), program=program)
    return PauliGroup([framed.pauli for framed in evolved], all_commute=True)


def evolution_of(stabilizers: PauliGroup, *, program: Program) -> list[PauliFrame]:
    sparse_inputs = list(stabilizers.generators)
    walk = walk_for_outcome_code(program, input_stabilizers=sparse_inputs)
    qubit_count = program.qubit_count
    for sparse in sparse_inputs:
        if sparse.support:
            qubit_count = max(qubit_count, max(sparse.support) + 1)
    return list(frame_group_of(walk.simulation, qubit_count=qubit_count).generators)


def frame_group_of(
    simulation: OutcomeCompleteSimulation,
    *,
    qubit_count: int | None = None,
) -> FrameGroup:
    clifford = simulation.clifford
    sign_rows = list(simulation.sign_matrix.rows)
    count = simulation.qubit_count if qubit_count is None else qubit_count
    return FrameGroup(
        PauliFrame(
            Pauli.from_dense(clifford.image_z(qubit)),
            frozenset(sign_rows[qubit].support),
        )
        for qubit in range(count)
    )


__all__ = ["evolution_of", "frame_group_of", "stabilizer_group_of"]
