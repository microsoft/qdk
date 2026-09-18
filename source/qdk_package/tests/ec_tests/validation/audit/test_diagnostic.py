"""Tests for `Diagnostic`, `Severity`, and `Phase`."""

from __future__ import annotations

import dataclasses

import pytest

from qdk.ec._audit import Diagnostic, Phase, Severity


def test_severity_enum_values() -> None:
    assert {s.value for s in Severity} == {"info", "warning", "error"}


def test_diagnostic_is_frozen() -> None:
    diag = Diagnostic(rule="r/x", severity=Severity.ERROR, summary="x", where="y")
    with pytest.raises(dataclasses.FrozenInstanceError):
        diag.summary = "modified"  # type: ignore[misc]


def test_diagnostic_default_detail_is_empty() -> None:
    diag = Diagnostic(rule="r/x", severity=Severity.WARNING, summary="x", where="y")
    assert diag.detail == ""


def test_diagnostic_dataclass_replace_preserves_other_fields() -> None:
    """`Auditor`'s strict mode uses dataclasses.replace to promote
    severity. Pin that the rest of the fields ride along."""
    original = Diagnostic(
        rule="r/x",
        severity=Severity.WARNING,
        summary="x",
        where="y",
        detail="z",
    )
    promoted = dataclasses.replace(original, severity=Severity.ERROR)
    assert promoted.rule == original.rule
    assert promoted.summary == original.summary
    assert promoted.where == original.where
    assert promoted.detail == original.detail
    assert promoted.severity is Severity.ERROR


def test_phase_enum_values() -> None:
    assert {p.value for p in Phase} == {"structural", "semantic", "informational"}


@pytest.mark.parametrize("severity", list(Severity))
@pytest.mark.parametrize("detail", ["", "line one\nline two"])
def test_diagnostic_str_formats_severity_location_and_detail(
    severity: Severity, detail: str
) -> None:
    diagnostic = Diagnostic("test/rule", severity, "message", "location", detail)
    expected = f"[{severity.name}] test/rule\nlocation\nmessage"
    if detail:
        expected += "\n    line one\n    line two"

    assert str(diagnostic) == expected
    assert repr(diagnostic).startswith("Diagnostic(rule=")
