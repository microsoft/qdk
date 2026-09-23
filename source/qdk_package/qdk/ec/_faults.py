"""Internal intrinsic Pauli and readout-fault effects of qodec gadgets."""

from __future__ import annotations

from collections.abc import Iterable, Iterator, Mapping, Sequence
from dataclasses import dataclass

from binar import BitMatrix, BitVector
import qodec as qc

from ._analysis.propagation.interpreter import program_of, propagate_faults
from ._analysis.propagation.frames import FrameGroup
from ._frames import FrameMap
from ._references import LogicalSign, ReadoutSign, StabilizerSign, reference_terms
from ._analysis.propagation.pauli import Pauli, relabel
from ._analysis.propagation.pauli_remap import (
    Basis,
    encoding_qubit_relocation,
    logical_chars,
    remap_to_global,
)


@dataclass(frozen=True, init=False, repr=False)
class FaultEvent:
    """One immutable, deterministic event affecting circuit-call outputs.

    Use ``after`` for quantum errors, readout errors, or both. Multiply events
    to combine their changes, including across calls. One event costs one in a
    distance search regardless of its weight. The representation is private;
    ``repr`` shows equivalent constructor expressions using call-local indexes.
    ``str`` uses sparse Paulis and explicit call-local readouts, for example
    ``X_0 after call 2`` and ``flip call 2 readout 0``. Parentheses group mixed
    events and events spanning multiple calls.

    ``FaultEvent()`` is the identity event. The optional ``locations`` mapping
    constructs post-call Pauli errors, with the same call indexes as ``after``.
    Input Paulis are copied. The ``locations`` property returns an ordered tuple
    of ``FaultEvent.Location`` values describing the combined change at each
    affected call, not the history of fault occurrences. Locations are stored
    in call-index order; multiplication merges them without sorting again.
    """

    _locations: tuple[FaultEvent.Location, ...]

    def __init__(
        self,
        locations: Mapping[int, Pauli] | None = None,
    ) -> None:
        normalized = {
            int(location): error
            for location, error in (locations or {}).items()
            if error.weight
        }
        object.__setattr__(
            self,
            "_locations",
            tuple(
                self.Location._create(call, error, frozenset())
                for call, error in sorted(normalized.items())
            ),
        )

    @classmethod
    def _from_sorted_locations(
        cls, locations: tuple[FaultEvent.Location, ...]
    ) -> "FaultEvent":
        event = cls()
        object.__setattr__(event, "_locations", locations)
        return event

    @classmethod
    def after(
        cls,
        instruction: int,
        error: Pauli | None = None,
        *,
        readout_flips: int | Sequence[int] = (),
    ) -> "FaultEvent":
        """Affect the outputs of the call at a zero-based Circuit.calls index.

        ``error`` is a post-call Pauli on circuit qubits. ``readout_flips`` is
        one index or a sequence of indexes into this call's own readouts, starting
        at zero, not the full circuit record or gadget logical readouts. A bit
        flip changes the reported result without changing the quantum state.
        Booleans are not indexes. Call and readout bounds are checked on replay.
        """
        if not isinstance(instruction, int) or isinstance(instruction, bool):
            raise TypeError("fault call index must be an integer")
        flips = (
            (readout_flips,) if isinstance(readout_flips, int) else tuple(readout_flips)
        )
        if any(
            not isinstance(index, int) or isinstance(index, bool) for index in flips
        ):
            raise TypeError("readout indexes must be integers")
        pauli = Pauli.identity() if error is None else error
        if not pauli.weight and not flips:
            return cls()
        return cls._from_sorted_locations(
            (cls.Location._create(instruction, pauli, frozenset(flips)),)
        )

    @property
    def locations(self) -> tuple[FaultEvent.Location, ...]:
        """Combined changes ordered by zero-based ``Circuit.calls()`` index.

        Each affected call appears once. Canceled changes are omitted; the
        identity event returns an empty tuple. Locations are immutable snapshots
        and their ``error`` properties return copies.
        """
        return self._locations

    @property
    def weight(self) -> int:
        """Sum of Pauli weights and recorded-bit flips, not distance-search cost."""
        return sum(
            location._error.weight + len(location.readout_flips)
            for location in self._locations
        )

    def __mul__(self, other: "FaultEvent") -> "FaultEvent":
        if not isinstance(other, FaultEvent):
            return NotImplemented
        combined = []
        left_index = right_index = 0
        while left_index < len(self._locations) and right_index < len(other._locations):
            left = self._locations[left_index]
            right = other._locations[right_index]
            if left.after_call < right.after_call:
                combined.append(left)
                left_index += 1
            elif right.after_call < left.after_call:
                combined.append(right)
                right_index += 1
            else:
                error = left._error * right._error
                flips = left.readout_flips ^ right.readout_flips
                if error.weight or flips:
                    combined.append(
                        self.Location._create(left.after_call, error, flips)
                    )
                left_index += 1
                right_index += 1
        combined.extend(self._locations[left_index:])
        combined.extend(other._locations[right_index:])
        return type(self)._from_sorted_locations(tuple(combined))

    def __hash__(self) -> int:
        return hash(self._locations)

    def __str__(self) -> str:
        parts = []
        for location in self._locations:
            error, flips = location._error, location.readout_flips
            changes = (
                [f"{error:sparse,ascii} after call {location.after_call}"]
                if error.weight
                else []
            )
            if flips:
                label = "readout" if len(flips) == 1 else "readouts"
                changes.append(
                    f"flip call {location.after_call} {label} {', '.join(map(str, sorted(flips)))}"
                )
            description = "; ".join(changes)
            if len(changes) > 1:
                description = f"({description})"
            parts.append(description)
        if not parts:
            return "no fault"
        description = "; ".join(parts)
        return f"({description})" if len(parts) > 1 else description

    def __repr__(self) -> str:
        name = type(self).__name__
        parts = []
        for location in self._locations:
            error, flips = location._error, location.readout_flips
            arguments = [str(location.after_call)]
            if error.weight:
                arguments.append(f"Pauli({str(error)!r})")
            if flips:
                readouts = sorted(flips)
                arguments.append(
                    f"readout_flips={readouts[0] if len(readouts) == 1 else readouts}"
                )
            parts.append(f"{name}.after({', '.join(arguments)})")
        return " * ".join(parts) if parts else f"{name}()"

    @dataclass(frozen=True, init=False, repr=False, slots=True)
    class Location:
        """The combined Pauli error and readout flips after one circuit call.

        Obtain locations from ``FaultEvent.locations``. Equality and hashing
        compare the call index, Pauli error, and readout flips, not circuit
        ownership. Mutating a returned Pauli does not change this location.
        """

        _after_call: int
        _error: Pauli
        _readout_flips: frozenset[int]

        def __new__(cls) -> FaultEvent.Location:
            raise TypeError("FaultEvent.Location is returned by FaultEvent.locations")

        @classmethod
        def _create(
            cls, after_call: int, error: Pauli, readout_flips: frozenset[int]
        ) -> FaultEvent.Location:
            location = object.__new__(cls)
            object.__setattr__(location, "_after_call", after_call)
            object.__setattr__(location, "_error", error.copy())
            object.__setattr__(location, "_readout_flips", readout_flips)
            return location

        @property
        def after_call(self) -> int:
            """Zero-based ``Circuit.calls()`` index after which this fault is applied."""
            return self._after_call

        @property
        def error(self) -> Pauli:
            """Copy of the post-call Pauli on circuit qubits; identity for readout-only faults."""
            return self._error.copy()

        @property
        def readout_flips(self) -> frozenset[int]:
            """Flipped indexes within this call's own readouts, starting at zero."""
            return self._readout_flips

        def __repr__(self) -> str:
            return (
                f"FaultEvent.Location(after_call={self.after_call}, "
                f"error=Pauli({str(self._error)!r}), "
                f"readout_flips={self.readout_flips!r})"
            )


@dataclass(frozen=True, init=False, repr=False, slots=True)
class FaultEffect:
    """An immutable set of changed checks, readouts, and output signs.

    References are relative to the analyzed gadget snapshot: ``checks[i]``,
    ``readouts[i]`` (including flags), and ``out[k].{x,z,stabilizers}[i]``.
    Output signs include declared frame corrections. They describe changes
    from fault-free execution, not changes to the gadget's equations.

    Construction expands final selectors and deduplicates canonical targets;
    bounds require a gadget and are checked by analysis or resolution. Iteration
    yields References in numeric order: checks, readouts, then outputs by block
    and x/z/stabilizer index. Membership requires one selected target.

    XOR combines effects against the same target layout. Equality and hashing
    compare targets only, not gadget ownership. An empty effect means no recorded
    change, not no fault; a nonempty effect need not be a logical failure.
    """

    _references: frozenset[qc.Reference]

    def __init__(self, references: Iterable[qc.ReferenceLike] = ()) -> None:
        if isinstance(references, (str, qc.Reference)):
            raise TypeError("expected an iterable of references, not one reference")
        object.__setattr__(
            self,
            "_references",
            frozenset(
                expanded
                for reference in references
                for expanded in _effect_references(reference)
            ),
        )

    @classmethod
    def _from_references(cls, references: Iterable[qc.Reference]) -> FaultEffect:
        effect = cls()
        object.__setattr__(effect, "_references", frozenset(references))
        return effect

    @property
    def checks(self) -> tuple[qc.Reference, ...]:
        """Changed check references, in increasing check-index order."""
        return tuple(
            reference
            for reference in self
            if reference.segments[0] == qc.Reference.Field("checks")
        )

    @property
    def readouts(self) -> tuple[qc.Reference, ...]:
        """Changed readout references, including flags, in increasing index order."""
        return tuple(
            reference
            for reference in self
            if reference.segments[0] == qc.Reference.Field("readouts")
        )

    @property
    def frames(self) -> tuple[qc.Reference, ...]:
        """Changed output signs, ordered by block, x/z/stabilizers, then index."""
        return tuple(
            reference
            for reference in self
            if reference.segments[0] == qc.Reference.Field("out")
        )

    def __contains__(self, reference: qc.ReferenceLike) -> bool:
        references = _effect_references(reference)
        first = next(references)
        if next(references, None) is not None:
            raise ValueError("membership requires a single target")
        return first in self._references

    def __iter__(self) -> Iterator[qc.Reference]:
        return iter(sorted(self._references, key=_effect_order))

    def __len__(self) -> int:
        return len(self._references)

    def __xor__(self, other: FaultEffect) -> FaultEffect:
        if not isinstance(other, FaultEffect):
            return NotImplemented
        return type(self)._from_references(self._references ^ other._references)

    def __str__(self) -> str:
        return repr([reference.path for reference in self])

    def __repr__(self) -> str:
        return f"{type(self).__name__}({self})"


def _effect_references(reference: qc.ReferenceLike) -> Iterator[qc.Reference]:
    """Validate effect targets and let qodec expand and canonicalize their paths."""
    parsed = (
        reference if isinstance(reference, qc.Reference) else qc.Reference(reference)
    )
    segments = parsed.segments
    match segments[:-1]:
        case (qc.Reference.Field("checks" | "readouts"),):
            pass
        case (
            qc.Reference.Field("out"),
            qc.Reference.Index(),
            qc.Reference.Field("x" | "z" | "stabilizers"),
        ) | (
            qc.Reference.Field("out"),
            qc.Reference.Index(),
            qc.Reference.Field("code"),
            qc.Reference.Field("x" | "z" | "stabilizers"),
        ):
            pass
        case _:
            raise ValueError(f"unsupported fault-effect target {parsed.path!r}")
    match segments[-1]:
        case qc.Reference.Index() | qc.Reference.Slice() | qc.Reference.Union():
            return iter(parsed.expand())
        case _:
            raise ValueError(f"fault-effect target {parsed.path!r} requires an index")


def _effect_order(reference: qc.Reference) -> tuple[int, int, int, int]:
    match reference.segments:
        case (
            qc.Reference.Field(("checks" | "readouts") as field),
            qc.Reference.Index(index),
        ):
            return (0 if field == "checks" else 1, 0, 0, index)
        case (
            qc.Reference.Field("out"),
            qc.Reference.Index(entry),
            qc.Reference.Field(("x" | "z" | "stabilizers") as field),
            qc.Reference.Index(index),
        ):
            return (2, entry, ("x", "z", "stabilizers").index(field), index)
        case _:
            raise ValueError(f"unsupported fault-effect target {reference.path!r}")


def fault_effects_of(
    gadget: qc.Gadget, basis: Sequence[FaultEvent]
) -> tuple[FaultEffect, ...]:
    """Map an explicit Pauli/readout fault basis to probability-free effects.

    Positionally aligned with ``basis``. The whole basis is evaluated in one
    simulation, which is why there is no single-fault entry point.
    """
    return _gadget_fault_data(gadget, basis)[0]


def _gadget_fault_data(
    gadget: qc.Gadget,
    basis: Sequence[FaultEvent],
    *,
    observables: FrameGroup = FrameGroup(()),
) -> tuple[tuple[FaultEffect, ...], tuple[frozenset[int], ...]]:
    """Return effects and extra probe flips from one propagation."""
    fault_basis = tuple(basis)
    if not fault_basis:
        return (), ()

    program = program_of(gadget)
    z_probes, z_layout = _build_basis_probes(gadget.outputs, "Z")
    x_probes, x_layout = _build_basis_probes(gadget.outputs, "X")
    stabilizer_probes = []
    stabilizer_paths = []
    for entry, encoding in enumerate(gadget.outputs):
        relocation = encoding_qubit_relocation(encoding)
        for index, operator in enumerate(encoding.code.stabilizers):
            stabilizer_probes.append(relabel(Pauli(operator), relocation))
            stabilizer_paths.append(f"out[{entry}].stabilizers[{index}]")
    probes = z_probes + x_probes + stabilizer_probes
    deltas, hidden_count, outcome_count = propagate_faults(
        program,
        fault_basis,
        probes + [item.pauli for item in observables.generators],
        residual_frames=[frozenset() for _ in probes]
        + [item.frame for item in observables.generators],
    )
    paths = [
        *(f"circuit.readouts[{index}]" for index in range(outcome_count)),
        *(f"out[{entry}].z[{index}]" for entry, index in z_layout),
        *(f"out[{entry}].x[{index}]" for entry, index in x_layout),
        *stabilizer_paths,
    ]
    values = {
        path: BitVector(
            bool(deltas[hidden_count + row, fault]) for fault in range(len(fault_basis))
        )
        for row, path in enumerate(paths)
    }
    frames = FrameMap(gadget)
    zero = BitVector.zeros(len(fault_basis))
    values["1"] = zero
    frame_values = frames.evaluate(values, zero)
    for target, correction in frame_values.items():
        values[target] = values[target] ^ correction
    for index, path in enumerate(paths[outcome_count:]):
        for fault in range(len(fault_basis)):
            deltas[hidden_count + outcome_count + index, fault] = values[path][fault]
    for index, observable in enumerate(observables.generators):
        correction = frames.for_probe(observable.pauli, values, zero)
        for fault in correction.support:
            deltas[hidden_count + outcome_count + len(probes) + index, fault] ^= True
    checks, readouts = _parity_effects(gadget, values, len(fault_basis))
    columns = (
        [
            (qc.Reference(f"checks[{index}]"), value)
            for index, value in enumerate(checks)
        ]
        + [
            (qc.Reference(f"readouts[{index}]"), value)
            for index, value in enumerate(readouts)
        ]
        + [(qc.Reference(path), values[path]) for path in paths[outcome_count:]]
    )
    effects = tuple(
        FaultEffect._from_references(
            reference for reference, value in columns if value[fault_index]
        )
        for fault_index in range(len(fault_basis))
    )
    indicators = _probe_flips(
        deltas,
        hidden_count + outcome_count + len(probes),
        len(observables.generators),
        len(fault_basis),
    )
    return effects, indicators


def _probe_flips(
    deltas: BitMatrix, offset: int, probe_count: int, fault_count: int
) -> tuple[frozenset[int], ...]:
    return tuple(
        frozenset(
            probe for probe in range(probe_count) if deltas[offset + probe, fault]
        )
        for fault in range(fault_count)
    )


def _parity_effects(
    gadget: qc.Gadget, values: Mapping[str, BitVector], fault_count: int
) -> tuple[list[BitVector], list[BitVector]]:
    count = len(gadget.readouts)

    def equation(
        references: Sequence[qc.Reference | int],
    ) -> tuple[BitVector, BitVector]:
        external = BitVector.zeros(fault_count)
        readouts = BitVector.zeros(count)
        for reference in references:
            if isinstance(reference, int):
                continue
            for term in reference_terms(reference):
                if isinstance(term, ReadoutSign):
                    if term.index >= count:
                        raise ValueError(f"readout reference {term} is out of bounds")
                    readouts[term.index] = not readouts[term.index]
                    continue
                if isinstance(term, (LogicalSign, StabilizerSign)):
                    encodings = gadget.inputs if term.side == "in" else gadget.outputs
                    entry = term.entry
                    field = (
                        term.basis if isinstance(term, LogicalSign) else "stabilizers"
                    )
                    if entry >= len(encodings) or term.index >= len(
                        getattr(encodings[entry].code, field)
                    ):
                        raise ValueError(f"encoding reference {term} is out of bounds")
                    if term.side == "in":
                        continue
                path = str(term)
                if path not in values:
                    raise ValueError(f"circuit reference {term} is out of bounds")
                external = external ^ values[path]
        return external, readouts

    definitions = [equation(readout.equation) for readout in gadget.readouts]
    readout_values = [value for value, _ in definitions]
    if any(dependencies.weight for _, dependencies in definitions):
        matrix = BitMatrix.zeros(count, count + fault_count)
        for index, (value, dependencies) in enumerate(definitions):
            for dependency in dependencies.support:
                matrix[index, dependency] = True
            matrix[index, index] = not matrix[index, index]
            for fault in range(fault_count):
                matrix[index, count + fault] = value[fault]
        if matrix.echelonize() != list(range(count)):
            raise ValueError(
                "readout equations do not uniquely determine fault effects"
            )
        readout_values = list(
            matrix.submatrix(
                list(range(count)), list(range(count, count + fault_count))
            ).rows
        )
    checks = []
    for check in gadget.checks:
        value, dependencies = equation(check)
        for index in dependencies.support:
            value = value ^ readout_values[index]
        checks.append(value)
    return checks, readout_values


def _build_basis_probes(
    encodings: Sequence[qc.gadgets.Encoding], basis: Basis
) -> tuple[list[Pauli], list[tuple[int, int]]]:
    probes = []
    layout = []
    for entry, encoding in enumerate(encodings):
        relocation = encoding_qubit_relocation(encoding)
        for index, characters in enumerate(logical_chars(encoding.code, basis)):
            probes.append(remap_to_global(characters, relocation))
            layout.append((entry, index))
    return probes, layout


__all__ = [
    "FaultEffect",
    "FaultEvent",
]
