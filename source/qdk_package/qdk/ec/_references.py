"""Interpret gadget-local parity references as circuit outcomes, declared
readouts, or encoding signs. Selectors produce terms in order, preserving duplicates.

===================================  =========================
reference text                       atom
===================================  =========================
``circuit.readouts[<sel>]``          :class:`Outcome`
``readouts[<sel>]``                  :class:`ReadoutSign`
``(in|out)[<e>].stabilizers[<i>]``   :class:`StabilizerSign`
``(in|out)[<e>].(x|z)[<i>]``         :class:`LogicalSign`
===================================  =========================

``<sel>`` is a single index, a stop-exclusive slice (``N:M``, ``N:M:K``), or a
union (``N,M,P``); a selector addressing several records parses to one
:class:`Outcome` per record.
"""

from __future__ import annotations

from collections.abc import Iterable, Iterator
from dataclasses import dataclass
from typing import Literal, Union

from qodec import Reference, ReferenceLike

#: Which side of a gadget boundary an encoding reference names.
Side = Literal["in", "out"]

#: Which operator list of a boundary encoding a sign reference names.
Basis = Literal["x", "z"]


@dataclass(frozen=True)
class Outcome:
    """One measurement record of the gadget's own circuit."""

    index: int

    def __str__(self) -> str:
        return f"circuit.readouts[{self.index}]"


@dataclass(frozen=True)
class ReadoutSign:
    """One declared gadget readout, before substituting its equation."""

    index: int

    def __str__(self) -> str:
        return f"readouts[{self.index}]"


@dataclass(frozen=True)
class StabilizerSign:
    """The sign of one stabilizer of a boundary encoding.

    ``entry`` is the positional index into the gadget's ``inputs`` / ``outputs``
    encoding list; ``index`` selects a generator of that encoding's code.
    """

    side: Side
    entry: int
    index: int

    @property
    def key(self) -> tuple[int, int]:
        """This stabilizer's side-independent identity.

        A sign one gadget writes as ``out[...]`` the next gadget reads as
        ``in[...]``, so anything carrying signs across gadgets keys on this.
        """
        return (self.entry, self.index)

    def __str__(self) -> str:
        return f"{self.side}[{self.entry}].stabilizers[{self.index}]"


@dataclass(frozen=True)
class LogicalSign:
    """The sign of one logical operator of a boundary encoding."""

    side: Side
    entry: int
    basis: Basis
    index: int

    @property
    def key(self) -> tuple[int, Basis, int]:
        """This logical operator's side-independent identity."""
        return (self.entry, self.basis, self.index)

    def __str__(self) -> str:
        return f"{self.side}[{self.entry}].{self.basis}[{self.index}]"


Atom = Union[Outcome, StabilizerSign, LogicalSign, Literal[0, 1]]

#: One parity equation, parsed.
Equation = tuple[Atom, ...]


def reference_terms(
    reference: ReferenceLike,
) -> Iterator[Outcome | ReadoutSign | StabilizerSign | LogicalSign]:
    """Check gadget-local syntax immediately and produce selected terms lazily."""
    parsed = reference if isinstance(reference, Reference) else Reference(reference)
    segments = parsed.segments
    if not segments:
        raise ValueError(f"model address {parsed.path!r} is not a parity reference")
    match segments[-1]:
        case Reference.Index(index):
            indices = (index,)
        case Reference.Slice(start, stop, step):
            indices = range(start, stop, step)
        case Reference.Union(values):
            indices = values
        case _:
            raise ValueError(f"model address {parsed.path!r} is not a parity reference")
    match segments[:-1]:
        case (Reference.Field("circuit"), Reference.Field("readouts")):
            return (Outcome(index) for index in indices)
        case (Reference.Field("readouts"),):
            return (ReadoutSign(index) for index in indices)
        case (
            Reference.Field(("in" | "out") as side),
            Reference.Index(entry),
            Reference.Field("stabilizers"),
        ):
            return (StabilizerSign(side, entry, index) for index in indices)
        case (
            Reference.Field(("in" | "out") as side),
            Reference.Index(entry),
            Reference.Field(("x" | "z") as basis),
        ):
            return (LogicalSign(side, entry, basis, index) for index in indices)
        case _:
            raise ValueError(f"model address {parsed.path!r} is not a parity reference")


def reference_term(
    reference: ReferenceLike,
) -> Outcome | ReadoutSign | StabilizerSign | LogicalSign:
    """Require one selected parity term without expanding the remaining selection."""
    terms = reference_terms(reference)
    first = next(terms)
    if next(terms, None) is not None:
        raise ValueError(
            f"reference {str(reference)!r} must select exactly one position"
        )
    return first


def _parse_atom(reference: ReferenceLike | Literal[0, 1]) -> list[Atom]:
    if isinstance(reference, int):
        if type(reference) is not int or reference not in (0, 1):
            raise ValueError("parity constants must be integer bits 0 or 1")
        return [reference]
    return [
        term for term in reference_terms(reference) if not isinstance(term, ReadoutSign)
    ]


def parse_equation(references: Iterable[ReferenceLike | Literal[0, 1]]) -> Equation:
    """Circuit outcomes, encoding signs, and literal bits in declared order.

    Declared-readout references are omitted; general model addresses raise ValueError.
    """
    return tuple(atom for reference in references for atom in _parse_atom(reference))


def parse_equations(
    equations: Iterable[Iterable[ReferenceLike | Literal[0, 1]]],
) -> tuple[Equation, ...]:
    """Parse parity equations, such as a gadget's ``checks``."""
    return tuple(parse_equation(equation) for equation in equations)


def outcomes_of(equation: Iterable[Atom]) -> list[int]:
    """The measurement-record indices an equation addresses, in order."""
    return [atom.index for atom in equation if isinstance(atom, Outcome)]


def stabilizer_signs_of(
    equation: Iterable[Atom], *, side: Side | None = None
) -> list[StabilizerSign]:
    """The stabilizer-sign atoms of an equation, optionally one side only."""
    return [
        atom
        for atom in equation
        if isinstance(atom, StabilizerSign) and side in (None, atom.side)
    ]


def outcome_equation(indices: Iterable[int]) -> Equation:
    """An outcome-XOR pattern as an equation."""
    return tuple(Outcome(index) for index in indices)


def as_references(
    atoms: Iterable[ReferenceLike | Atom],
) -> list[ReferenceLike | Literal[0, 1]]:
    """One parity equation in the shape qodec's setters accept."""
    return [atom if isinstance(atom, int) else str(atom) for atom in atoms]


__all__ = [
    "Atom",
    "Basis",
    "Equation",
    "LogicalSign",
    "Outcome",
    "Side",
    "StabilizerSign",
    "as_references",
    "outcome_equation",
    "outcomes_of",
    "parse_equation",
    "parse_equations",
    "stabilizer_signs_of",
]
