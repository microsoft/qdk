"""Intrinsic Pauli-fault effects of qodec gadgets."""

from __future__ import annotations

from collections.abc import Iterator, Sequence
from dataclasses import dataclass, field
from typing import Any

import qodec
from qodec.circuits import Program

from ._qodec_compat import (
    check_outcomes,
    observables_as_xor_map,
    realization,
)
from ._analysis.propagation.interpreter import propagate_faults
from ._analysis.propagation.pauli import Pauli, PauliCharacter
from ._analysis.propagation.pauli_remap import (
    encoding_qubit_relocation,
    remap_to_global,
)


@dataclass(frozen=True)
class Fault:
    """A Pauli fault injected after one or more program instructions."""

    errors: dict[int, Pauli]


@dataclass(frozen=True)
class FaultEffect:
    """The intrinsic semantic effect of one fault-basis element."""

    flipped_checks: frozenset[int] = field(default_factory=frozenset)
    flipped_observables: frozenset[int] = field(default_factory=frozenset)
    residuals: dict[str, Pauli] = field(default_factory=dict)


@dataclass(frozen=True)
class FaultProfile:
    """A positional mapping from an explicit fault basis to its effects."""

    basis: tuple[Fault, ...]
    effects: tuple[FaultEffect, ...]

    def __len__(self) -> int:
        return len(self.basis)

    def __iter__(self) -> Iterator[tuple[Fault, FaultEffect]]:
        return iter(zip(self.basis, self.effects))


def fault_profile_of(gadget: qodec.Gadget, basis: Sequence[Fault]) -> FaultProfile:
    """Map an explicit Pauli fault basis to probability-free effects."""
    fault_basis = tuple(basis)
    if not fault_basis:
        return FaultProfile((), ())

    channel = realization(gadget)
    program = Program(channel.instructions, channel.isa)
    checks = [check_outcomes(atoms) for atoms in gadget.checks]
    observable_map = observables_as_xor_map(gadget)
    observables = list(observable_map.values())
    flag_names = set(gadget.implements.flags)
    flag_indices = {
        index for index, name in enumerate(observable_map) if name in flag_names
    }
    z_probes, z_layout = _build_basis_probes(channel.encoding_out, "Z")
    x_probes, x_layout = _build_basis_probes(channel.encoding_out, "X")
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
        flipped_observables = frozenset(
            index
            for index, positions in enumerate(observables)
            if index not in flag_indices
            and sum(position in flipped_outcomes for position in positions) % 2
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
                flipped_observables,
                _combine_residual_passes(
                    channel.encoding_out,
                    z_flips,
                    z_layout,
                    x_flips,
                    x_layout,
                ),
            )
        )
    return FaultProfile(fault_basis, tuple(effects))


def fault_effects_of(gadget: qodec.Gadget, basis: Sequence[Fault]) -> list[FaultEffect]:
    """Return only the effects from :func:`fault_profile_of`."""
    return list(fault_profile_of(gadget, basis).effects)


def _build_basis_probes(
    encodings: Sequence[qodec.gadgets.Encoding], basis: str
) -> tuple[list[Pauli], list[tuple[str, int]]]:
    probes = []
    layout = []
    for encoding in encodings:
        relocation = encoding_qubit_relocation(encoding)
        for index, characters in enumerate(_logical_chars(encoding.code, basis)):
            probes.append(remap_to_global(characters, relocation))
            layout.append((encoding.operand, index))
    return probes, layout


def _logical_chars(code: Any, basis: str) -> Iterator[dict[int, "PauliCharacter"]]:
    x_operators = getattr(code, "x", None)
    z_operators = getattr(code, "z", None)
    if x_operators is not None and z_operators is not None:
        for operator in (x_operators if basis == "X" else z_operators):
            yield _pauli_string_to_chars(str(operator))
        return
    offset = 0 if basis == "X" else 1
    for index in range(code.logical_qubit_count):
        yield code.logical_basis[2 * index + offset].characters


def _pauli_string_to_chars(
    pauli_str: str,
) -> dict[int, "PauliCharacter"]:
    characters: dict[int, "PauliCharacter"] = {}
    for token in pauli_str.split():
        basis, _, index = token.partition("_")
        if basis not in ("I", "X", "Y", "Z"):
            raise ValueError(f"unrecognised Pauli letter {basis!r}")
        characters[int(index)] = basis  # type: ignore[assignment]
    return characters


def _combine_residual_passes(
    encodings: Sequence[qodec.gadgets.Encoding],
    z_flips: set[int],
    z_layout: list[tuple[str, int]],
    x_flips: set[int],
    x_layout: list[tuple[str, int]],
) -> dict[str, Pauli]:
    residuals: dict[str, dict[int, PauliCharacter]] = {
        encoding.operand: {} for encoding in encodings
    }
    flips: dict[tuple[str, int], dict[str, bool]] = {}
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
    "Fault",
    "FaultEffect",
    "FaultProfile",
    "fault_effects_of",
    "fault_profile_of",
]
