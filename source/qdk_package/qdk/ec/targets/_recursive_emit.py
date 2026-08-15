"""Helpers for recursive multi-layer stim emission.

This module holds the recursive-composition helpers used by
:meth:`qdk.ec.targets.stim.StimEmitter._build_circuit_recursive` to fold every
translation's decoding surface (``checks`` / ``frames`` / ``readouts``) down to
physical measurement records.

Kept separate from :mod:`qdk.ec.targets.stim` so the emitter module stays
focused on circuit assembly. Nothing here imports the emitter, so there is
no import cycle.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Literal

import stim

import qodec as qc

from .._readouts import observable_slots, observe_count_of
from .._references import (
    Basis,
    Equation,
    logical_signs_of,
    outcomes_of,
    parse_equations,
    stabilizer_signs_of,
)
from ._qubit_alloc import PhysicalQubitAllocator

#: ``(encoding entry, stabilizer index) -> records whose XOR carries its sign``.
StabilizerFrames = dict[tuple[int, int], frozenset[int]]

#: ``(encoding entry, basis, index) -> records whose XOR carries its sign``.
LogicalFrames = dict[tuple[int, Basis, int], frozenset[int]]

#: Where a gadget's boundary signs come from.
#:
#: ``"declared"`` means every referenced sign is seeded by an upstream
#: ``out[...]`` declaration: an unseeded input stabilizer is an under-specified
#: qodec, and a preparation's undeclared-source sign seeds the empty record set
#: (an empty XOR being ``+1``). ``"positional"`` is the single-edge fallback for
#: qodecs that do not declare their preparation frames: an unseeded sign
#: resolves to the empty set and the emitter reaches into the preceding
#: gadget's records by position instead.
#:
#: The two answers move together, so they are one value rather than a pair of
#: booleans that could disagree.
FrameSourcing = Literal["declared", "positional"]


def _has_out_stab(check: Equation) -> bool:
    return bool(stabilizer_signs_of(check, side="out"))


@dataclass(frozen=True)
class Provenance:
    """Which physical records carry each of a gadget body's readouts.

    This is the only thing that differs between emitting a single lowering edge
    and composing a whole layer chain. On a single edge a gadget's ``k``-th
    readout *is* its own ``k``-th record; composed, it is whatever set of
    physical records the layer below folded up into it. Everything else about
    resolving a parity equation is identical, which is why it is the one thing
    :func:`resolve_records` and :func:`update_frame_maps` take.
    """

    records: tuple[frozenset[int], ...]

    @staticmethod
    def own_records(base: int, count: int) -> "Provenance":
        """A body whose readouts are its own records, starting at ``base``."""
        return Provenance(tuple(frozenset({base + index}) for index in range(count)))

    def __len__(self) -> int:
        return len(self.records)

    def __getitem__(self, index: int) -> frozenset[int]:
        return self.records[index]


@dataclass
class FrameMaps:
    """The boundary signs in flight, as the record sets currently carrying them."""

    stabilizers: StabilizerFrames = field(default_factory=dict)
    logicals: LogicalFrames = field(default_factory=dict)


@dataclass
class _RecursiveEmitState:
    """Mutable state threaded through layer-composing emission.

    ``frames`` holds one :class:`FrameMaps` per lowering edge, since a frame at
    level *L* spans level *L*'s gadgets. ``global_rec`` is the absolute count of
    physical records appended so far.
    """

    combined: stim.Circuit
    allocator: PhysicalQubitAllocator
    global_rec: int
    frames: list[FrameMaps]
    noise: dict[str, float]


def resolve_records(
    equation: Equation,
    provenance: Provenance,
    frames: FrameMaps,
    gadget: qc.Gadget,
    *,
    sourcing: FrameSourcing = "positional",
) -> set[int]:
    """XOR-resolve a parity equation to the physical records carrying its value.

    An outcome maps through ``provenance``; an ``in`` stabilizer or logical sign
    maps to the frame currently carrying that sign. ``sourcing`` decides what an
    unseeded ``in`` stabilizer sign means — see :data:`FrameSourcing`. An
    unseeded *logical* sign is always the empty set: a deterministic ``+1``
    representative.
    """
    records: set[int] = set()
    for index in outcomes_of(equation):
        if index >= len(provenance):
            raise NotImplementedError(
                f"gadget {gadget.implements.mnemonic!r}: circuit.readouts[{index}] "
                f"is out of range (body exposes {len(provenance)} readouts)"
            )
        records ^= set(provenance[index])
    for sign in stabilizer_signs_of(equation, side="in"):
        if sourcing == "declared" and sign.key not in frames.stabilizers:
            raise NotImplementedError(
                f"gadget {gadget.implements.mnemonic!r}: input stabilizer "
                f"frame {sign.key} has not been seeded by any prior gadget; "
                f"composing layers requires an explicit out.* declaration "
                f"upstream"
            )
        records ^= set(frames.stabilizers.get(sign.key, frozenset()))
    for sign in logical_signs_of(equation, side="in"):
        records ^= set(frames.logicals.get(sign.key, frozenset()))
    return records


def _stabilizer_source_records(
    check: Equation, provenance: Provenance, frames: FrameMaps
) -> frozenset[int]:
    """Records carrying the ``out`` stabilizer sign a check declares.

    Logical signs are not sources here: a stabilizer's boundary sign is fixed by
    measurements and other stabilizer frames alone.
    """
    records: set[int] = set()
    for index in outcomes_of(check):
        records ^= set(provenance[index])
    for sign in stabilizer_signs_of(check, side="in"):
        records ^= set(frames.stabilizers.get(sign.key, frozenset()))
    return frozenset(records)


def update_frame_maps(
    gadget: qc.Gadget,
    provenance: Provenance,
    frames: FrameMaps,
    *,
    sourcing: FrameSourcing,
) -> None:
    """Apply this gadget's ``out[...]`` sign declarations to ``frames``.

    A declaration names the new record set carrying an output sign as the XOR of
    the gadget's own body readouts and any referenced input frames. Signs the
    gadget does not declare keep their existing frame, so a gadget that
    re-measures only part of the code carries the rest forward.

    A gadget's output state must be a valid codeword of its declared output
    encoding, so every output-code stabilizer has a well-defined boundary sign,
    and a gadget should declare ``out[<e>].stabilizers[i]`` for every ``i``. A
    declaration with neither readouts nor an input frame — a preparation
    asserting a deterministic sign — is seeded or left unset according to
    ``sourcing`` (see :data:`FrameSourcing`).
    """
    checks = parse_equations(gadget.checks)

    declared: StabilizerFrames = {}
    for check in checks:
        outs = stabilizer_signs_of(check, side="out")
        if not outs:
            continue
        sourced = outcomes_of(check) or stabilizer_signs_of(check, side="in")
        if not sourced and sourcing == "positional":
            continue
        records = _stabilizer_source_records(check, provenance, frames)
        for sign in outs:
            declared[sign.key] = records
    frames.stabilizers.update(declared)

    # A rotating logical's representative accumulates over other logical frames
    # as well, and resolves against the stabilizer frames just declared above.
    declared_logicals: LogicalFrames = {}
    for check in checks:
        outs_logical = logical_signs_of(check, side="out")
        if not outs_logical:
            continue
        records = frozenset(resolve_records(check, provenance, frames, gadget))
        for sign in outs_logical:
            declared_logicals[sign.key] = records
    frames.logicals.update(declared_logicals)


def exposed_readout_records(
    gadget: qc.Gadget,
    provenance: Provenance,
    frames: FrameMaps,
) -> dict[str, frozenset[int]]:
    """Physical records behind each readout the gadget exposes to its parent.

    Keyed by positional readout name (``"0"``, ``"1"``, ...); the value is the
    set of records whose XOR carries that readout's value. Every observe outcome
    the instruction declares must have a positional ``gadget.readouts`` entry.
    """
    declared = observe_count_of(gadget.implements)
    slots = observable_slots(gadget)
    if len(slots) < declared:
        raise NotImplementedError(
            f"gadget {gadget.implements.mnemonic!r} observes readout "
            f"{str(len(slots))!r} but declares no readout equation at "
            f"position {len(slots)}"
        )
    return {
        slot.name: frozenset(
            resolve_records(
                slot.equation, provenance, frames, gadget, sourcing="declared"
            )
        )
        for slot in slots
    }
