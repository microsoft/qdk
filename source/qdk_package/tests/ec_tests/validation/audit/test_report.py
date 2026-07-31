"""Tests for `qdk.ec.audit.Report`."""
from __future__ import annotations

from qdk.ec.audit import Diagnostic, Phase, Report, Severity


def _make(rule: str, severity: Severity, where: str = "x") -> Diagnostic:
    return Diagnostic(rule=rule, severity=severity, summary="x", where=where)


def test_empty_report_is_ok() -> None:
    report = Report()
    assert report.ok
    assert report.errors() == ()
    assert report.warnings() == ()
    assert report.informational() == ()


def test_report_with_only_warnings_is_ok() -> None:
    report = Report(diagnostics=(_make("a", Severity.WARNING),))
    assert report.ok
    assert report.warnings() == (_make("a", Severity.WARNING),)
    assert report.errors() == ()


def test_report_with_error_is_not_ok() -> None:
    report = Report(diagnostics=(
        _make("a", Severity.WARNING),
        _make("b", Severity.ERROR),
    ))
    assert not report.ok
    assert len(report.errors()) == 1
    assert len(report.warnings()) == 1


def test_by_rule_groups_diagnostics() -> None:
    report = Report(diagnostics=(
        _make("rule/x", Severity.ERROR),
        _make("rule/y", Severity.WARNING),
        _make("rule/x", Severity.INFO),
    ))
    grouped = report.by_rule()
    assert set(grouped.keys()) == {"rule/x", "rule/y"}
    assert len(grouped["rule/x"]) == 2
    assert len(grouped["rule/y"]) == 1


def test_by_artifact_groups_diagnostics() -> None:
    report = Report(diagnostics=(
        _make("a", Severity.ERROR, where="gadget[1]"),
        _make("a", Severity.ERROR, where="gadget[1]"),
        _make("b", Severity.ERROR, where="gadget[2]"),
    ))
    grouped = report.by_artifact()
    assert set(grouped.keys()) == {"gadget[1]", "gadget[2]"}
    assert len(grouped["gadget[1]"]) == 2


def test_str_summary_includes_counts() -> None:
    report = Report(diagnostics=(
        _make("a", Severity.ERROR),
        _make("b", Severity.WARNING),
    ))
    text = str(report)
    assert "1 error(s)" in text
    assert "1 warning(s)" in text
    assert "2 total" in text


def test_str_empty_is_ok_message() -> None:
    assert "ok" in str(Report()).lower()


def test_str_includes_diagnostic_detail_indented() -> None:
    diag = Diagnostic(
        rule="r/x",
        severity=Severity.ERROR,
        summary="boom",
        where="here",
        detail="line one\nline two",
    )
    text = str(Report(diagnostics=(diag,)))
    assert "    line one" in text
    assert "    line two" in text


def test_informational_split() -> None:
    report = Report(diagnostics=(
        _make("a", Severity.INFO),
        _make("b", Severity.WARNING),
    ))
    assert len(report.informational()) == 1
    assert report.ok


def test_diagnostic_phase_enum_values() -> None:
    """Phase enum is used by rules; sanity-check the three members exist."""
    members = {p.name for p in Phase}
    assert members == {"STRUCTURAL", "SEMANTIC", "INFORMATIONAL"}
