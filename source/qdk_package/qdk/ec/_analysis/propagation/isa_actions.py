"""Remap ISA action operators onto a program's concrete qubits."""

from __future__ import annotations

from typing import Mapping, TYPE_CHECKING

from paulimer import DensePauli

from .pauli import Pauli, parse_term

if TYPE_CHECKING:
    from paulimer import PauliCharacter


def remap_pauli(pauli_str: str, qubit_map: Mapping[int, int]) -> Pauli:
    """The Pauli ``pauli_str`` names, each term placed through ``qubit_map``."""
    characters: dict[int, "PauliCharacter"] = {}
    for token in pauli_str.split():
        basis, index = parse_term(token)
        if basis != "I":
            characters[qubit_map[index]] = basis
    return Pauli(characters)


def build_clifford_images(
    generators: dict[str, str],
    qubit_map: dict[int, int],
    local_map: dict[int, int],
    qubit_count: int,
) -> list[DensePauli]:
    placement = {index: local_map[qubit] for index, qubit in qubit_map.items()}
    images: dict[tuple[str, int], DensePauli] = {}
    for lhs, rhs in generators.items():
        lhs_basis, lhs_index = parse_term(lhs.strip())
        images[(lhs_basis, placement[lhs_index])] = DensePauli.from_sparse(
            remap_pauli(rhs.strip(), placement), qubit_count
        )

    result = []
    for qubit in range(qubit_count):
        for basis in ("X", "Z"):
            result.append(
                images.get(
                    (basis, qubit),
                    DensePauli.from_sparse(Pauli({qubit: basis}), qubit_count),
                )
            )
    return result
