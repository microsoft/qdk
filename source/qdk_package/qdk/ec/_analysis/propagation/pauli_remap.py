"""Remap encoded logical Paulis onto physical program qubits."""

from __future__ import annotations

from collections.abc import Iterable, Iterator, Mapping, Sequence
from typing import Any, TYPE_CHECKING

from .pauli import Pauli

if TYPE_CHECKING:
    from paulimer import PauliCharacter


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


def code_qubit_count(code: Any) -> int:
    support = getattr(code, "support", None)
    if support is not None and not callable(support):
        return len(support)
    max_index = -1
    for characters in _all_operator_chars(code):
        if characters:
            max_index = max(max_index, max(characters))
    return max_index + 1


def encoding_qubit_relocation(encoding: Any) -> dict[int, int]:
    support = [int(qubit) for qubit in encoding.support]
    return encoding_relocation(support, code_qubit_count(encoding.code))


def remap_to_global(
    characters: dict[int, "PauliCharacter"],
    relocation: Mapping[int, int],
) -> Pauli:
    return Pauli(
        {relocation[index]: character for index, character in characters.items()}
    )


def flat_logical_paulis(encodings: Iterable[Any]) -> list[Pauli]:
    paulis = []
    for encoding in encodings:
        relocation = encoding_qubit_relocation(encoding)
        for characters in _flat_logical_chars(encoding.code):
            paulis.append(remap_to_global(characters, relocation))
    return paulis


def _flat_logical_chars(code: Any) -> Iterator[dict[int, "PauliCharacter"]]:
    x_operators = getattr(code, "x", None)
    z_operators = getattr(code, "z", None)
    if x_operators is not None and z_operators is not None:
        for x_operator, z_operator in zip(list(x_operators), list(z_operators)):
            yield _pauli_string_to_chars(str(x_operator))
            yield _pauli_string_to_chars(str(z_operator))
        return
    for pauli in code.logical_basis:
        yield pauli.characters


def _all_operator_chars(code: Any) -> Iterator[dict[int, "PauliCharacter"]]:
    for stabilizer in getattr(code, "stabilizers", []):
        yield _pauli_string_to_chars(str(stabilizer))
    for destabilizer in getattr(code, "destabilizers", []):
        yield _pauli_string_to_chars(str(destabilizer))
    x_operators = getattr(code, "x", None)
    z_operators = getattr(code, "z", None)
    if x_operators is not None and z_operators is not None:
        for operator in x_operators:
            yield _pauli_string_to_chars(str(operator))
        for operator in z_operators:
            yield _pauli_string_to_chars(str(operator))
    for logical in getattr(code, "logicals", []):
        yield _pauli_string_to_chars(logical.x)
        yield _pauli_string_to_chars(logical.z)
    for gauge in getattr(code, "gauges", []):
        yield _pauli_string_to_chars(str(gauge))


def _pauli_string_to_chars(pauli_str: str) -> dict[int, "PauliCharacter"]:
    characters: dict[int, "PauliCharacter"] = {}
    for token in pauli_str.split():
        basis, _, index = token.partition("_")
        if basis not in ("I", "X", "Y", "Z"):
            raise ValueError(f"unrecognised Pauli letter {basis!r}")
        characters[int(index)] = basis  # type: ignore[assignment]
    return characters
