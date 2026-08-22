"""Tests for the instruction-set unreferenced-block audit rule.

The rule flags a block type that an ISA declares but no instruction operand
encodes into — a leftover qodec does not reject at load. It is skipped for
ISAs whose instructions use no block operands at all (e.g. a physical gate
ISA), where the block model does not apply.
"""
from __future__ import annotations

from collections.abc import Iterator

import qodec as qc
from qdk.ec.lint import Diagnostic, Severity
from qdk.ec.lint.rules.instruction_set import UnreferencedBlockRule


def _placeholder_qodec() -> qc.Qodec:
    return qc.Qodec(layers=[qc.Layer(qc.InstructionSet("_placeholder"))])


def _diags(rule: object, isa: qc.InstructionSet) -> list[Diagnostic]:
    iterator: Iterator[Diagnostic] = rule(  # type: ignore[operator]
        isa, qodec=_placeholder_qodec()
    )
    return list(iterator)


def test_unreferenced_block_clean_on_repetition3(rep3_qodec: qc.Qodec) -> None:
    rule = UnreferencedBlockRule()
    for isa in rep3_qodec.instruction_sets.values():
        assert _diags(rule, isa) == [], f"unexpected diagnostics in {isa.name}"


def test_unreferenced_block_fires_for_unused_block() -> None:
    operand = qc.instructions.BlockOperand("used")
    isa = qc.InstructionSet(
        name="two_blocks",
        blocks=[
            qc.instructions.Block("used", encodes=1),
            qc.instructions.Block("spare", encodes=1),
        ],
        instructions=[
            qc.Instruction(mnemonic="op", inputs=[operand], outputs=[operand]),
        ],
    )
    diagnostics = _diags(UnreferencedBlockRule(), isa)
    assert any("'spare'" in d.summary for d in diagnostics)
    assert all(d.severity is Severity.INFO for d in diagnostics)


def test_unreferenced_block_skipped_when_no_block_operands() -> None:
    """A gate ISA whose instructions use no block operands is not block-modelled."""
    isa = qc.InstructionSet(
        name="gates",
        blocks=[qc.instructions.Block("qubit", encodes=1)],
        instructions=[qc.Instruction(mnemonic="noop")],
    )
    assert _diags(UnreferencedBlockRule(), isa) == []
