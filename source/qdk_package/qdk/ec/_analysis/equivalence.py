"""Equivalence between qodec gadgets."""

from __future__ import annotations

from typing import Iterable

import qodec as qc

from .circuit_action import realized_action_of

EncodingSignature = tuple[tuple[int, tuple[int, ...]], ...]


def gadgets_equivalent(left: qc.Gadget, right: qc.Gadget) -> bool:
    return (
        _encoding_signature(left.inputs) == _encoding_signature(right.inputs)
        and _encoding_signature(left.outputs) == _encoding_signature(right.outputs)
        and realized_action_of(left).is_equivalent_to(realized_action_of(right))
    )


def why_not_equivalent(left: qc.Gadget, right: qc.Gadget) -> str:
    left_inputs = _encoding_signature(left.inputs)
    right_inputs = _encoding_signature(right.inputs)
    if left_inputs != right_inputs:
        return f"Input encodings differ: {left_inputs!r} vs {right_inputs!r}."
    left_outputs = _encoding_signature(left.outputs)
    right_outputs = _encoding_signature(right.outputs)
    if left_outputs != right_outputs:
        return f"Output encodings differ: {left_outputs!r} vs {right_outputs!r}."
    left_action = realized_action_of(left)
    right_action = realized_action_of(right)
    if left_action.is_equivalent_to(right_action):
        return ""
    if left_action.is_equivalent_to(right_action, modulo_paulis=True):
        return "Logical actions differ in their outcome-dependent Pauli signs."
    return "Logical actions differ."


def _encoding_signature(
    encodings: Iterable[qc.Encoding],
) -> EncodingSignature:
    return tuple(
        (entry, tuple(int(qubit) for qubit in encoding.support))
        for entry, encoding in enumerate(encodings)
    )


__all__ = [
    "gadgets_equivalent",
    "why_not_equivalent",
]
