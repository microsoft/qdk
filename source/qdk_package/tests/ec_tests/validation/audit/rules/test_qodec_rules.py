"""Tests for whole-qodec audit rules."""

from __future__ import annotations

from collections.abc import Iterator

import qodec as qc
from qdk.ec._audit import Diagnostic, Severity
from qdk.ec._audit.rules.qodec import (
    MissingRealizationRule,
    MissingSourceInstructionRule,
)


def _diags(rule: object, qodec: qc.Qodec) -> list[Diagnostic]:
    iterator: Iterator[Diagnostic] = rule(qodec, qodec=qodec)  # type: ignore[operator]
    return list(iterator)


# ---------------------------------------------------------------------------
# Happy paths
# ---------------------------------------------------------------------------


def test_missing_source_instruction_clean(rep3_qodec: qc.Qodec) -> None:
    assert _diags(MissingSourceInstructionRule(), rep3_qodec) == []


def test_missing_realization_clean(rep3_qodec: qc.Qodec) -> None:
    assert _diags(MissingRealizationRule(), rep3_qodec) == []


# ---------------------------------------------------------------------------
# Negatives
# ---------------------------------------------------------------------------


def test_missing_realization_fires_when_gadget_omitted(
    rep3_qodec: qc.Qodec,
) -> None:
    """Drop one gadget from the top layer; the rule should flag it as an
    instruction without a realization."""
    layer0 = rep3_qodec.layers[0]
    kept = {name: gadget for name, gadget in layer0.gadgets.items() if name != "idle"}
    bogus = qc.Qodec(
        layers=[
            qc.Layer(layer0.isa, gadgets=kept),
            rep3_qodec.layers[1],
        ],
        name="rep3_bogus",
    )
    diagnostics = _diags(MissingRealizationRule(), bogus)
    flagged = [d.summary for d in diagnostics if "'idle'" in d.summary]
    assert flagged
    assert all(d.severity is Severity.ERROR for d in diagnostics)
