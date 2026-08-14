"""Parsers for qodec's property-path reference grammar.

A qodec parity equation is a flat list of JsonPath-style references relative
to the gadget root: ``circuit.readouts[<sel>]`` for a measurement record and
``(in|out)[<entry>].(stabilizers|x|z)[<i>]`` for a boundary encoding sign.
``qodec.Reference`` validates a path but does not decompose it, so this module
is the single place qdk.ec turns those strings into indices.
"""

from __future__ import annotations

import re
from collections.abc import Iterable
from dataclasses import dataclass

import qodec

_READOUT_RE = re.compile(r"^circuit\.readouts\[([^\]]+)\]$")
_ENCODING_REF_RE = re.compile(r"^(in|out)\[(\d+)\]\.(stabilizers|x|z)\[(\d+)\]$")


def _expand_bracket_selector(token: str) -> list[int]:
    """Expand a JsonPath bracket-selector token into explicit indices.

    Supports single index ``N``, slice ``N:M`` / ``N:M:K`` (stop-exclusive),
    and union ``N,M,P``. Returns the list of selected indices in declared order.
    """
    token = token.strip()
    if not token:
        return []
    if "," in token and ":" not in token:
        return [int(part.strip()) for part in token.split(",")]
    if ":" in token:
        parts = token.split(":")
        if len(parts) == 2:
            start, stop = int(parts[0]), int(parts[1])
            step = 1
        elif len(parts) == 3:
            start, stop, step = int(parts[0]), int(parts[1]), int(parts[2])
        else:
            return []
        if step <= 0:
            return []
        return list(range(start, stop, step))
    return [int(token)]


@dataclass(frozen=True)
class EncodingAtom:
    """A parsed ``(in|out)[<entry>].<basis>[<i>]`` encoding-sign reference.

    ``entry`` is the positional index into the gadget's ``inputs`` /
    ``outputs`` encoding list.
    """

    side: str  # "in" | "out"
    entry: int
    basis: str  # "stabilizers" | "x" | "z"
    index: int


def parse_encoding_atom(atom: object) -> EncodingAtom | None:
    """Parse a single ``(in|out)[<entry>].(stabilizers|x|z)[<i>]`` atom.

    Returns ``None`` for atoms of any other shape.
    """
    match = _ENCODING_REF_RE.match(str(atom))
    if match is None:
        return None
    return EncodingAtom(
        side=match.group(1),
        entry=int(match.group(2)),
        basis=match.group(3),
        index=int(match.group(4)),
    )


def parse_stabilizer_atom(
    atom: object, side: str | None = None
) -> tuple[int, int] | None:
    """Parse a ``(in|out)[<entry>].stabilizers[<i>]`` atom to ``(entry, index)``.

    Restricts to the ``stabilizers`` basis. When ``side`` is given the
    atom's side must match it. Returns ``None`` for any other shape.
    """
    parsed = parse_encoding_atom(atom)
    if parsed is None or parsed.basis != "stabilizers":
        return None
    if side is not None and parsed.side != side:
        return None
    return (parsed.entry, parsed.index)


def outcome_indices(atoms: Iterable[object]) -> list[int]:
    """Measurement-record indices addressed by ``circuit.readouts[<sel>]`` atoms.

    ``<sel>`` is a single index, a JsonPath slice (``N:M``, ``N:M:K``), or a
    union (``N,M,P``). Atoms of any other shape (encoding signs, declared-readout
    references) are silently ignored.
    """
    out: list[int] = []
    for atom in atoms:
        match = _READOUT_RE.match(str(atom))
        if match is not None:
            out.extend(_expand_bracket_selector(match.group(1)))
    return out


def outcome_index_of_atom(key: object) -> int:
    """Parse a single readout atom into a measurement-record index.

    Accepts ``circuit.readouts[<i>]`` or a bare decimal-string index. Unlike
    :func:`outcome_indices`, the atom must address exactly one record.
    """
    match = _READOUT_RE.match(str(key))
    if match is None:
        return int(str(key))
    indices = _expand_bracket_selector(match.group(1))
    if len(indices) != 1:
        raise ValueError(f"readout atom {key!r} must address exactly one outcome")
    return indices[0]


def readout_atoms(indices: Iterable[int]) -> list[qodec.ReferenceLike]:
    """Serialise an outcome-XOR pattern as ``circuit.readouts[<i>]`` atoms."""
    return [f"circuit.readouts[{index}]" for index in indices]


def as_references(atoms: Iterable[object]) -> list[qodec.ReferenceLike]:
    """One parity equation in the shape qodec's setters accept."""
    return [str(atom) for atom in atoms]


__all__ = [
    "EncodingAtom",
    "as_references",
    "outcome_index_of_atom",
    "outcome_indices",
    "parse_encoding_atom",
    "parse_stabilizer_atom",
    "readout_atoms",
]
