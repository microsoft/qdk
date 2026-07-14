# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Guards that no Rust spelling leaks into a Python ``repr``.

The bindings render every ``repr`` by hand or by macro, so a Rust ``Display`` or
``Debug`` spelling can slip through unnoticed: ``Some("3.0")`` instead of
``"3.0"``, lowercase ``true``, or a ``Span { lo: 0, hi: 1 }`` struct-debug form.
The sweep below matches those shapes structurally rather than by raw substring,
so legitimate string content such as ``StringLiteral('{ a: b }')`` cannot trip
it.
"""

from __future__ import annotations

import re
from typing import Any, Iterator

import pytest

from qdk.openqasm import parser, semantic

# `=Some(` style Rust option debug in a field position.
_RUST_OPTION = re.compile(r"=\s*Some\(")
# A bare Rust bool in a field position. Python spells these `True` / `False`.
_RUST_BOOL = re.compile(r"=\s*(?:true|false)\b")
# Rust struct-debug in a field position: `field=Name { inner: value }`. Anchoring
# on `=` keeps quoted string content such as `value='Span { lo: 1 }'` from
# tripping the sweep.
_RUST_STRUCT_DEBUG = re.compile(r"=\s*[A-Z]\w*\s\{\s\w+:\s")

_LEAK_PATTERNS = (
    ("rust option debug", _RUST_OPTION),
    ("rust bool", _RUST_BOOL),
    ("rust struct debug", _RUST_STRUCT_DEBUG),
)

_CORPUS = (
    'OPENQASM 3.0; include "stdgates.inc"; qubit[2] q; bit[2] c; h q[0]; '
    "cx q[0], q[1]; c = measure q;",
    "OPENQASM 3.0; const angle a = pi / 2; const duration d = 100ns; "
    'const bit[4] b = "1011"; const bool t = true; const int n = -5;',
    "OPENQASM 3.0; array[int[32], 2, 2] grid = {{1, 2}, {3, 4}}; "
    "def f(int[32] a) -> int[32] { return a + 1; }",
    'OPENQASM 3.0;\n@my.note text\n#pragma qdk.box.open\nqubit q;\ncal { raw; }\n',
    "OPENQASM 3.0; qubit ;",
    "OPENQASM 3.0; int x = missing;",
    'OPENQASM 3.0; "a string literal"; bool eq = "x" == "y";',
    "OPENQASM 3.1; int x = 1; switch (x) { case 1 { } default { } }",
    "",
)


def _walk(node: Any) -> Iterator[Any]:
    yield node
    for child in node.children():
        yield from _walk(child)


def _reprs_under_test() -> Iterator[tuple[str, str]]:
    """Yields ``(description, repr)`` for every object a user can reach."""
    for source in _CORPUS:
        parsed = parser.parse(source)
        analyzed = semantic.analyze(source)
        for result in (parsed, analyzed):
            yield "result", repr(result)
            yield "document", repr(result.document)
            yield "source_map", repr(result.document.source_map)
            for source_file in result.document.source_map:
                yield "source_file", repr(source_file)
            for diagnostic in result.diagnostics:
                yield "diagnostic", repr(diagnostic)
                for label in diagnostic.labels:
                    yield "label", repr(label)
                    yield "span", repr(label.span)
        for node in _walk(parsed.program):
            yield f"syntax {type(node).__name__}", repr(node)
            yield "span", repr(node.span)
        for node in _walk(analyzed.program):
            yield f"semantic {type(node).__name__}", repr(node)
        yield "symbols", repr(analyzed.symbols)
        for symbol in analyzed.symbols:
            yield "symbol", repr(symbol)
            yield "type", repr(symbol.ty)


def test_no_rust_spellings_leak_into_any_repr() -> None:
    leaks: list[str] = []
    for description, rendered in _reprs_under_test():
        for label, pattern in _LEAK_PATTERNS:
            if pattern.search(rendered):
                leaks.append(f"{description}: {label} in {rendered!r}")
    assert not leaks, "Rust spellings leaked into repr:\n" + "\n".join(leaks)


def test_sweep_detects_an_injected_regression() -> None:
    """The sweep is only useful if it actually fails on a real leak."""
    for rendered in (
        'Program(statements=[1 items], version=Some("3.0"))',
        "ParseResult(has_errors=false, diagnostics=[0 items])",
        'Type("const int", is_const=true)',
        "Label(span=Span { lo: 20, hi: 21 }, message=None)",
    ):
        assert any(
            pattern.search(rendered) for _, pattern in _LEAK_PATTERNS
        ), f"sweep failed to detect a known leak in {rendered!r}"


def test_sweep_does_not_trip_on_legitimate_string_content() -> None:
    """Quoted content that merely looks like Rust must not fail the sweep."""
    for rendered in (
        "StringLiteral(value='Some(x)')",
        "StringLiteral(value='true')",
        "Pragma(command='Span { lo: 1 }')",
        'Include(filename="Some(stdgates.inc)")',
    ):
        matched = [label for label, pattern in _LEAK_PATTERNS if pattern.search(rendered)]
        assert not matched, f"{rendered!r} falsely matched {matched}"


@pytest.mark.parametrize(
    ("source", "expected"),
    [
        ('OPENQASM 3.0; qubit q;', 'Program(statements=[1 items], version="3.0")'),
        ("qubit q;", "Program(statements=[1 items], version=None)"),
    ],
)
def test_program_repr_uses_python_spellings(source: str, expected: str) -> None:
    assert repr(parser.parse(source).program) == expected


def test_result_reprs_use_python_bools() -> None:
    parsed = parser.parse("OPENQASM 3.0; qubit q;")
    analyzed = semantic.analyze("OPENQASM 3.0; qubit q;")
    assert repr(parsed) == "ParseResult(has_errors=False, diagnostics=[0 items])"
    assert repr(analyzed) == "AnalysisResult(has_errors=False, diagnostics=[0 items])"


def test_type_repr_uses_python_bools() -> None:
    analyzed = semantic.analyze("OPENQASM 3.0; const int n = 1;")
    symbol = analyzed.symbols.lookup("n")
    assert symbol is not None
    # A resolved type is now a concrete node; the base rendering stays on `name`.
    assert repr(symbol.ty) == "IntType(size=None)"
    assert str(symbol.ty) == "const int"
    assert symbol.ty.is_const is True


def test_label_repr_reuses_the_span_repr() -> None:
    parsed = parser.parse("OPENQASM 3.0; qubit ;")
    label = parsed.diagnostics[0].labels[0]
    assert repr(label) == f"Label(span={label.span!r}, message=None)"
    assert repr(label.span) == "Span(lo=20, hi=21)"


def test_diagnostic_repr_qualifies_the_severity() -> None:
    parsed = parser.parse("OPENQASM 3.0; qubit ;")
    diagnostic = parsed.diagnostics[0]
    assert repr(diagnostic).startswith("Diagnostic(severity=Severity.Error, message=")


def test_annotation_has_an_informative_repr() -> None:
    program = parser.parse("OPENQASM 3.0;\n@my.note text\nqubit q;\n").program
    annotation = program.statements[-1].annotations[0]
    assert repr(annotation) == 'Annotation(identifier="my.note", value="text")'

    program = parser.parse("OPENQASM 3.0;\n@my.note\nqubit q;\n").program
    annotation = program.statements[-1].annotations[0]
    assert repr(annotation) == 'Annotation(identifier="my.note", value=None)'


def test_no_node_renders_the_placeholder_repr() -> None:
    """Every node must report its own fields, not a bare `Name(...)`."""
    placeholders = [
        rendered
        for description, rendered in _reprs_under_test()
        if rendered.endswith("(...)")
        if description.startswith(("syntax", "semantic"))
    ]
    assert not placeholders, f"nodes still render a placeholder repr: {placeholders}"


def test_no_field_renders_as_unrepresentable() -> None:
    """A field that cannot be read points at a name mismatch in the macro."""
    unreadable = [
        f"{description}: {rendered}"
        for description, rendered in _reprs_under_test()
        if "<unrepresentable>" in rendered
    ]
    assert not unreadable, f"unreadable fields in repr: {unreadable}"


def test_raw_identifier_fields_use_their_python_name() -> None:
    """`r#type` is a Rust spelling; Python sees `type`."""
    program = parser.parse("OPENQASM 3.0; int n = 1;").program
    rendered = repr(program.statements[0])
    assert "type=IntType(" in rendered
    assert "r#" not in rendered


def test_quantum_gate_repr_names_the_gate() -> None:
    program = parser.parse(
        'OPENQASM 3.0; include "stdgates.inc"; qubit[2] q; ctrl @ x q[0], q[1];'
    ).program
    gate = next(s for s in program.statements if type(s).__name__ == "QuantumGate")
    assert repr(gate) == (
        "QuantumGate(name=Identifier(name='x'), modifiers=[1 items], "
        "args=[0 items], qubits=[2 items], duration=None)"
    )


@pytest.mark.parametrize(
    ("source", "expected", "layer"),
    [
        # @sexpr
        ("OPENQASM 3.0; int n = 1;", "IntegerLiteral(value=1)", "syntax"),
        # @stype
        ("OPENQASM 3.0; int[32] n = 1;", "IntType(size=IntegerLiteral(value=32))", "syntax"),
        # @saux
        ("OPENQASM 3.0; qubit[4] q; let a = q[0:1];", "RangeDefinition(", "syntax"),
        # @sstmt
        ("OPENQASM 3.0; qubit q;", "QubitDeclaration(qubit=Identifier(name='q')", "syntax"),
        # @expr
        ("OPENQASM 3.0; int n = 1;", "LiteralExpression(value=1", "semantic"),
        # @stmt
        ("OPENQASM 3.0; qubit q;", "QubitDeclaration(name='q')", "semantic"),
        # @aux
        (
            'OPENQASM 3.0; include "stdgates.inc"; qubit[2] q; ctrl @ x q[0], q[1];',
            "QuantumGateModifier(modifier=GateModifierName.CTRL",
            "semantic",
        ),
    ],
)
def test_each_generated_category_reports_its_fields(
    source: str, expected: str, layer: str
) -> None:
    program = (
        parser.parse(source).program
        if layer == "syntax"
        else semantic.analyze(source).program
    )
    found = [repr(node) for node in _walk(program) if expected in repr(node)]
    assert found, f"no {layer} node rendered {expected!r} for {source!r}"


def test_repr_never_expands_a_child_list() -> None:
    """A repr over a large tree must stay constant-size."""
    source = 'OPENQASM 3.0;\ninclude "stdgates.inc";\nqubit[4] q;\n' + "h q[0];\n" * 5000
    program = parser.parse(source).program
    rendered = repr(program)
    assert len(program.statements) > 5000
    assert rendered == 'Program(statements=[5002 items], version="3.0")'
    assert len(rendered) < 100
