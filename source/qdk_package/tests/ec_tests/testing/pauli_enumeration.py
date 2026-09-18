"""Exhaustive sparse-Pauli enumeration, for tests that need small error sets.

Lives here rather than in ``qdk.ec``: nothing in the package enumerates Paulis
by weight, since distance search goes through ``_analysis.distance_solvers``.
"""

from __future__ import annotations

import math
from typing import Iterable, Iterator, cast

from more_itertools import nth_combination, nth_product
from paulimer import SparsePauli

from qdk.ec._analysis.propagation.pauli import Pauli, PauliCharacter


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


__all__ = ["PauliEnumerator"]
