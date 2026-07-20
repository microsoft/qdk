# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest

from qdk.openqasm import parser, semantic
from qdk.openqasm.semantic import (
    BinaryExpression,
    ClassicalDeclaration,
    Diagnostic,
    Expression,
    Identifier,
    Program,
    QASMNode,
    QASMVisitor,
    QuantumGate,
    Severity,
    Span,
    Statement,
    Symbol,
    SymbolTable,
    Type,
)
from qdk.openqasm.source import SourceDocument

_STDGATES = 'OPENQASM 3.0; include "stdgates.inc"; qubit q; x q;'


def test_analyze_returns_semantic_program() -> None:
    result = semantic.analyze(_STDGATES)
    assert not result.has_errors
    program = result.program
    assert isinstance(program, Program)
    assert type(program).__name__ == "Program"
    assert program.version == "3.0"


def test_analysis_result_and_program_share_document_with_truthful_root_span() -> None:
    source = 'OPENQASM 3.0; include "child.inc"; int entry = 1;'
    result = semantic.analyze(
        source,
        path="main.qasm",
        includes={"child.inc": "int included = 2;"},
    )

    assert result.program.document is result.document
    assert isinstance(result.document, SourceDocument)
    assert result.program.span == result.document.entry.span
    root_range = result.document.source_map.range_from_span(result.program.span)
    assert root_range.source_id == 0


def test_semantic_entry_and_included_nodes_and_symbols_map_through_document() -> None:
    result = semantic.analyze(
        'OPENQASM 3.0; include "child.inc"; int entry = 1;',
        path="main.qasm",
        includes={"child.inc": "int included = 2;"},
    )
    source_map = result.document.source_map
    statement_source_ids = {
        source_map.range_from_span(statement.span).source_id
        for statement in result.program.statements
    }
    entry_symbol = result.symbols.lookup("entry")
    included_symbol = result.symbols.lookup("included")

    assert statement_source_ids == {0, 1}
    assert entry_symbol is not None
    assert included_symbol is not None
    assert source_map.range_from_span(entry_symbol.span).source_id == 0
    assert source_map.range_from_span(included_symbol.span).source_id == 1


def test_canonical_dump_rejects_semantic_program() -> None:
    program = semantic.analyze(_STDGATES).program
    with pytest.raises(TypeError):
        parser.dumps(program)  # type: ignore[arg-type]


def test_semantic_node_names_and_isinstance() -> None:
    result = semantic.analyze(_STDGATES)
    gate = result.program.statements[-1]
    assert type(gate).__name__ == "QuantumGate"
    assert isinstance(gate, Statement)
    assert isinstance(gate, QASMNode)
    assert not isinstance(gate, Expression)


def test_semantic_expression_carries_type_and_const_value() -> None:
    result = semantic.analyze("OPENQASM 3.0; const int a = 1 + 2;")
    decl = result.program.statements[-1]
    assert isinstance(decl, ClassicalDeclaration)
    init = decl.init_expr
    assert isinstance(init, BinaryExpression)
    assert isinstance(init, Expression)
    assert isinstance(init.ty, Type)
    assert init.const_value == 3


def test_resolved_identifier_exposes_symbol() -> None:
    result = semantic.analyze(_STDGATES)
    gate = result.program.statements[-1]
    assert isinstance(gate, QuantumGate)
    operand = gate.qubits[0]
    assert isinstance(operand, Identifier)
    symbol = operand.symbol
    assert isinstance(symbol, Symbol)
    assert symbol.name == "q"


def test_symbol_table_lookup_by_id() -> None:
    result = semantic.analyze(_STDGATES)
    gate = result.program.statements[-1]
    assert isinstance(gate, QuantumGate)
    operand = gate.qubits[0]
    assert isinstance(operand, Identifier)
    symbol = result.symbols.get(operand.symbol_id)
    assert isinstance(symbol, Symbol)
    assert symbol.name == "q"
    assert isinstance(symbol.ty, Type)


def test_analyze_semantic_error_reports_diagnostics() -> None:
    result = semantic.analyze("OPENQASM 3.0; x undefined_qubit;")
    assert result.has_errors
    assert result.diagnostics
    assert [d.message for d in result.errors] == [
        d.message for d in result.diagnostics
    ]
    diagnostic = result.diagnostics[0]
    assert isinstance(diagnostic, Diagnostic)
    assert isinstance(diagnostic.message, str)
    assert diagnostic.message
    assert isinstance(diagnostic.severity, Severity)


@pytest.mark.parametrize(
    ("includes", "expected_source_id"),
    [
        ({"broken.inc": "int included = missing;"}, 1),
        (
            {
                "nested.inc": 'include "broken.inc";',
                "broken.inc": "int included = missing;",
            },
            2,
        ),
    ],
)
def test_analyze_included_semantic_diagnostics_use_owning_source(
    includes: dict[str, str], expected_source_id: int
) -> None:
    include_path = "nested.inc" if "nested.inc" in includes else "broken.inc"
    result = semantic.analyze(
        f'OPENQASM 3.0; include "{include_path}";',
        path="main.qasm",
        includes=includes,
    )

    assert result.has_errors
    diagnostic = next(
        diagnostic
        for diagnostic in result.diagnostics
        if diagnostic.code == "Qdk.Qasm.Lowerer.UndefinedSymbol"
    )
    source_ids = {
        result.document.source_map.range_from_span(label.span).source_id
        for label in diagnostic.labels
    }
    assert source_ids == {expected_source_id}
    assert "broken.inc" in str(diagnostic)


def test_analyze_diagnostic_full_message() -> None:
    result = semantic.analyze("OPENQASM 3.0; int a = b;")
    assert result.has_errors
    assert len(result.diagnostics) == 1

    diagnostic = result.diagnostics[0]
    assert diagnostic.message == "undefined symbol: b"
    assert diagnostic.severity == Severity.Error
    assert diagnostic.code == "Qdk.Qasm.Lowerer.UndefinedSymbol"

    assert len(diagnostic.labels) == 1
    label = diagnostic.labels[0]
    assert isinstance(label.span, Span)
    assert (label.span.lo, label.span.hi) == (22, 23)
    # The span points at the offending identifier `b` in the source.
    assert "OPENQASM 3.0; int a = b;"[label.span.lo : label.span.hi] == "b"


def test_diagnostic_value_repr_and_hash_policy_is_explicit() -> None:
    result = semantic.analyze("OPENQASM 3.0; int a = b;")
    diagnostic = result.diagnostics[0]
    copied_diagnostic = result.errors[0]
    label = diagnostic.labels[0]
    copied_label = copied_diagnostic.labels[0]

    assert diagnostic == copied_diagnostic
    assert repr(diagnostic).startswith("Diagnostic(")
    with pytest.raises(TypeError):
        hash(diagnostic)
    assert label == copied_label
    assert repr(label).startswith("Label(")
    assert hash(label) == hash(copied_label)
    with pytest.raises(AttributeError):
        diagnostic.message = "changed"  # type: ignore[misc]


def test_analyze_diagnostic_pretty_str() -> None:
    result = semantic.analyze("OPENQASM 3.0; int a = b;")
    diagnostic = result.diagnostics[0]
    # str() renders the miette graphical, source-annotated form (no color,
    # fixed width) so the output is deterministic.
    expected = (
        "Qdk.Qasm.Lowerer.UndefinedSymbol\n"
        "\n"
        "  × undefined symbol: b\n"
        "   ╭─[<source>:1:23]\n"
        " 1 │ OPENQASM 3.0; int a = b;\n"
        "   ·                       ─\n"
        "   ╰────\n"
    )
    assert str(diagnostic) == expected


def test_analyze_diagnostic_render_options() -> None:
    diagnostic = semantic.analyze("OPENQASM 3.0; int a = b;").diagnostics[0]

    # No-color + Unicode at width 80 matches the fixed str() rendering.
    assert diagnostic.render(color=False, unicode=True, width=80) == str(diagnostic)

    # Color emits ANSI escape codes.
    colored = diagnostic.render(color=True)
    assert "\x1b[" in colored

    # ASCII mode avoids color and Unicode box-drawing.
    ascii_render = diagnostic.render(color=False, unicode=False)
    assert "\x1b[" not in ascii_render
    assert "×" not in ascii_render
    assert "╭" not in ascii_render
    assert "`----" in ascii_render





def test_visitor_counts_semantic_gate_calls_and_recurses() -> None:
    class GateCounter(QASMVisitor):
        def __init__(self) -> None:
            self.count = 0

        def visit_QuantumGate(self, node: object) -> None:
            self.count += 1
            self.generic_visit(node)

    result = semantic.analyze(
        'OPENQASM 3.0; include "stdgates.inc"; qubit q; x q; y q; z q;'
    )
    counter = GateCounter()
    counter.visit(result.program)
    assert counter.count == 3


def test_visitor_generic_visit_walks_semantic_tree() -> None:
    class NodeCounter(QASMVisitor):
        def __init__(self) -> None:
            self.count = 0

        def generic_visit(self, node: object) -> None:
            self.count += 1
            super().generic_visit(node)

    result = semantic.analyze(_STDGATES)
    counter = NodeCounter()
    counter.visit(result.program)
    assert counter.count > 1


def test_qasm_visitor_shared_across_layers() -> None:
    assert semantic.QASMVisitor is parser.QASMVisitor


def test_analyze_with_dict_includes() -> None:
    source = 'OPENQASM 3.0; include "custom.inc"; qubit q; my_gate q;'
    includes = {"custom.inc": "gate my_gate q { }"}
    result = semantic.analyze(source, includes=includes)
    assert not result.has_errors


def test_analyze_with_callable_includes() -> None:
    source = 'OPENQASM 3.0; include "custom.inc"; qubit q; my_gate q;'

    def resolver(path: str) -> str:
        assert path == "custom.inc"
        return "gate my_gate q { }"

    result = semantic.analyze(source, includes=resolver)
    assert not result.has_errors


def test_analyze_symbol_table_iterable() -> None:
    result = semantic.analyze(_STDGATES)
    table = result.symbols
    assert isinstance(table, SymbolTable)
    symbols = list(table)
    assert len(symbols) == len(table)
    assert symbols
    for symbol in symbols:
        assert isinstance(symbol, Symbol)
        assert isinstance(symbol.name, str)
        assert isinstance(symbol.ty, Type)
        assert isinstance(symbol.span, Span)


def test_analyze_symbol_table_lookup_and_missing() -> None:
    result = semantic.analyze(_STDGATES)
    table = result.symbols
    symbol = table.lookup("q")
    assert symbol is not None
    assert symbol.name == "q"
    round_tripped = table.get(symbol.id)
    assert round_tripped is not None
    assert round_tripped.name == "q"
    assert table.lookup("does_not_exist") is None
