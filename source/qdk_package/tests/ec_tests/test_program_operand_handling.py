"""Positional block operands determine the analysis's logical-qubit layout."""

from __future__ import annotations

from collections import UserList
from collections.abc import MutableSequence

import pytest

import qodec as qc
from qodec.gadgets import Circuit
from qdk.ec._layout import ProgramLayout
from ec_tests.testing.qodecs import c4


@pytest.fixture
def c4_qodec() -> qc.Qodec:
    return c4()


@pytest.fixture
def c4_isa(c4_qodec: qc.Qodec) -> qc.InstructionSet:
    return c4_qodec.layers[0].instruction_set


def test_explicit_operands_are_accepted(c4_isa: qc.InstructionSet) -> None:
    program = Circuit(
        c4_isa,
        "- prepare_zz: [q]\n- idle: {operands: [q]}",
        format="yaml",
    )
    assert [call.operands for call in program.calls()] == [["q"], ["q"]]
    layout = ProgramLayout.of(program)
    assert layout.total_qubits == 2
    assert layout.call_qubit_map(program.calls()[1]) == {0: 0, 1: 1}


def test_multiqubit_blocks_keep_positional_order(c4_isa: qc.InstructionSet) -> None:
    program = Circuit(
        c4_isa,
        "- idle: [left]\n- transversal_cx: [right, left]",
        format="yaml",
    )
    layout = ProgramLayout.of(program)
    assert layout.total_qubits == 4
    assert layout.call_qubit_map(program.calls()[1]) == {0: 2, 1: 3, 2: 0, 3: 1}


def test_unknown_mnemonic_is_rejected(c4_isa: qc.InstructionSet) -> None:
    program = Circuit(c4_isa, "- not_an_instruction: [q]", format="yaml")
    with pytest.raises(ValueError, match="not_an_instruction"):
        ProgramLayout.of(program)


def test_variadic_operands_bind_every_block() -> None:
    operand = qc.instructions.BlockOperand("pair", is_variadic=True)
    instruction_set = qc.InstructionSet(
        "pairs",
        blocks=[qc.instructions.Block("pair", encodes=2)],
        instructions=[qc.Instruction("idle", inputs=[operand], outputs=[operand])],
    )
    circuit = Circuit(instruction_set, "- idle: [left, middle, right]", format="yaml")
    layout = ProgramLayout.of(circuit)
    assert layout.total_qubits == 6
    assert layout.call_qubit_map(circuit.calls()[0]) == {
        index: index for index in range(6)
    }


def test_bound_operands_accept_non_list_mutable_sequences() -> None:
    fixed = qc.instructions.BlockOperand("single")
    variadic = qc.instructions.BlockOperand("pair", is_variadic=True)
    operands: MutableSequence[qc.instructions.BlockOperand] = UserList(
        [fixed, variadic]
    )
    values: MutableSequence[int | str] = UserList([0, "left", "right"])

    assert ProgramLayout._bound_operands(operands, values) == [
        (fixed, 0),
        (variadic, "left"),
        (variadic, "right"),
    ]
    assert operands == [fixed, variadic]
    assert values == [0, "left", "right"]


def test_string_operand_is_one_label(c4_isa: qc.InstructionSet) -> None:
    circuit = Circuit(c4_isa, "- idle: ['left block']", format="yaml")
    layout = ProgramLayout.of(circuit)
    assert layout.instance_bases == {"left block": 0}
    assert layout.total_qubits == 2
