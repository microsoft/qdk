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


def test_semantic_gate_modifiers_preserve_kind_and_argument() -> None:
    source = """OPENQASM 3.0;
    include "stdgates.inc";
    qubit[3] q;
    inv @ x q[0];
    pow(2) @ x q[0];
    ctrl(2) @ x q[0], q[1], q[2];
    negctrl(2) @ x q[0], q[1], q[2];
    """
    result = semantic.analyze(source)
    assert not result.has_errors

    gates = result.program.statements[-4:]
    modifiers = [gate.modifiers[0] for gate in gates]
    assert [modifier.modifier for modifier in modifiers] == [
        "inv",
        "pow",
        "ctrl",
        "negctrl",
    ]
    assert modifiers[0].argument is None
    assert modifiers[1].argument.value == 2
    assert [modifier.argument.const_value for modifier in modifiers[2:]] == [2, 2]


def test_gate_modifiers_preserve_implicit_counts_and_source_order() -> None:
    source = """OPENQASM 3.0;
    include "stdgates.inc";
    qubit[4] q;
    ctrl @ inv @ x q[0], q[1];
    negctrl @ ctrl(2) @ x q[0], q[1], q[2], q[3];
    """
    for layer in (parser, semantic):
        result = layer.parse(source) if layer is parser else layer.analyze(source)
        assert not result.has_errors
        gates = result.program.statements[-2:]
        assert [[modifier.modifier for modifier in gate.modifiers] for gate in gates] == [
            ["ctrl", "inv"],
            ["negctrl", "ctrl"],
        ]
        if layer is parser:
            assert gates[0].modifiers[0].argument is None
            assert gates[1].modifiers[0].argument is None
        else:
            assert gates[0].modifiers[0].argument.const_value == 1
            assert gates[1].modifiers[0].argument.const_value == 1


def test_index_ranges_preserve_omitted_components() -> None:
    left = parser.parse("OPENQASM 3.0; array[int[32], 4] a; int x = a[1:];")
    right = parser.parse("OPENQASM 3.0; array[int[32], 4] a; int x = a[:1];")

    left_range = left.program.statements[-1].init_expr.indices[0].values[0]
    right_range = right.program.statements[-1].init_expr.indices[0].values[0]
    assert left_range.start.value == 1
    assert left_range.end is None
    assert right_range.start is None
    assert right_range.end.value == 1

    semantic_left = semantic.analyze(
        "OPENQASM 3.0; array[int[32], 4] a; int x = a[1:];"
    )
    semantic_right = semantic.analyze(
        "OPENQASM 3.0; array[int[32], 4] a; int x = a[:1];"
    )
    left_range = semantic_left.program.statements[-1].init_expr.indices[0]
    right_range = semantic_right.program.statements[-1].init_expr.indices[0]
    assert left_range.start.const_value == 1
    assert left_range.end is None
    assert right_range.start is None
    assert right_range.end.const_value == 1


def test_index_ranges_preserve_all_components() -> None:
    source = """OPENQASM 3.0;
    bit[8] a;
    let empty = a[:];
    let empty_step = a[::];
    let bounded = a[1:2];
    let stepped = a[1:2:3];
    let negative = a[5:-1:1];
    """
    expected = [
        (None, None, None),
        (None, None, None),
        (1, None, 2),
        (1, 2, 3),
        (5, -1, 1),
    ]
    for layer in (parser, semantic):
        result = layer.parse(source) if layer is parser else layer.analyze(source)
        assert not result.has_errors
        actual = []
        for statement in result.program.statements[-5:]:
            range_node = statement.exprs[0].indices[0]
            if layer is parser:
                range_node = range_node.values[0]
                value_of = lambda expression: (
                    expression.value
                    if hasattr(expression, "value")
                    else -expression.operand.value
                )
            else:
                value_of = lambda expression: expression.const_value
            actual.append(
                tuple(
                    None if expression is None else value_of(expression)
                    for expression in (range_node.start, range_node.step, range_node.end)
                )
            )
        assert actual == expected


def test_syntax_indices_preserve_dimensions_and_discrete_sets() -> None:
    indexed = parser.parse(
        "OPENQASM 3.0; array[int[32], 2, 3] a; int x = a[1, 0:2];"
    ).program.statements[-1].init_expr
    assert len(indexed.indices) == 1
    assert len(indexed.indices[0].values) == 2
    assert indexed.indices[0].values[0].value == 1
    assert indexed.indices[0].values[1].start.value == 0

    alias = parser.parse(
        "OPENQASM 3.0; bit[4] a; let b = a[{0, 2}];"
    ).program.statements[-1]
    assert [value.value for value in alias.exprs[0].indices[0].values] == [0, 2]


def test_switch_preserves_cases_and_optional_default() -> None:
    two_cases = """OPENQASM 3.0; int a;
    switch (a) { case 1 { a = 2; } case 3 { a = 4; a = 5; } }
    """
    one_case = """OPENQASM 3.0; int a;
    switch (a) { case 1, 3 { a = 2; a = 4; a = 5; } default { } }
    """
    for layer in (parser, semantic):
        first = layer.parse(two_cases) if layer is parser else layer.analyze(two_cases)
        second = layer.parse(one_case) if layer is parser else layer.analyze(one_case)
        first_switch = first.program.statements[-1]
        second_switch = second.program.statements[-1]
        assert [len(case.labels) for case in first_switch.cases] == [1, 1]
        assert [len(case.body) for case in first_switch.cases] == [1, 2]
        assert first_switch.default is None
        assert [len(case.labels) for case in second_switch.cases] == [2]
        assert second_switch.default == []


def test_subroutine_parameters_preserve_name_and_type_grouping() -> None:
    source = """OPENQASM 3.0;
    def f(int[8] a, float[32] b, qubit q) -> int { return a; }
    """
    syntax_def = parser.parse(source).program.statements[0]
    assert [parameter.identifier.name for parameter in syntax_def.params] == ["a", "b", "q"]
    assert [parameter.type_name for parameter in syntax_def.params] == ["int", "float", "qubit"]
    assert [
        [expression.value for expression in parameter.type_expressions]
        for parameter in syntax_def.params
    ] == [[8], [32], []]

    semantic_def = semantic.analyze(source).program.statements[0]
    assert [parameter.name for parameter in semantic_def.params] == ["a", "b", "q"]
    assert all(parameter.symbol.id == parameter.symbol_id for parameter in semantic_def.params)
    assert [parameter.symbol.ty.name for parameter in semantic_def.params] == [
        "int[8]",
        "float[32]",
        "qubit",
    ]


def test_array_reference_parameters_preserve_type_grouping() -> None:
    source = """OPENQASM 3.0;
    def f(
        readonly array[int[8], 2] a,
        mutable array[float[32], #dim=2] b
    ) { }
    """
    syntax_result = parser.parse(source)
    assert not syntax_result.has_errors
    syntax_params = syntax_result.program.statements[0].params
    assert [parameter.identifier.name for parameter in syntax_params] == ["a", "b"]
    assert [parameter.type_name for parameter in syntax_params] == [
        "readonly static array[int]",
        "mutable dynamic array[float]",
    ]
    assert [[expr.value for expr in parameter.type_expressions] for parameter in syntax_params] == [
        [2],
        [2],
    ]

    semantic_result = semantic.analyze(source)
    assert not semantic_result.has_errors
    semantic_params = semantic_result.program.statements[0].params
    assert [parameter.name for parameter in semantic_params] == ["a", "b"]
    assert [len(parameter.type_expressions) for parameter in semantic_params] == [2, 2]
    assert all(parameter.symbol.id == parameter.symbol_id for parameter in semantic_params)


def test_statement_annotations_preserve_values_and_spans() -> None:
    source = """OPENQASM 3.0;
    @first
    @vendor.payload 23
    int x = 1;
    """
    for layer in (parser, semantic):
        result = layer.parse(source) if layer is parser else layer.analyze(source)
        statement = result.program.statements[0]
        assert [annotation.identifier for annotation in statement.annotations] == [
            "first",
            "vendor.payload",
        ]
        assert [annotation.value for annotation in statement.annotations] == [None, "23"]
        assert statement.annotations[0].value_span is None
        assert statement.annotations[1].value_span.hi > statement.annotations[1].value_span.lo
