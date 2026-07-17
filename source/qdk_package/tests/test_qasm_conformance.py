# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from dataclasses import fields, is_dataclass
from enum import Enum
from typing import Any

import openqasm3
import pytest

from qdk.openqasm import parser


def _normalize_ast(value: Any) -> Any:
    if isinstance(value, Enum):
        return type(value).__name__, value.name
    if is_dataclass(value):
        return (
            type(value).__name__,
            tuple(
                (field.name, _normalize_ast(getattr(value, field.name)))
                for field in fields(value)
                if field.name != "span"
            ),
        )
    if isinstance(value, list):
        return tuple(_normalize_ast(item) for item in value)
    return value


def _reference_initializer(source: str) -> Any:
    program = openqasm3.parse(source)
    declaration = program.statements[0]
    return declaration.init_expression


@pytest.mark.parametrize(
    "expression",
    [
        "1 | 2 ^ 3",
        "1 ^ 2 | 3",
        "(1 | 2) ^ 3",
        "1 ^ (2 | 3)",
        "1 ^ 2 & 3",
        "1 & 2 ^ 3",
        "(1 ^ 2) & 3",
        "1 & (2 ^ 3)",
    ],
)
def test_canonical_bitwise_precedence_matches_reference_parser(expression: str) -> None:
    assert openqasm3.__version__ == "1.0.1"
    source = f"OPENQASM 3.0; int value = {expression};"
    parsed = parser.parse(source)

    assert not parsed.has_errors
    canonical = parser.dumps(parsed.program)
    assert _normalize_ast(_reference_initializer(canonical)) == _normalize_ast(
        _reference_initializer(source)
    )
