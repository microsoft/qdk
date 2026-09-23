"""Tests for the private audit report implementation."""

from __future__ import annotations

from pathlib import Path

import pytest
import qodec as qc

from qdk.ec._audit import Diagnostic, Phase, Report, Severity
from qdk.ec._audit._auditor import Auditor
from qdk.ec._audit.rules.gadget import CheckMismatchRule


def _make(rule: str, severity: Severity, where: str = "x") -> Diagnostic:
    return Diagnostic(rule=rule, severity=severity, summary="x", where=where)


def test_empty_report_is_ok() -> None:
    report = Report()
    assert report.ok
    assert report.errors == ()
    assert report.warnings == ()
    assert report.informational == ()


def test_report_with_only_warnings_is_ok() -> None:
    report = Report(diagnostics=(_make("a", Severity.WARNING),))
    assert report.ok
    assert report.warnings == (_make("a", Severity.WARNING),)
    assert report.errors == ()


def test_report_with_error_is_not_ok() -> None:
    report = Report(
        diagnostics=(
            _make("a", Severity.WARNING),
            _make("b", Severity.ERROR),
        )
    )
    assert not report.ok
    assert len(report.errors) == 1
    assert len(report.warnings) == 1


def test_by_rule_groups_diagnostics() -> None:
    report = Report(
        diagnostics=(
            _make("rule/x", Severity.ERROR),
            _make("rule/y", Severity.WARNING),
            _make("rule/x", Severity.INFO),
        )
    )
    grouped = report.by_rule()
    assert set(grouped.keys()) == {"rule/x", "rule/y"}
    assert len(grouped["rule/x"]) == 2
    assert len(grouped["rule/y"]) == 1


def test_by_artifact_groups_diagnostics() -> None:
    report = Report(
        diagnostics=(
            _make("a", Severity.ERROR, where="gadget[1]"),
            _make("a", Severity.ERROR, where="gadget[1]"),
            _make("b", Severity.ERROR, where="gadget[2]"),
        )
    )
    grouped = report.by_artifact()
    assert set(grouped.keys()) == {"gadget[1]", "gadget[2]"}
    assert len(grouped["gadget[1]"]) == 2


def test_str_summary_includes_counts() -> None:
    report = Report(
        diagnostics=(
            _make("a", Severity.ERROR),
            _make("b", Severity.WARNING),
        )
    )
    text = str(report)
    assert "1 error(s)" in text
    assert "1 warning(s)" in text
    assert "0 informational" in text


def test_str_empty_is_ok_message() -> None:
    assert str(Report()) == "audit: ok"


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
    report = Report(
        diagnostics=(
            _make("a", Severity.INFO),
            _make("b", Severity.WARNING),
        )
    )
    assert len(report.informational) == 1
    assert report.ok


def test_report_formats_location_and_evidence_without_extra_advice() -> None:
    diagnostic = Diagnostic(
        rule="gadget/readout-mismatch",
        severity=Severity.ERROR,
        summary="readouts[0] does not report the required logical X_0 measurement",
        where="layers[0].gadgets['measure_xx'] (C4 -> stim)",
        detail='Declared equation: ["circuit.readouts[0]"]\nVerified readout equation: ["circuit.readouts[0]", "circuit.readouts[1]"]',
    )
    assert str(Report((diagnostic,))) == (
        "[ERROR] gadget/readout-mismatch\n"
        "layers[0].gadgets['measure_xx'] (C4 -> stim)\n"
        "readouts[0] does not report the required logical X_0 measurement\n"
        '    Declared equation: ["circuit.readouts[0]"]\n'
        '    Verified readout equation: ["circuit.readouts[0]", "circuit.readouts[1]"]\n\n'
        "audit: 1 error(s), 0 warning(s), 0 informational"
    )


def test_report_reuses_diagnostic_text_in_severity_order() -> None:
    error = _make("test/error", Severity.ERROR)
    warning = _make("test/warning", Severity.WARNING)
    informational = _make("test/info", Severity.INFO)
    report = Report((informational, warning, error))

    assert str(report) == (
        f"{error}\n\n{warning}\n\n" "audit: 1 error(s), 1 warning(s), 1 informational"
    )
    assert str(Report((informational,))) == (
        "audit: 0 error(s), 0 warning(s), 1 informational"
    )


def test_diagnostic_phase_enum_values() -> None:
    """Phase enum is used by rules; sanity-check the three members exist."""
    members = {p.name for p in Phase}
    assert members == {"STRUCTURAL", "SEMANTIC", "INFORMATIONAL"}


@pytest.mark.parametrize("severity", [Severity.ERROR, Severity.WARNING])
@pytest.mark.parametrize("home_kind", ["parent", "prefix", "unavailable"])
def test_report_abbreviates_home_only_for_display(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    severity: Severity,
    home_kind: str,
) -> None:
    directory = tmp_path / "home-extra"
    directory.mkdir()
    file = directory / "protocol.bundle"
    file.write_text('{"layers": []}')
    location = qc.Qodec.load(file).resolve("").source_location
    assert location is not None

    def home() -> Path:
        if home_kind == "unavailable":
            raise RuntimeError("Cannot determine home directory")
        return directory if home_kind == "parent" else tmp_path / "home"

    monkeypatch.setattr(Path, "home", home)
    diagnostic = Diagnostic(
        "test/location", severity, "message", "qodec", source_location=location
    )
    display_path = "~/protocol.bundle" if home_kind == "parent" else str(file)
    assert str(Report((diagnostic,))).splitlines()[:4] == [
        f"[{severity.name}] test/location",
        f"{display_path}:{location.line}",
        "qodec",
        "message",
    ]
    assert str(diagnostic).splitlines() == str(Report((diagnostic,))).splitlines()[:4]
    assert diagnostic.source_location is location
    assert location.path == file and location.path.is_absolute()


def test_report_points_to_loaded_check_equation(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(Path, "home", lambda: tmp_path / "home")
    protocol = qc.Qodec.load(Path(__file__).parents[2] / "testing/qodecs/c4.qodec.yaml")
    gadget = protocol.layers[0].gadgets["measure_zz"]
    gadget.checks = [["circuit.readouts[0]"]]
    file = tmp_path / "protocol.bundle"
    file.write_text(protocol.dumps(), encoding="utf-8")
    loaded = qc.Qodec.load(file)
    report = Auditor(rules=[CheckMismatchRule()]).audit(loaded)
    diagnostic = report.errors[0]
    location = diagnostic.source_location
    assert location is not None and location.path == file
    assert (
        "circuit.readouts[0]"
        in file.read_text(encoding="utf-8").splitlines()[location.line - 1]
    )
    assert str(report).splitlines()[:4] == [
        "[ERROR] gadget/check-mismatch",
        f"{file}:{location.line}",
        diagnostic.where,
        diagnostic.summary,
    ]
    assert diagnostic.where == "layers[0].gadgets['measure_zz'] (C4 -> stim)"
    assert Auditor(rules=[CheckMismatchRule()]).audit(loaded) == report
    assert (
        Auditor(rules=[CheckMismatchRule()]).audit(protocol).errors[0].source_location
        is None
    )
