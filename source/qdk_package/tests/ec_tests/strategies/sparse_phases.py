"""
Hypothesis strategies for Pauli phases.

The historical ``Phase`` class with conditional phases has been removed from the
public API. ``sparse_phases`` now yields the four allowed unit-magnitude complex
phases. The ``min_conditions``/``max_conditions`` parameters are accepted for
backward compatibility with older test signatures and are ignored.
"""
from typing import Optional
from hypothesis import strategies


def sparse_phases(
    min_conditions: int = 0,  # pylint: disable=unused-argument
    max_conditions: Optional[int] = 10,  # pylint: disable=unused-argument
) -> strategies.SearchStrategy[complex]:
    return strategies.sampled_from([1 + 0j, -1 + 0j, 1j, -1j])


def compatible_sparse_phases(
    min_size: int = 2,
    max_size: Optional[int] = None,
    min_conditions: int = 0,  # pylint: disable=unused-argument
    max_conditions: Optional[int] = 10,  # pylint: disable=unused-argument
) -> strategies.SearchStrategy[list[complex]]:
    return strategies.lists(sparse_phases(), min_size=min_size, max_size=max_size)
