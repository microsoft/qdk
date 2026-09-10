"""Tests for structural declaration issues."""

from __future__ import annotations

import qodec as qc

from ec_tests.testing.qodecs import c4
from qdk.ec._analysis.declaration_issues import declaration_issues


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

    assert declaration_issues(gadget).unsupported_atoms == ("Rotate",)


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

    assert declaration_issues(gadget).unsupported_atoms == ("Pauli",)
