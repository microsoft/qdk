"""Tests for operand handling at the Program / Target boundary.

In the current qodec model block operands are *positional*: a
`BlockOperand` has no name, and an `InstructionCall`'s ``inputs`` /
``outputs`` dict keys are cosmetic parser-convention labels that qdk.ec
matches to the instruction's declared operands *by position*. The program
body itself is validated against its ISA when qodec parses it, so
`Program` performs no operand-key validation of its own — it only checks
that every call's mnemonic exists in the ISA.

These tests pin two things that must keep working under that model:

1. ``Program`` accepts positionally-bound calls (single- and multi-block)
   and rejects only unknown *mnemonics*.
2. ``StimSampler`` emits a single stim circuit with *disjoint* physical
   qubit ranges per block instance, so a two-block program runs correctly
   rather than silently fusing the blocks onto the same wires.
"""
from __future__ import annotations

import numpy as np
import pytest

pytest.importorskip("stim")

import qodec  # noqa: E402
from qodec.circuits import Program  # noqa: E402
from ec_tests.testing.qodecs import c4  # noqa: E402
from qdk.ec.targets import StimSampler  # noqa: E402


@pytest.fixture
def c4_codec() -> qodec.Qodec:
    return c4()


@pytest.fixture
def c4_isa(c4_codec: qodec.Qodec) -> qodec.InstructionSet:
    return c4_codec.layers[0].isa


# ----------------------------------------------------------------------------
# Program construction: positional operands, mnemonic-only validation
# ----------------------------------------------------------------------------


def test_explicit_operands_are_accepted(c4_isa: qodec.InstructionSet) -> None:
    """A program with explicitly bound operands is accepted."""
    program = Program(
        [
            qodec.instructions.InstructionCall("prepare_zz", outputs={"block": "q"}),
            qodec.instructions.InstructionCall(
                "idle", inputs={"block": "q"}, outputs={"block": "q"}
            ),
        ],
        c4_isa,
    )
    assert len(program.instructions) == 2


def test_operand_keys_are_cosmetic(c4_isa: qodec.InstructionSet) -> None:
    """Operands are matched positionally, so the dict *key* a call uses is a
    cosmetic label: an arbitrary key binds the same (single) operand."""
    program = Program(
        [qodec.instructions.InstructionCall("idle", inputs={"anything": "q"}, outputs={"anything": "q"})],
        c4_isa,
    )
    assert len(program.instructions) == 1


def test_unknown_mnemonic_is_rejected(c4_isa: qodec.InstructionSet) -> None:
    """A call to a mnemonic absent from the ISA is rejected at construction."""
    with pytest.raises(KeyError, match="absent from its ISA"):
        Program(
            [qodec.instructions.InstructionCall("not_an_instruction", inputs={"block": "q"})],
            c4_isa,
        )


# ----------------------------------------------------------------------------
# StimSampler: disjoint physical qubit ranges per block instance
# ----------------------------------------------------------------------------


def test_stim_sampler_runs_single_block_program(c4_codec: qodec.Qodec, c4_isa: qodec.InstructionSet) -> None:
    """An explicit single-block program executes correctly: the noiseless
    memory experiment produces no detection events or observable flips."""
    sampler = StimSampler(c4_codec)
    program = Program(
        [
            qodec.instructions.InstructionCall("prepare_zz", outputs={"block": "A"}),
            qodec.instructions.InstructionCall(
                "idle", inputs={"block": "A"}, outputs={"block": "A"}
            ),
            qodec.instructions.InstructionCall("measure_zz", inputs={"block": "A"}),
        ],
        c4_isa,
    )
    result = sampler.execute(program, shots=100)
    events = sampler.emitter.detection_events(program, np.asarray(result))
    flips = sampler.emitter.observable_flips(program, np.asarray(result))
    assert events.sum() == 0
    assert flips.sum() == 0


def test_stim_sampler_handles_two_block_program(c4_codec: qodec.Qodec, c4_isa: qodec.InstructionSet) -> None:
    """Two independent c4 blocks A and B compile to a single stim circuit
    with disjoint physical qubit ranges (4 data qubits each). Noiseless
    execution must produce no detection events on either block."""
    sampler = StimSampler(c4_codec)
    program = Program(
        [
            qodec.instructions.InstructionCall("prepare_zz", outputs={"block": "A"}),
            qodec.instructions.InstructionCall("prepare_zz", outputs={"block": "B"}),
            qodec.instructions.InstructionCall(
                "idle", inputs={"block": "A"}, outputs={"block": "A"}
            ),
            qodec.instructions.InstructionCall(
                "idle", inputs={"block": "B"}, outputs={"block": "B"}
            ),
            qodec.instructions.InstructionCall("measure_zz", inputs={"block": "A"}),
            qodec.instructions.InstructionCall("measure_zz", inputs={"block": "B"}),
        ],
        c4_isa,
    )
    circuit = sampler.emitter.build_circuit(program)
    # Two independent c4 blocks must occupy disjoint data-qubit ranges.
    assert circuit.num_qubits >= 8
    batch = sampler.execute(program, shots=64)
    events = sampler.emitter.detection_events(program, np.asarray(batch))
    flips = sampler.emitter.observable_flips(program, np.asarray(batch))
    assert len(batch) == 64
    assert events.sum() == 0
    assert flips.sum() == 0


def test_stim_sampler_allocates_fresh_block_for_unproduced_input(
    c4_codec: qodec.Qodec, c4_isa: qodec.InstructionSet
) -> None:
    """An ``idle`` call asks for input block ``B`` that no prior call
    produced. The sampler silently allocates fresh physical qubits for B
    (each ``(block, position)`` key is independent); validating that a block
    was previously produced is a higher-level concern handled elsewhere,
    not by the stim sampler."""
    sampler = StimSampler(c4_codec)
    program = Program(
        [
            qodec.instructions.InstructionCall("prepare_zz", outputs={"block": "A"}),
            qodec.instructions.InstructionCall(
                "idle", inputs={"block": "B"}, outputs={"block": "B"}
            ),
        ],
        c4_isa,
    )
    batch = sampler.execute(program, shots=10)
    assert len(batch) == 10
