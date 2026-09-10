"""Tests for stabilizer evaluation through simulation targets."""

from __future__ import annotations

import qodec as qc
from paulimer import PauliGroup

from qdk.ec._analysis.propagation.frames import PauliFrame
from qdk.ec._analysis.propagation.interpreter import (
    program_of,
    walk_for_outcome_code,
)
from qdk.ec._analysis.propagation.stabilizer import frame_group_of


def test_walking_a_program_stabilizes_every_qubit(idle_gadget: qc.Gadget) -> None:
    program = program_of(idle_gadget)
    walk = walk_for_outcome_code(program)

    frames = list(frame_group_of(walk.simulation).generators)

    assert all(isinstance(framed, PauliFrame) for framed in frames)
    group = PauliGroup([framed.pauli for framed in frames], all_commute=True)
    assert len(group.generators) == program.qubit_count
