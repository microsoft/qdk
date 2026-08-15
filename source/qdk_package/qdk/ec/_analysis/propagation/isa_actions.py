"""Remap ISA action operators onto a program's concrete qubits."""

from __future__ import annotations

from typing import Any, Mapping, TYPE_CHECKING

from paulimer import DensePauli

from ..._operands import qubit_labels
from .pauli import Pauli, parse_term

if TYPE_CHECKING:
    from paulimer import PauliCharacter


def block_stride(isa: Any) -> int:
    """Qubits per block instance, for an ISA whose blocks share one width.

    The walker addresses a qubit as ``operand_index * stride + offset`` — the
    same convention :func:`~.pauli_remap.encoding_relocation` uses to place an
    encoding. That flat scheme has room for exactly one width: with two, the
    ranges of differently sized blocks would overlap.
    """
    widths = {int(block.encodes) for block in isa.blocks}
    if len(widths) > 1:
        raise NotImplementedError(
            f"instruction set {getattr(isa, 'name', '?')!r} declares blocks of "
            f"differing widths {sorted(widths)}; exact propagation addresses "
            "qubits as operand_index * stride, which admits only one width"
        )
    return next(iter(widths), 1)


def call_qubit_map(call: Any, stride: int) -> dict[int, int]:
    result: dict[int, int] = {}
    flat = 0
    for value in call.inputs.values():
        for label in qubit_labels(value):
            block_index = int(label)
            for offset in range(stride):
                result[flat] = block_index * stride + offset
                flat += 1
    return result


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
