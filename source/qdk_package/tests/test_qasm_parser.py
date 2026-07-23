# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import importlib
from typing import Any

import pytest

from qdk import openqasm
from qdk.openqasm import parser, semantic
from qdk.openqasm.parser import (
    BinaryExpression,
    ClassicalDeclaration,
    Diagnostic,
    Expression,
    Identifier,
    IndexList,
    Pragma,
    Program,
    QASMNode,
    QASMVisitor,
    QuantumGate,
    RangeDefinition,
    Severity,
    Span,
    Statement,
    SubroutineParameter,
)


def test_parse_returns_program() -> None:
    result = parser.parse("OPENQASM 3.0; qubit q; x q;")
    assert not result.has_errors
    assert result.diagnostics == []
    program = result.program
    assert isinstance(program, Program)
    assert type(program).__name__ == "Program"
    assert program.version == "3.0"


def test_parse_result_exposes_program_document() -> None:
    source = "OPENQASM 3.0; qubit q;"
    result = parser.parse(source, path="main.qasm")

    assert result.document is result.program.document
    assert result.document.entry.path == "main.qasm"
    assert result.document.entry.text == source


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


@pytest.mark.parametrize(
    ("includes", "expected_source_id"),
    [
        ({"broken.inc": "int included = ;"}, 1),
        (
            {
                "nested.inc": 'include "broken.inc";',
                "broken.inc": "int included = ;",
            },
            2,
        ),
    ],
)
def test_parse_included_syntax_diagnostics_use_owning_source(
    includes: dict[str, str], expected_source_id: int
) -> None:
    include_path = "nested.inc" if "nested.inc" in includes else "broken.inc"
    result = parser.parse(
        f'OPENQASM 3.0; include "{include_path}";',
        path="main.qasm",
        includes=includes,
    )

    assert result.has_errors
    diagnostic = result.diagnostics[0]
    source_ids = {
        result.document.source_map.range_from_span(label.span).source_id
        for label in diagnostic.labels
    }
    assert source_ids == {expected_source_id}
    assert "broken.inc" in str(diagnostic)


def test_parse_program_strict_calls_parse_once_and_preserves_error_payload(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    result = parser.parse("OPENQASM 3.0; qubit; qubit;")
    assert len(result.diagnostics) == 2
    calls: list[tuple[str, str, object]] = []

    def spy(source: str, *, path: str, includes: object) -> Any:
        calls.append((source, path, includes))
        return result

    monkeypatch.setattr(parser, "parse", spy)
    with pytest.raises(parser.QASM3ParsingError) as caught:
        parser.parse_program(
            "broken",
            path="main.qasm",
            includes={"defs.inc": ""},
        )

    error = caught.value
    assert calls == [("broken", "main.qasm", {"defs.inc": ""})]
    assert error.result is result
    assert isinstance(error.diagnostics, tuple)
    assert [diagnostic.render(color=False) for diagnostic in error.diagnostics] == [
        diagnostic.render(color=False) for diagnostic in result.diagnostics
    ]
    assert str(error) == "\n\n".join(
        diagnostic.render(color=False).rstrip("\n")
        for diagnostic in result.diagnostics
    )
    assert not str(error).endswith("\n")
    assert "\x1b[" not in str(error)
    with pytest.raises(AttributeError):
        error.result = result  # type: ignore[misc]
    with pytest.raises(AttributeError):
        error.diagnostics = ()  # type: ignore[misc]


@pytest.mark.parametrize("permissive", [False, True])
def test_parse_program_success_calls_parse_once_and_returns_same_program(
    monkeypatch: pytest.MonkeyPatch, permissive: bool
) -> None:
    result = parser.parse("OPENQASM 3.0; qubit q;")
    calls = 0

    def spy(source: str, *, path: str, includes: object) -> Any:
        nonlocal calls
        calls += 1
        assert source == "source"
        assert path == "<source>"
        assert includes is None
        return result

    monkeypatch.setattr(parser, "parse", spy)

    assert parser.parse_program("source", permissive=permissive) is result.program
    assert calls == 1


def test_parse_program_permissive_returns_recovered_program_once(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    result = parser.parse("OPENQASM 3.0; qubit;")
    calls = 0

    def spy(source: str, *, path: str, includes: object) -> Any:
        nonlocal calls
        calls += 1
        return result

    monkeypatch.setattr(parser, "parse", spy)

    assert parser.parse_program("broken", permissive=True) is result.program
    assert calls == 1


@pytest.mark.parametrize(
    ("source", "includes"),
    [
        ('OPENQASM 3.0; include "missing.inc";', None),
        (
            'OPENQASM 3.0; include "broken.inc";',
            lambda path: (_ for _ in ()).throw(RuntimeError(f"cannot resolve {path}")),
        ),
    ],
)
def test_parse_program_strict_raises_for_include_and_resolver_errors(
    source: str, includes: parser.IncludeResolver
) -> None:
    with pytest.raises(parser.QASM3ParsingError) as caught:
        parser.parse_program(source, includes=includes)

    assert caught.value.result.has_errors
    assert caught.value.diagnostics


def test_qasm3_parsing_error_uses_fallback_without_diagnostics() -> None:
    class ResultWithoutDiagnostics:
        diagnostics: list[Diagnostic] = []

    error = parser.QASM3ParsingError(ResultWithoutDiagnostics())  # type: ignore[arg-type]

    assert str(error) == "OpenQASM parsing failed"
    assert error.diagnostics == ()


def test_parse_program_public_exports_and_value_error_compatibility() -> None:
    assert openqasm.parse_program is parser.parse_program
    assert openqasm.QASM3ParsingError is parser.QASM3ParsingError
    assert issubclass(parser.QASM3ParsingError, ValueError)


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


def test_pragma_exposes_authoritative_command_and_derived_views() -> None:
    source = "OPENQASM 3.0;\npragma vendor.mode exact/*opaque*/  \nqubit q;"
    result = parser.parse(source)

    assert not result.has_errors
    program = result.program
    pragma = program.statements[0]
    assert isinstance(pragma, Pragma)
    assert pragma.command == "vendor.mode exact/*opaque*/  "
    assert pragma.name == "vendor.mode"
    assert pragma.value == "exact/*opaque*/  "
    assert pragma.children() == []
    assert program.children()[0] is pragma
    assert not hasattr(program, "pragmas")
    with pytest.raises(AttributeError):
        pragma.command = "changed"  # type: ignore[misc]


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


def test_corrected_syntax_field_shapes_match_runtime_values() -> None:
    source = """OPENQASM 3.0;
    array[int, 2] values;
    int selected = values[0];
    def f(int value) { }
    for int i in [0:1] { }
    """
    result = parser.parse(source)
    assert not result.has_errors

    statements = result.program.statements
    assert isinstance(statements[1].init_expr.indices[0], IndexList)
    assert isinstance(statements[2].params[0], SubroutineParameter)
    assert isinstance(statements[3].iterable, RangeDefinition)


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
    assert [parameter.type_name for parameter in syntax_def.params] == [
        "int[8]",
        "float[32]",
        "qubit",
    ]
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
        "readonly array[int[8], 2]",
        "mutable array[float[32], #dim = 2]",
    ]
    assert [
        [expr.value for expr in parameter.type_expressions]
        for parameter in syntax_params
    ] == [[8, 2], [32, 2]]

    semantic_result = semantic.analyze(source)
    assert not semantic_result.has_errors
    semantic_params = semantic_result.program.statements[0].params
    assert [parameter.name for parameter in semantic_params] == ["a", "b"]
    assert [len(parameter.type_expressions) for parameter in semantic_params] == [2, 2]
    assert all(parameter.symbol.id == parameter.symbol_id for parameter in semantic_params)


def test_syntax_type_fields_use_canonical_schema_and_preserve_expressions() -> None:
    source = """OPENQASM 3.0;
    array[int[8], 2, 3] values;
    const uint[16] count = 1;
    def f(readonly array[float[32], 4] items) -> complex[float[64]] { }
    extern ext(mutable array[angle[20], #dim = 2], bit[7]) -> int[9];
    for uint[5] i in [0:1] { }
    input bit[6] bits;
    int[10] casted = int[11](1);
    """
    result = parser.parse(source)
    assert not result.has_errors

    statements = result.program.statements
    array_decl = statements[0]
    const_decl = statements[1]
    subroutine = statements[2]
    external = statements[3]
    for_loop = statements[4]
    input_decl = statements[5]
    cast_decl = statements[6]
    cast = cast_decl.init_expr

    assert array_decl.type_name == "array[int[8], 2, 3]"
    assert [expr.value for expr in array_decl.type_expressions] == [8, 2, 3]
    assert const_decl.type_name == "uint[16]"
    assert [expr.value for expr in const_decl.type_expressions] == [16]
    assert subroutine.return_type_name == "complex[float[64]]"
    assert [expr.value for expr in subroutine.return_type_expressions] == [64]
    assert external.param_type_names == [
        "mutable array[angle[20], #dim = 2]",
        "bit[7]",
    ]
    assert [expr.value for expr in external.param_type_expressions] == [20, 2, 7]
    assert external.return_type_name == "int[9]"
    assert [expr.value for expr in external.return_type_expressions] == [9]
    assert for_loop.type_name == "uint[5]"
    assert [expr.value for expr in for_loop.type_expressions] == [5]
    assert input_decl.type_name == "bit[6]"
    assert [expr.value for expr in input_decl.type_expressions] == [6]
    assert cast_decl.type_name == "int[10]"
    assert [expr.value for expr in cast_decl.type_expressions] == [10]
    assert cast.type_name == "int[11]"
    assert [expr.value for expr in cast.type_expressions] == [11]


def test_all_exported_classes_resolve_through_declared_module() -> None:
    for public_module in (parser, semantic):
        for name in public_module.__all__:
            exported = getattr(public_module, name)
            if not isinstance(exported, type):
                continue
            declared_module = importlib.import_module(exported.__module__)
            assert getattr(declared_module, exported.__name__) is exported


def test_all_exported_node_classes_have_runtime_documentation() -> None:
    for public_module in (parser, semantic):
        for name in public_module.__all__:
            exported = getattr(public_module, name)
            if isinstance(exported, type) and issubclass(exported, QASMNode):
                assert exported.__doc__ is not None
                assert exported.__doc__.strip()


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
