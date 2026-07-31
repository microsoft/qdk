"""Helpers for working with the typed Python operand values that
:class:`qodec.instructions.InstructionCall` now carries.

The Rust IR's :class:`qodec::ir::Operand` enum maps to Python primitives:

- ``Qubit(usize)`` / ``Integer(i64)`` → :class:`int`
- ``QubitList(Vec<usize>)`` → :class:`list[int]`
- ``Number(f64)`` → :class:`float`
- ``Text(String)`` → :class:`str`
- ``StringList(Vec<String>)`` → :class:`list[str]`

Errata's compilers and analysis code historically processed every operand
value as a whitespace-separated string; this module bridges the typed
world to that string-token contract without forcing every call site to
duplicate the type-dispatch logic.
"""

from __future__ import annotations

from typing import Any


def value_tokens(value: Any) -> list[str]:
    """Return a list of string tokens for an :class:`InstructionCall` operand value.

    A single :class:`int` / :class:`float` becomes a one-element list
    of its string repr; :class:`list` becomes the per-element string
    repr; :class:`str` is split on whitespace; anything else falls back
    to its single-string repr.
    """
    if isinstance(value, str):
        return value.split()
    if isinstance(value, list):
        return [str(item) for item in value]
    if isinstance(value, (int, float)):
        return [str(value)]
    return [str(value)]


def value_to_string(value: Any) -> str:
    """Render an operand value as a single whitespace-joined string.

    The inverse of :func:`value_tokens` modulo whitespace normalization.
    Useful for compilers (`relocate`, `recursive_lowering`) that emit
    string-valued :class:`InstructionCall` outputs.
    """
    if isinstance(value, str):
        return value
    if isinstance(value, list):
        return " ".join(str(item) for item in value)
    return str(value)


__all__ = ["value_to_string", "value_tokens"]
