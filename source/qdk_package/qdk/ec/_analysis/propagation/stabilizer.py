"""Stabilizer-state evaluation through qodec programs."""

from __future__ import annotations

from paulimer import OutcomeCompleteSimulation

from .frames import FrameGroup, PauliFrame
from .pauli import Pauli


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


__all__ = ["frame_group_of"]
