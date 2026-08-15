"""Qodec-bound program executors, and how to compose them.

This module defines the small vocabulary the sampler stack is built from:

* :class:`Target` — a generic, qodec-bound executor whose :meth:`Target.execute`
  samples a `Program` and returns a result of some type ``R`` (a
  ``Target[Batch]`` is a sampler).
* :class:`Sampler` — the structural contract for "anything that produces a
  `Batch`", so consumers can accept any backend, not one concrete target.
* :class:`ComposableTarget` / :class:`CompositeTarget` — assemble one
  per-translation target per layer into a single executor over a whole layered
  qodec. This is what samplers like ``UniversalSampler`` are built on.
"""

from __future__ import annotations

from typing import Callable, Generic, Protocol, TypeVar, runtime_checkable

import qodec as qc
from qodec.circuits import Program

from .results import Batch

Result_co = TypeVar("Result_co", covariant=True)
Result = TypeVar("Result")
Readin = TypeVar("Readin")
Readout = TypeVar("Readout")
Targetlike = TypeVar("Targetlike")

#: A callable that binds a qodec to a target-like executor.
Factory = Callable[[qc.Qodec], Targetlike]

#: A callable that binds a qodec and the target below it to a composed executor.
ComposedFactory = Callable[
    [qc.Qodec, "Target[Result]"], "ComposableTarget[Result, Result]"
]


class Target(Generic[Result_co]):
    """Generic, qodec-bound view onto a program executor.

    Stores the bound qodec at construction; subclasses parameterise the
    result type ``Result_co`` and implement :meth:`execute`, which samples
    ``shots`` independent shots of ``program`` and returns a result of type
    ``Result_co``.
    """

    def __init__(self, qodec: qc.Qodec) -> None:
        self._qodec = qodec

    @property
    def qodec(self) -> qc.Qodec:
        return self._qodec

    def execute(self, program: Program, *, shots: int) -> Result_co:
        raise NotImplementedError


@runtime_checkable
class Sampler(Protocol):
    """The minimum contract for "produces a `Batch` from a program".

    Any `Target[Batch]` satisfies it; consumers accept a `Sampler` rather than
    a concrete target so the backend is swappable.
    """

    @property
    def qodec(self) -> qc.Qodec: ...

    def execute(self, program: Program, *, shots: int) -> "Batch": ...


class ComposableTarget(Target[Readout], Generic[Readin, Readout]):
    """A Target that realizes one lowering over the layer below it.

    ``below`` is the target for the layer immediately beneath this one, taken at
    construction. :meth:`execute` lowers its program one step, delegates to
    ``below``, and lifts the result back up. ``Readin`` is ``below``'s result
    type; ``Readout`` is this layer's.
    """

    def __init__(self, qodec: qc.Qodec, below: Target[Readin]) -> None:
        super().__init__(qodec)
        self._below = below

    @property
    def below(self) -> Target[Readin]:
        """The target for the layer immediately beneath this one."""
        return self._below

    def execute(self, program: Program, *, shots: int) -> Readout:
        raise NotImplementedError


class CompositeTarget(Target[Result]):
    """A Target over a compound qodec, assembled from per-layer ComposableTargets.

    Each adjacent layer pair (``qodec.slice(i, i + 2)``) is one lowering. The
    bottom lowering is executed directly by ``runtime``; each upper lowering is
    realized by a ``ComposableTarget`` built over the layer below it.
    ``execute`` delegates to the top of the stack.
    """

    def __init__(
        self,
        qodec: qc.Qodec,
        runtime: Factory[Target[Result]],
        processors: ComposedFactory[Result],
    ) -> None:
        super().__init__(qodec)
        if len(qodec.layers) < 2:
            raise ValueError(
                "CompositeTarget requires a qodec with at least two layers "
                "(one lowering edge)"
            )
        # One simple qodec per lowering: slice(i, i + 2) covers layers i and i+1.
        layers = [qodec.slice(i, i + 2) for i in range(len(qodec.layers) - 1)]
        # The floor (bottom) lowering is run by the runtime; each upper lowering
        # is built over the one below it, so the stack assembles bottom-up.
        below: Target[Result] = runtime(layers[-1])
        for layer in reversed(layers[:-1]):
            below = processors(layer, below)
        self._top = below

    def execute(self, program: Program, *, shots: int) -> Result:
        return self._top.execute(program, shots=shots)


CompositeSampler = CompositeTarget[Batch]
