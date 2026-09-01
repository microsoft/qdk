"""Internal intrinsic Pauli-fault effects of qodec gadgets."""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass, field
from types import MappingProxyType

import qodec as qc

from ._analysis.propagation.interpreter import program_of, propagate_faults
from ._analysis.propagation.pauli import Pauli, PauliCharacter
from ._analysis.propagation.pauli_remap import (
    Basis,
    encoding_qubit_relocation,
    logical_chars,
    remap_to_global,
)
from ._readouts import readout_slots
from ._references import outcomes_of, parse_equations


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
    fault_basis = tuple(basis)
    if not fault_basis:
        return ()

    program = program_of(gadget)
    checks = [outcomes_of(check) for check in parse_equations(gadget.checks)]
    readouts = [outcomes_of(slot.equation) for slot in readout_slots(gadget)]
    z_probes, z_layout = _build_basis_probes(gadget.outputs, "Z")
    x_probes, x_layout = _build_basis_probes(gadget.outputs, "X")
    deltas, hidden_count, outcome_count = propagate_faults(
        program, fault_basis, z_probes + x_probes
    )
    z_offset = hidden_count + outcome_count
    x_offset = z_offset + len(z_probes)
    effects = []
    for fault_index in range(len(fault_basis)):
        flipped_outcomes = {
            index
            for index in range(outcome_count)
            if deltas[hidden_count + index, fault_index]
        }
        flipped_checks = frozenset(
            index
            for index, positions in enumerate(checks)
            if sum(position in flipped_outcomes for position in positions) % 2
        )
        readout_flips = frozenset(
            index
            for index, positions in enumerate(readouts)
            if sum(position in flipped_outcomes for position in positions) % 2
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
    return tuple(effects)


def _build_basis_probes(
    encodings: Sequence[qc.Encoding], basis: Basis
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
    encodings: Sequence[qc.Encoding],
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
