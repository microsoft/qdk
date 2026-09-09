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
    report = Auditor().audit_gadget(
        protocol.layers[0].gadgets["measure_xx"], qodec=protocol
    )
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
        ["circuit.readouts[0]", "circuit.readouts[2]"],
    ]
    errors = [
        item
        for item in audit(protocol).errors
        if item.rule == "gadget/readout-mismatch"
    ]
    assert len(errors) == 1
    diagnostic = errors[0]
    assert diagnostic.where == "layers[0].gadgets['measure_xx'] (C4 -> stim)"
    assert (
        diagnostic.summary == "readouts[0] (logical X_0): measurement parity mismatch"
    )
    assert 'Declared: ["circuit.readouts[0]", "in[0].x[0]"]' in diagnostic.detail
    assert (
        'Verified measurement parity: ["circuit.readouts[0]", "circuit.readouts[1]"]'
        in diagnostic.detail
    )
    expected = json.loads(diagnostic.detail.splitlines()[1].split(": ", 1)[1])
    gadget.readouts = [expected, list(gadget.readouts[1].equation)]
    assert not [
        item
        for item in Auditor().audit_gadget(gadget, qodec=protocol).errors
        if item.rule == "gadget/readout-mismatch"
    ]
    assert "encoding-sign terms not checked" in diagnostic.detail


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
        "layers[0].gadgets['idle'] (repetition3 -> repetition3)",
        f"layers[1].gadgets['idle'] (repetition3 -> {target.instruction_set.name})",
    }


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
        ["circuit.readouts[2]", "circuit.readouts[3]"],
        ["circuit.readouts[1]", "circuit.readouts[3]"],
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
        'Relation terms: ["out[0].stabilizers[1]", "in[0].stabilizers[1]"]\n'
        "Parity: 1 (not a valid zero-parity check)."
        in diagnostic.detail
    )
    assert "Verified relation:" not in diagnostic.detail


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
    report = Auditor().audit_gadget(stripped, qodec=rep3_qodec)
    assert not report.ok
    assert "gadget/missing-observable" in {d.rule for d in report.errors}


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
# Negative: gadget/reference-out-of-bounds
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
    assert "gadget/reference-out-of-bounds" in {d.rule for d in report.errors}


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
    assert "gadget/reference-out-of-bounds" in {d.rule for d in report.errors}


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
    assert "gadget/missing-flag" in {d.rule for d in report.errors}


def test_prepared_declared_input_is_rejected(rep3_qodec: qc.Qodec) -> None:
    idle = rep3_qodec.layers[0].gadgets["idle"]
    circuit = qc.gadgets.Circuit(
        idle.circuit.instruction_set,
        f"R 0\n{idle.circuit.source}",
        format=idle.circuit.format,
    )
    corrupted = qc.Gadget(
        idle.implements,
        circuit,
        inputs=list(idle.inputs),
        outputs=list(idle.outputs),
        checks=list(idle.checks),
        readouts=list(idle.readouts),
    )

    report = Auditor().audit_gadget(corrupted, qodec=rep3_qodec)
    assert "gadget/prepared-input" in {d.rule for d in report.errors}


# ----------------------------------------------------------------------------
# Phase ordering: structural errors short-circuit the semantic phase
# ----------------------------------------------------------------------------


def test_structural_error_skips_semantic_phase(rep3_qodec: qc.Qodec) -> None:
    """A missing observable (structural) skips action-mismatch (semantic)."""
    measure_z = rep3_qodec.layers[0].gadgets["measure_z"]
    stripped = _clone(measure_z, readouts=[])
    report = Auditor().audit_gadget(stripped, qodec=rep3_qodec)
    rules_fired = {d.rule for d in report.diagnostics}
    assert "gadget/missing-observable" in rules_fired
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
