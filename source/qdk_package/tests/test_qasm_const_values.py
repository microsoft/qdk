# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Guards that constant values arrive as structured data, never rendered strings.

Angle and duration constants used to arrive as ``'1.5707963267948966'`` and
``'100.0 ns'``, so a caller had to parse a number back out of a string. Both
``LiteralExpression.value`` and ``const_value`` project through the same native
helper, and between them they reach every literal kind, so these tests pin the
Python type each kind produces on whichever accessor can reach it.
"""

from __future__ import annotations

import math
from typing import Any, List, Optional

import pytest

from qdk.openqasm import semantic


def _analyze(source: str) -> Any:
    result = semantic.analyze(source)
    errors = [d.message for d in result.diagnostics if "Error" in str(d.severity)]
    assert not errors, f"source did not analyze cleanly: {errors}"
    assert result.program is not None
    return result.program


def _literals(source: str) -> List[Any]:
    found: List[Any] = []

    def walk(node: Any) -> None:
        if isinstance(node, semantic.LiteralExpression):
            found.append(node)
        for child in node.children():
            walk(child)

    walk(_analyze(source))
    return found


def _initializer_constant(declaration: str) -> Optional[Any]:
    program = _analyze(f"OPENQASM 3.0;\n{declaration}\n")
    return program.statements[-1].init_expr.const_value


# Kinds observable on a literal node. A `bigint` needs a bare expression
# statement: in a declaration it is typed as an error, because OpenQASM promotion
# rules do not cover integers wider than 64 bits.
_LITERAL_KINDS = [
    ("bool", "OPENQASM 3.0;\ntrue;\n", bool),
    ("int", "OPENQASM 3.0;\n3;\n", int),
    ("bigint", "OPENQASM 3.0;\n340282366920938463463374607431768211455;\n", int),
    ("float", "OPENQASM 3.0;\n1.5;\n", float),
    ("complex", "OPENQASM 3.0;\n2.0im;\n", complex),
    ("bitstring", 'OPENQASM 3.0;\nconst bit[4] v = "1010";\n', str),
    ("duration", "OPENQASM 3.0;\nconst duration v = 100ns;\n", semantic.Duration),
]

# Kinds observable on a const-folded declaration. `angle` appears only here:
# OpenQASM has no angle literal, so an angle constant is always a folded result.
_CONST_KINDS = [
    ("bool", "const bool v = true;", bool),
    ("bit", "const bit v = 1;", bool),
    ("int", "const int[8] v = 3;", int),
    ("float", "const float[64] v = 1.5;", float),
    ("complex", "const complex[float[64]] v = 2.0im;", complex),
    ("bitstring", 'const bit[4] v = "1010";', str),
    ("angle", "const angle v = pi/2;", semantic.Angle),
    ("duration", "const duration v = 100ns;", semantic.Duration),
]


@pytest.mark.parametrize(
    ("source", "expected"),
    [(src, ty) for _, src, ty in _LITERAL_KINDS],
    ids=[name for name, _, _ in _LITERAL_KINDS],
)
def test_each_literal_kind_projects_to_its_documented_type(
    source: str, expected: type
) -> None:
    literals = _literals(source)
    assert any(isinstance(lit.value, expected) for lit in literals), (
        f"no literal projected to {expected.__name__}; "
        f"got {[type(lit.value).__name__ for lit in literals]}"
    )


@pytest.mark.parametrize(
    ("declaration", "expected"),
    [(decl, ty) for _, decl, ty in _CONST_KINDS],
    ids=[name for name, _, _ in _CONST_KINDS],
)
def test_each_folded_constant_has_its_documented_type(
    declaration: str, expected: type
) -> None:
    value = _initializer_constant(declaration)
    assert isinstance(value, expected), f"got {type(value).__name__}: {value!r}"


def test_no_numeric_constant_is_a_string_in_disguise() -> None:
    """A bitstring is legitimately ``str``; every other kind must be data."""
    for name, declaration, _ in _CONST_KINDS:
        if name == "bitstring":
            continue
        value = _initializer_constant(declaration)
        assert not isinstance(value, str), f"{name} folded to the string {value!r}"


def test_an_array_literal_projects_to_none() -> None:
    """Recorded so a future change to array projection is a deliberate one."""
    source = "OPENQASM 3.0;\narray[int[8], 2] v = {1, 2};\n"
    arrays = [lit for lit in _literals(source) if lit.value is None]
    assert arrays, "expected the array literal to project a None value"
    assert arrays[0].elements, "an array literal still exposes its elements"


def test_a_non_constant_expression_has_no_constant_value() -> None:
    program = _analyze("OPENQASM 3.0;\nint[8] v = 1;\nint[8] w = v + 1;\n")
    assert program.statements[-1].init_expr.const_value is None


def test_an_angle_constant_carries_radians_and_its_fixed_point_pair() -> None:
    value = _initializer_constant("const angle v = pi/2;")
    assert math.isclose(value.radians, math.pi / 2, rel_tol=1e-12)
    # The analyzer folds at full float precision regardless of the declared width.
    assert value.size == 53
    assert value.value == 1 << (value.size - 2)


def test_the_declared_angle_width_is_on_the_type_not_the_value() -> None:
    program = _analyze("OPENQASM 3.0;\nconst angle[4] v = pi/2;\n")
    initializer = program.statements[-1].init_expr
    assert initializer.ty.size == 4
    assert initializer.const_value.size == 53


def test_angle_radians_are_computed_from_the_fixed_point_pair() -> None:
    assert math.isclose(semantic.Angle(1, 2).radians, math.pi / 2, rel_tol=1e-12)
    assert math.isclose(semantic.Angle(2, 2).radians, math.pi, rel_tol=1e-12)
    assert semantic.Angle(0, 4).radians == 0.0


def test_angle_radians_stay_within_one_turn() -> None:
    for size in (1, 2, 4, 8, 53):
        largest = semantic.Angle((1 << size) - 1, size)
        assert 0.0 <= largest.radians < 2 * math.pi


@pytest.mark.parametrize(
    ("literal", "expected_value", "expected_unit"),
    [
        ("100ns", 100.0, semantic.TimeUnit.NS),
        ("1.5s", 1.5, semantic.TimeUnit.S),
        ("2ms", 2.0, semantic.TimeUnit.MS),
        ("3us", 3.0, semantic.TimeUnit.US),
        ("10dt", 10.0, semantic.TimeUnit.DT),
    ],
)
def test_a_duration_keeps_the_unit_it_was_written_in(
    literal: str, expected_value: float, expected_unit: Any
) -> None:
    value = _initializer_constant(f"const duration v = {literal};")
    assert value.value == expected_value
    assert value.unit is expected_unit


def test_the_value_classes_are_frozen() -> None:
    with pytest.raises(AttributeError):
        semantic.Angle(1, 4).value = 2  # type: ignore[misc]
    with pytest.raises(AttributeError):
        semantic.Duration(1.0, semantic.TimeUnit.NS).value = 2.0  # type: ignore[misc]


def test_the_value_classes_render_python_spellings() -> None:
    assert repr(semantic.Duration(100.0, semantic.TimeUnit.NS)) == (
        "Duration(value=100.0, unit=TimeUnit.NS)"
    )
    assert repr(semantic.Angle(1, 2)) == (
        "Angle(value=1, size=2, radians=1.5707963267948966)"
    )
