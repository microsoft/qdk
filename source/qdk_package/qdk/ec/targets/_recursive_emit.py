"""Helpers for recursive multi-layer stim emission.

This module holds the property-path *atom* parsers shared by both stim
emission paths, plus the recursive-composition helpers used by
:meth:`qdk.ec.targets.stim.StimEmitter._build_circuit_recursive` to fold
every translation's decoding surface (``checks`` / ``frames`` /
``readouts``) down to physical measurement records.

Kept separate from :mod:`qdk.ec.targets.stim` so the emitter module stays
focused on circuit assembly. Nothing here imports the emitter, so there is
no import cycle.
"""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass

import stim

import qodec

from .._qodec_compat import (
    check_outcomes,
    parse_encoding_atom,
    parse_stabilizer_atom,
)
from ._qubit_alloc import PhysicalQubitAllocator


def _parse_stab_in_atom(atom: str) -> tuple[int, int] | None:
    return parse_stabilizer_atom(atom, side="in")


def _parse_stab_out_atom(atom: str) -> tuple[int, int] | None:
    return parse_stabilizer_atom(atom, side="out")


def _parse_logical_in_atom(atom: str) -> tuple[int, str, int] | None:
    """Parse an ``in[<entry>].(x|z)[i]`` logical-observable sign atom.

    Returns ``(entry, basis, index)`` with ``basis in {"x", "z"}``, or
    ``None`` for any other shape (including stabilizer atoms).
    """
    parsed = parse_encoding_atom(atom)
    if parsed is None or parsed.basis not in ("x", "z") or parsed.side != "in":
        return None
    return (parsed.entry, parsed.basis, parsed.index)


def _parse_logical_out_atom(atom: str) -> tuple[int, str, int] | None:
    """Parse an ``out[<entry>].(x|z)[i]`` logical-observable sign atom."""
    parsed = parse_encoding_atom(atom)
    if parsed is None or parsed.basis not in ("x", "z") or parsed.side != "out":
        return None
    return (parsed.entry, parsed.basis, parsed.index)


def _has_out_stab(check: Sequence[str]) -> bool:
    return any(str(atom).startswith("out[") for atom in check)


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
    frame_maps: list[dict[tuple[int, int], frozenset[int]]]
    logical_frame_maps: list[dict[tuple[int, str, int], frozenset[int]]]
    noise: dict[str, float]


def _observe_names(gadget: qodec.Gadget) -> list[str]:
    """Ordered readout names this gadget's objective exposes to its parent.

    Observe outcomes are positional in the current model, so these are the
    string indices ``"0"``, ``"1"``, ... of the objective's ``Observe``
    observables, in declaration order. A parent gadget's ``body.readouts``
    index this gadget's outputs in exactly this order.
    """
    from qodec.actions import Observe  # local import to avoid cycle

    names: list[str] = []
    position = 0
    for atom in gadget.implements.action:
        if isinstance(atom, Observe):
            for _obs in atom.observables:
                names.append(str(position))
                position += 1
    return names


def _resolve_atoms_records(
    atoms: Sequence[str],
    body_prov: list[frozenset[int]],
    frame_map: dict[tuple[int, int], frozenset[int]],
    logical_frame_map: dict[tuple[int, str, int], frozenset[int]],
    gadget: qodec.Gadget,
) -> set[int]:
    """XOR-resolve a parity equation to a set of physical record indices.

    ``body.readouts[k]`` maps to ``body_prov[k]``; ``in.<op>.stab[i]`` maps
    to the frame currently carrying that stabilizer's sign; ``in.<op>.(x|z)[i]``
    maps to the logical frame carrying that observable's sign (empty when
    unseeded, i.e. a deterministic ``+1`` representative). An ``in``
    stabilizer reference with no seeded frame is unsupported here (the flat
    path's positional fallback does not apply once surfaces compose
    explicitly).
    """
    records: set[int] = set()
    for index in check_outcomes(atoms):
        if index >= len(body_prov):
            raise NotImplementedError(
                f"gadget {gadget.implements.mnemonic!r}: body.readouts[{index}] "
                f"is out of range (body exposes {len(body_prov)} readouts)"
            )
        records ^= set(body_prov[index])
    for atom in atoms:
        ref = _parse_stab_in_atom(atom)
        if ref is None:
            continue
        if ref not in frame_map:
            raise NotImplementedError(
                f"gadget {gadget.implements.mnemonic!r}: input stabilizer "
                f"frame {ref} has not been seeded by any prior gadget; the "
                f"recursive emitter requires an explicit out.* declaration "
                f"upstream"
            )
        records ^= set(frame_map[ref])
    for atom in atoms:
        logical_ref = _parse_logical_in_atom(atom)
        if logical_ref is not None:
            records ^= set(logical_frame_map.get(logical_ref, frozenset()))
    return records


def _update_frame_map_recursive(
    gadget: qodec.Gadget,
    frame_map: dict[tuple[int, int], frozenset[int]],
    logical_frame_map: dict[tuple[int, str, int], frozenset[int]],
    body_prov: list[frozenset[int]],
) -> None:
    """Apply this gadget's frame declarations using composed provenance.

    Mirrors ``stim._update_frame_map`` but resolves ``body.readouts[k]`` to
    the record set ``body_prov[k]`` and — unlike the flat path — seeds a
    *deterministic* output stabilizer (no readouts, no input frame) to the
    empty record set (an empty XOR is always ``+1``, the sign a fresh
    preparation asserts), instead of falling back to a positional record.

    This implements the agreed frame-seeding model (findings doc Q2): a
    gadget's output state must be a valid codeword of its declared output
    encoding, so every output-code stabilizer has a well-defined boundary
    sign. A gadget therefore declares ``out.<op>.stabilizers[i]`` for every
    ``i`` — either ``XOR(body.readouts…, in…)`` (measured/propagated) or the
    empty set (deterministic preparation seed). Because every frame is
    established at preparation, later gadgets only ever *compare* against an
    existing entry; an ``in`` reference with no seeded frame is an
    under-specified codec and is rejected (see
    :func:`_resolve_atoms_records`), with no positional fallback.
    """
    new_entries: dict[tuple[int, int], frozenset[int]] = {}

    def record_declaration(
        out_refs: list[tuple[int, int]],
        outcome_indices: list[int],
        in_refs: list[tuple[int, int]],
    ) -> None:
        if not out_refs:
            return
        records: set[int] = set()
        for index in outcome_indices:
            records ^= set(body_prov[index])
        for in_ref in in_refs:
            records ^= set(frame_map.get(in_ref, frozenset()))
        frozen = frozenset(records)
        for out_ref in out_refs:
            new_entries[out_ref] = frozen

    for check in gadget.checks:
        out_refs = [
            ref
            for ref in (_parse_stab_out_atom(atom) for atom in check)
            if ref is not None
        ]
        if not out_refs:
            continue
        in_refs = [
            ref
            for ref in (_parse_stab_in_atom(atom) for atom in check)
            if ref is not None
        ]
        record_declaration(out_refs, list(check_outcomes(check)), in_refs)

    frame_map.update(new_entries)

    # Logical (x/z) frames use REPLACE semantics (full XOR of the declared
    # source atoms), exactly like stabilizer frames. A check that carries an
    # ``out[entry].(x|z)[i]`` atom re-expresses that rotating logical's
    # representative; the record set carrying its sign is the XOR of the
    # check's body readouts, stabilizer in-frames, and logical in-frames.
    # Static-logical codecs (c4, surface) declare no out-logical atoms, so
    # this leaves ``logical_frame_map`` untouched.
    new_logical: dict[tuple[int, str, int], frozenset[int]] = {}
    for check in gadget.checks:
        logical_outs = [
            ref
            for ref in (_parse_logical_out_atom(atom) for atom in check)
            if ref is not None
        ]
        if not logical_outs:
            continue
        records: set[int] = set()
        for index in check_outcomes(check):
            records ^= set(body_prov[index])
        for atom in check:
            stab_ref = _parse_stab_in_atom(atom)
            if stab_ref is not None:
                records ^= set(frame_map.get(stab_ref, frozenset()))
                continue
            logical_ref = _parse_logical_in_atom(atom)
            if logical_ref is not None:
                records ^= set(logical_frame_map.get(logical_ref, frozenset()))
        frozen = frozenset(records)
        for logical_out in logical_outs:
            new_logical[logical_out] = frozen
    logical_frame_map.update(new_logical)


def _call_readout_prov(
    gadget: qodec.Gadget,
    body_prov: list[frozenset[int]],
    frame_map: dict[tuple[int, int], frozenset[int]],
    logical_frame_map: dict[tuple[int, str, int], frozenset[int]],
) -> dict[str, frozenset[int]]:
    """Provenance of each readout the gadget exposes to its parent.

    Keyed by positional readout name (``"0"``, ``"1"``, ...); the value is the
    set of physical records whose XOR carries that readout's value. Every
    observe outcome the objective exposes must have a positional
    ``gadget.readouts`` entry.
    """
    prov: dict[str, frozenset[int]] = {}
    readouts = gadget.readouts
    for position, name in enumerate(_observe_names(gadget)):
        if position >= len(readouts):
            raise NotImplementedError(
                f"gadget {gadget.implements.mnemonic!r} observes readout "
                f"{name!r} but declares no readout equation at position {position}"
            )
        atoms = readouts[position]
        prov[name] = frozenset(
            _resolve_atoms_records(
                atoms, body_prov, frame_map, logical_frame_map, gadget
            )
        )
    return prov
