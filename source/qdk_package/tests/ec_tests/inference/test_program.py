"""Tests for qodec programs exposed through simulation targets."""

from types import SimpleNamespace

import pytest

from qdk.ec._analysis.propagation.interpreter import program_of
from qodec.circuits import Program
import qodec as qc


def test_program_rejects_unknown_mnemonic() -> None:
    isa = SimpleNamespace(instructions={})
    call = SimpleNamespace(mnemonic="rx", inputs={})
    with pytest.raises(KeyError, match="rx"):
        Program([call], isa)


def test_program_lookup_returns_instruction(idle_gadget: qc.Gadget) -> None:
    program = program_of(idle_gadget)
    first = program.instructions[0]
    instr_def = program.lookup(first.mnemonic)
    assert instr_def.mnemonic == first.mnemonic


def test_program_lookup_raises_on_unknown_mnemonic(idle_gadget: qc.Gadget) -> None:
    program = program_of(idle_gadget)
    with pytest.raises(KeyError, match="rx"):
        program.lookup("rx")
