"""Tests for source-backed qodec circuits used by the analyses."""

import pytest

from qdk.ec._analysis.propagation.interpreter import program_of, walk_program
from qdk.ec._layout import ProgramLayout
from qodec.gadgets import Circuit
import qodec as qc
from ec_tests.testing.optional import requires_stim


def test_program_rejects_unknown_mnemonic() -> None:
    circuit = Circuit(qc.InstructionSet("empty"), "- rx: []", format="yaml")
    with pytest.raises(ValueError, match="rx"):
        _ = circuit.calls()


@requires_stim
def test_program_lookup_returns_instruction(idle_gadget: qc.Gadget) -> None:
    program = program_of(idle_gadget)
    first = program.calls()[0]
    instr_def = program.instruction_set.instructions[first.mnemonic]
    assert instr_def.mnemonic == first.mnemonic


def test_program_lookup_raises_on_unknown_mnemonic(idle_gadget: qc.Gadget) -> None:
    program = program_of(idle_gadget)
    with pytest.raises(KeyError, match="rx"):
        _ = program.instruction_set.instructions["rx"]


def _padding_instruction_set() -> qc.InstructionSet:
    qubit = qc.instructions.BlockOperand("qubit")
    return qc.InstructionSet(
        "padding",
        blocks=[qc.instructions.Block("qubit", 1)],
        instructions=[
            qc.Instruction(
                "R", outputs=[qubit], action=[qc.actions.Stabilize(["Z_0"])]
            ),
            qc.Instruction("M", inputs=[qubit], action=[qc.actions.Observe(["Z_0"])]),
            qc.Instruction("MPAD0", action=[qc.actions.Observe(["I"])]),
            qc.Instruction("MPAD1", action=[qc.actions.Observe(["-I"])]),
        ],
    )


@requires_stim
@pytest.mark.parametrize(
    ("source", "expected", "qubit_count"),
    [
        ("MPAD 0 1 1 0", [False, True, True, False], 0),
        ("R 0 1\nM 0\nMPAD 0 1\nM 1", [False, False, True, False], 2),
        ("REPEAT 2 {\nMPAD(0) 1 0\n}", [True, False, True, False], 0),
    ],
)
def test_mpad_executes_as_constant_record_bits(
    source: str, expected: list[bool], qubit_count: int
) -> None:
    program = Circuit(_padding_instruction_set(), source, format="stim")
    result = walk_program(program)
    values = [
        bool(result.simulation.outcome_shift[row]) for row in result.observe_outcomes
    ]
    assert values == expected
    assert not any(
        result.simulation.random_outcome_indicator[row]
        for row in result.observe_outcomes
    )
    assert len(program.readouts) == len(expected)
    assert ProgramLayout.of(program).total_qubits == qubit_count
