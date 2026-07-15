# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from qdk.openqasm import parser, semantic
from qdk.openqasm.parser import (
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
)


def test_parse_returns_program() -> None:
    result = parser.parse("OPENQASM 3.0; qubit q; x q;")
    assert not result.has_errors
    assert result.diagnostics == []
    program = result.program
    assert isinstance(program, Program)
    assert type(program).__name__ == "Program"
    assert program.version == "3.0"


def test_node_names_and_isinstance() -> None:
    result = parser.parse("OPENQASM 3.0; qubit q; x q;")
    statements = result.program.statements

    qubit_decl = statements[0]
    assert type(qubit_decl).__name__ == "QubitDeclaration"
    assert isinstance(qubit_decl, Statement)
    assert isinstance(qubit_decl, QASMNode)
    assert not isinstance(qubit_decl, Expression)

    gate = statements[1]
    assert type(gate).__name__ == "QuantumGate"
    assert isinstance(gate, QuantumGate)
    assert isinstance(gate, Statement)


def test_identifier_name() -> None:
    result = parser.parse("OPENQASM 3.0; qubit q; x q;")
    gate = result.program.statements[1]
    assert isinstance(gate, QuantumGate)
    assert isinstance(gate.name, Identifier)
    assert isinstance(gate.name, Expression)
    assert gate.name.name == "x"


def test_binary_expression_operands() -> None:
    result = parser.parse("OPENQASM 3.0; int a = 1 + 2;")
    decl = result.program.statements[0]
    assert type(decl).__name__ == "ClassicalDeclaration"
    assert isinstance(decl, ClassicalDeclaration)
    init = decl.init_expr
    assert isinstance(init, BinaryExpression)
    assert isinstance(init, Expression)
    assert init.op == "Add"
    assert init.lhs.value == 1
    assert init.rhs.value == 2


def test_syntax_expression_has_no_semantic_accessors() -> None:
    result = parser.parse("OPENQASM 3.0; int a = 1 + 2;")
    decl = result.program.statements[0]
    assert isinstance(decl, ClassicalDeclaration)
    init = decl.init_expr
    assert not hasattr(init, "ty")
    assert not hasattr(init, "symbol")
    assert not hasattr(init, "const_value")


def test_parse_broken_program_reports_diagnostics() -> None:
    result = parser.parse("OPENQASM 3.0; qubit;")
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
    for label in diagnostic.labels:
        assert isinstance(label.span, Span)


def test_visitor_counts_gates_and_recurses() -> None:
    class GateCounter(QASMVisitor):
        def __init__(self) -> None:
            self.count = 0

        def visit_QuantumGate(self, node: object) -> None:
            self.count += 1
            self.generic_visit(node)

    result = parser.parse("OPENQASM 3.0; qubit q; x q; y q; z q;")
    counter = GateCounter()
    counter.visit(result.program)
    assert counter.count == 3


def test_visitor_generic_visit_walks_whole_tree() -> None:
    class NodeCounter(QASMVisitor):
        def __init__(self) -> None:
            self.count = 0

        def generic_visit(self, node: object) -> None:
            self.count += 1
            super().generic_visit(node)

    result = parser.parse("OPENQASM 3.0; qubit q; x q;")
    counter = NodeCounter()
    counter.visit(result.program)
    # Program + qubit decl + its identifier + gate + its name identifier + operand.
    assert counter.count > 3


def test_qasm_visitor_shared_across_layers() -> None:
    assert parser.QASMVisitor is semantic.QASMVisitor


def test_parse_with_dict_includes() -> None:
    source = 'OPENQASM 3.0; include "custom.inc"; qubit q; my_gate q;'
    includes = {"custom.inc": "gate my_gate q { }"}
    result = parser.parse(source, includes=includes)
    assert not result.has_errors


def test_parse_with_callable_includes() -> None:
    source = 'OPENQASM 3.0; include "custom.inc"; qubit q; my_gate q;'

    def resolver(path: str) -> str:
        assert path == "custom.inc"
        return "gate my_gate q { }"

    result = parser.parse(source, includes=resolver)
    assert not result.has_errors


def test_parse_statement_spans_and_children() -> None:
    result = parser.parse("OPENQASM 3.0; qubit q; x q;")
    for statement in result.program.statements:
        assert isinstance(statement, Statement)
        assert isinstance(statement.span, Span)
        assert statement.span.hi >= statement.span.lo
        assert isinstance(statement.children(), list)
