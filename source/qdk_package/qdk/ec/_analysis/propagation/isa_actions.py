"""Remap ISA action operators onto a program's concrete qubits."""

from __future__ import annotations

from typing import Any, TYPE_CHECKING

from paulimer import DensePauli

from ..._typed_ir import value_tokens
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
        for token in value_tokens(value):
            block_index = int(token)
            for offset in range(stride):
                result[flat] = block_index * stride + offset
                flat += 1
    return result


def remap_pauli(pauli_str: str, qubit_map: dict[int, int]) -> Pauli:
    characters: dict[int, "PauliCharacter"] = {}
    for token in pauli_str.split():
        basis, index = parse_term(token)
        if basis != "I":
            characters[qubit_map[index]] = basis
    return Pauli(characters)


def remap_pauli_str(
    pauli_str: str,
    qubit_map: dict[int, int],
    local_map: dict[int, int],
) -> str:
    tokens = []
    for token in pauli_str.split():
        basis, index = parse_term(token)
        tokens.append(f"{basis}_{local_map[qubit_map[index]]}")
    return " ".join(tokens)


def dense_pauli(text: str, qubit_count: int) -> DensePauli:
    return DensePauli.from_sparse(Pauli(text), qubit_count)


def build_clifford_images(
    generators: dict[str, str],
    qubit_map: dict[int, int],
    local_map: dict[int, int],
    qubit_count: int,
) -> list[DensePauli]:
    images: dict[tuple[str, int], DensePauli] = {}
    for lhs, rhs in generators.items():
        lhs_basis, lhs_index = parse_term(lhs.strip())
        local_qubit = local_map[qubit_map[lhs_index]]
        rhs_dense = remap_pauli_str(rhs.strip(), qubit_map, local_map)
        images[(lhs_basis, local_qubit)] = dense_pauli(rhs_dense, qubit_count)

    result = []
    for qubit in range(qubit_count):
        for basis in ("X", "Z"):
            result.append(
                images.get(
                    (basis, qubit),
                    dense_pauli(f"{basis}_{qubit}", qubit_count),
                )
            )
    return result
