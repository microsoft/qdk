"""Diagnostic evidence distinguishes missing observables from incorrect equations."""

from __future__ import annotations

import json
from pathlib import Path

from binar import BitVector
import pytest
import qodec as qc

from qdk.ec._audit import Auditor
from qdk.ec._audit._parity import ParityAnalysis
from qdk.ec._audit.rules.gadget import (
    ActionMismatchRule,
    MissingFlagRule,
    MissingObservableRule,
    FlagMismatchRule,
    ReadoutMismatchRule,
)
from qdk.ec._audit.rules.qodec import StructuralValidationRule


def _c4() -> qc.Qodec:
    return qc.Qodec.load(Path(__file__).parents[1] / "testing/qodecs/c4.qodec.yaml")


@pytest.mark.parametrize("retained", [0, 1])
def test_missing_observables_supply_verified_equations(retained: int) -> None:
    protocol = _c4()
    gadget = protocol.layers[0].gadgets["measure_xx"]
    gadget.readouts = [list(readout.equation) for readout in gadget.readouts[:retained]]
    report = Auditor(
        rules=[StructuralValidationRule(), MissingObservableRule()]
    ).audit_gadget(gadget, qodec=protocol)
    assert len(report.errors) == 2 - retained
    analysis = ParityAnalysis(gadget)
    for position, diagnostic in enumerate(report.errors, start=retained):
        assert diagnostic.rule == "gadget/missing-observable"
        assert diagnostic.summary == (
            f"readouts[{position}] has no equation for the required logical X_{position} measurement"
        )
        assert diagnostic.detail.startswith("Verified readout equation: ")
        candidate = json.loads(
            diagnostic.detail.removeprefix("Verified readout equation: ")
        )
        assert analysis.value(candidate) == analysis.expected[position]


@pytest.mark.parametrize("format,source", [("stim", "M 0 1 2 3"), ("opaque", "")])
def test_missing_observables_remain_errors_without_candidates(
    format: str, source: str
) -> None:
    protocol = _c4()
    gadget = protocol.layers[0].gadgets["measure_xx"]
    gadget.readouts.clear()
    gadget.circuit = qc.gadgets.Circuit(
        gadget.circuit.instruction_set, source, format=format
    )
    report = Auditor(rules=[MissingObservableRule()]).audit_gadget(
        gadget, qodec=protocol
    )
    assert len(report.errors) == 2
    assert all(diagnostic.detail == "" for diagnostic in report.errors)


@pytest.mark.parametrize("retained", [0, 1])
def test_missing_flags_report_only_the_missing_equation(retained: int) -> None:
    instruction = qc.Instruction("flag_pair", flags=["reject_x", "reject_z"])
    gadget = qc.Gadget(
        instruction,
        qc.gadgets.Circuit(qc.InstructionSet("physical"), "", format="stim"),
        readouts=[[] for _ in range(retained)],
    )
    report = Auditor(
        rules=[StructuralValidationRule(), MissingFlagRule()]
    ).audit_gadget(gadget, qodec=qc.Qodec([]))
    assert len(report.errors) == 2 - retained
    for position, diagnostic in enumerate(report.errors, start=retained):
        assert diagnostic.rule == "gadget/missing-flag"
        assert diagnostic.summary == (
            f"readouts[{position}] has no equation for flag {instruction.flags[position]!r}"
        )
        assert diagnostic.detail == ""


@pytest.mark.parametrize("format", ["stim", "opaque"])
def test_preparation_without_inputs_reports_only_actual_analysis_failures(
    format: str,
) -> None:
    physical = _c4().layers[1].instruction_set
    instruction = qc.Instruction(
        "prepare_z",
        outputs=[qc.instructions.BlockOperand("qubit")],
        action=[qc.actions.Stabilize(["Z_0"])],
    )
    gadget = qc.Gadget(
        instruction,
        qc.gadgets.Circuit(physical, "R 0", format=format),
        outputs=[
            qc.gadgets.Encoding(
                qc.Code("qubit", stabilizers=[], x=["X_0"], z=["Z_0"]), support=["0"]
            )
        ],
    )
    report = Auditor(rules=[ActionMismatchRule()]).audit_gadget(
        gadget, qodec=qc.Qodec([])
    )
    if format == "stim":
        assert not report.diagnostics
    else:
        assert not report.errors and not report.informational
        assert len(report.warnings) == 1
        assert (
            report.warnings[0].summary == "Logical action not checked: analysis failed"
        )
        assert (
            report.warnings[0].detail
            == "ValueError: No source parser registered for '.opaque'"
        )


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
            "equation is inconsistent",
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
    assert "Verified readout equation:" in diagnostic.detail


def test_contradiction_does_not_blame_an_independent_readout() -> None:
    protocol = _c4()
    gadget = protocol.layers[0].gadgets["measure_xx"]
    gadget.readouts = [
        ["readouts[0]", 1],
        ["circuit.readouts[0]", "circuit.readouts[2]", "in[0].x[1]"],
    ]
    diagnostics = list(ReadoutMismatchRule()(gadget, qodec=protocol))
    assert len(diagnostics) == 1
    assert diagnostics[0].summary == "readouts[0].equation is inconsistent"
    assert diagnostics[0]._path == "readouts[0].equation"
    candidate = json.loads(
        diagnostics[0]
        .detail.splitlines()[1]
        .removeprefix("Verified readout equation: ")
    )
    analysis = ParityAnalysis(gadget)
    assert analysis.value(candidate) == analysis.expected[0]


def test_conflicting_observables_report_one_error_with_both_candidates() -> None:
    protocol = _c4()
    gadget = protocol.layers[0].gadgets["measure_xx"]
    gadget.readouts = [["readouts[1]"], ["readouts[0]", 1]]
    diagnostics = list(ReadoutMismatchRule()(gadget, qodec=protocol))
    assert len(diagnostics) == 1
    assert diagnostics[0].summary == (
        "readouts[0].equation, readouts[1].equation are inconsistent"
    )
    assert "Verified readout equation for readouts[0]:" in diagnostics[0].detail
    assert "Verified readout equation for readouts[1]:" in diagnostics[0].detail


@pytest.mark.parametrize("dependent_flag", [False, True])
def test_conflict_dependents_are_unverified_not_mismatches(
    dependent_flag: bool,
) -> None:
    protocol = _c4()
    gadget = protocol.layers[0].gadgets["measure_xx"]
    if dependent_flag:
        gadget.implements.flags = ["reject"]
    gadget.readouts = [
        ["readouts[0]", 1],
        (
            ["circuit.readouts[0]", "circuit.readouts[2]", "in[0].x[1]"]
            if dependent_flag
            else ["readouts[0]"]
        ),
        *([["readouts[0]"]] if dependent_flag else []),
    ]
    report = Auditor(rules=[ReadoutMismatchRule(), FlagMismatchRule()]).audit_gadget(
        gadget, qodec=protocol
    )
    assert len(report.errors) == 1
    assert len(report.warnings) == 1
    assert "depends on inconsistent readout equations" in report.warnings[0].detail


def test_flag_conflict_is_reported_once_without_inventing_flag_equations() -> None:
    instruction = qc.Instruction("flag_pair", flags=["reject_x", "reject_z"])
    gadget = qc.Gadget(
        instruction,
        qc.gadgets.Circuit(qc.InstructionSet("physical"), "", format="stim"),
        readouts=[["readouts[1]"], ["readouts[0]", 1]],
    )
    report = Auditor(rules=[ReadoutMismatchRule(), FlagMismatchRule()]).audit_gadget(
        gadget, qodec=qc.Qodec([])
    )
    assert len(report.errors) == 1
    assert report.errors[0].rule == "gadget/flag-mismatch"
    assert "readouts[0].equation, readouts[1].equation" in report.errors[0].summary
    assert "Verified readout equation" not in report.errors[0].detail


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
