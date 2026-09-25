"""Remap ISA action operators onto a program's concrete qubits."""

from __future__ import annotations

from typing import Mapping, TypeVar

from paulimer import DensePauli

from .pauli import Pauli, parse_term, relabel

_PauliKey = TypeVar("_PauliKey", bound=str)


def remap_pauli(pauli_str: str, qubit_map: Mapping[int, int]) -> Pauli:
    """The Pauli ``pauli_str`` names, each term placed through ``qubit_map``."""
    operator = Pauli(pauli_str)
    placement = {qubit: qubit_map[qubit] for qubit in operator.support}
    return relabel(operator, placement)


def build_clifford_images(
    generators: Mapping[_PauliKey, str],
    qubit_map: dict[int, int],
    local_map: dict[int, int],
    qubit_count: int,
) -> list[DensePauli]:
    placement = {index: local_map[qubit] for index, qubit in qubit_map.items()}
    images: dict[tuple[str, int], DensePauli] = {}
    for lhs, rhs in generators.items():
        lhs_basis, lhs_index = parse_term(lhs.strip())
        if lhs_basis not in ("X", "Z"):
            raise NotImplementedError(
                f"Clifford key {lhs!r} is not an X or Z generator."
            )
        key = (lhs_basis, placement[lhs_index])
        image = DensePauli.from_sparse(remap_pauli(rhs.strip(), placement), qubit_count)
        if key in images and images[key] != image:
            raise ValueError(f"Clifford generator {lhs!r} has conflicting images.")
        images[key] = image

    result = []
    for qubit in range(qubit_count):
        for basis in ("X", "Z"):
            result.append(
                images.get(
                    (basis, qubit),
                    DensePauli.from_sparse(Pauli({qubit: basis}), qubit_count),
                )
            )
    for first, first_image in enumerate(result):
        for second in range(first + 1, len(result)):
            second_image = result[second]
            should_commute = first // 2 != second // 2
            if first_image.commutes_with(second_image) != should_commute:
                first_label = f"{'X' if first % 2 == 0 else 'Z'}_{first // 2}"
                second_label = f"{'X' if second % 2 == 0 else 'Z'}_{second // 2}"
                expected = "commute" if should_commute else "anticommute"
                raise ValueError(
                    f"Images of {first_label} and {second_label} must {expected}: "
                    f"{first_label} -> {first_image}, {second_label} -> {second_image}. "
                    "Unlisted generators have identity images."
                )
    return result
