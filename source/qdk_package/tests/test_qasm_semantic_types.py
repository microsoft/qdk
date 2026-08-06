# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Guards that resolved types are structured nodes, not a name and a flat list.

Analysis used to collapse a resolved type into ``Type.name`` and hang the widths
and dimensions off declarations as an unlabeled ``ty_exprs`` list, so
``array[int[8], 2, 3]`` arrived as ``[8, 2, 3]`` with no way to tell the element
width from the dimensions. These tests pin the concrete node for every resolved
variant and assert that structure is reachable without parsing ``name``.
"""

from __future__ import annotations

from typing import Any, List

import pytest

from qdk.openqasm import parser, semantic

# Every resolved `Type` variant, and the source construct that produces it.
_DECLARED_TYPES = [
    ("int", "int[8] v;", "IntType"),
    ("uint", "uint[16] v;", "UintType"),
    ("float", "float[64] v;", "FloatType"),
    ("angle", "angle[32] v;", "AngleType"),
    ("complex", "complex[float[64]] v;", "ComplexType"),
    ("bit", "bit v;", "BitType"),
    ("bool", "bool v;", "BoolType"),
    ("duration", "duration v;", "DurationType"),
    ("stretch", "stretch v;", "StretchType"),
    ("bit_array", "bit[4] v;", "BitArrayType"),
    ("array", "array[int[8], 2] v;", "ArrayType"),
]


def _analyze(source: str) -> Any:
    result = semantic.analyze(source)
    errors = [d.message for d in result.diagnostics if "Error" in str(d.severity)]
    assert not errors, f"source did not analyze cleanly: {errors}"
    assert result.program is not None
    return result.program


def _declared_type(declaration: str) -> Any:
    program = _analyze(f"OPENQASM 3.0;\n{declaration}\n")
    return program.statements[-1].type


def _concrete_type_classes() -> List[type]:
    return [
        value
        for name in semantic.__all__
        if isinstance(value := getattr(semantic, name), type)
        and issubclass(value, semantic.Type)
        and value is not semantic.Type
    ]


# Every resolved variant is now produced. The corpus below must reach all of
# them, so this map is empty; it stays as the mechanism for recording a variant
# that loses its producer.
_UNREACHABLE: dict[str, str] = {}

# Sources that between them reach every type node.
_COVERAGE_CORPUS = [
    "OPENQASM 3.0;\nint[8] a;\nuint[16] b;\nfloat[64] c;\nangle[32] d;\n",
    "OPENQASM 3.0;\ncomplex[float[64]] z;\nbool f;\nbit b;\nbit[4] r;\n",
    "OPENQASM 3.0;\nduration d = 10ns;\nstretch s;\n",
    "OPENQASM 3.0;\nqubit q;\nqubit[2] r;\n",
    "OPENQASM 3.0;\narray[int[8], 2, 3] v;\n",
    "OPENQASM 3.0;\ndef f(readonly array[int[8], 2] a, "
    "mutable array[float[32], #dim = 3] b) {\n}\n",
    "OPENQASM 3.0;\ngate g(theta) a, b {\n}\n",
    "OPENQASM 3.0;\ndef nothing() {\n}\n",
    "OPENQASM 3.0;\nextern ext(int[8]) -> int[9];\n",
    "OPENQASM 3.0;\nint[8] v = undefined_name;\n",
    # The three constructs whose types had no producer before.
    "OPENQASM 3.0;\nbit[4] b;\nbit[2] c = b[0:1];\n",
    "OPENQASM 3.0;\nfor int[8] i in {1, 2} {\n}\n",
    'OPENQASM 3.0;\ninclude "stdgates.inc";\nh $0;\n',
]


def _collect_types(node: Any, seen: set[str]) -> None:
    for attribute in ("ty", "type", "return_type", "base_type"):
        value = getattr(node, attribute, None)
        if isinstance(value, semantic.Type):
            seen.add(type(value).__name__)
            _collect_types(value, seen)
    for parameter in getattr(node, "parameter_types", None) or ():
        seen.add(type(parameter).__name__)
        _collect_types(parameter, seen)
    for child in node.children():
        _collect_types(child, seen)


def _reached_by_corpus() -> set[str]:
    reached: set[str] = set()
    for source in _COVERAGE_CORPUS:
        result = semantic.analyze(source)
        if result.program is not None:
            _collect_types(result.program, reached)
        for symbol in result.symbols:
            reached.add(type(symbol.ty).__name__)
    return reached


def _measurement_operand(source: str) -> Any:
    found: List[Any] = []

    def walk(node: Any) -> None:
        if type(node).__name__ == "QuantumMeasurement":
            found.append(node.children()[0])
        for child in node.children():
            walk(child)

    walk(_analyze(source))
    assert len(found) == 1
    return found[0]


def _index(source: str) -> Any:
    found: List[Any] = []

    def walk(node: Any) -> None:
        if type(node).__name__ == "IndexExpression":
            found.extend(node.children()[1:])
        for child in node.children():
            walk(child)

    walk(_analyze(source))
    assert len(found) == 1
    return found[0]


@pytest.mark.parametrize(
    ("declaration", "expected"),
    [(decl, cls) for _, decl, cls in _DECLARED_TYPES],
    ids=[name for name, _, _ in _DECLARED_TYPES],
)
def test_each_declared_type_resolves_to_its_node(declaration: str, expected: str) -> None:
    assert type(_declared_type(declaration)).__name__ == expected


def test_a_resolved_type_is_not_a_syntax_node() -> None:
    """Resolved types carry no source position, so they are deliberately not nodes."""
    resolved = _declared_type("int[8] v;")
    assert isinstance(resolved, semantic.Type)
    assert not isinstance(resolved, parser.QASMNode)
    assert not hasattr(resolved, "span")


def test_widths_are_reachable_without_reading_the_name() -> None:
    assert _declared_type("int[8] v;").size == 8
    assert _declared_type("int v;").size is None
    assert _declared_type("bit[4] v;").size == 4
    assert _declared_type("complex[float[64]] v;").size == 64


def test_an_array_separates_its_element_type_from_its_dimensions() -> None:
    """The defect the flat `ty_exprs` list could not express."""
    resolved = _declared_type("array[int[8], 2, 3] v;")
    assert type(resolved).__name__ == "ArrayType"
    assert type(resolved.base_type).__name__ == "IntType"
    assert resolved.base_type.size == 8
    assert resolved.dimensions == [2, 3]


def test_array_references_report_their_mutability_and_shape() -> None:
    program = _analyze(
        "OPENQASM 3.0;\n"
        "def f(readonly array[int[8], 2] a, mutable array[float[32], #dim = 3] b) {\n}\n"
    )
    static, dynamic = (parameter.type for parameter in program.statements[-1].params)

    assert type(static).__name__ == "StaticArrayReferenceType"
    assert static.dimensions == [2]
    assert static.mutability is parser.AccessControl.READONLY

    assert type(dynamic).__name__ == "DynArrayReferenceType"
    assert dynamic.num_dimensions == 3
    assert dynamic.mutability is parser.AccessControl.MUTABLE
    assert type(dynamic.base_type).__name__ == "FloatType"


def test_a_subroutine_reports_a_structured_signature() -> None:
    program = _analyze("OPENQASM 3.0;\ndef f(int[8] n) -> uint[16] {\n  return 1;\n}\n")
    definition = program.statements[-1]
    assert type(definition.return_type).__name__ == "UintType"
    assert definition.return_type.size == 16
    assert type(definition.params[0].type).__name__ == "IntType"


def test_a_subroutine_without_a_return_type_reports_void() -> None:
    program = _analyze("OPENQASM 3.0;\ndef f() {\n}\n")
    assert type(program.statements[-1].return_type).__name__ == "VoidType"


def test_an_extern_reports_its_whole_signature_as_a_function_type() -> None:
    program = _analyze("OPENQASM 3.0;\nextern ext(int[8], bit[4]) -> int[9];\n")
    signature = program.statements[-1].type
    assert type(signature).__name__ == "FunctionType"
    assert [type(t).__name__ for t in signature.parameter_types] == [
        "IntType",
        "BitArrayType",
    ]
    assert signature.return_type.size == 9


def test_quantum_and_gate_types_resolve() -> None:
    """Quantum and gate types reach Python through the symbol table, not statements."""
    result = semantic.analyze("OPENQASM 3.0;\nqubit q;\nqubit[2] r;\ngate g(theta) a, b {\n}\n")
    by_name = {symbol.name: symbol for symbol in result.symbols}

    assert type(by_name["q"].ty).__name__ == "QubitType"
    assert type(by_name["r"].ty).__name__ == "QubitArrayType"
    assert by_name["r"].ty.size == 2

    gate = by_name["g"].ty
    assert type(gate).__name__ == "GateType"
    assert gate.num_classical_args == 1
    assert gate.num_qubit_args == 2


def test_the_loop_variable_type_resolves() -> None:
    loop = _analyze("OPENQASM 3.0;\nfor uint[5] i in [0:3] {\n}\n").statements[-1]
    assert type(loop.type).__name__ == "UintType"
    assert loop.type.size == 5


def test_an_unresolvable_type_is_an_error_type() -> None:
    result = semantic.analyze("OPENQASM 3.0;\nint[8] v = undefined_name;\n")
    initializer = result.program.statements[-1].init_expr
    assert type(initializer.ty).__name__ == "ErrorType"


def test_expression_types_dispatch_by_isinstance() -> None:
    """P05's point: `expr.ty` is a node you can branch on, not a string to parse."""
    program = _analyze("OPENQASM 3.0;\nconst int[8] a = 3;\nconst float[64] b = 1.5;\n")
    types = [statement.init_expr.ty for statement in program.statements]
    assert isinstance(types[0], semantic.IntType)
    assert isinstance(types[1], semantic.FloatType)
    assert all(isinstance(t, semantic.Type) for t in types)


def test_the_name_rendering_is_still_available() -> None:
    resolved = _declared_type("array[int[8], 2] v;")
    assert resolved.name == "array[int[8], 2]"
    assert str(resolved) == "array[int[8], 2]"


def test_type_nodes_render_python_spellings() -> None:
    assert repr(_declared_type("int[8] v;")) == "IntType(size=8)"
    assert repr(_declared_type("bool v;")) == "BoolType()"
    assert repr(_declared_type("array[int[8], 2] v;")) == (
        "ArrayType(base_type=IntType(size=8), dimensions=[2])"
    )


def test_no_public_accessor_named_ty_exprs_remains() -> None:
    """The flat expression lists the type nodes replace."""
    offenders = []
    for name in semantic.__all__:
        cls = getattr(semantic, name)
        if not isinstance(cls, type):
            continue
        for accessor in ("ty_exprs", "type_expressions", "return_types"):
            if hasattr(cls, accessor):
                offenders.append(f"semantic.{name}.{accessor}")
    assert not offenders, "flat type-expression accessors remain:\n" + "\n".join(offenders)


def test_every_resolved_variant_has_a_producer() -> None:
    """A type node that loses its producer must be noticed."""
    concrete = _concrete_type_classes()
    assert len(concrete) == 22, f"expected 22 resolved type nodes, found {len(concrete)}"

    reached = _reached_by_corpus()
    expected = {cls.__name__ for cls in concrete} - set(_UNREACHABLE)
    missing = sorted(expected - reached)
    assert not missing, (
        "these resolved type nodes are not produced by any corpus source. "
        "Either add a source, or record them in _UNREACHABLE with a reason:\n"
        + "\n".join(missing)
    )


def test_the_unreachable_list_stays_honest() -> None:
    """An exempted type that gains a producer should lose its exemption."""
    now_reachable = sorted(set(_UNREACHABLE) & _reached_by_corpus())
    assert not now_reachable, (
        "these types are exempted but the corpus now reaches them; "
        "drop them from _UNREACHABLE:\n" + "\n".join(now_reachable)
    )


def test_an_operand_reports_a_type_however_the_qubit_was_written() -> None:
    """A declared qubit and a hardware qubit answer `ty` in the same position."""
    declared = _measurement_operand("OPENQASM 3.0;\nqubit q;\nbit c;\nc = measure q;\n")
    hardware = _measurement_operand("OPENQASM 3.0;\nbit c;\nc = measure $0;\n")

    assert isinstance(declared.ty, semantic.QubitType)
    assert isinstance(hardware.ty, semantic.HardwareQubitType)
    assert isinstance(hardware, semantic.SemanticExpression)
    # A hardware qubit is a physical reference, so it resolves to no declaration.
    assert hardware.const_value is None
    assert hardware.symbol is None


def test_an_index_reports_a_type_however_it_was_written() -> None:
    """An integer index and a range index answer `ty` in the same position."""
    integer = _index("OPENQASM 3.0;\nbit[4] b;\nbit c = b[1];\n")
    ranged = _index("OPENQASM 3.0;\nbit[4] b;\nbit[2] c = b[0:1];\n")

    assert isinstance(integer.ty, semantic.IntType)
    assert isinstance(ranged.ty, semantic.RangeType)


def test_a_discrete_set_reports_a_set_type() -> None:
    iterable = _analyze("OPENQASM 3.0;\nfor int[8] i in {1, 2} {\n}\n").statements[-1].iterable
    assert isinstance(iterable.ty, semantic.SetType)


def test_a_for_loop_range_reports_a_range_type() -> None:
    iterable = _analyze("OPENQASM 3.0;\nfor int[8] i in [0:3] {\n}\n").statements[-1].iterable
    assert isinstance(iterable.ty, semantic.RangeType)
