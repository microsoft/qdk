"""Lift a gadget's declared instruction into an expected logical action."""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass, field
from typing import Any, cast

import qodec as qc

from .._readouts import flag_slots, observable_slots
from .propagation.pauli import Pauli, PauliCharacter
from .propagation.pauli_remap import (
    encoding_qubit_relocation,
    flat_logical_paulis,
    flat_logical_slots,
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
    inputs = flat_logical_paulis(gadget.inputs)
    output_probes = flat_logical_paulis(gadget.outputs)
    names = [slot.name for slot in observable_slots(gadget)]
    index_by_name = {name: index for index, name in enumerate(names)}
    expected_observables: list[Pauli | None] = [None] * len(names)
    missing_observables: list[str] = []
    missing_flags: list[str] = []
    unsupported: list[str] = []
    bound_flags: list[str] = []
    cliffords: list[Clifford] = []

    bound_flag_slots = len(flag_slots(gadget))
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
                    expected_observables[index_by_name[name]] = _resolve_declared_pauli(
                        observable.pauli, gadget
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
    images = _flat_input_generator_names(gadget.inputs)
    for clifford in clifford_actions:
        images = [
            _apply_clifford_to_pauli_string(image, clifford.generators)
            for image in images
        ]
    return [
        _resolve_declared_pauli(image, gadget) if image.strip() else Pauli({})
        for image in images
    ]


def _flat_input_generator_names(
    encodings: Sequence[qc.Encoding],
) -> list[str]:
    names: list[str] = []
    flat = 0
    for encoding in encodings:
        for _ in range(len(list(encoding.code.x))):
            names.extend((f"X_{flat}", f"Z_{flat}"))
            flat += 1
    return names


def _apply_clifford_to_pauli_string(pauli_str: str, generators: dict[str, str]) -> str:
    return " ".join(
        generators.get(token, token)
        for token in pauli_str.split()
        if generators.get(token, token)
    )


def _resolve_declared_pauli(pauli_str: str, gadget: qc.Gadget) -> Pauli:
    flat_map = flat_logical_slots(list(gadget.inputs) + list(gadget.outputs))
    characters: dict[int, PauliCharacter] = {}
    for token in pauli_str.split():
        basis, _, index_text = token.partition("_")
        flat_index = int(index_text) if index_text else 0
        if flat_index >= len(flat_map):
            raise ValueError(
                f"declared Pauli {pauli_str!r} references flat logical "
                f"qubit {flat_index} beyond the gadget's encodings"
            )
        encoding, local_index = flat_map[flat_index]
        if basis == "X":
            logicals = [list(encoding.code.x)[local_index]]
        elif basis == "Z":
            logicals = [list(encoding.code.z)[local_index]]
        elif basis == "Y":
            logicals = [
                list(encoding.code.x)[local_index],
                list(encoding.code.z)[local_index],
            ]
        else:
            raise ValueError(f"unrecognised basis letter {basis!r}")
        relocation = encoding_qubit_relocation(encoding)
        for logical in logicals:
            for sub_token in str(logical).split():
                sub_basis, _, sub_index = sub_token.partition("_")
                if sub_index:
                    qubit = relocation[int(sub_index)]
                    characters[qubit] = _multiply_basis(
                        characters.get(qubit),
                        cast(PauliCharacter, sub_basis),
                    )
    final: dict[int, PauliCharacter] = {
        qubit: basis for qubit, basis in characters.items() if basis != "I"
    }
    return Pauli(final)


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
