"""Tests for source-backed qodec circuits used by the analyses."""

import pytest

from qdk.ec._analysis.propagation.interpreter import program_of
from qodec.gadgets import Circuit
import qodec as qc


def test_program_rejects_unknown_mnemonic() -> None:
    circuit = Circuit(qc.InstructionSet("empty"), "- rx: []", format="yaml")
    with pytest.raises(ValueError, match="rx"):
        _ = circuit.calls


def test_program_lookup_returns_instruction(idle_gadget: qc.Gadget) -> None:
    program = program_of(idle_gadget)
    first = program.calls[0]
    instr_def = program.instruction_set.instructions[first.mnemonic]
    assert instr_def.mnemonic == first.mnemonic


def test_program_lookup_raises_on_unknown_mnemonic(idle_gadget: qc.Gadget) -> None:
    program = program_of(idle_gadget)
    with pytest.raises(KeyError, match="rx"):
        _ = program.instruction_set.instructions["rx"]
