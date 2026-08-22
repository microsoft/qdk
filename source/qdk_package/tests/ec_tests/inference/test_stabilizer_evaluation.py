"""Tests for stabilizer evaluation through simulation targets."""

from __future__ import annotations

import qodec as qc

from qdk.ec._analysis.propagation import (
    evolution_of,
    program_of,
    stabilizer_group_of,
)
from paulimer import PauliGroup

from qdk.ec._analysis.propagation.frames import PauliFrame


def test_stabilizer_group_of_idle_channel(idle_gadget: qc.Gadget) -> None:
    program = program_of(idle_gadget)
    group = stabilizer_group_of(program)
    assert isinstance(group, PauliGroup)
    assert len(group.generators) == program.qubit_count


def test_evolution_of_empty_matches_stabilizer_group_of(
    idle_gadget: qc.Gadget,
) -> None:
    program = program_of(idle_gadget)
    evolved = evolution_of(PauliGroup([], all_commute=True), program=program)
    assert all(isinstance(framed, PauliFrame) for framed in evolved)
    stripped = PauliGroup([framed.pauli for framed in evolved], all_commute=True)
    assert stripped == stabilizer_group_of(program)
