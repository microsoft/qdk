"""Diagnostic evidence distinguishes missing observables from incorrect equations."""

from __future__ import annotations

import json
from pathlib import Path

from binar import BitVector
import pytest
import qodec as qc

from qdk.ec._audit import Auditor
from qdk.ec._audit._parity import ParityAnalysis
from qdk.ec._audit.rules.gadget import ReadoutMismatchRule


def _c4() -> qc.Qodec:
    return qc.Qodec.load(Path(__file__).parents[1] / "testing/qodecs/c4.qodec.yaml")


def test_wrong_observable_explains_unavailable_result_without_counterexamples() -> None:
    protocol = _c4()
    gadget = protocol.layers[0].gadgets["measure_xx"]
    gadget.circuit.source = "M 0 1 2 3"
    report = Auditor(rules=[ReadoutMismatchRule()]).audit_gadget(gadget, qodec=protocol)
    assert len(report.errors) == 2
    diagnostic = report.errors[0]
    assert (
        diagnostic.summary
        == "The circuit does not provide the required logical X_0 measurement result for readouts[0]"
    )
    assert diagnostic.detail.splitlines() == [
        'Declared equation: ["circuit.readouts[0]", "circuit.readouts[1]"]',
        "No readout formula using circuit bits and incoming frame signs can recover this result.",
    ]
    analysis = ParityAnalysis(gadget)
    constant = BitVector(
        index == len(analysis.expected[0]) - 1
        for index in range(len(analysis.expected[0]))
    )
    assert analysis.candidate(analysis.expected[0]) is None
    assert analysis.candidate(analysis.expected[0] ^ constant) is None


def test_frame_correction_shows_only_declared_and_verified_equations(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    def unexpected_witness(*args: object, **kwargs: object) -> str:
        pytest.fail("A verified equation makes a counterexample unnecessary.")

    monkeypatch.setattr(ParityAnalysis, "witness", unexpected_witness)
    protocol = _c4()
    gadget = protocol.layers[0].gadgets["measure_xx"]
    diagnostic = next(iter(ReadoutMismatchRule()(gadget, qodec=protocol)))
    assert (
        diagnostic.summary
        == "readouts[0] does not report the required logical X_0 measurement"
    )
    detail = diagnostic.detail.splitlines()
    assert len(detail) == 2
    declared = json.loads(detail[0].removeprefix("Declared equation: "))
    required = json.loads(detail[1].removeprefix("Verified readout equation: "))
    analysis = ParityAnalysis(gadget)
    assert analysis.value(declared) != analysis.expected[0]
    assert analysis.value(required) == analysis.expected[0]


def test_constant_inversion_is_not_reported_as_a_missing_observable() -> None:
    physical = _c4().layers[1].instruction_set
    operand = qc.instructions.BlockOperand("qubit")
    instruction = qc.Instruction(
        "measure_z", inputs=[operand], action=[qc.actions.Observe(["Z_0"])]
    )
    encoding = qc.gadgets.Encoding(
        qc.Code("qubit", stabilizers=[], x=["X_0"], z=["Z_0"]), support=["0"]
    )
    gadget = qc.Gadget(
        instruction,
        qc.gadgets.Circuit(physical, "X 0\nM 0", format="stim"),
        inputs=[encoding],
        readouts=[["circuit.readouts[0]", "in[0].z[0]"]],
    )
    protocol = qc.Qodec(
        [
            qc.Layer(
                qc.InstructionSet(
                    "logical",
                    blocks=[qc.instructions.Block("qubit", encodes=1)],
                    instructions=[instruction],
                ),
                gadgets=[gadget],
            ),
            qc.Layer(physical),
        ]
    )
    diagnostic = next(iter(ReadoutMismatchRule()(gadget, qodec=protocol)))
    required = json.loads(
        diagnostic.detail.splitlines()[1].removeprefix("Verified readout equation: ")
    )
    assert 1 in required
    gadget.readouts = [required]
    assert not list(ReadoutMismatchRule()(gadget, qodec=protocol))
    assert "does not provide" not in diagnostic.summary
    assert len(diagnostic.detail.splitlines()) == 2


@pytest.mark.parametrize(
    "equation,summary,explanation",
    [
        (["readouts[0]"], "an undetermined readout", "allow both 0 and 1"),
        (
            ["readouts[0]", "circuit.readouts[0]"],
            "inconsistent readout equations",
            "cannot all hold",
        ),
    ],
)
def test_dependency_failures_explain_why_no_bit_is_defined(
    equation: list[str],
    summary: str,
    explanation: str,
) -> None:
    protocol = _c4()
    gadget = protocol.layers[0].gadgets["measure_xx"]
    gadget.readouts = [
        equation,
        ["circuit.readouts[0]", "circuit.readouts[2]", "in[0].x[1]"],
    ]
    diagnostic = next(iter(ReadoutMismatchRule()(gadget, qodec=protocol)))
    assert summary in diagnostic.summary
    assert explanation in diagnostic.detail
    assert "Declared equation produces:" not in diagnostic.detail
    assert "Verified readout equation:" not in diagnostic.detail


def test_constant_offset_can_still_have_a_verified_readout_equation() -> None:
    physical = _c4().layers[1].instruction_set
    operand = qc.instructions.BlockOperand("qubit")
    instruction = qc.Instruction(
        "measure_z", inputs=[operand], action=[qc.actions.Observe(["Z_0"])]
    )
    encoding = qc.gadgets.Encoding(
        qc.Code("qubit", stabilizers=[], x=["X_0"], z=["Z_0"]), support=["0"]
    )
    gadget = qc.Gadget(
        instruction,
        qc.gadgets.Circuit(physical, "X 0\nM 0\nR 1\nX 1\nM 1", format="stim"),
        inputs=[encoding],
        readouts=[["circuit.readouts[0]", "in[0].z[0]"]],
    )
    protocol = qc.Qodec([])
    diagnostic = next(iter(ReadoutMismatchRule()(gadget, qodec=protocol)))
    assert "Verified readout equation:" in diagnostic.detail
    assert "circuit.readouts[1]" in diagnostic.detail.splitlines()[1]
    assert "constant inversion" not in diagnostic.summary
    assert "does not provide" not in diagnostic.summary
