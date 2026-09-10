"""Tests for structural declaration issues."""

from __future__ import annotations

import qodec as qc

from ec_tests.testing.qodecs import c4
from qdk.ec._analysis.declaration_issues import declaration_issues
from qdk.ec._audit import Auditor, Severity
from qdk.ec._audit.rules.gadget import UnsupportedActionStepRule


def test_complete_measurement_declaration_has_no_issues() -> None:
    gadget = c4().layers[0].gadgets["measure_zz"]

    assert declaration_issues(gadget).missing_observables == ()


def test_bound_flag_is_reported_independently() -> None:
    gadget = c4().layers[0].gadgets["prepare_zz"]

    assert declaration_issues(gadget).bound_flags == ("reject",)


def test_missing_observables_are_structural_issues() -> None:
    original = c4().layers[0].gadgets["measure_zz"]
    gadget = qc.Gadget(
        original.implements,
        original.circuit,
        inputs=list(original.inputs),
        checks=[list(check) for check in original.checks],
        readouts=[],
    )

    assert declaration_issues(gadget).missing_observables == ("0", "1")


def test_unsupported_action_is_reported_without_computing_an_action() -> None:
    original = c4().layers[0].gadgets["measure_zz"]
    instruction = qc.Instruction(
        mnemonic="rotated",
        inputs=[qc.instructions.BlockOperand("c4")],
        action=[qc.actions.Rotate("Z_0 Z_1", angle=0.5)],
    )
    gadget = qc.Gadget(
        instruction,
        original.circuit,
        inputs=list(original.inputs),
        checks=[list(check) for check in original.checks],
    )

    assert declaration_issues(gadget).unsupported_steps == ("Rotate",)


def test_conditional_pauli_is_not_supported_by_declaration_checks() -> None:
    original = c4().layers[0].gadgets["idle"]
    operand = qc.instructions.BlockOperand("c4")
    instruction = qc.Instruction(
        mnemonic="conditional",
        inputs=[operand],
        outputs=[operand],
        flags=["flag"],
        action=[qc.actions.Pauli("X_0", condition=qc.actions.Condition(["flag"]))],
    )
    gadget = qc.Gadget(
        instruction,
        original.circuit,
        inputs=list(original.inputs),
        outputs=list(original.outputs),
        checks=[list(check) for check in original.checks],
        readouts=[{"flag": ["circuit.readouts[0]"]}],
    )

    assert declaration_issues(gadget).unsupported_steps == ("Pauli",)


def test_unsupported_action_step_rule_name_and_filter() -> None:
    protocol = c4()
    original = protocol.layers[0].gadgets["measure_zz"]
    instruction = qc.Instruction(
        "rotated",
        inputs=original.implements.inputs,
        action=[qc.actions.Rotate("Z_0", angle=0.5)],
    )
    gadget = qc.Gadget(instruction, original.circuit, inputs=original.inputs)
    rule = UnsupportedActionStepRule()
    report = Auditor(rules=[rule]).audit_gadget(gadget, qodec=protocol)
    assert len(report.warnings) == 1
    diagnostic = report.warnings[0]
    assert diagnostic.rule == "gadget/unsupported-action-step"
    assert diagnostic.severity is Severity.WARNING
    assert (
        diagnostic.summary
        == "implements.action[0] (Rotate) is not supported by the action verifier"
    )
    assert (
        not Auditor(rules=[rule], disabled=(rule.name,))
        .audit_gadget(gadget, qodec=protocol)
        .diagnostics
    )
