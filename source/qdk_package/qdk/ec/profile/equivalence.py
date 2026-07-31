"""Logical-action equivalence between qodec gadgets."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Iterable

import qodec

from .._qodec_compat import observables_as_xor_map, realization
from .propagation.interpreter import propagate_input_paulis
from .propagation.pauli_remap import flat_logical_paulis

EncodingSignature = tuple[tuple[str, tuple[int, ...]], ...]


@dataclass(frozen=True)
class LogicalImage:
    output_logical_flips: frozenset[int]
    observable_flips: frozenset[int]


@dataclass(frozen=True)
class LogicalAction:
    encoding_in: EncodingSignature
    encoding_out: EncodingSignature
    images: tuple[LogicalImage, ...]


def logical_action_of(gadget: qodec.Gadget) -> LogicalAction:
    channel = realization(gadget)
    inputs = flat_logical_paulis(channel.encoding_in)
    probes = flat_logical_paulis(channel.encoding_out)
    if not inputs:
        return LogicalAction(
            _encoding_signature(channel.encoding_in),
            _encoding_signature(channel.encoding_out),
            (),
        )
    deltas, hidden_count, outcome_count = propagate_input_paulis(
        channel, inputs, residual_probes=probes
    )
    observables = list(observables_as_xor_map(gadget).values())
    probe_offset = hidden_count + outcome_count
    images = []
    for shot in range(len(inputs)):
        outcome_flips = {
            outcome
            for outcome in range(outcome_count)
            if deltas[hidden_count + outcome, shot]
        }
        images.append(
            LogicalImage(
                frozenset(
                    index
                    for index in range(len(probes))
                    if deltas[probe_offset + index, shot]
                ),
                frozenset(
                    index
                    for index, positions in enumerate(observables)
                    if sum(position in outcome_flips for position in positions) % 2
                ),
            )
        )
    return LogicalAction(
        _encoding_signature(channel.encoding_in),
        _encoding_signature(channel.encoding_out),
        tuple(images),
    )


def gadgets_equivalent(left: qodec.Gadget, right: qodec.Gadget) -> bool:
    return logical_action_of(left) == logical_action_of(right)


def why_not_equivalent(left: qodec.Gadget, right: qodec.Gadget) -> str:
    left_action = logical_action_of(left)
    right_action = logical_action_of(right)
    if left_action.encoding_in != right_action.encoding_in:
        return (
            f"Input encodings differ: {left_action.encoding_in!r} vs "
            f"{right_action.encoding_in!r}."
        )
    if left_action.encoding_out != right_action.encoding_out:
        return (
            f"Output encodings differ: {left_action.encoding_out!r} vs "
            f"{right_action.encoding_out!r}."
        )
    for index, (left_image, right_image) in enumerate(
        zip(left_action.images, right_action.images)
    ):
        if left_image != right_image:
            return (
                f"Image of input logical Pauli {index} differs: "
                f"{left_image!r} vs {right_image!r}."
            )
    if len(left_action.images) != len(right_action.images):
        return (
            f"Input logical-basis size differs: {len(left_action.images)} vs "
            f"{len(right_action.images)}."
        )
    return ""


def _encoding_signature(
    encodings: Iterable[qodec.gadgets.Encoding],
) -> EncodingSignature:
    return tuple(
        (encoding.operand, tuple(int(qubit) for qubit in encoding.support))
        for encoding in encodings
    )


__all__ = [
    "LogicalAction",
    "LogicalImage",
    "gadgets_equivalent",
    "logical_action_of",
    "why_not_equivalent",
]
