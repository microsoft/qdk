"""Declaration redundancy is distinct from noiseless measurement agreement."""

from __future__ import annotations

import json
from pathlib import Path

import pytest
import qodec as qc

from qdk.ec import audit
from qdk.ec._audit import Auditor, Phase, Severity
from qdk.ec._audit._parity import terms_of
from qdk.ec._audit.rules.code import CodeAlgebraRule, RedundantStabilizerRule
from qdk.ec._audit.rules.gadget import RedundantCheckRule, VacuousCheckRule
from qdk.ec._analysis.propagation.pauli import Pauli


@pytest.mark.parametrize(
    "equation",
    [
        [],
        ["circuit.readouts[0]", "circuit.readouts[00]"],
        ["circuit.readouts[0:2]", "circuit.readouts[0,1]"],
    ],
)
def test_vacuous_checks_are_informational(
    rep3_qodec: qc.Qodec, equation: list[str]
) -> None:
    gadget = rep3_qodec.layers[0].gadgets["measure_z"]
    gadget.checks = [equation]
    rule = VacuousCheckRule()
    report = Auditor(rules=[rule], include_informational=True, strict=True).audit(
        rep3_qodec
    )
    assert report.ok and not report.warnings
    assert rule.phase is Phase.INFORMATIONAL
    assert len(report.informational) == 1
    diagnostic = report.informational[0]
    assert diagnostic.rule == "gadget/vacuous-check"
    assert diagnostic.severity is Severity.INFO
    assert diagnostic._path == "checks[0]"
    assert (
        json.loads(
            diagnostic.detail.splitlines()[0].removeprefix("Declared equation: ")
        )
        == equation
    )
    assert not list(RedundantCheckRule()(gadget, qodec=rep3_qodec))
    assert not Auditor(rules=[rule]).audit(rep3_qodec).diagnostics
    assert (
        not Auditor(rules=[rule], include_informational=True, disabled=[rule.name])
        .audit(rep3_qodec)
        .diagnostics
    )


def test_redundant_checks_name_earlier_independent_declarations(
    rep3_qodec: qc.Qodec,
) -> None:
    gadget = rep3_qodec.layers[0].gadgets["measure_z"]
    first, second = gadget.checks
    gadget.checks = [[], first, first, second, (*first, *second), (*second, *first)]
    diagnostics = list(RedundantCheckRule()(gadget, qodec=rep3_qodec))
    assert [item._path for item in diagnostics] == [
        "checks[2]",
        "checks[4]",
        "checks[5]",
    ]
    expected = [["checks[1]"], ["checks[1]", "checks[3]"], ["checks[1]", "checks[3]"]]
    for diagnostic, factors in zip(diagnostics, expected):
        assert diagnostic.severity is Severity.INFO
        assert (
            json.loads(
                diagnostic.detail.splitlines()[1].removeprefix("XOR of checks: ")
            )
            == factors
        )
    assert terms_of(gadget.checks[4]) == terms_of(
        (*gadget.checks[1], *gadget.checks[3])
    )


def test_selector_spellings_and_term_order_do_not_hide_redundancy(
    rep3_qodec: qc.Qodec,
) -> None:
    gadget = rep3_qodec.layers[0].gadgets["measure_z"]
    gadget.checks = [
        ["circuit.readouts[0:2]"],
        ["circuit.readouts[01]", "circuit.readouts[0]"],
    ]
    assert len(list(RedundantCheckRule()(gadget, qodec=rep3_qodec))) == 1


def test_equal_noiseless_measurements_are_not_formally_redundant(
    rep3_qodec: qc.Qodec,
) -> None:
    gadget = qc.Gadget(
        qc.Instruction("probe"),
        qc.gadgets.Circuit(
            rep3_qodec.layers[1].instruction_set, "R 0\nM 0\nM 0", format="stim"
        ),
        checks=[["circuit.readouts[0]"], ["circuit.readouts[1]"]],
    )
    for rule in (VacuousCheckRule(), RedundantCheckRule()):
        assert not list(rule(gadget, qodec=rep3_qodec))
    gadget.circuit.format = "custom"
    gadget.checks = [*gadget.checks, ["circuit.readouts[00]"]]
    assert len(list(RedundantCheckRule()(gadget, qodec=rep3_qodec))) == 1


def test_readout_terms_remain_distinct_without_substitution(
    rep3_qodec: qc.Qodec,
) -> None:
    gadget = rep3_qodec.layers[0].gadgets["measure_z"]
    gadget.checks = [["readouts[0]"], list(gadget.readouts[0].equation)]
    assert not list(RedundantCheckRule()(gadget, qodec=rep3_qodec))


def test_stabilizer_dependency_has_rank_and_exact_product() -> None:
    code = qc.Code(
        "repetition",
        ["Z_0 Z_1", "Z_1 Z_2", "Z_0 Z_2", "Z_0 Z_1"],
        ["X_0 X_1 X_2"],
        ["Z_0"],
    )
    rule = RedundantStabilizerRule()
    assert rule.phase is Phase.INFORMATIONAL
    report = Auditor(
        rules=[CodeAlgebraRule(), rule], include_informational=True, strict=True
    ).audit_code(code, qodec=qc.Qodec([]))
    assert report.ok and not report.warnings
    diagnostics = report.informational
    assert [item._path for item in diagnostics] == ["stabilizers[2]", "stabilizers[3]"]
    assert (
        'Product of generators: ["stabilizers[0]", "stabilizers[1]"]'
        in diagnostics[0].detail
    )
    assert 'Product of generators: ["stabilizers[0]"]' in diagnostics[1].detail
    assert all(
        "Independent stabilizer rank: 2 of 4" in item.detail for item in diagnostics
    )
    assert Pauli(code.stabilizers[0]) * Pauli(code.stabilizers[1]) == Pauli(
        code.stabilizers[2]
    )


@pytest.mark.parametrize(
    "stabilizers, count", [([], 0), (["Z_0", "Z_1"], 0), (["I"], 1)]
)
def test_empty_independent_and_identity_stabilizers(
    stabilizers: list[str], count: int
) -> None:
    code = qc.Code("code", [*stabilizers], [], [])
    findings = list(RedundantStabilizerRule()(code, qodec=qc.Qodec([])))
    assert len(findings) == count
    if findings:
        assert "operator is +I" in findings[0].detail
        assert "rank: 0 of 1" in findings[0].detail


@pytest.mark.parametrize(
    "stabilizers", [["X_0", "Z_0", "X_0"], ["X_0 X_1", "Z_0 Z_1", "Y_0 Y_1"]]
)
def test_invalid_code_has_no_redundancy_notice(stabilizers: list[str]) -> None:
    code = qc.Code("invalid", [*stabilizers], [], [])
    report = Auditor(
        rules=[CodeAlgebraRule(), RedundantStabilizerRule()], include_informational=True
    ).audit_code(code, qodec=qc.Qodec([]))
    assert report.errors and not report.informational
    assert not list(RedundantStabilizerRule()(code, qodec=qc.Qodec([])))


def test_new_rule_filtering_and_loaded_source_locations(
    rep3_qodec: qc.Qodec, tmp_path: Path
) -> None:
    gadget = rep3_qodec.layers[0].gadgets["measure_z"]
    first = gadget.checks[0]
    gadget.checks = [[], *gadget.checks, first]
    code = gadget.inputs[0].code
    code.stabilizers = [*code.stabilizers, code.stabilizers[0]]
    file = tmp_path / "protocol.bundle"
    file.write_text(rep3_qodec.dumps(), encoding="utf-8")
    protocol = qc.Qodec.load(file)
    names = {
        "gadget/vacuous-check",
        "gadget/redundant-check",
        "code/redundant-stabilizer",
    }
    report = audit(protocol, promote_warnings=True)
    findings = [item for item in report.informational if item.rule in names]
    assert {item.rule for item in findings} == names
    assert all(
        item.source_location is not None and item.source_location.path == file
        for item in findings
    )
    assert not names.intersection(
        item.rule for item in audit(protocol, disabled=tuple(names)).diagnostics
    )
