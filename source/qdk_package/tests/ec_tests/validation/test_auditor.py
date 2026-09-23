"""Tests for the private audit framework and built-in rules.

Inputs come from the vendored, current-model ``repetition3`` qodec
(``tests/analysis/audit/fixtures/repetition3.qodec.yaml``, exposed by the
``rep3_qodec`` fixture), so these tests exercise the audit against a real
loaded qodec.
"""

from __future__ import annotations

from collections.abc import Iterator
import json
from pathlib import Path

import qodec as qc
from qdk.ec._audit import (
    Auditor,
    Diagnostic,
    Phase,
    Severity,
    audit,
)

# ----------------------------------------------------------------------------
# Helpers: rebuild a gadget with the current API, optionally corrupting it.
# ----------------------------------------------------------------------------


def _atoms(readout: qc.gadgets.Readout) -> list[str]:
    """The authored parity terms of a typed readout."""
    return [str(atom) for atom in readout.equation]


def _clone(
    gadget: qc.Gadget,
    *,
    checks: list[list[str]] | None = None,
    readouts: list[list[str]] | None = None,
) -> qc.Gadget:
    """A copy of ``gadget`` with its ``checks`` / ``readouts`` optionally replaced."""
    return qc.Gadget(
        gadget.implements,
        gadget.circuit,
        inputs=list(gadget.inputs),
        outputs=list(gadget.outputs),
        checks=(
            [[str(atom) for atom in check] for check in gadget.checks]
            if checks is None
            else checks
        ),
        readouts=(
            [_atoms(readout) for readout in gadget.readouts]
            if readouts is None
            else readouts
        ),
    )


# ----------------------------------------------------------------------------
# Smoke: the shipped qodec audits with no errors.
# ----------------------------------------------------------------------------


def test_repetition3_audits_without_errors(rep3_qodec: qc.Qodec) -> None:
    report = audit(rep3_qodec)
    assert report.ok, str(report)


def test_repetition3_audits_clean_with_informational(
    rep3_qodec: qc.Qodec,
) -> None:
    report = Auditor(include_informational=True).audit(rep3_qodec)
    assert report.ok, str(report)


def test_c4_measure_xx_readouts_are_consistent() -> None:
    protocol = qc.Qodec.load(
        str(Path(__file__).parents[1] / "testing" / "qodecs" / "c4.qodec.yaml")
    )
    gadget = protocol.layers[0].gadgets["measure_xx"]
    gadget.readouts = [
        (*readout.equation, f"in[0].x[{index}]")
        for index, readout in enumerate(gadget.readouts)
    ]
    report = Auditor().audit_gadget(gadget, qodec=protocol)
    assert not [
        item for item in report.errors if item.rule == "gadget/readout-mismatch"
    ], str(report)


def test_readout_message_has_layer_observable_and_verified_parity() -> None:
    protocol = qc.Qodec.load(
        str(Path(__file__).parents[1] / "testing" / "qodecs" / "c4.qodec.yaml")
    )
    gadget = protocol.layers[0].gadgets["measure_xx"]
    gadget.readouts = [
        ["circuit.readouts[0]", "in[0].x[0]"],
        ["circuit.readouts[0]", "circuit.readouts[2]", "in[0].x[1]"],
    ]
    errors = [
        item
        for item in audit(protocol).errors
        if item.rule == "gadget/readout-mismatch"
        and "gadgets['measure_xx']" in item.where
    ]
    assert len(errors) == 1
    diagnostic = errors[0]
    assert diagnostic.where == "layers[0].gadgets['measure_xx'] (C4 -> stim)"
    assert (
        diagnostic.summary
        == "readouts[0] does not report the required logical X_0 measurement"
    )
    assert (
        'Declared equation: ["circuit.readouts[0]", "in[0].x[0]"]' in diagnostic.detail
    )
    assert (
        'Verified readout equation: ["in[0].x[0]", "circuit.readouts[0]", "circuit.readouts[1]"]'
        in diagnostic.detail
    )
    expected = json.loads(diagnostic.detail.splitlines()[1].split(": ", 1)[1])
    gadget.readouts = [expected, list(gadget.readouts[1].equation)]
    assert not [
        item
        for item in Auditor().audit_gadget(gadget, qodec=protocol).errors
        if item.rule == "gadget/readout-mismatch"
    ]
    assert len(diagnostic.detail.splitlines()) == 2


def test_same_gadget_at_two_layers_has_distinct_locations(rep3_qodec: qc.Qodec) -> None:
    source, target = rep3_qodec.layers
    gadget = source.gadgets["idle"]
    gadget.checks = []
    protocol = qc.Qodec(layers=[source, source, target])
    report = audit(protocol)
    locations = {
        item.where
        for item in report.warnings
        if item.rule == "gadget/incomplete-output-frame"
        and ".gadgets['idle']" in item.where
    }
    assert locations == {
        f"layers[1].gadgets['idle'] (repetition3 -> {target.instruction_set.name})",
    }
    assert any(
        item.rule == "qodec/invalid-structure"
        and item.where == "layers[0].gadgets['idle'] (repetition3 -> repetition3)"
        for item in report.errors
    )


def test_detached_gadget_location_does_not_guess_layer(rep3_qodec: qc.Qodec) -> None:
    detached = _clone(rep3_qodec.layers[0].gadgets["idle"], checks=[])
    report = Auditor().audit_gadget(detached, qodec=rep3_qodec)
    assert {item.where for item in report.warnings} == {"gadget['idle']"}


def test_c4_readout_equivalent_modulo_check_is_accepted() -> None:
    protocol = qc.Qodec.load(
        str(Path(__file__).parents[1] / "testing" / "qodecs" / "c4.qodec.yaml")
    )
    gadget = protocol.layers[0].gadgets["measure_xx"]
    gadget.readouts = [
        [
            "circuit.readouts[2]",
            "circuit.readouts[3]",
            "in[0].x[0]",
            "in[0].stabilizers[0]",
        ],
        [
            "circuit.readouts[1]",
            "circuit.readouts[3]",
            "in[0].x[1]",
            "in[0].stabilizers[0]",
        ],
    ]
    report = Auditor().audit_gadget(gadget, qodec=protocol)
    assert not [
        item for item in report.errors if item.rule == "gadget/readout-mismatch"
    ]


def test_output_frame_message_contains_verified_relation() -> None:
    protocol = qc.Qodec.load(
        str(Path(__file__).parents[1] / "testing" / "qodecs" / "c4.qodec.yaml")
    )
    gadget = protocol.layers[0].gadgets["transversal_cx"]
    report = Auditor().audit_gadget(gadget, qodec=protocol)
    diagnostic = next(
        item for item in report.warnings if "out[0].stabilizers[0]" in item.summary
    )
    assert diagnostic.where == "layers[0].gadgets['transversal_cx'] (C4 -> stim)"
    assert "X_0 X_1 X_2 X_3" in diagnostic.summary
    assert (
        'Verified relation: ["out[0].stabilizers[0]", "in[0].stabilizers[0]", "in[1].stabilizers[0]"]'
        in diagnostic.detail
    )
    relation = json.loads(diagnostic.detail.split("Verified relation: ", 1)[1])
    gadget.checks = [relation]
    assert [str(term) for term in gadget.checks[0]] == relation
    assert "Include" not in diagnostic.detail


def test_output_frame_relation_preserves_constant_sign() -> None:
    protocol = qc.Qodec.load(
        str(Path(__file__).parents[1] / "testing" / "qodecs" / "c4.qodec.yaml")
    )
    gadget = protocol.layers[0].gadgets["x0"]
    gadget.circuit = qc.gadgets.Circuit(
        gadget.circuit.instruction_set, "X 0", format="stim"
    )
    report = Auditor().audit_gadget(gadget, qodec=protocol)
    diagnostic = next(
        item for item in report.warnings if "out[0].stabilizers[1]" in item.summary
    )
    assert (
        'Verified relation: ["out[0].stabilizers[1]", "in[0].stabilizers[1]", 1]'
        in diagnostic.detail
    )


# ----------------------------------------------------------------------------
# Per-artifact entry points
# ----------------------------------------------------------------------------


def test_audit_gadget_only_runs_gadget_rules(rep3_qodec: qc.Qodec) -> None:
    gadget = rep3_qodec.layers[0].gadgets["measure_z"]
    report = Auditor(include_informational=True).audit_gadget(gadget, qodec=rep3_qodec)
    assert report.ok, str(report)
    assert all(d.rule.startswith("gadget/") for d in report.diagnostics)


# ----------------------------------------------------------------------------
# Negative: gadget/missing-observable (a measure gadget's readout is dropped)
# ----------------------------------------------------------------------------


def test_dropped_readouts_triggers_missing_observable(
    rep3_qodec: qc.Qodec,
) -> None:
    measure_z = rep3_qodec.layers[0].gadgets["measure_z"]
    stripped = _clone(measure_z, readouts=[])
    for strict in (False, True):
        report = Auditor(strict=strict).audit_gadget(stripped, qodec=rep3_qodec)
        assert not report.ok
        assert "gadget/missing-observable" in {d.rule for d in report.errors}
        assert "gadget/missing-observable" not in {d.rule for d in report.informational}


# ----------------------------------------------------------------------------
# Negative: gadget/readout-mismatch (a readout's outcome atom is dropped)
# ----------------------------------------------------------------------------


def test_truncated_readout_triggers_readout_mismatch(
    rep3_qodec: qc.Qodec,
) -> None:
    measure_z = rep3_qodec.layers[0].gadgets["measure_z"]
    truncated: list[list[str]] = []
    for readout in measure_z.readouts:
        atoms = _atoms(readout)
        record_atoms = [a for a in atoms if a.startswith("circuit.readouts")]
        other = [a for a in atoms if not a.startswith("circuit.readouts")]
        truncated.append(other + record_atoms[1:])
    corrupted = _clone(measure_z, readouts=truncated)
    report = Auditor().audit_gadget(corrupted, qodec=rep3_qodec)
    assert not report.ok
    assert "gadget/readout-mismatch" in {d.rule for d in report.errors}


# ----------------------------------------------------------------------------
# Negative: qodec reference bounds
# ----------------------------------------------------------------------------


def test_out_of_range_encoding_entry_is_flagged(
    rep3_qodec: qc.Qodec,
) -> None:
    """``measure_z`` destroys its logical, so it has no output encoding; an
    ``out[...]`` reference is therefore out of range."""
    measure_z = rep3_qodec.layers[0].gadgets["measure_z"]
    checks = [[str(a) for a in check] for check in measure_z.checks]
    checks.append(["out[5].stabilizers[0]"])
    corrupted = _clone(measure_z, checks=checks)
    report = Auditor().audit_gadget(corrupted, qodec=rep3_qodec)
    assert not report.ok
    assert "qodec/invalid-structure" in {d.rule for d in report.errors}


def test_out_of_range_stabilizer_index_is_flagged(
    rep3_qodec: qc.Qodec,
) -> None:
    """The repetition code has two stabilizers, so ``stabilizers[9]`` is out
    of range even though the entry index is valid."""
    idle = rep3_qodec.layers[0].gadgets["idle"]
    checks = [[str(a) for a in check] for check in idle.checks]
    checks.append(["in[0].stabilizers[9]"])
    corrupted = _clone(idle, checks=checks)
    report = Auditor().audit_gadget(corrupted, qodec=rep3_qodec)
    assert "qodec/invalid-structure" in {d.rule for d in report.errors}


def test_all_reference_targets_are_bounds_checked(rep3_qodec: qc.Qodec) -> None:
    gadget = rep3_qodec.layers[0].gadgets["measure_z"]
    for reference, valid_range in (
        ("circuit.readouts[0,999]", "out of bounds for 3 entries"),
        ("readouts[0:2]", "out of bounds for 1 entries"),
    ):
        for field in ("checks", "readouts"):
            changed = _clone(gadget, **{field: [[reference]]})
            report = Auditor().audit_gadget(changed, qodec=rep3_qodec)
            errors = [
                item for item in report.errors if item.rule == "qodec/invalid-structure"
            ]
            assert len(errors) == 1
            assert f"{field}[0]" in errors[0].summary
            assert valid_range in errors[0].summary


# ----------------------------------------------------------------------------
# Negative: gadget/missing-flag (an instruction declares a flag the gadget's
# readouts do not bind)
# ----------------------------------------------------------------------------


def test_unbound_flag_triggers_missing_flag(rep3_qodec: qc.Qodec) -> None:
    stim_isa = rep3_qodec.layers[1].instruction_set
    code = rep3_qodec.codes["repetition3"]
    operand = qc.instructions.BlockOperand("repetition3")
    flagged = qc.Instruction(
        "prepare_flagged",
        outputs=[operand],
        flags=["reject"],
        action=[qc.actions.Stabilize(["Z_0"])],
    )
    circuit = qc.gadgets.Circuit(stim_isa, "R 0 1 2", format="stim")
    encoding = qc.gadgets.Encoding(code, support=["0", "1", "2"])
    # readouts=[] leaves the declared 'reject' flag unbound.
    gadget = qc.Gadget(flagged, circuit, outputs=[encoding], readouts=[])
    report = Auditor().audit_gadget(gadget, qodec=rep3_qodec)
    assert not report.ok
    diagnostic = next(d for d in report.errors if d.rule == "gadget/missing-flag")
    assert diagnostic.summary == "readouts[0] has no equation for flag 'reject'"
    assert diagnostic.detail == ""
    strict_report = Auditor(strict=True).audit_gadget(gadget, qodec=rep3_qodec)
    assert not strict_report.ok
    assert "gadget/missing-flag" in {d.rule for d in strict_report.errors}
    gadget.readouts = [{"reject": []}]
    assert "gadget/missing-flag" not in {
        d.rule for d in Auditor().audit_gadget(gadget, qodec=rep3_qodec).diagnostics
    }


def test_reset_of_declared_input_is_checked_against_instruction() -> None:
    block = qc.instructions.Block("qubit", encodes=1)
    operand = qc.instructions.BlockOperand("qubit")
    reset = qc.Instruction(
        "reset_z",
        inputs=[operand],
        outputs=[operand],
        action=[qc.actions.Stabilize(["Z_0"])],
    )
    idle = qc.Instruction("idle", inputs=[operand], outputs=[operand])
    physical_reset = qc.Instruction(
        "R", outputs=[operand], action=[qc.actions.Stabilize(["Z_0"])]
    )
    physical = qc.InstructionSet(
        "physical", blocks=[block], instructions=[physical_reset]
    )
    encoding = qc.gadgets.Encoding(
        qc.Code("qubit", stabilizers=[], x=["X_0"], z=["Z_0"]), support=["0"]
    )
    for instruction, expected_rules in (
        (reset, set()),
        (idle, {"gadget/action-mismatch"}),
    ):
        logical = qc.InstructionSet(
            "logical", blocks=[block], instructions=[instruction]
        )
        gadget = qc.Gadget(
            instruction,
            qc.gadgets.Circuit(physical, "R 0", format="stim"),
            inputs=[encoding],
            outputs=[encoding],
        )
        protocol = qc.Qodec([qc.Layer(logical, gadgets=[gadget]), qc.Layer(physical)])
        protocol.validate()

        report = audit(protocol)
        assert {item.rule for item in report.diagnostics} == expected_rules, str(report)
        assert {item.rule for item in report.errors} == expected_rules, str(report)


# ----------------------------------------------------------------------------
# Phase ordering: structural errors short-circuit the semantic phase
# ----------------------------------------------------------------------------


def test_structural_error_skips_semantic_phase(rep3_qodec: qc.Qodec) -> None:
    """An unresolved reference skips dependent semantic checks."""
    measure_z = rep3_qodec.layers[0].gadgets["measure_z"]
    stripped = _clone(measure_z, readouts=[["in[9].x[0]"]])
    report = Auditor().audit_gadget(stripped, qodec=rep3_qodec)
    rules_fired = {d.rule for d in report.diagnostics}
    assert "qodec/invalid-structure" in rules_fired
    assert "gadget/action-mismatch" not in rules_fired
    assert "gadget/readout-mismatch" not in rules_fired


def test_structural_error_only_skips_semantics_for_its_target(
    rep3_qodec: qc.Qodec,
) -> None:
    class _StructuralOnIdle:
        name = "test/structural-idle"
        severity = Severity.ERROR
        phase = Phase.STRUCTURAL
        target = qc.Gadget

        def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
            if isinstance(target, qc.Gadget) and target.implements.mnemonic == "idle":
                yield Diagnostic(self.name, self.severity, "invalid idle", "idle")

    class _SemanticOnMeasure:
        name = "test/semantic-measure"
        severity = Severity.ERROR
        phase = Phase.SEMANTIC
        target = qc.Gadget

        def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
            if (
                isinstance(target, qc.Gadget)
                and target.implements.mnemonic == "measure_z"
            ):
                yield Diagnostic(
                    self.name,
                    self.severity,
                    "invalid measurement",
                    "measure_z",
                )

    report = Auditor(rules=[_StructuralOnIdle(), _SemanticOnMeasure()]).audit_layer(
        rep3_qodec.layers[0],
        qodec=rep3_qodec,
    )

    assert {(item.rule, item.where) for item in report.diagnostics} == {
        ("test/structural-idle", "idle"),
        ("test/semantic-measure", "measure_z"),
    }


# ----------------------------------------------------------------------------
# gadget/incomplete-output-frame
# ----------------------------------------------------------------------------


def test_incomplete_output_frame_quiet_for_complete_gadget(
    rep3_qodec: qc.Qodec,
) -> None:
    # ``idle`` declares an out[0].stabilizers[i] sign for every stabilizer.
    idle = rep3_qodec.layers[0].gadgets["idle"]
    report = Auditor(include_informational=True).audit_gadget(idle, qodec=rep3_qodec)
    fired = [
        d for d in report.diagnostics if d.rule == "gadget/incomplete-output-frame"
    ]
    assert not fired, str(report)


def test_incomplete_output_frame_fires_when_out_frames_dropped(
    rep3_qodec: qc.Qodec,
) -> None:
    idle = rep3_qodec.layers[0].gadgets["idle"]
    stripped = _clone(idle, checks=[])
    report = Auditor().audit_gadget(stripped, qodec=rep3_qodec)
    fired = [
        d for d in report.diagnostics if d.rule == "gadget/incomplete-output-frame"
    ]
    assert fired, str(report)
    assert all(d.severity is Severity.WARNING for d in fired)
    assert all(".stabilizers[" in d.summary for d in fired), str(report)


# ----------------------------------------------------------------------------
# Strict mode promotes warnings to errors
# ----------------------------------------------------------------------------


def test_strict_mode_promotes_warnings(rep3_qodec: qc.Qodec) -> None:
    """Strict mode turns every WARNING into ERROR."""

    class _AlwaysWarn:
        name = "test/always-warn"
        severity = Severity.WARNING
        phase = Phase.STRUCTURAL
        target = qc.Gadget

        def __call__(
            self, target: object, *, qodec: qc.Qodec
        ) -> "Iterator[Diagnostic]":
            yield Diagnostic(
                rule=self.name,
                severity=self.severity,
                summary="always warn",
                where="test",
            )

    auditor = Auditor(rules=[_AlwaysWarn()], strict=True)
    gadget = rep3_qodec.layers[0].gadgets["measure_z"]
    report = auditor.audit_gadget(gadget, qodec=rep3_qodec)
    assert not report.ok
    assert all(d.severity is Severity.ERROR for d in report.diagnostics)


# ----------------------------------------------------------------------------
# Disabled rules
# ----------------------------------------------------------------------------


def test_disabled_rule_is_skipped(rep3_qodec: qc.Qodec) -> None:
    measure_z = rep3_qodec.layers[0].gadgets["measure_z"]
    stripped = _clone(measure_z, readouts=[])
    auditor = Auditor(disabled={"gadget/missing-observable"})
    report = auditor.audit_gadget(stripped, qodec=rep3_qodec)
    rules_fired = {d.rule for d in report.diagnostics}
    assert "gadget/missing-observable" not in rules_fired
