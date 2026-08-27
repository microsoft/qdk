# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Guards that one user call produces one telemetry event pair.

Each public OpenQASM entry point emits a start event before the work and a
duration event after it. The compatibility wrapper deliberately calls
the native entry point rather than the public one, so that a caller who uses the
wrapper is counted once rather than twice. Nothing enforced that, so a refactor
that made the wrapper delegate to :func:`parse` would silently double every
count without failing a test.

These tests observe the events at the sink rather than at the event helpers, so
they also fail if an entry point stops emitting entirely.
"""

from __future__ import annotations

from typing import Any, Dict, Iterator, List, Tuple

import pytest

from qdk import telemetry_events
from qdk.openqasm import parser, semantic


@pytest.fixture
def recorded_events(monkeypatch: pytest.MonkeyPatch) -> Iterator[List[Tuple[str, Any]]]:
    """Capture telemetry at the sink, so nothing leaves the process."""
    events: List[Tuple[str, Any]] = []

    def record(name: str, value: Any, properties: Dict[str, Any] | None = None, **_: Any) -> None:
        events.append((name, value))

    monkeypatch.setattr(telemetry_events, "log_telemetry", record)
    yield events


def test_parse_emits_exactly_one_start_and_one_duration_event(
    recorded_events: List[Tuple[str, Any]],
) -> None:
    parser.parse("OPENQASM 3.0; qubit q;")
    assert [name for name, _ in recorded_events] == [
        "qsharp.parse_qasm",
        "qsharp.parse_qasm.durationMs",
    ]


def test_analyze_emits_exactly_one_start_and_one_duration_event(
    recorded_events: List[Tuple[str, Any]],
) -> None:
    semantic.analyze("OPENQASM 3.0; qubit q;")
    assert [name for name, _ in recorded_events] == [
        "qsharp.analyze_qasm",
        "qsharp.analyze_qasm.durationMs",
    ]


def test_dumps_emits_exactly_one_start_and_one_duration_event(
    recorded_events: List[Tuple[str, Any]],
) -> None:
    program = parser.parse("OPENQASM 3.0; qubit q;").program
    recorded_events.clear()
    parser.dumps(program)
    assert [name for name, _ in recorded_events] == [
        "qsharp.dumps_qasm",
        "qsharp.dumps_qasm.durationMs",
    ]


def test_the_compatibility_wrapper_does_not_double_count_a_single_call(
    recorded_events: List[Tuple[str, Any]],
) -> None:
    """The wrapper calls the native parser, not the public one, for this reason."""
    parser.parse_program("OPENQASM 3.0; qubit q;")
    assert [name for name, _ in recorded_events] == [
        "qsharp.parse_qasm",
        "qsharp.parse_qasm.durationMs",
    ]


def test_a_wrapper_raise_still_reports_the_parse_that_ran(
    recorded_events: List[Tuple[str, Any]],
) -> None:
    """Strictness is the wrapper's policy, not a parse failure.

    The underlying parse completes and records diagnostics; only then does the
    wrapper raise. The duration event therefore still fires, and it describes
    work that genuinely happened.
    """
    with pytest.raises(parser.QASM3ParsingError):
        parser.parse_program("OPENQASM 3.0; qubit;")
    assert [name for name, _ in recorded_events] == [
        "qsharp.parse_qasm",
        "qsharp.parse_qasm.durationMs",
    ]
