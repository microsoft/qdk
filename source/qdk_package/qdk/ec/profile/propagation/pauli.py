"""Pauli conveniences used by exact propagation and qodec profiling."""

from __future__ import annotations

import math
from typing import Final, Iterable, Iterator, Literal, cast, get_args

from more_itertools import nth_combination, nth_product
from paulimer import SparsePauli

Pauli = SparsePauli
PauliCharacter = Literal["I", "X", "Y", "Z"]
pauli_characters: Final[frozenset[str]] = frozenset(get_args(PauliCharacter))

_PHASE_TO_EXPONENT: dict[complex, int] = {
    1 + 0j: 0,
    0 + 1j: 1,
    -1 + 0j: 2,
    0 - 1j: 3,
}


def identity(phase: complex = 1) -> Pauli:
    """Return the identity Pauli with an optional unit scalar phase."""
    try:
        exponent = _PHASE_TO_EXPONENT[complex(phase)]
    except KeyError as error:
        raise ValueError(f"Unsupported phase: {phase!r}") from error
    return SparsePauli({}, exponent=exponent)


def characters_of(pauli: Pauli) -> dict[int, PauliCharacter]:
    """Return non-identity characters keyed by qubit index."""
    return {
        qubit: cast(PauliCharacter, character)
        for qubit, character in zip(pauli.support, pauli.characters)
    }


def as_literal(character: str) -> PauliCharacter:
    if character not in pauli_characters:
        raise ValueError(f"Invalid Pauli character: {character}")
    return cast(PauliCharacter, character)


def as_literals(string: str) -> Iterator[PauliCharacter]:
    yield from map(as_literal, string)


class PauliEnumerator:
    """Enumerate sparse Paulis by support and weight."""

    def __init__(self, support: Iterable[int], characters: str = "XYZ"):
        self._support = tuple(sorted(support))
        self._types = characters

    def of_weight(self, weight: int) -> Iterator[Pauli]:
        if weight == 0:
            yield SparsePauli({})
            return
        support_count = math.comb(len(self._support), weight)
        character_count = len(self._types) ** weight
        total_count = support_count * character_count
        repeated_types = [self._types] * weight

        def getitem(index: int) -> Pauli:
            support_index, character_index = divmod(index, character_count)
            support = nth_combination(self._support, weight, support_index)
            chars = nth_product(character_index, *repeated_types)
            return Pauli(cast("dict[int, PauliCharacter]", dict(zip(support, chars))))

        yield from (getitem(index) for index in range(total_count))

    def by_weight(self, weights: Iterable[int] | None = None) -> Iterator[Pauli]:
        if weights is None:
            weights = range(len(self._support))
        for weight in weights:
            yield from self.of_weight(weight)

    def up_to_weight(self, maximum: int) -> Iterator[Pauli]:
        return self.by_weight(range(maximum + 1))


__all__ = [
    "Pauli",
    "PauliCharacter",
    "PauliEnumerator",
    "as_literal",
    "as_literals",
    "characters_of",
    "identity",
]
