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

from dataclasses import dataclass

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


def _has_out_stab(check: Equation) -> bool:
    return bool(stabilizer_signs_of(check, side="out"))


@dataclass
class _RecursiveEmitState:
    """Mutable state threaded through recursive multi-layer emission.

    ``frame_maps`` holds one ``(operand, stab index) -> {record indices}``
    map per translation level (frames at level *L* span level *L*'s
    gadgets); ``logical_frame_maps`` is the analogous per-level
    ``(operand, basis, index) -> {record indices}`` map for logical
    observable signs (``basis`` is ``"x"`` or ``"z"``), carrying a
    rotating logical's accumulated Pauli frame across a level's gadgets.
    ``global_rec`` is the absolute count of physical records appended so
    far.
    """

    combined: stim.Circuit
    allocator: PhysicalQubitAllocator
    global_rec: int
    frame_maps: list[StabilizerFrames]
    logical_frame_maps: list[LogicalFrames]
    noise: dict[str, float]


def _resolve_equation_records(
    equation: Equation,
    body_prov: list[frozenset[int]],
    frame_map: StabilizerFrames,
    logical_frame_map: LogicalFrames,
    gadget: qc.Gadget,
) -> set[int]:
    """XOR-resolve a parity equation to a set of physical record indices.

    An :class:`Outcome` maps to ``body_prov[k]``; an ``in`` stabilizer sign maps
    to the frame currently carrying that stabilizer's sign; an ``in`` logical
    sign maps to the frame carrying that observable's sign (empty when unseeded,
    i.e. a deterministic ``+1`` representative). An ``in`` stabilizer sign with
    no seeded frame is unsupported here (the flat path's positional fallback
    does not apply once surfaces compose explicitly).
    """
    records: set[int] = set()
    for index in outcomes_of(equation):
        if index >= len(body_prov):
            raise NotImplementedError(
                f"gadget {gadget.implements.mnemonic!r}: circuit.readouts[{index}] "
                f"is out of range (body exposes {len(body_prov)} readouts)"
            )
        records ^= set(body_prov[index])
    for sign in stabilizer_signs_of(equation, side="in"):
        if sign.key not in frame_map:
            raise NotImplementedError(
                f"gadget {gadget.implements.mnemonic!r}: input stabilizer "
                f"frame {sign.key} has not been seeded by any prior gadget; the "
                f"recursive emitter requires an explicit out.* declaration "
                f"upstream"
            )
        records ^= set(frame_map[sign.key])
    for sign in logical_signs_of(equation, side="in"):
        records ^= set(logical_frame_map.get(sign.key, frozenset()))
    return records


def _stabilizer_source_records(
    check: Equation,
    body_prov: list[frozenset[int]],
    frame_map: StabilizerFrames,
) -> frozenset[int]:
    """Records carrying the ``out`` stabilizer sign a check declares.

    Logical signs are not sources here: a stabilizer's boundary sign is fixed by
    measurements and other stabilizer frames alone.
    """
    records: set[int] = set()
    for index in outcomes_of(check):
        records ^= set(body_prov[index])
    for sign in stabilizer_signs_of(check, side="in"):
        records ^= set(frame_map.get(sign.key, frozenset()))
    return frozenset(records)


def _logical_source_records(
    check: Equation,
    body_prov: list[frozenset[int]],
    frame_map: StabilizerFrames,
    logical_frame_map: LogicalFrames,
) -> frozenset[int]:
    """Records carrying the ``out`` logical sign a check declares.

    A rotating logical's representative accumulates over other logical frames as
    well as measurements and stabilizer frames.
    """
    records = set(_stabilizer_source_records(check, body_prov, frame_map))
    for sign in logical_signs_of(check, side="in"):
        records ^= set(logical_frame_map.get(sign.key, frozenset()))
    return frozenset(records)


def _update_frame_maps_recursive(
    gadget: qc.Gadget,
    frame_map: StabilizerFrames,
    logical_frame_map: LogicalFrames,
    body_prov: list[frozenset[int]],
) -> None:
    """Apply this gadget's frame declarations using composed provenance.

    Mirrors the flat path's ``stim._update_frame_maps`` but resolves an
    :class:`Outcome` to the record set ``body_prov[k]`` and — unlike the flat
    path — seeds a *deterministic* output stabilizer (no readouts, no input
    frame) to the empty record set (an empty XOR is always ``+1``, the sign a
    fresh preparation asserts), instead of falling back to a positional record.

    A gadget's output state must be a valid codeword of its declared output
    encoding, so every output-code stabilizer has a well-defined boundary sign.
    A gadget therefore declares ``out[<e>].stabilizers[i]`` for every ``i`` —
    either an XOR of readouts and ``in`` signs (measured/propagated) or the
    empty set (deterministic preparation seed). Because every frame is
    established at preparation, later gadgets only ever *compare* against an
    existing entry; an ``in`` reference with no seeded frame is an
    under-specified qodec and is rejected (see
    :func:`_resolve_equation_records`), with no positional fallback.
    """
    new_stabilizers: StabilizerFrames = {}
    checks = parse_equations(gadget.checks)
    for check in checks:
        outs = stabilizer_signs_of(check, side="out")
        if not outs:
            continue
        records = _stabilizer_source_records(check, body_prov, frame_map)
        for sign in outs:
            new_stabilizers[sign.key] = records
    frame_map.update(new_stabilizers)

    # Logical frames resolve against the stabilizer frames this gadget just
    # declared, so they are computed after the update above.
    new_logicals: LogicalFrames = {}
    for check in checks:
        outs = logical_signs_of(check, side="out")
        if not outs:
            continue
        records = _logical_source_records(
            check, body_prov, frame_map, logical_frame_map
        )
        for sign in outs:
            new_logicals[sign.key] = records
    logical_frame_map.update(new_logicals)


def _call_readout_prov(
    gadget: qc.Gadget,
    body_prov: list[frozenset[int]],
    frame_map: StabilizerFrames,
    logical_frame_map: LogicalFrames,
) -> dict[str, frozenset[int]]:
    """Provenance of each readout the gadget exposes to its parent.

    Keyed by positional readout name (``"0"``, ``"1"``, ...); the value is the
    set of physical records whose XOR carries that readout's value. Every
    observe outcome the objective exposes must have a positional
    ``gadget.readouts`` entry.
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
            _resolve_equation_records(
                slot.equation, body_prov, frame_map, logical_frame_map, gadget
            )
        )
        for slot in slots
    }
