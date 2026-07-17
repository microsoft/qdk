# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Lossless raw tokenization for OpenQASM source.

The eager token stream preserves exact text, trivia, unknown characters, and
unterminated strings and bitstrings. Spans are source-local, half-open UTF-8
byte ranges. This is a raw lexer surface, not parser tokens or a concrete
syntax tree.

``RawTokenKind`` and ``RawToken`` use value equality and representation and are
hashable. Token ``detail`` values remain preview data and may change from
conformance findings without changing the coarse token categories.
"""

from dataclasses import dataclass
from enum import Enum

from .._native import (  # type: ignore
    RawToken as _NativeRawToken,
    Span,
    qasm_tokenize as _qasm_tokenize,
)


class RawTokenKind(str, Enum):
    """A stable, coarse category for a lossless OpenQASM token."""

    BITSTRING = "bitstring"
    COMMENT = "comment"
    HARDWARE_QUBIT = "hardware-qubit"
    IDENTIFIER = "identifier"
    LITERAL_FRAGMENT = "literal-fragment"
    NEWLINE = "newline"
    NUMBER = "number"
    PUNCTUATION = "punctuation"
    STRING = "string"
    UNKNOWN = "unknown"
    WHITESPACE = "whitespace"


@dataclass(frozen=True, slots=True)
class RawToken:
    """One frozen lossless token with a source-local UTF-8 byte span."""

    kind: RawTokenKind
    span: Span
    text: str
    is_trivia: bool
    detail: str | None
    is_complete: bool


def tokenize(source: str, /) -> list[RawToken]:
    """Eagerly tokenize source without parsing or resolving includes."""
    return [_from_native(token) for token in _qasm_tokenize(source)]


def _from_native(token: _NativeRawToken) -> RawToken:
    return RawToken(
        kind=RawTokenKind(token.kind.value),
        span=token.span,
        text=token.text,
        is_trivia=token.is_trivia,
        detail=token.detail,
        is_complete=token.is_complete,
    )

__all__ = ["tokenize", "RawToken", "RawTokenKind"]
