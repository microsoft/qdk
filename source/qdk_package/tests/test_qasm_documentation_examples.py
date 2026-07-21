# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

import io

import pytest

import qdk.openqasm.parser as parser
import qdk.openqasm.semantic as semantic
from qdk.openqasm.parser import QASMVisitor


def test_package_discovery_example() -> None:
    # Mirrors qdk/__init__.py: qdk.openqasm parser and semantic discovery.
    parsed = parser.parse("OPENQASM 3.0; qubit q;")
    analyzed = semantic.analyze("OPENQASM 3.0; const int value = 1 + 2;")

    assert not parsed.has_errors
    assert not analyzed.has_errors


def test_parse_and_source_navigation_example() -> None:
    # Mirrors README and skill: Parse and navigate sources.
    # Mirrors parser.py: parse entry point and immutable source document.
    parsed = parser.parse(
        'OPENQASM 3.0; include "defs.inc"; qubit q;',
        path="memory://workspace/main.qasm",
        includes={"memory://workspace/defs.inc": "gate local q { x q; }"},
    )

    assert not parsed.has_errors
    assert parsed.program.document is parsed.document
    source_file = parsed.document.source_map.find("memory://workspace/defs.inc")
    assert source_file is not None
    position = parsed.document.source_map.position_at(source_file.id, 5)
    assert parsed.document.source_map.byte_offset(source_file.id, position) == 5
    assert source_file.path == "memory://workspace/defs.inc"


def test_semantic_analysis_and_diagnostics_example() -> None:
    # Mirrors README and skill: Analyze symbols and diagnostics.
    # Mirrors semantic.py: result diagnostics and global spans.
    analysis = semantic.analyze(
        'OPENQASM 3.0; include "stdgates.inc"; qubit q; h q; int value = missing;',
        path="main.qasm",
    )

    assert analysis.has_errors
    assert analysis.program.document is analysis.document
    diagnostic = next(
        diagnostic
        for diagnostic in analysis.diagnostics
        if diagnostic.code == "Qdk.Qasm.Lowerer.UndefinedSymbol"
    )
    source_range = analysis.document.source_map.range_from_span(
        diagnostic.labels[0].span
    )
    assert source_range.source_id == analysis.document.entry.id


def test_logical_resolver_and_case_sensitive_keys_example() -> None:
    # Mirrors README and skill: Resolve includes and Include resolver contract.
    resolved = parser.parse(
        'OPENQASM 3.0; include "./Case.inc"; include "case.inc";',
        path="memory://workspace/main.qasm",
        includes={
            "memory://workspace/Case.inc": "int upper = 1;",
            "memory://workspace/case.inc": "int lower = 2;",
        },
    )

    assert not resolved.has_errors
    assert resolved.document.source_map.find("memory://workspace/Case.inc") is not None
    assert resolved.document.source_map.find("memory://workspace/case.inc") is not None


def test_qdk_include_intrinsics_example() -> None:
    # Mirrors README, parser.py, semantic.py, and skill: qdk.inc intrinsics.
    analysis = semantic.analyze(
        'OPENQASM 3.0; include "qdk.inc"; qubit q; '
        "int result = mresetz_checked(q); postselectz(0, q);"
    )

    assert not analysis.has_errors


def test_resolver_failures_are_result_diagnostics() -> None:
    # Mirrors README, parser.py, semantic.py, and skill: resolver failures.
    def failing_resolver(path: str) -> str | None:
        raise RuntimeError(f"cannot resolve {path}")

    callback_result = parser.parse(
        'OPENQASM 3.0; include "callback.inc";',
        path="memory://workspace/main.qasm",
        includes=failing_resolver,
    )
    no_fallback_result = semantic.analyze(
        'OPENQASM 3.0; include "not-on-filesystem.inc";',
        path="memory://workspace/main.qasm",
    )

    assert callback_result.has_errors
    assert callback_result.document.source_map.get(1).resolution_status == "unresolved"
    assert no_fallback_result.has_errors
    assert (
        no_fallback_result.document.source_map.get(1).resolution_status == "unresolved"
    )


def test_visitor_context_example() -> None:
    # Mirrors README and skill: Visit syntax and semantic trees.
    class GateNames(QASMVisitor):
        def visit_QuantumGate(self, node: object, context: list[str]) -> None:
            context.append(node.name.name)  # type: ignore[attr-defined]
            self.generic_visit(node, context)

    names: list[str] = []
    program = parser.parse("OPENQASM 3.0; qubit q; x q; y q;").program
    GateNames().visit(program, names)

    assert names == ["x", "y"]


def test_canonical_dump_and_writer_example() -> None:
    # Mirrors README and skill: Write canonical source and serialize syntax.
    program = parser.parse_program("OPENQASM 3.0; qubit q; x q;")
    canonical = parser.dumps(program)
    stream = io.StringIO()
    parser.dump(program, stream)

    assert canonical == "OPENQASM 3.0;\nqubit q;\nx q;\n"
    assert stream.getvalue() == canonical


def test_canonical_dump_failure_contract() -> None:
    # Mirrors README, parser.py, and skill: canonical serialization failures.
    recovered = parser.parse("OPENQASM 3.0; int value = ;").program
    with pytest.raises(parser.QASMUnparseError) as caught:
        parser.dumps(recovered)
    assert caught.value.code == "recovered-syntax"

    expected = RuntimeError("write failed")

    class FailingStream:
        def write(self, value: str) -> int:
            del value
            raise expected

    valid = parser.parse_program("OPENQASM 3.0; qubit q;")
    with pytest.raises(RuntimeError) as writer_error:
        parser.dump(valid, FailingStream())  # type: ignore[arg-type]
    assert writer_error.value is expected
