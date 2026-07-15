# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from qdk.openqasm import parser, semantic
from qdk.openqasm.semantic import (
    BinaryOpExpr,
    ClassicalDecl,
    Diagnostic,
    Expression,
    GateCall,
    Program,
    QASMNode,
    QASMVisitor,
    ResolvedIdent,
    Severity,
    Span,
    Statement,
    Symbol,
    SymbolTable,
    Type,
)

_STDGATES = 'OPENQASM 3.0; include "stdgates.inc"; qubit q; x q;'


def test_analyze_returns_semantic_program() -> None:
    result = semantic.analyze(_STDGATES)
    assert not result.has_errors
    program = result.program
    assert isinstance(program, Program)
    assert type(program).__name__ == "Program"
    assert program.version == "3.0"


def test_semantic_node_names_and_isinstance() -> None:
    result = semantic.analyze(_STDGATES)
    gate = result.program.statements[-1]
    assert type(gate).__name__ == "GateCall"
    assert isinstance(gate, Statement)
    assert isinstance(gate, QASMNode)
    assert not isinstance(gate, Expression)


def test_semantic_expression_carries_type_and_const_value() -> None:
    result = semantic.analyze("OPENQASM 3.0; const int a = 1 + 2;")
    decl = result.program.statements[-1]
    assert isinstance(decl, ClassicalDecl)
    init = decl.init_expr
    assert isinstance(init, BinaryOpExpr)
    assert isinstance(init, Expression)
    assert isinstance(init.ty, Type)
    assert init.const_value == 3


def test_resolved_identifier_exposes_symbol() -> None:
    result = semantic.analyze(_STDGATES)
    gate = result.program.statements[-1]
    assert isinstance(gate, GateCall)
    operand = gate.qubits[0]
    assert isinstance(operand, ResolvedIdent)
    symbol = operand.symbol
    assert isinstance(symbol, Symbol)
    assert symbol.name == "q"


def test_symbol_table_lookup_by_id() -> None:
    result = semantic.analyze(_STDGATES)
    gate = result.program.statements[-1]
    assert isinstance(gate, GateCall)
    operand = gate.qubits[0]
    assert isinstance(operand, ResolvedIdent)
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

        def visit_GateCall(self, node: object) -> None:
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
