"""Stabilizer-group helpers for exact propagation."""

from __future__ import annotations

from itertools import compress
from typing import Iterable, Sequence

import binar
from more_itertools import flatten
from paulimer import PauliGroup

from .pauli import Pauli


def is_stabilizer_group(group: PauliGroup) -> bool:
    return group.is_abelian and 2 not in group.phases


def subgroup_of(
    group: PauliGroup, *, indicated_by: Iterable[Iterable[int]]
) -> PauliGroup:
    if len(group.generators) == 0:
        return group
    return PauliGroup(
        element_of(group, indicated_by=[bool(value) for value in indicator])
        for indicator in indicated_by
    )


def element_of(group: PauliGroup, indicated_by: Iterable[bool]) -> Pauli:
    element = Pauli.identity()
    for generator in compress(group.generators, indicated_by):
        element = element * generator
    return element


def restriction_indicator_basis_of(
    group: PauliGroup, *, supported_by: Iterable[int]
) -> Iterable[Sequence[int]]:
    if len(group.generators) == 0:
        return []

    bitmap = {
        "I": (False, False),
        "X": (True, False),
        "Y": (True, True),
        "Z": (False, True),
    }
    complemented_by = set(group.support) - set(supported_by)

    def to_bits(pauli: Pauli) -> list[bool]:
        return list(flatten(bitmap[pauli[index]] for index in complemented_by))

    def to_indicator(bits: binar.BitVector) -> list[int]:
        return list(map(int, bits))

    complement_generators = binar.BitMatrix(list(map(to_bits, group.generators)))
    nullspace = binar.null_space(complement_generators.T)
    return map(to_indicator, (row for row in nullspace.rows if row.weight > 0))


def rank_extension_of(rows: Sequence[Sequence[int]]) -> Sequence[Sequence[int]]:
    if len(rows) == 0:
        return rows
    binary_rows = binar.BitMatrix(rows)  # type: ignore[arg-type]
    pivots = binary_rows.echelonize()
    row_length = binary_rows.column_count
    extension_columns = set(range(row_length)) - set(pivots)
    extension = [[0] * row_length for _ in range(len(extension_columns))]
    for row, column in zip(extension, extension_columns):
        row[column] = 1
    return extension


__all__ = [
    "element_of",
    "is_stabilizer_group",
    "rank_extension_of",
    "restriction_indicator_basis_of",
    "subgroup_of",
]
