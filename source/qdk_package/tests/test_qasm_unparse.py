# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

import io
from typing import Any

import pytest

from qdk import openqasm
from qdk.openqasm import parser


def test_canonical_format_version_and_alias_identity() -> None:
    assert parser.CANONICAL_FORMAT_VERSION == 1
    assert openqasm.CANONICAL_FORMAT_VERSION == 1
    assert parser.unparse is parser.dumps
    assert openqasm.unparse is openqasm.dumps


def test_dumps_canonicalizes_current_versions() -> None:
    cases = [
        ("OPENQASM 2.0; qreg q[2]; creg c[2];", "OPENQASM 2.0;"),
        ("OPENQASM 3.0; qubit[2] q; bit[2] c;", "OPENQASM 3.0;"),
        ("OPENQASM 3.1; qubit q;", "OPENQASM 3.1;"),
    ]
    for source, header in cases:
        result = parser.parse(source)
        assert not result.has_errors
        emitted = parser.dumps(result.program)
        assert emitted.startswith(header + "\n")
        assert emitted.endswith("\n")
        assert not emitted.endswith("\n\n")
        assert "\r" not in emitted
        reparsed = parser.parse(emitted)
        assert not reparsed.has_errors
        assert parser.dumps(reparsed.program) == emitted


def test_dumps_preserves_include_without_expanding_or_resolving() -> None:
    calls: list[str] = []

    def resolver(path: str) -> str:
        calls.append(path)
        return "gate local q { x q; }"

    result = parser.parse(
        'OPENQASM 3.0; include "custom.inc"; qubit q; local q;',
        includes=resolver,
    )
    assert not result.has_errors
    assert calls == ["custom.inc"]

    calls.clear()
    emitted = parser.dumps(result.program)
    assert calls == []
    assert 'include "custom.inc";' in emitted
    assert "gate local" not in emitted


def test_dump_writes_once_without_flush_or_close() -> None:
    program = parser.parse("OPENQASM 3.0; qubit q;").program

    class Stream:
        def __init__(self) -> None:
            self.calls: list[str] = []
            self.flushed = False
            self.closed = False

        def write(self, value: str) -> int:
            self.calls.append(value)
            return len(value)

        def flush(self) -> None:
            self.flushed = True

        def close(self) -> None:
            self.closed = True

    stream = Stream()
    assert parser.dump(program, stream) is None  # type: ignore[arg-type]
    assert stream.calls == [parser.dumps(program)]
    assert not stream.flushed
    assert not stream.closed


def test_dump_propagates_stream_exception() -> None:
    program = parser.parse("OPENQASM 3.0; qubit q;").program
    expected = RuntimeError("write failed")

    class FailingStream:
        def write(self, value: str) -> int:
            del value
            raise expected

    with pytest.raises(RuntimeError) as caught:
        parser.dump(program, FailingStream())  # type: ignore[arg-type]
    assert caught.value is expected


def test_dumps_rejects_recovered_entry_source_with_payload() -> None:
    result = parser.parse("OPENQASM 3.0; int value = ;")
    assert result.has_errors

    with pytest.raises(parser.QASMUnparseError) as caught:
        parser.dumps(result.program)

    error = caught.value
    assert error.code == "recovered-syntax"
    assert error.span is not None
    assert error.diagnostics
    assert isinstance(error.diagnostics, tuple)
    with pytest.raises(AttributeError):
        error.code = "changed"  # type: ignore[misc]
    with pytest.raises(AttributeError):
        error.span = None  # type: ignore[misc]
    with pytest.raises(AttributeError):
        error.diagnostics = ()  # type: ignore[misc]


def test_dumps_rejects_foreign_program() -> None:
    with pytest.raises(TypeError):
        parser.dumps(object())  # type: ignore[arg-type]


def test_dump_supports_text_io() -> None:
    program = parser.parse("OPENQASM 3.0; qubit q;").program
    stream = io.StringIO()
    parser.dump(program, stream)
    assert stream.getvalue() == parser.dumps(program)


def test_dumps_ignores_mutable_foreign_attributes() -> None:
    program = parser.parse("OPENQASM 3.0; qubit q;").program
    with pytest.raises(AttributeError):
        setattr(program, "version", "2.0")
    assert parser.dumps(program).startswith("OPENQASM 3.0;\n")


def test_public_exception_is_value_error() -> None:
    assert issubclass(parser.QASMUnparseError, ValueError)
    assert openqasm.QASMUnparseError is parser.QASMUnparseError


def test_public_functions_reject_arbitrary_any() -> None:
    foreign: Any = None
    with pytest.raises(TypeError):
        openqasm.dumps(foreign)
