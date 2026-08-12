"""Remap ISA action operators onto a program's concrete qubits."""

from __future__ import annotations

from typing import Any, TYPE_CHECKING

import qodec
from paulimer import DensePauli

from ..._typed_ir import value_tokens
from .pauli import Pauli

if TYPE_CHECKING:
    from paulimer import PauliCharacter

    from qodec.circuits import Program


def block_strides(isa: Any) -> dict[str, int]:
    blocks = list(isa.blocks)
    result = {block.name: int(block.encodes) for block in blocks}
    if len(blocks) == 1:
        result[""] = int(blocks[0].encodes)
    return result


def block_operands(program: "Program") -> list[qodec.instructions.BlockOperand]:
    result: list[qodec.instructions.BlockOperand] = []
    for call in program.instructions:
        instruction = program.lookup(call.mnemonic)
        declared = list(instruction.inputs) + list(instruction.outputs)
        for position in range(len(call.inputs)):
            result.append(
                declared[position] if position < len(declared) else declared[-1]
            )
    return result


def call_qubit_map(call: Any, strides: dict[str, int]) -> dict[int, int]:
    stride = strides.get("", next(iter(strides.values()), 1))
    result: dict[int, int] = {}
    flat = 0
    for value in call.inputs.values():
        for token in value_tokens(value):
            block_index = int(token)
            for offset in range(stride):
                result[flat] = block_index * stride + offset
                flat += 1
    return result


def build_qubit_map(
    call: Any,
    operands: list[qodec.instructions.BlockOperand],
    strides: dict[str, int],
) -> dict[int, int]:
    del operands
    return call_qubit_map(call, strides)


def remap_pauli(pauli_str: str, qubit_map: dict[int, int]) -> Pauli:
    characters: dict[int, "PauliCharacter"] = {}
    for token in pauli_str.split():
        basis, index = parse_basis_index(token)
        if basis != "I":
            characters[qubit_map[index]] = basis  # type: ignore[assignment]
    return Pauli(characters)


def remap_pauli_str(
    pauli_str: str,
    qubit_map: dict[int, int],
    local_map: dict[int, int],
) -> str:
    tokens = []
    for token in pauli_str.split():
        basis, index = parse_basis_index(token)
        tokens.append(f"{basis}_{local_map[qubit_map[index]]}")
    return " ".join(tokens)


def parse_basis_index(token: str) -> tuple[str, int]:
    if "_" in token:
        basis, index = token.split("_", 1)
        return basis, int(index)
    return token, 0


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
        lhs_basis, lhs_index = parse_basis_index(lhs.strip())
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
