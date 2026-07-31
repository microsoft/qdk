"""Result types shared by sampling targets.

A readout is one shot's hard measurement bits; a batch is a sequence of shots.
Optional soft-confidence and erasure-herald channels remain result metadata,
not decoder contracts.
"""

from __future__ import annotations

from collections.abc import Iterable, Sequence

Readouts = Sequence[bool]
"""One shot's hard measurement bits."""

Batch = Sequence[Readouts]
"""Many shots of hard measurement bits."""


class SoftBatch(tuple):  # type: ignore[type-arg]
    """A batch carrying a parallel per-bit error-probability grid."""

    probabilities: Sequence[Sequence[float]]

    def __new__(
        cls,
        readouts: Iterable[Readouts],
        probabilities: Sequence[Sequence[float]],
    ) -> "SoftBatch":
        self = tuple.__new__(cls, readouts)
        if len(self) != len(probabilities):
            raise ValueError(
                f"probabilities shots ({len(probabilities)}) != "
                f"bits shots ({len(self)})"
            )
        self.probabilities = probabilities
        return self


class HeraldedBatch(tuple):  # type: ignore[type-arg]
    """A batch carrying a parallel per-bit erasure-herald grid."""

    leaks: Sequence[Sequence[bool]]

    def __new__(
        cls,
        readouts: Iterable[Readouts],
        leaks: Sequence[Sequence[bool]],
    ) -> "HeraldedBatch":
        self = tuple.__new__(cls, readouts)
        if len(self) != len(leaks):
            raise ValueError(f"leaks shots ({len(leaks)}) != bits shots ({len(self)})")
        self.leaks = leaks
        return self


class SoftView:
    """A tolerant soft-confidence view over any batch."""

    def __init__(self, batch: Batch) -> None:
        self.bits: Batch = batch
        existing = getattr(batch, "probabilities", None)
        self.probabilities: Sequence[Sequence[float]] = (
            existing if existing is not None else [[0.0] * len(row) for row in batch]
        )

    @property
    def is_soft(self) -> bool:
        return getattr(self.bits, "probabilities", None) is not None


class HeraldedView:
    """A tolerant erasure-herald view over any batch."""

    def __init__(self, batch: Batch) -> None:
        self.bits: Batch = batch
        existing = getattr(batch, "leaks", None)
        self.leaks: Sequence[Sequence[bool]] = (
            existing if existing is not None else [[False] * len(row) for row in batch]
        )

    @property
    def is_heralded(self) -> bool:
        return getattr(self.bits, "leaks", None) is not None


__all__ = [
    "Batch",
    "HeraldedBatch",
    "HeraldedView",
    "Readouts",
    "SoftBatch",
    "SoftView",
]
