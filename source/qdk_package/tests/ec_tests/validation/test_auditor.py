"""Tests for the `qdk.ec.lint` framework and built-in rules.

Inputs come from the vendored, current-model ``repetition3`` qodec
(``tests/analysis/audit/fixtures/repetition3.qodec.yaml``, exposed by the
``rep3_qodec`` fixture), so these tests exercise the audit against a real
loaded qodec.
"""

from __future__ import annotations

from collections.abc import Iterator, Mapping, Sequence

import qodec as qc
from qdk.ec.lint import (
    Auditor,
    Diagnostic,
    Phase,
    Severity,
    diagnose as audit,
)

# ----------------------------------------------------------------------------
# Helpers: rebuild a gadget with the current API, optionally corrupting it.
# ----------------------------------------------------------------------------


def _atoms(readout: Sequence[object] | Mapping[str, Sequence[object]]) -> list[str]:
    """Flatten one ``readouts`` entry (bare list or ``{name: list}``) to atoms."""
    if isinstance(readout, Mapping):
        (equation,) = readout.values()
        return [str(atom) for atom in equation]
    return [str(atom) for atom in readout]


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
    report = audit(rep3_qodec, include_informational=True)
    assert report.ok, str(report)


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
    assert "gadget/missing-observable" in {d.rule for d in report.errors()}


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
    assert "gadget/readout-mismatch" in {d.rule for d in report.errors()}


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
    assert "gadget/reference-out-of-bounds" in {d.rule for d in report.errors()}


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
    assert "gadget/reference-out-of-bounds" in {d.rule for d in report.errors()}


# ----------------------------------------------------------------------------
# Negative: gadget/missing-flag (an instruction declares a flag the gadget's
# readouts do not bind)
# ----------------------------------------------------------------------------


def test_unbound_flag_triggers_missing_flag(rep3_qodec: qc.Qodec) -> None:
    stim_isa = rep3_qodec.layers[1].isa
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
    assert "gadget/missing-flag" in {d.rule for d in report.errors()}


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
