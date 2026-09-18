"""Tests for whole-qodec audit rules."""

from __future__ import annotations

from collections.abc import Iterator

import qodec as qc
from qdk.ec._audit import Diagnostic, Severity, audit
from qdk.ec._audit.rules import default_rules
from qdk.ec._audit.rules.qodec import (
    MissingRealizationRule,
    StructuralValidationRule,
)


def _diags(rule: object, qodec: qc.Qodec) -> list[Diagnostic]:
    iterator: Iterator[Diagnostic] = rule(qodec, qodec=qodec)  # type: ignore[operator]
    return list(iterator)


# ---------------------------------------------------------------------------
# Happy paths
# ---------------------------------------------------------------------------


def test_missing_source_instruction_clean(rep3_qodec: qc.Qodec) -> None:
    assert _diags(StructuralValidationRule(), rep3_qodec) == []


def test_missing_realization_clean(rep3_qodec: qc.Qodec) -> None:
    assert _diags(MissingRealizationRule(), rep3_qodec) == []


# ---------------------------------------------------------------------------
# Negatives
# ---------------------------------------------------------------------------


def test_missing_realization_fires_when_gadget_omitted(
    rep3_qodec: qc.Qodec,
) -> None:
    layer0 = rep3_qodec.layers[0]
    kept = {name: gadget for name, gadget in layer0.gadgets.items() if name != "idle"}
    partial = qc.Qodec(
        layers=[
            qc.Layer(layer0.instruction_set, gadgets=kept),
            rep3_qodec.layers[1],
        ],
        name="rep3_partial",
    )
    rule = MissingRealizationRule()
    diagnostics = _diags(rule, partial)
    assert len(diagnostics) == 1
    assert diagnostics[0].severity is Severity.INFO
    assert diagnostics[0].summary == "No explicit gadget for instruction 'idle'"
    assert diagnostics[0].detail == "No entry: layers[0].gadgets['idle']"

    disabled = tuple(item.name for item in default_rules() if item.name != rule.name)
    for promote_warnings in (False, True):
        report = audit(partial, disabled=disabled, promote_warnings=promote_warnings)
        assert report.ok
        assert report.errors == report.warnings == ()
        assert report.informational == tuple(diagnostics)

    assert rule.name not in {
        item.rule for item in audit(partial, disabled=(rule.name,)).diagnostics
    }
