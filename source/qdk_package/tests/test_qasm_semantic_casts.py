# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Guards the semantic layer's cast expressions.

Analysis inserts casts the source never wrote, so a caller reading a semantic
tree sees `Cast` nodes with no syntactic counterpart. These tests pin when a
cast appears, how an inserted cast is distinguished from a written one, and
where its target type is reachable.
"""

from __future__ import annotations

from typing import Any, List

import pytest

from qdk.openqasm import parser, semantic


def _analyze(source: str) -> Any:
    result = semantic.analyze(source)
    errors = [d.message for d in result.diagnostics if "Error" in str(d.severity)]
    assert not errors, f"source did not analyze cleanly: {errors}"
    assert result.program is not None
    return result.program


def _casts(source: str) -> List[Any]:
    found: List[Any] = []

    def walk(node: Any) -> None:
        if isinstance(node, semantic.Cast):
            found.append(node)
        for child in node.children():
            walk(child)

    walk(_analyze(source))
    return found


def _only_cast(source: str) -> Any:
    casts = _casts(source)
    assert len(casts) == 1, f"expected exactly one cast, got {len(casts)}"
    return casts[0]


def test_a_written_cast_is_explicit() -> None:
    cast = _only_cast("OPENQASM 3.0;\nint[8] i = 3;\nfloat[64] f = float[64](i);\n")
    assert cast.kind is semantic.CastKind.EXPLICIT
    assert type(cast.operand).__name__ == "Identifier"


def test_a_widening_assignment_inserts_an_implicit_cast() -> None:
    cast = _only_cast("OPENQASM 3.0;\nint[8] i = 3;\nint[16] j = i;\n")
    assert cast.kind is semantic.CastKind.IMPLICIT
    assert cast.ty.size == 16


def test_an_assignment_that_needs_no_conversion_inserts_no_cast() -> None:
    assert _casts("OPENQASM 3.0;\nint[8] i = 3;\nint[8] j = i;\n") == []


def test_a_condition_inserts_an_implicit_bool_cast() -> None:
    cast = _only_cast("OPENQASM 3.0;\nbit c;\nif (c) {\n}\n")
    assert cast.kind is semantic.CastKind.IMPLICIT
    assert isinstance(cast.ty, semantic.BoolType)


def test_a_measurement_is_cast_to_the_declared_type() -> None:
    cast = _only_cast("OPENQASM 3.0;\nqubit q;\nint[8] i = measure q;\n")
    assert cast.kind is semantic.CastKind.IMPLICIT
    assert type(cast.operand).__name__ == "QuantumMeasurement"
    assert cast.ty.size == 8


def test_binary_promotion_casts_only_the_narrower_operand() -> None:
    source = (
        "OPENQASM 3.0;\nint[8] a = 1;\nfloat[64] b = 2.0;\nfloat[64] c = a + b;\n"
    )
    cast = _only_cast(source)
    assert cast.kind is semantic.CastKind.IMPLICIT
    assert isinstance(cast.ty, semantic.FloatType)
    assert cast.operand.name == "a"


@pytest.mark.parametrize(
    ("declaration", "expected_type"),
    [
        ("float[64] f = float[64](i);", "FloatType"),
        ("bit[8] b = bit[8](i);", "BitArrayType"),
        ("bool t = bool(i);", "BoolType"),
        ("uint[8] u = uint[8](i);", "UintType"),
    ],
)
def test_an_explicit_cast_reports_its_target_type(
    declaration: str, expected_type: str
) -> None:
    cast = _only_cast(f"OPENQASM 3.0;\nint[8] i = 1;\n{declaration}\n")
    assert type(cast.ty).__name__ == expected_type


def test_casts_nest_through_their_operand() -> None:
    casts = _casts(
        "OPENQASM 3.0;\nint[8] i = 3;\nfloat[64] f = float[64](int[16](i));\n"
    )
    assert len(casts) == 2
    outer, inner = casts
    assert isinstance(outer.ty, semantic.FloatType)
    assert isinstance(inner.ty, semantic.IntType)
    assert inner.ty.size == 16
    assert type(outer.operand).__name__ == "Cast"
    assert type(inner.operand).__name__ == "Identifier"


def test_a_cast_folds_a_constant_operand() -> None:
    cast = _only_cast(
        "OPENQASM 3.0;\nconst int[8] i = 3;\nconst float[64] f = float[64](i);\n"
    )
    assert cast.const_value == 3.0
    assert cast.ty.is_const is True


def test_a_cast_over_a_runtime_value_has_no_constant() -> None:
    cast = _only_cast("OPENQASM 3.0;\nint[8] i = 3;\nfloat[64] f = float[64](i);\n")
    assert cast.const_value is None


def test_an_explicit_cast_spans_the_written_form() -> None:
    source = "OPENQASM 3.0;\nint[8] i = 3;\nfloat[64] f = float[64](i);\n"
    cast = _only_cast(source)
    assert source[cast.span.lo : cast.span.hi] == "float[64](i)"


def test_an_inserted_cast_borrows_its_operand_position() -> None:
    """An implicit cast has no source text of its own to point at."""
    source = "OPENQASM 3.0;\nint[8] i = 3;\nint[16] j = i;\n"
    cast = _only_cast(source)
    assert cast.span == cast.operand.span
    assert source[cast.span.lo : cast.span.hi] == "i"


def test_a_cast_does_not_traverse_into_its_target_type() -> None:
    """The semantic target type is a resolved type, not a child node.

    The syntax layer does traverse into its type node, so this is the one place
    the two layers deliberately disagree.
    """
    source = "OPENQASM 3.0;\nint[8] i = 3;\nfloat[64] f = float[64](i);\n"
    semantic_cast = _only_cast(source)
    assert [type(c).__name__ for c in semantic_cast.children()] == ["Identifier"]
    assert not isinstance(semantic_cast.ty, parser.QASMNode)

    syntax_cast = parser.parse(source).program.statements[-1].init_expr
    assert [type(c).__name__ for c in syntax_cast.children()] == [
        "FloatType",
        "Identifier",
    ]


def test_an_impossible_cast_reports_a_diagnostic_and_builds_no_cast() -> None:
    result = semantic.analyze("OPENQASM 3.0;\nqubit q;\nint[8] i = int[8](q);\n")
    messages = [d.message for d in result.diagnostics if "Error" in str(d.severity)]
    assert any("cannot cast expression of type qubit" in m for m in messages)

    found = []

    def walk(node: Any) -> None:
        if isinstance(node, semantic.Cast):
            found.append(node)
        for child in node.children():
            walk(child)

    walk(result.program)
    assert found == []


def test_a_cast_renders_an_informative_repr() -> None:
    cast = _only_cast("OPENQASM 3.0;\nint[8] i = 3;\nfloat[64] f = float[64](i);\n")
    # The repr carries no symbol-table index, so it is fully determined by the
    # source rather than by how many builtins happen to precede the operand.
    assert repr(cast) == "Cast(operand=Identifier(name='i'), kind=CastKind.EXPLICIT)"
