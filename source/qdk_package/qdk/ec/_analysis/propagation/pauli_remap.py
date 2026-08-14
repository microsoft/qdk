"""Remap encoded logical Paulis onto physical program qubits."""

from __future__ import annotations

from collections.abc import Iterable, Iterator, Mapping, Sequence
from typing import Literal, TYPE_CHECKING

import qodec as qc

from .pauli import Pauli, characters_of_string

if TYPE_CHECKING:
    from paulimer import PauliCharacter

#: Which of a code's two logical operator lists to read.
Basis = Literal["X", "Z"]


def encoding_relocation(support: Sequence[int], num_code_qubits: int) -> dict[int, int]:
    num_blocks = len(support)
    if num_blocks == 0:
        return {}
    block_size, remainder = divmod(num_code_qubits, num_blocks)
    if remainder != 0:
        raise ValueError(
            f"code qubit count {num_code_qubits} is not divisible by its "
            f"{num_blocks} support blocks"
        )
    operand_footprint: dict[int, int] = {}
    for operand in support:
        operand_footprint[operand] = operand_footprint.get(operand, 0) + block_size
    relocation: dict[int, int] = {}
    placed_in_operand: dict[int, int] = {}
    for block_index, operand in enumerate(support):
        placed = placed_in_operand.get(operand, 0)
        base = operand * operand_footprint[operand]
        for offset in range(block_size):
            code_qubit = block_index * block_size + offset
            relocation[code_qubit] = base + placed * block_size + offset
        placed_in_operand[operand] = placed + 1
    return relocation


def code_qubit_count(code: qc.Code) -> int:
    """One past the highest qubit index any of the code's operators mentions."""
    highest = -1
    for characters in _all_operator_chars(code):
        if characters:
            highest = max(highest, max(characters))
    return highest + 1


def encoding_qubit_relocation(encoding: qc.Encoding) -> dict[int, int]:
    support = [int(qubit) for qubit in encoding.support]
    return encoding_relocation(support, code_qubit_count(encoding.code))


def remap_to_global(
    characters: dict[int, "PauliCharacter"],
    relocation: Mapping[int, int],
) -> Pauli:
    return Pauli(
        {relocation[index]: character for index, character in characters.items()}
    )


def flat_logical_paulis(encodings: Iterable[qc.Encoding]) -> list[Pauli]:
    paulis = []
    for encoding in encodings:
        relocation = encoding_qubit_relocation(encoding)
        for characters in _flat_logical_chars(encoding.code):
            paulis.append(remap_to_global(characters, relocation))
    return paulis


def flat_logical_slots(
    encodings: Iterable[qc.Encoding],
) -> list[tuple[qc.Encoding, int]]:
    """``(encoding, local logical index)`` per logical qubit, in flat order.

    An action token ``X_<t>`` names the ``t``-th entry of this list, so this is
    how a flat token index resolves to the encoding that carries it.
    """
    return [
        (encoding, local)
        for encoding in encodings
        for local in range(len(list(encoding.code.x)))
    ]


def logical_chars(code: qc.Code, basis: Basis) -> list[dict[int, "PauliCharacter"]]:
    """Characters of the code's logical operators in one basis, in order."""
    operators = code.x if basis == "X" else code.z
    return [characters_of_string(str(operator)) for operator in operators]


def _flat_logical_chars(code: qc.Code) -> Iterator[dict[int, "PauliCharacter"]]:
    for x_characters, z_characters in zip(
        logical_chars(code, "X"), logical_chars(code, "Z")
    ):
        yield x_characters
        yield z_characters


def _all_operator_chars(code: qc.Code) -> Iterator[dict[int, "PauliCharacter"]]:
    for group in (code.stabilizers, code.destabilizers, code.x, code.z):
        for operator in group:
            yield characters_of_string(str(operator))
