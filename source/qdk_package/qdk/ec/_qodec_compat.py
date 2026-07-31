"""Bridge between qdk.ec's analysis helpers and the current ``qodec.Gadget`` API.

A pre-0029 ``Gadget`` exposed separate ``observables``/``flags`` fields, a
``body`` circuit, a named-operand ``realization`` channel, and a settable
``fault_model``. The current model unifies all of that:

- ``Gadget.implements`` is the realized ISA ``Instruction`` (was ``objective``).
- ``Gadget.circuit`` is the program source plus its target ISA (was ``body``).
- ``Gadget.inputs`` / ``Gadget.outputs`` are positional ``Encoding`` lists; the
  named-operand ``realization`` channel is gone.
- ``Gadget.checks`` is ``list[list[Reference]]`` — each inner list a flat parity
  equation of atom strings (``circuit.readouts[<i>]``,
  ``(in|out)[<entry>].{stabilizers,x,z}[<i>]``).
- ``Gadget.readouts`` is one positional list merging the old observables and
  flags: the implemented instruction's ``observe`` outcomes first, then its
  ``flags:`` flags (each a single parity). Each entry is a bare parity equation
  (``list[Reference]``) or a single-key ``{name: equation}`` mapping.
- Fault models are no longer a qodec concept.

This module supplies the small bridge qdk.ec's analysis layer uses to read that
model without duplicating the atom-parsing logic at every call site.
"""

from __future__ import annotations

import re
from collections.abc import Iterable, Mapping
from dataclasses import dataclass, field

import qodec

#: Matches a measurement-record atom. The current grammar spells it
#: ``circuit.readouts[<i>]``; the legacy ``body.readouts[<i>]`` spelling is still
#: accepted on input so partially-migrated artifacts keep parsing.
_READOUT_RE = re.compile(r"^(?:circuit|body)\.readouts(?:\.(\d+)|\[([^\]]+)\])$")
_ENCODING_REF_RE = re.compile(
    r"^(in|out)\[(\d+)\]\." r"(stabilizers|x|z)(?:\.(\d+)|\[(\d+)\])$"
)


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
    ``outputs`` encoding list (the property-path grammar is positional;
    the old operand-name form ``in.<name>.`` is gone).
    """

    side: str  # "in" | "out"
    entry: int
    basis: str  # "stabilizers" | "x" | "z"
    index: int


def parse_encoding_atom(atom: str) -> EncodingAtom | None:
    """Parse a single ``(in|out)[<entry>].(stabilizers|x|z)[<i>]`` atom.

    Accepts both the dot (``.<i>``) and bracket (``[<i>]``) trailing-index
    shapes. Returns ``None`` for atoms of any other shape.
    """
    match = _ENCODING_REF_RE.match(str(atom))
    if match is None:
        return None
    return EncodingAtom(
        side=match.group(1),
        entry=int(match.group(2)),
        basis=match.group(3),
        index=int(match.group(4) or match.group(5)),
    )


def parse_stabilizer_atom(atom: str, side: str | None = None) -> tuple[int, int] | None:
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


@dataclass(frozen=True)
class EncodingView:
    """A positional qodec ``Encoding`` presented with a positional ``operand``.

    The pre-positional model keyed encodings by an ``operand`` name; the
    current model keys them by position in the gadget's ``inputs`` /
    ``outputs`` list. This view exposes the positional ``entry`` index as a
    string ``operand`` so that reference strings built as
    ``f"in[{enc.operand}].stabilizers[{i}]"`` land on the positional grammar,
    and so that the (entry-indexed) operand can still be used as a dict key
    to correlate in/out encodings and residuals.
    """

    entry: int
    code: qodec.Code
    support: list[str]

    @property
    def operand(self) -> str:
        return str(self.entry)


@dataclass(frozen=True)
class Channel:
    """A ``Gadget`` presented as a circuit-plus-encodings channel.

    Bundles the gadget's program (``isa`` + ``body`` source, with the parsed
    ``instructions`` available lazily) and its positional boundary encodings
    (``encoding_in`` / ``encoding_out``), so analysis code can read a gadget
    uniformly regardless of how it was authored.

    ``instructions`` is parsed on demand from the underlying circuit: structural
    analysis that only needs the encodings never triggers the (sometimes
    partial) source parse, so a parse failure surfaces only to the semantic
    callers that actually walk the program.
    """

    isa: qodec.InstructionSet
    body: str  # the circuit source text
    encoding_in: list[EncodingView]
    encoding_out: list[EncodingView]
    _circuit: "qodec.Circuit" = field(repr=False, compare=False)

    @property
    def instructions(self) -> list[qodec.instructions.InstructionCall]:
        """The circuit's instruction calls, parsed from the source on demand."""
        return list(self._circuit.instructions)


def realization(gadget: qodec.Gadget) -> Channel:
    """Present ``gadget`` as a :class:`Channel` (circuit + positional encodings).

    ``realization(gadget).encoding_in[k].operand`` is ``str(k)`` — the
    positional entry index, matching the positional reference grammar. The
    circuit source is not parsed until :attr:`Channel.instructions` is read.
    """
    circuit = gadget.circuit
    return Channel(
        isa=circuit.isa,
        body=circuit.source,
        encoding_in=[
            EncodingView(index, encoding.code, list(encoding.support))
            for index, encoding in enumerate(gadget.inputs)
        ],
        encoding_out=[
            EncodingView(index, encoding.code, list(encoding.support))
            for index, encoding in enumerate(gadget.outputs)
        ],
        _circuit=circuit,
    )


def observe_count(gadget: qodec.Gadget) -> int:
    """Number of ``observe`` outcome bits the gadget's instruction declares.

    These are the leading entries of ``gadget.readouts`` (the observables);
    the remaining ``len(gadget.implements.flags)`` entries are the flags.
    """
    return sum(
        len(action.observables)
        for action in gadget.implements.action
        if isinstance(action, qodec.actions.Observe)
    )


def _readout_equation(entry: "list[object] | Mapping[str, list[object]]") -> list[str]:
    """The flat atom-string list of one ``gadget.readouts`` entry.

    A readout entry is either a bare parity equation (a list of references)
    or a single-key ``{name: equation}`` mapping; both reduce to the same
    flat atom list.
    """
    if isinstance(entry, Mapping):
        (equation,) = entry.values()
        return [str(atom) for atom in equation]
    return [str(atom) for atom in entry]


def outcome_indices(atoms: Iterable[str]) -> list[int]:
    """Realization-outcome indices addressed by ``circuit.readouts[<sel>]`` atoms.

    ``<sel>`` is a single index, a JsonPath slice (``N:M``, ``N:M:K``), or a
    union (``N,M,P``). The legacy ``body.readouts`` spelling is also accepted.
    Atoms of any other shape (encoding stabilizers, declared-readout
    references) are silently ignored.
    """
    out: list[int] = []
    for atom in atoms:
        match = _READOUT_RE.match(str(atom))
        if match is None:
            continue
        dot_index, bracket_token = match.group(1), match.group(2)
        if dot_index is not None:
            out.append(int(dot_index))
        elif bracket_token is not None:
            out.extend(_expand_bracket_selector(bracket_token))
    return out


def outcome_index_of_atom(key: str) -> int:
    """Parse a single readout atom into a realization-outcome index.

    Accepts the ``circuit.readouts[<i>]`` bracket atom shape (or the legacy
    ``body.readouts`` spelling, dot or bracket), or a bare decimal-string
    outcome index. Unlike :func:`outcome_indices`, the bracket form must
    address exactly one index (single-outcome atoms never carry
    slices/unions).
    """
    match = _READOUT_RE.match(str(key))
    if match is not None:
        dot_index, bracket_token = match.group(1), match.group(2)
        if dot_index is not None:
            return int(dot_index)
        indices = _expand_bracket_selector(bracket_token)
        if len(indices) != 1:
            raise ValueError(f"readout atom {key!r} must address exactly one outcome")
        return indices[0]
    return int(str(key))


def observables_as_xor_map(gadget: "qodec.Gadget") -> dict[str, list[int]]:
    """Realization observables: positional name → realization-outcome XOR.

    The observables are the *leading* entries of ``gadget.readouts`` — one per
    ``observe`` outcome of the implemented instruction (see
    :func:`observe_count`). The trailing flag entries are deliberately
    excluded: a flag is a decoder-blind side-channel bit, not a logical
    observable. Each entry is keyed by its position as a string (``"0"``,
    ``"1"``, ...).
    """
    n_observables = min(observe_count(gadget), len(gadget.readouts))
    return {
        str(position): outcome_indices(_readout_equation(gadget.readouts[position]))
        for position in range(n_observables)
    }


def observable_names(gadget: "qodec.Gadget") -> list[str]:
    """Names addressable through :func:`observables_as_xor_map` for ``gadget``.

    One name per *bound* observe outcome (the leading readout entries), as the
    position string (``"0"``, ``"1"``, ...). A gadget that declares fewer
    readouts than its instruction has observe outcomes binds only the leading
    ones; the rest are reported missing by the auditor.
    """
    return [
        str(position)
        for position in range(min(observe_count(gadget), len(gadget.readouts)))
    ]


def check_outcomes(check_atoms: Iterable[str]) -> list[int]:
    """Realization-outcome indices addressed by a check's atom list.

    A convenience wrapper over :func:`outcome_indices` for the atoms of one
    ``gadget.checks`` parity equation.
    """
    return outcome_indices(check_atoms)


def readout_atoms(outcome_indices_in: Iterable[int]) -> list[str]:
    """Serialise an outcome-XOR pattern as a list of ``circuit.readouts[<i>]`` atoms."""
    return [f"circuit.readouts[{i}]" for i in outcome_indices_in]


def set_gadget_readouts(
    gadget: "qodec.Gadget", named_xor: Mapping[str, Iterable[int]]
) -> None:
    """Set the observe-outcome entries of ``gadget.readouts`` from an XOR map.

    ``named_xor`` is a position-keyed observable-XOR map (decimal-string keys
    ``"0"``, ``"1"``, ...); each becomes one ``circuit.readouts[...]`` parity
    equation, in positional order. Non-positional (flag-named) keys are ignored.

    Any pre-authored trailing flag entries (those past the observe-outcome
    count) are preserved: flags carry no Pauli expectation, so they are authored
    by hand rather than discovered, and re-deriving the observables must not
    drop them.
    """
    positional: dict[int, list[str]] = {}
    for name, indices in named_xor.items():
        if str(name).isdigit():
            positional[int(name)] = readout_atoms(indices)
    observables = [positional[i] for i in sorted(positional)]
    flags = list(gadget.readouts)[observe_count(gadget) :]
    gadget.readouts = observables + flags
