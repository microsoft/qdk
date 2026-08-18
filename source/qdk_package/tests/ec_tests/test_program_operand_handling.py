"""Tests for positional operand handling in qodec programs.

In the current qodec model block operands are *positional*: a
`BlockOperand` has no name, and an `InstructionCall`'s ``inputs`` /
``outputs`` dict keys are cosmetic parser-convention labels that qdk.ec
matches to the instruction's declared operands *by position*. The program
body itself is validated against its ISA when qodec parses it, so
`Program` performs no operand-key validation of its own — it only checks
that every call's mnemonic exists in the ISA.

These tests pin that ``Program`` accepts positionally-bound calls and rejects
only unknown *mnemonics*.
"""

from __future__ import annotations

import pytest

import qodec as qc
from qodec.circuits import Program
from ec_tests.testing.qodecs import c4


@pytest.fixture
def c4_qodec() -> qc.Qodec:
    return c4()


@pytest.fixture
def c4_isa(c4_qodec: qc.Qodec) -> qc.InstructionSet:
    return c4_qodec.layers[0].isa


# ----------------------------------------------------------------------------
# Program construction: positional operands, mnemonic-only validation
# ----------------------------------------------------------------------------


def test_explicit_operands_are_accepted(c4_isa: qc.InstructionSet) -> None:
    """A program with explicitly bound operands is accepted."""
    program = Program(
        [
            qc.instructions.InstructionCall("prepare_zz", outputs={"block": "q"}),
            qc.instructions.InstructionCall(
                "idle", inputs={"block": "q"}, outputs={"block": "q"}
            ),
        ],
        c4_isa,
    )
    assert len(program.instructions) == 2


def test_operand_keys_are_cosmetic(c4_isa: qc.InstructionSet) -> None:
    """Operands are matched positionally, so the dict *key* a call uses is a
    cosmetic label: an arbitrary key binds the same (single) operand."""
    program = Program(
        [
            qc.instructions.InstructionCall(
                "idle", inputs={"anything": "q"}, outputs={"anything": "q"}
            )
        ],
        c4_isa,
    )
    assert len(program.instructions) == 1


def test_unknown_mnemonic_is_rejected(c4_isa: qc.InstructionSet) -> None:
    """A call to a mnemonic absent from the ISA is rejected at construction."""
    with pytest.raises(KeyError, match="absent from its ISA"):
        Program(
            [
                qc.instructions.InstructionCall(
                    "not_an_instruction", inputs={"block": "q"}
                )
            ],
            c4_isa,
        )
