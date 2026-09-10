"""Independent measurement-check omissions and the input-frame contract."""

from __future__ import annotations

import json

import pytest
import qodec as qc

from qdk.ec import audit
from qdk.ec._audit._parity import ParityAnalysis
from qdk.ec._audit.rules import default_rules
from qdk.ec._audit.rules.gadget import MissingCheckRule


def test_builtin_rule_ids_are_exact() -> None:
    names = [rule.name for rule in default_rules()]
    assert len(names) == len(set(names))
    assert set(names) == {
        "qodec/invalid-structure",
        "instruction-set/invalid-clifford",
        "instruction-set/unreferenced-block",
        "code/invalid-algebra",
        "gadget/missing-check",
        "gadget/missing-observable",
        "gadget/missing-flag",
        "gadget/missing-realization",
        "gadget/unsupported-action-step",
        "gadget/check-mismatch",
        "gadget/flag-mismatch",
        "gadget/action-mismatch",
        "gadget/readout-mismatch",
        "gadget/incomplete-output-frame",
    }


def test_missing_checks_are_independent_verified_and_informational(
    rep3_qodec: qc.Qodec,
) -> None:
    gadget = rep3_qodec.layers[0].gadgets["measure_z"]
    gadget.checks = []
    disabled = tuple(
        rule.name for rule in default_rules() if rule.name != "gadget/missing-check"
    )
    report = audit(rep3_qodec, disabled=disabled, promote_warnings=True)
    findings = [
        item for item in report.informational if "gadgets['measure_z']" in item.where
    ]
    assert len(findings) == 2
    assert report.ok and not report.warnings
    assert all(item.rule == "gadget/missing-check" for item in findings)
    analysis = ParityAnalysis(gadget)
    checks: list[list[str]] = []
    for item in findings:
        equation: list[str] = json.loads(
            item.detail.splitlines()[0].removeprefix("Verified relation: ")
        )
        assert analysis.value(equation).is_zero
        assert any(path.startswith("in[") for path in equation)
        checks.append(equation)
    gadget.checks = checks
    assert ParityAnalysis(gadget).missing_checks() == ()
    assert not [
        item
        for item in audit(rep3_qodec, disabled=("gadget/missing-check",)).diagnostics
        if item.rule == "gadget/missing-check"
    ]


def test_complete_and_equivalent_check_bases_do_not_report_omissions(
    rep3_qodec: qc.Qodec,
) -> None:
    gadget = rep3_qodec.layers[0].gadgets["measure_z"]
    assert ParityAnalysis(gadget).missing_checks() == ()
    first, second = gadget.checks
    gadget.checks = [(*first, *second), first]
    assert ParityAnalysis(gadget).missing_checks() == ()
    gadget.checks = [(*first, *second)]
    assert len(ParityAnalysis(gadget).missing_checks()) == 1


def test_duplicate_vacuous_and_invalid_checks_do_not_hide_omissions(
    rep3_qodec: qc.Qodec,
) -> None:
    gadget = rep3_qodec.layers[0].gadgets["measure_z"]
    first = gadget.checks[0]
    gadget.checks = [
        first,
        first,
        ["circuit.readouts[0]"],
        ["circuit.readouts[00]", "circuit.readouts[0:1]"],
    ]
    missing = ParityAnalysis(gadget).missing_checks()
    assert len(missing) == 1
    gadget.checks = [first, *missing]
    assert ParityAnalysis(gadget).missing_checks() == ()


def test_readout_definitions_account_for_checks_written_through_readouts(
    rep3_qodec: qc.Qodec,
) -> None:
    gadget = rep3_qodec.layers[0].gadgets["measure_z"]
    first, second = gadget.checks
    gadget.checks = [
        ["readouts[0]", "in[0].z[0]", "circuit.readouts[1]", "in[0].stabilizers[0]"],
        second,
    ]
    assert ParityAnalysis(gadget).missing_checks() == ()
    gadget.checks = [first]
    assert len(ParityAnalysis(gadget).missing_checks()) == 1


def test_zero_flags_and_readout_dependencies_are_not_duplicate_suggestions(
    rep3_qodec: qc.Qodec,
) -> None:
    physical = rep3_qodec.layers[1].instruction_set
    gadget = qc.Gadget(
        qc.Instruction("flagged", flags=["first", "second"]),
        qc.gadgets.Circuit(physical, "R 0 1\nM 0 1", format="stim"),
        readouts=[["readouts[1]"], ["circuit.readouts[0]"]],
    )
    assert ParityAnalysis(gadget).missing_checks() == (("circuit.readouts[1]",),)
    gadget.checks = [["circuit.readouts[1]"]]
    assert ParityAnalysis(gadget).missing_checks() == ()


@pytest.mark.parametrize(
    "source,expected",
    [("R 0\nH 0\nM 0", ()), ("R 0\nM 0", (("circuit.readouts[0]",),))],
)
def test_random_bits_are_not_checks_but_constant_zero_bits_are(
    rep3_qodec: qc.Qodec,
    source: str,
    expected: tuple[tuple[str, ...], ...],
) -> None:
    physical = rep3_qodec.layers[1].instruction_set
    operand = qc.instructions.BlockOperand("qubit")
    physical.instructions = [
        *physical.instructions.values(),
        qc.Instruction(
            "H",
            inputs=[operand],
            outputs=[operand],
            action=[qc.actions.Clifford({"X_0": "Z_0", "Z_0": "X_0"})],
        ),
    ]
    gadget = qc.Gadget(
        qc.Instruction("probe"),
        qc.gadgets.Circuit(physical, source, format="stim"),
    )
    assert ParityAnalysis(gadget).missing_checks() == expected


def test_parity_one_relations_are_not_suggested(rep3_qodec: qc.Qodec) -> None:
    physical = rep3_qodec.layers[1].instruction_set
    operand = qc.instructions.BlockOperand("qubit")
    physical.instructions = [
        *physical.instructions.values(),
        qc.Instruction(
            "X", inputs=[operand], outputs=[operand], action=[qc.actions.Pauli("X_0")]
        ),
    ]
    gadget = qc.Gadget(
        qc.Instruction("probe"),
        qc.gadgets.Circuit(physical, "R 0\nX 0\nM 0", format="stim"),
    )
    assert ParityAnalysis(gadget).missing_checks() == ()


def test_input_only_identities_do_not_add_missing_measurement_checks(
    rep3_qodec: qc.Qodec,
) -> None:
    gadget = rep3_qodec.layers[0].gadgets["measure_z"]
    gadget.inputs[0].code.stabilizers = ["Z_0 Z_1", "Z_1 Z_2", "Z_0 Z_2"]
    gadget.checks = []
    analysis = ParityAnalysis(gadget)
    missing = analysis.missing_checks()
    assert len(missing) == 2
    assert all(
        any(path.startswith("circuit.") for path in equation) for equation in missing
    )
    assert all(analysis.value(equation).is_zero for equation in missing)


def test_input_frames_are_supplied_not_required_measurements(
    rep3_qodec: qc.Qodec,
) -> None:
    gadget = rep3_qodec.layers[0].gadgets["idle"]
    gadget.circuit = qc.gadgets.Circuit(
        gadget.circuit.instruction_set, "[]", format="yaml"
    )
    gadget.checks = [
        [f"in[0].stabilizers[{index}]", f"out[0].stabilizers[{index}]"]
        for index in range(2)
    ]
    analysis = ParityAnalysis(gadget)
    assert analysis.missing_checks() == ()
    assert analysis.unresolved_outputs() == ()
    gadget.checks = []
    analysis = ParityAnalysis(gadget)
    assert analysis.missing_checks() == ()
    assert len(analysis.unresolved_outputs()) == 2


def test_unavailable_analysis_is_informational_not_a_missing_equation(
    rep3_qodec: qc.Qodec,
) -> None:
    gadget = rep3_qodec.layers[0].gadgets["measure_z"]
    gadget.circuit.format = "custom"
    findings = list(MissingCheckRule()(gadget, qodec=rep3_qodec))
    assert len(findings) == 1
    assert findings[0].severity.name == "INFO"
    assert "analysis unavailable" in findings[0].summary
    assert "Verified relation" not in findings[0].detail
