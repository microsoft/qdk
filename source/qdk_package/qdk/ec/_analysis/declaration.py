"""Lift a gadget's declared instruction into an expected logical action."""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass, field
from typing import Any, cast

import qodec as qc

from .._readouts import readouts_of
from .propagation.pauli import Pauli, PauliCharacter, characters_of_string
from .propagation.pauli_remap import (
    declared_pauli_of,
    flat_logical_paulis,
    logical_pauli_of,
)
from .equivalence import LogicalAction, LogicalImage, _encoding_signature


@dataclass(frozen=True)
class DeclarationLift:
    expected: LogicalAction | None
    missing_observables: tuple[str, ...] = field(default_factory=tuple)
    missing_flags: tuple[str, ...] = field(default_factory=tuple)
    unsupported_atoms: tuple[str, ...] = field(default_factory=tuple)
    bound_flags: tuple[str, ...] = field(default_factory=tuple)


def lift_declaration(gadget: qc.Gadget) -> DeclarationLift:
    from qodec.actions import Clifford, Observe, Pauli as PauliAction, Stabilize

    instruction = gadget.implements
    readouts = readouts_of(gadget)
    inputs = flat_logical_paulis(gadget.inputs)
    output_probes = flat_logical_paulis(gadget.outputs)
    names = [slot.name for slot in readouts.observables]
    index_by_name = {name: index for index, name in enumerate(names)}
    expected_observables: list[Pauli | None] = [None] * len(names)
    missing_observables: list[str] = []
    missing_flags: list[str] = []
    unsupported: list[str] = []
    bound_flags: list[str] = []
    cliffords: list[Clifford] = []

    bound_flag_slots = len(readouts.flags)
    for index, flag_name in enumerate(instruction.flags):
        (bound_flags if index < bound_flag_slots else missing_flags).append(flag_name)

    observe_position = 0
    for action in instruction.action:
        if isinstance(action, Stabilize):
            continue
        if isinstance(action, PauliAction):
            if action.condition is not None:
                unsupported.append(type(action).__name__)
            continue
        if isinstance(action, Clifford):
            if action.condition is not None:
                unsupported.append(type(action).__name__)
            else:
                cliffords.append(action)
            continue
        if isinstance(action, Observe):
            for observable in action.observables:
                name = str(observe_position)
                observe_position += 1
                if name not in index_by_name:
                    missing_observables.append(name)
                else:
                    expected_observables[index_by_name[name]] = declared_pauli_of(
                        list(gadget.inputs) + list(gadget.outputs), observable.pauli
                    )
            continue
        unsupported.append(type(action).__name__)

    if missing_observables or missing_flags or unsupported:
        return DeclarationLift(
            None,
            tuple(missing_observables),
            tuple(missing_flags),
            tuple(unsupported),
            tuple(bound_flags),
        )

    image_paulis = _expected_image_paulis(
        inputs=inputs,
        clifford_actions=cliffords,
        gadget=gadget,
    )
    images = []
    for image in image_paulis:
        images.append(
            LogicalImage(
                frozenset(
                    index
                    for index, probe in enumerate(output_probes)
                    if not image.commutes_with(probe)
                ),
                frozenset(
                    index
                    for index, expected in enumerate(expected_observables)
                    if expected is not None and not image.commutes_with(expected)
                ),
            )
        )
    return DeclarationLift(
        LogicalAction(
            _encoding_signature(gadget.inputs),
            _encoding_signature(gadget.outputs),
            tuple(images),
        ),
        bound_flags=tuple(bound_flags),
    )


def _expected_image_paulis(
    *,
    inputs: list[Pauli],
    clifford_actions: list[Any],
    gadget: qc.Gadget,
) -> list[Pauli]:
    if not clifford_actions:
        return list(inputs)
    encodings = list(gadget.inputs) + list(gadget.outputs)
    images = _flat_input_generators(gadget.inputs)
    for clifford in clifford_actions:
        images = [_clifford_image(image, clifford.generators) for image in images]
    return [
        logical_pauli_of(encodings, [(basis, qubit) for qubit, basis in image.items()])
        for image in images
    ]


def _flat_input_generators(
    encodings: Sequence[qc.Encoding],
) -> list[dict[int, PauliCharacter]]:
    """One ``{flat logical qubit: basis}`` per input generator, X then Z."""
    count = sum(len(list(encoding.code.x)) for encoding in encodings)
    return [
        {flat: cast(PauliCharacter, basis)}
        for flat in range(count)
        for basis in ("X", "Z")
    ]


def _clifford_image(
    logical: dict[int, PauliCharacter], generators: dict[str, str]
) -> dict[int, PauliCharacter]:
    """Image of a flat-logical Pauli under one declared Clifford, ignoring phase.

    A Clifford maps a product to the product of its factors' images, so each
    ``X``/``Z`` factor is looked up on its own and the results multiplied. A
    generator the Clifford does not name is fixed.
    """
    image: dict[int, PauliCharacter] = {}
    for qubit, basis in logical.items():
        for factor in ("X", "Z") if basis == "Y" else (basis,):
            name = f"{factor}_{qubit}"
            for target, mapped in characters_of_string(
                generators.get(name, name)
            ).items():
                image[target] = _multiply_basis(image.get(target), mapped)
    return {qubit: basis for qubit, basis in image.items() if basis != "I"}


def _multiply_basis(
    left: PauliCharacter | None, right: PauliCharacter
) -> PauliCharacter:
    if left is None or left == "I":
        return right
    if right == "I":
        return left
    if left == right:
        return "I"
    return next(item for item in ("X", "Y", "Z") if item not in (left, right))


__all__ = ["DeclarationLift", "lift_declaration"]
