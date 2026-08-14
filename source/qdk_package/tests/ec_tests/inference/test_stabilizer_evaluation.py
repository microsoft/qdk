"""Tests for stabilizer evaluation through simulation targets."""

from __future__ import annotations

import qodec

from qdk.ec._analysis.propagation import (
    Program,
    evolution_of,
    stabilizer_group_of,
)
from paulimer import PauliGroup

from qdk.ec._analysis.propagation.frames import PauliFrame


def _program_of(gadget: qodec.Gadget) -> Program:
    return Program(gadget.circuit.instructions, gadget.circuit.isa)


def test_stabilizer_group_of_idle_channel(idle_gadget: qodec.Gadget) -> None:
    program = _program_of(idle_gadget)
    group = stabilizer_group_of(program)
    assert isinstance(group, PauliGroup)
    assert len(group.generators) == program.qubit_count


def test_evolution_of_empty_matches_stabilizer_group_of(
    idle_gadget: qodec.Gadget,
) -> None:
    program = _program_of(idle_gadget)
    evolved = evolution_of(PauliGroup([], all_commute=True), program=program)
    assert all(isinstance(framed, PauliFrame) for framed in evolved)
    stripped = PauliGroup([framed.pauli for framed in evolved], all_commute=True)
    assert stripped == stabilizer_group_of(program)
