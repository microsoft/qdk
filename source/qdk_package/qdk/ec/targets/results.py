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


class AnnotatedBatch(tuple):  # type: ignore[type-arg]
    """A batch carrying optional per-bit side channels.

    Still a plain sequence of shots, so anything accepting a :data:`Batch`
    accepts one of these. A channel that was not measured is ``None`` rather
    than absent, so asking whether a batch carries one is a value test rather
    than an attribute probe — see :func:`probabilities_of` and :func:`leaks_of`.
    """

    probabilities: Sequence[Sequence[float]] | None
    leaks: Sequence[Sequence[bool]] | None

    def __new__(
        cls,
        readouts: Iterable[Readouts],
        *,
        probabilities: Sequence[Sequence[float]] | None = None,
        leaks: Sequence[Sequence[bool]] | None = None,
    ) -> "AnnotatedBatch":
        self = tuple.__new__(cls, readouts)
        _check_shots(probabilities, len(self), "probabilities")
        _check_shots(leaks, len(self), "leaks")
        self.probabilities = probabilities
        self.leaks = leaks
        return self


def _check_shots(channel: Sequence[object] | None, shots: int, name: str) -> None:
    if channel is not None and len(channel) != shots:
        raise ValueError(f"{name} shots ({len(channel)}) != bits shots ({shots})")


def probabilities_of(batch: Batch) -> Sequence[Sequence[float]] | None:
    """The per-bit error probabilities ``batch`` carries, or ``None`` if none.

    A batch need not be an :class:`AnnotatedBatch` — a plain list of shots is a
    valid :data:`Batch` and simply carries no channels.
    """
    return getattr(batch, "probabilities", None)


def leaks_of(batch: Batch) -> Sequence[Sequence[bool]] | None:
    """The per-bit erasure heralds ``batch`` carries, or ``None`` if none."""
    return getattr(batch, "leaks", None)


__all__ = [
    "AnnotatedBatch",
    "Batch",
    "Readouts",
    "leaks_of",
    "probabilities_of",
]
