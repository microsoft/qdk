"""What a gadget's ``readouts`` list is, entry by entry.

``Gadget.readouts`` is one positional list holding two kinds of thing: the
implemented instruction's ``observe`` outcomes first, then its ``flags:`` flags.
The boundary between them is fixed by the *instruction*, not by the gadget, so
finding it means reading ``gadget.implements`` — and every consumer that wants
one kind has to re-derive the split to get it.

:func:`readout_slots` derives it once. Each :class:`ReadoutSlot` says which kind
an entry is, what it is called, and what it equates to; consumers filter that
value instead of re-slicing the list.
"""

from __future__ import annotations

from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass

import qodec as qc

from ._references import (
    Equation,
    as_references,
    outcome_equation,
    outcomes_of,
    parse_equation,
)


def observe_count_of(instruction: qc.Instruction) -> int:
    """Number of ``observe`` outcome bits an instruction declares."""
    return sum(
        len(action.observables)
        for action in instruction.action
        if isinstance(action, qc.actions.Observe)
    )


def readout_equation(entry: qc.Readout) -> Equation:
    """The parsed parity equation of one ``gadget.readouts`` entry.

    An entry is either a bare equation or a single-key ``{name: equation}``
    mapping; both reduce to the same atom list.
    """
    if isinstance(entry, Mapping):
        (equation,) = entry.values()
        return parse_equation(equation)
    return parse_equation(entry)


def as_readout(
    entry: Sequence[qc.ReferenceLike] | Mapping[str, Sequence[qc.ReferenceLike]],
) -> qc.ReadoutLike:
    """One readout entry in the shape qodec's setters accept."""
    if isinstance(entry, Mapping):
        return {name: as_references(equation) for name, equation in entry.items()}
    return as_references(entry)


@dataclass(frozen=True)
class ReadoutSlot:
    """One bound entry of ``gadget.readouts``.

    ``name`` is the positional name (``"0"``, ``"1"``, ...) for an observable and
    the declared flag name for a flag. An entry past everything the instruction
    declares falls back to its positional name.
    """

    position: int
    name: str
    is_flag: bool
    equation: Equation


@dataclass(frozen=True)
class GadgetReadouts:
    """Every bound entry of ``gadget.readouts``, already split by kind.

    A gadget may bind fewer entries than its instruction declares; only the
    entries actually present are reported, which is what lets the auditor see an
    unbound observable as a missing slot rather than crash on it.
    """

    slots: tuple[ReadoutSlot, ...]
    observables: tuple[ReadoutSlot, ...]
    flags: tuple[ReadoutSlot, ...]


def readouts_of(gadget: qc.Gadget) -> GadgetReadouts:
    """Bind ``gadget.readouts`` to its slots: observables first, then flags.

    This is the one place the observable/flag boundary is derived. Consumers
    that want both kinds take this value once rather than deriving it per view.
    """
    observe = observe_count_of(gadget.implements)
    flags = list(gadget.implements.flags)
    slots = []
    for position, entry in enumerate(gadget.readouts):
        flag_index = position - observe
        if 0 <= flag_index < len(flags):
            name = flags[flag_index]
        else:
            name = str(position)
        slots.append(
            ReadoutSlot(position, name, flag_index >= 0, readout_equation(entry))
        )
    return GadgetReadouts(
        tuple(slots),
        tuple(slot for slot in slots if not slot.is_flag),
        tuple(slot for slot in slots if slot.is_flag),
    )


def readout_slots(gadget: qc.Gadget) -> tuple[ReadoutSlot, ...]:
    """Every bound entry of ``gadget.readouts``: observables first, then flags."""
    return readouts_of(gadget).slots


def observable_slots(gadget: qc.Gadget) -> tuple[ReadoutSlot, ...]:
    """The gadget's bound observables — its Pauli-bearing readouts."""
    return readouts_of(gadget).observables


def flag_slots(gadget: qc.Gadget) -> tuple[ReadoutSlot, ...]:
    """The gadget's bound flags — decoder-blind side-channel bits."""
    return readouts_of(gadget).flags


def observables_as_xor_map(gadget: qc.Gadget) -> dict[str, list[int]]:
    """Gadget observables: positional name → measurement-record XOR."""
    return {
        slot.name: outcomes_of(slot.equation)
        for slot in readouts_of(gadget).observables
    }


def set_gadget_readouts(
    gadget: qc.Gadget, named_xor: Mapping[str, Iterable[int]]
) -> None:
    """Set the observable entries of ``gadget.readouts`` from an XOR map.

    ``named_xor`` is a position-keyed observable-XOR map (decimal-string keys
    ``"0"``, ``"1"``, ...); each becomes one parity equation, in positional
    order. Non-positional (flag-named) keys are ignored.

    Any pre-authored trailing flag entries are preserved: flags carry no Pauli
    expectation, so they are authored by hand rather than discovered, and
    re-deriving the observables must not drop them.
    """
    positional: dict[int, Equation] = {}
    for name, indices in named_xor.items():
        if str(name).isdigit():
            positional[int(name)] = outcome_equation(indices)
    readouts: list[qc.ReadoutLike] = [
        as_references(positional[index]) for index in sorted(positional)
    ]
    authored = list(gadget.readouts)[len(observable_slots(gadget)) :]
    readouts.extend(as_readout(entry) for entry in authored)
    gadget.readouts = readouts


__all__ = [
    "GadgetReadouts",
    "ReadoutSlot",
    "as_readout",
    "flag_slots",
    "observable_slots",
    "observables_as_xor_map",
    "observe_count_of",
    "readout_equation",
    "readout_slots",
    "readouts_of",
    "set_gadget_readouts",
]
