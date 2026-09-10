"""Internal intrinsic Pauli-fault effects of qodec gadgets."""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass, field
from types import MappingProxyType

from binar import BitMatrix, BitVector
import qodec as qc

from ._analysis.propagation.interpreter import program_of, propagate_faults
from ._analysis.propagation.frames import FrameGroup
from ._analysis.propagation.pauli import Pauli, PauliCharacter, relabel
from ._analysis.propagation.pauli_remap import (
    Basis,
    encoding_qubit_relocation,
    logical_chars,
    remap_to_global,
)


@dataclass(frozen=True)
class FaultEvent:
    """One deterministic Pauli fault injected after named instructions."""

    locations: Mapping[int, Pauli]

    def __post_init__(self) -> None:
        normalized = {
            int(location): error
            for location, error in self.locations.items()
            if error.weight
        }
        object.__setattr__(self, "locations", MappingProxyType(normalized))

    @classmethod
    def after(cls, instruction: int, error: Pauli) -> "FaultEvent":
        return cls({instruction: error})

    @property
    def weight(self) -> int:
        return sum(error.weight for error in self.locations.values())

    def __mul__(self, other: "FaultEvent") -> "FaultEvent":
        combined = dict(self.locations)
        for location, error in other.locations.items():
            product = combined.get(location, Pauli.identity()) * error
            if product.weight:
                combined[location] = product
            else:
                combined.pop(location, None)
        return FaultEvent(combined)

    def __hash__(self) -> int:
        return hash(
            tuple(
                sorted(
                    (location, str(error)) for location, error in self.locations.items()
                )
            )
        )


@dataclass(frozen=True)
class FaultEffect:
    """What one fault does at a gadget's checks, readouts, and outputs."""

    syndrome: frozenset[int] = field(default_factory=frozenset)
    readout_flips: frozenset[int] = field(default_factory=frozenset)
    output_error: Mapping[int, Pauli] = field(default_factory=dict)

    def __post_init__(self) -> None:
        object.__setattr__(
            self, "output_error", MappingProxyType(dict(self.output_error))
        )

    def __hash__(self) -> int:
        output = tuple(
            sorted((entry, str(error)) for entry, error in self.output_error.items())
        )
        return hash((self.syndrome, self.readout_flips, output))


def fault_effects_of(
    gadget: qc.Gadget, basis: Sequence[FaultEvent]
) -> tuple[FaultEffect, ...]:
    """Map an explicit Pauli fault basis to probability-free effects.

    Positionally aligned with ``basis``. The whole basis is evaluated in one
    simulation, which is why there is no single-fault entry point.
    """
    return _gadget_fault_data(gadget, basis)[0]


def _gadget_fault_data(
    gadget: qc.Gadget,
    basis: Sequence[FaultEvent],
    *,
    observables: FrameGroup = FrameGroup(()),
) -> tuple[
    tuple[FaultEffect, ...], tuple[frozenset[int], ...], tuple[frozenset[int], ...]
]:
    """Return effects, output syndromes, and extra probe flips from one propagation."""
    fault_basis = tuple(basis)
    if not fault_basis:
        return (), (), ()

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
    z_offset = hidden_count + outcome_count
    x_offset = z_offset + len(z_probes)
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
    checks, readouts = _parity_effects(gadget, values, len(fault_basis))
    effects = []
    for fault_index in range(len(fault_basis)):
        flipped_checks = frozenset(
            index for index, value in enumerate(checks) if value[fault_index]
        )
        readout_flips = frozenset(
            index for index, value in enumerate(readouts) if value[fault_index]
        )
        z_flips = {
            index
            for index in range(len(z_probes))
            if deltas[z_offset + index, fault_index]
        }
        x_flips = {
            index
            for index in range(len(x_probes))
            if deltas[x_offset + index, fault_index]
        }
        effects.append(
            FaultEffect(
                flipped_checks,
                readout_flips,
                _combine_residual_passes(
                    gadget.outputs,
                    z_flips,
                    z_layout,
                    x_flips,
                    x_layout,
                ),
            )
        )
    output_syndromes = tuple(
        frozenset(
            index
            for index, path in enumerate(stabilizer_paths)
            if values[path][fault_index]
        )
        for fault_index in range(len(fault_basis))
    )
    indicators = _probe_flips(
        deltas,
        hidden_count + outcome_count + len(probes),
        len(observables.generators),
        len(fault_basis),
    )
    return tuple(effects), output_syndromes, indicators


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
        references: Sequence[qc.gadgets.Reference],
    ) -> tuple[BitVector, BitVector]:
        external = BitVector.zeros(fault_count)
        readouts = BitVector.zeros(count)
        for reference in references:
            for term in reference.expand():
                if term.kind == "readout":
                    if term.index >= count:
                        raise ValueError(f"readout reference {term} is out of bounds")
                    readouts[term.index] = not readouts[term.index]
                    continue
                if term.kind == "encoding":
                    encodings = (
                        gadget.inputs if term.boundary == "in" else gadget.outputs
                    )
                    entry, field = term.entry, term.encoding_property
                    assert entry is not None and field is not None
                    if entry >= len(encodings) or term.index >= len(
                        getattr(encodings[entry].code, field)
                    ):
                        raise ValueError(f"encoding reference {term} is out of bounds")
                    if term.boundary == "in":
                        continue
                    path = f"out[{entry}].{field}[{term.index}]"
                else:
                    path = f"circuit.readouts[{term.index}]"
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


def _combine_residual_passes(
    encodings: Sequence[qc.gadgets.Encoding],
    z_flips: set[int],
    z_layout: list[tuple[int, int]],
    x_flips: set[int],
    x_layout: list[tuple[int, int]],
) -> dict[int, Pauli]:
    residuals: dict[int, dict[int, PauliCharacter]] = {
        entry: {} for entry in range(len(encodings))
    }
    flips: dict[tuple[int, int], dict[str, bool]] = {}
    for index, key in enumerate(z_layout):
        if index in z_flips:
            flips.setdefault(key, {})["x"] = True
    for index, key in enumerate(x_layout):
        if index in x_flips:
            flips.setdefault(key, {})["z"] = True
    for (encoding, logical), value in flips.items():
        x_residual = value.get("x", False)
        z_residual = value.get("z", False)
        if x_residual and z_residual:
            basis: PauliCharacter = "Y"
        elif x_residual:
            basis = "X"
        elif z_residual:
            basis = "Z"
        else:
            continue
        residuals[encoding][logical] = basis
    return {name: Pauli(characters) for name, characters in residuals.items()}


__all__ = [
    "FaultEffect",
    "FaultEvent",
]
