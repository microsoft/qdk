from __future__ import annotations
from typing import TypeAlias

from collections.abc import Hashable, Iterable
from dataclasses import dataclass

from qodec import Gadget
from qodec.gadgets import Reference

ReferenceKey: TypeAlias = tuple[str, str | None, int | None, str | None, int]


@dataclass(frozen=True)
class Parity:
    variables: frozenset[Hashable] = frozenset()
    constant: bool = False

    def __xor__(self, other: Parity) -> Parity:
        return Parity(self.variables ^ other.variables, self.constant ^ other.constant)


class InconsistentParity(ValueError):
    pass


class BinarySystem:
    def __init__(self, equations: Iterable[Parity] = ()) -> None:
        self._rows: dict[Hashable, Parity] = {}
        for equation in equations:
            self.add(equation)

    def add(self, equation: Parity) -> None:
        reduced = self.reduce(equation)
        if reduced.variables:
            pivot = next(iter(reduced.variables))
            self._rows[pivot] = reduced
        elif reduced.constant:
            raise InconsistentParity("Parity equations contain contradictory evidence")

    def reduce(self, expression: Parity) -> Parity:
        for pivot, row in self._rows.items():
            if pivot in expression.variables:
                expression ^= row
        return expression

    def value(self, expression: Parity) -> bool | None:
        reduced = self.reduce(expression)
        return None if reduced.variables else reduced.constant

    def solution(self) -> dict[Hashable, bool]:
        values: dict[Hashable, bool] = {}
        for pivot, row in reversed(tuple(self._rows.items())):
            values[pivot] = row.constant ^ bool(
                sum(
                    values.get(variable, False)
                    for variable in row.variables
                    if variable != pivot
                )
                % 2
            )
        return values


def reference_key(reference: Reference) -> ReferenceKey:
    return (
        reference.kind,
        reference.boundary,
        reference.entry,
        reference.encoding_property,
        reference.index,
    )


def expression(terms: Iterable[Reference | int]) -> Parity:
    result = Parity()
    for term in terms:
        if type(term) is int and term in (0, 1):
            result ^= Parity(constant=bool(term))
        elif isinstance(term, Reference):
            for selected in term.expand():
                result ^= Parity(frozenset({reference_key(selected)}))
        else:
            raise TypeError("Parity terms must be references or integer bits")
    return result


def validate_equations(gadget: Gadget, record_count: int | None = None) -> None:
    expected = gadget.implements.observe_count + len(gadget.implements.flags)
    if len(gadget.readouts) != expected:
        raise ValueError(
            f"Gadget {gadget.implements.mnemonic!r} requires {expected} readout equations"
        )
    equations = (
        *gadget.checks,
        *(readout.equation for readout in gadget.readouts),
        *gadget.frames.values(),
    )
    for terms in equations:
        for term in terms:
            if isinstance(term, str):
                term = Reference(term)
            if isinstance(term, Reference):
                for reference in term.expand():
                    if reference.kind == "readout":
                        valid = reference.index < len(gadget.readouts)
                    elif reference.kind == "circuit_readout":
                        valid = record_count is None or reference.index < record_count
                    else:
                        encodings = (
                            gadget.inputs
                            if reference.boundary == "in"
                            else gadget.outputs
                        )
                        entry, basis = reference.entry, reference.encoding_property
                        valid = (
                            entry is not None
                            and 0 <= entry < len(encodings)
                            and basis in ("stabilizers", "x", "z")
                            and reference.index
                            < len(getattr(encodings[entry].code, basis))
                        )
                    if not valid:
                        raise ValueError(
                            f"Gadget parity reference {reference!s} is outside its declared record or boundary"
                        )
            elif type(term) is not int or term not in (0, 1):
                raise ValueError(
                    "Gadget parity terms require references or integer bits"
                )


@dataclass(frozen=True)
class FrameDelta:
    output: int
    basis: str
    logical: int
    parity: Parity


def prepare_frames(gadget: Gadget) -> tuple[FrameDelta, ...]:
    aliases: dict[int, Parity] = {}
    active: set[int] = set()

    def expand_frame(terms: Iterable[Reference | str | int]) -> Parity:
        result = Parity()
        for term in terms:
            if isinstance(term, str):
                term = Reference(term)
            if isinstance(term, Reference):
                for reference in term.expand():
                    if reference.kind == "circuit_readout":
                        result ^= expression([reference])
                    elif reference.kind == "readout":
                        index = reference.index
                        if index in active or not 0 <= index < len(gadget.readouts):
                            raise ValueError(
                                "Frame aliases must be acyclic and reference declared readouts"
                            )
                        if index not in aliases:
                            active.add(index)
                            aliases[index] = expand_frame(
                                gadget.readouts[index].equation
                            )
                            active.remove(index)
                        result ^= aliases[index]
                    else:
                        raise ValueError("Frame values cannot reference encoding signs")
            elif type(term) is int and term in (0, 1):
                result ^= Parity(constant=bool(term))
            else:
                raise ValueError(
                    "Frame terms must be record references or integer bits"
                )
        return result

    frames = []
    targets = set()
    for target, terms in gadget.frames.items():
        references = Reference(target).expand()
        if len(references) != 1:
            raise ValueError("Frame keys must identify one output logical sign")
        reference = references[0]
        entry, basis = reference.entry, reference.encoding_property
        if (
            reference.boundary != "out"
            or entry is None
            or not 0 <= entry < len(gadget.outputs)
            or basis not in ("x", "z")
        ):
            raise ValueError("Frame keys must identify an output logical sign")
        if not 0 <= reference.index < len(getattr(gadget.outputs[entry].code, basis)):
            raise ValueError("Frame logical index is outside the output code")
        key = reference_key(reference)
        if key in targets:
            raise ValueError("Frame keys must not alias the same output sign")
        targets.add(key)
        frames.append(FrameDelta(entry, basis, reference.index, expand_frame(terms)))
    return tuple(frames)
