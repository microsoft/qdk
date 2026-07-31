"""Codec-bound program executors, and how to compose them.

This module defines the small vocabulary the sampler stack is built from:

* :class:`Target` — a generic, codec-bound executor whose :meth:`Target.execute`
  samples a `Program` and returns a result of some type ``R`` (a
  ``Target[Batch]`` is a sampler).
* :class:`Sampler` — the structural contract for "anything that produces a
  `Batch`", so consumers can accept any backend, not one concrete target.
* :class:`ComposableTarget` / :class:`CompositeTarget` — assemble one
  per-translation target per layer into a single executor over a whole layered
  codec. This is what samplers like ``UniversalSampler`` are built on.
"""

from __future__ import annotations

from typing import Callable, Generic, Protocol, TypeVar, runtime_checkable

import qodec
from qodec.circuits import Program

from .results import Batch

Result_co = TypeVar("Result_co", covariant=True)
Result = TypeVar("Result")
Readin = TypeVar("Readin")
Readout = TypeVar("Readout")
Targetlike = TypeVar("Targetlike")

#: A callable that binds a codec to a target-like executor.
Factory = Callable[[qodec.Qodec], Targetlike]


class Target(Generic[Result_co]):
    """Generic, codec-bound view onto a program executor.

    Stores the bound codec at construction; subclasses parameterise the
    result type ``Result_co`` and implement :meth:`execute`, which samples
    ``shots`` independent shots of ``program`` and returns a result of type
    ``Result_co``.
    """

    def __init__(self, codec: qodec.Qodec) -> None:
        self._codec = codec

    @property
    def codec(self) -> qodec.Qodec:
        return self._codec

    def execute(self, program: Program, *, shots: int) -> Result_co:
        raise NotImplementedError


@runtime_checkable
class Sampler(Protocol):
    """The minimum contract for "produces a `Batch` from a program".

    Any `Target[Batch]` satisfies it; consumers accept a `Sampler` rather than
    a concrete target so the backend is swappable.
    """

    @property
    def codec(self) -> qodec.Qodec: ...

    def execute(self, program: Program, *, shots: int) -> "Batch": ...


class ComposableTarget(Target[Readout], Generic[Readin, Readout]):
    """A Target that realizes one lowering by composing with the layer below.

    ``compose_with`` injects the lower target (the layer immediately below this
    one). After wiring, ``execute`` lowers its program one step, delegates to
    that lower target, and lifts the result back up. ``Readin`` is the lower
    target's result type; ``Readout`` is this layer's.
    """

    def compose_with(self, target: Target[Readin]) -> None:
        raise NotImplementedError

    def execute(self, program: Program, *, shots: int) -> Readout:
        raise NotImplementedError


class CompositeTarget(Target[Result]):
    """A Target over a compound qodec, assembled from per-layer ComposableTargets.

    Each adjacent layer pair (``codec.slice(i, i + 2)``) is one lowering. The
    bottom lowering is executed directly by ``runtime``; each upper lowering is
    realized by a ``ComposableTarget`` that ``compose_with`` the layer below it.
    ``execute`` delegates to the top of the wired stack.
    """

    def __init__(
        self,
        codec: qodec.Qodec,
        runtime: Factory[Target[Result]],
        processors: Factory[ComposableTarget[Result, Result]],
    ) -> None:
        super().__init__(codec)
        if len(codec.layers) < 2:
            raise ValueError(
                "CompositeTarget requires a codec with at least two layers "
                "(one lowering edge)"
            )
        # One simple qodec per lowering: slice(i, i + 2) covers layers i and i+1.
        layers = [codec.slice(i, i + 2) for i in range(len(codec.layers) - 1)]
        # The floor (bottom) lowering is run by the runtime; the upper lowerings
        # are realized by ComposableTargets, ordered top to bottom.
        self._runtime: Target[Result] = runtime(layers[-1])
        self._processors = [processors(layer) for layer in layers[:-1]]
        # Wire the stack bottom-up: each processor composes with the one below it.
        below: Target[Result] = self._runtime
        for processor in reversed(self._processors):
            processor.compose_with(below)
            below = processor
        self._top = below

    def execute(self, program: Program, *, shots: int) -> Result:
        return self._top.execute(program, shots=shots)


CompositeSampler = CompositeTarget[Batch]
