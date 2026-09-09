"""The atom vocabulary behind qodec's property-path reference grammar.

A qodec parity equation is a flat list of JsonPath-style references relative to
the gadget root. That grammar is *text*, and text is a poor thing to reason
with: asking "does this check constrain an output stabilizer?" of a string means
knowing the grammar at the asking site. This module is the one place qdk.ec
turns those strings into values and back, so everything else matches on atom
types instead.

===================================  =========================
reference text                       atom
===================================  =========================
``circuit.readouts[<sel>]``          :class:`Outcome`
``(in|out)[<e>].stabilizers[<i>]``   :class:`StabilizerSign`
``(in|out)[<e>].(x|z)[<i>]``         :class:`LogicalSign`
===================================  =========================

``<sel>`` is a single index, a stop-exclusive slice (``N:M``, ``N:M:K``), or a
union (``N,M,P``); a selector addressing several records parses to one
:class:`Outcome` per record.
"""

from __future__ import annotations

from collections.abc import Iterable
from dataclasses import dataclass
from typing import Literal, Union

from qodec.gadgets import Reference, ReferenceLike

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


Atom = Union[Outcome, StabilizerSign, LogicalSign]

#: One parity equation, parsed.
Equation = tuple[Atom, ...]


def _parse_atom(reference: ReferenceLike) -> list[Atom]:
    atoms: list[Atom] = []
    for term in Reference(reference).expand():
        if term.kind == "circuit_readout":
            atoms.append(Outcome(term.index))
        elif term.kind == "encoding":
            side, entry, basis = term.boundary, term.entry, term.encoding_property
            assert side is not None and entry is not None
            if basis == "stabilizers":
                atoms.append(StabilizerSign(side, entry, term.index))
            elif basis in ("x", "z"):
                atoms.append(LogicalSign(side, entry, basis, term.index))
    return atoms


def parse_equation(references: Iterable[ReferenceLike]) -> Equation:
    """Every atom of one parity equation, in declared order.

    References of a shape this module does not model are dropped rather than
    rejected: qodec validates the path grammar itself, and an equation may
    legitimately carry atoms qdk.ec has no use for.
    """
    return tuple(atom for reference in references for atom in _parse_atom(reference))


def parse_equations(
    equations: Iterable[Iterable[ReferenceLike]],
) -> tuple[Equation, ...]:
    """A list of parity equations — a gadget's ``checks``, say — parsed."""
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


def as_references(atoms: Iterable[ReferenceLike | Atom]) -> list[ReferenceLike]:
    """One parity equation in the shape qodec's setters accept."""
    return [str(atom) for atom in atoms]


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
