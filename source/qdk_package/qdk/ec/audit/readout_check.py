"""Functional readout verification for gadget audit rules."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Iterable

from binar import BitVector
import qodec
from qodec.circuits import Program

from .._qodec_compat import observables_as_xor_map, realization
from ..profile.circuit_action import realization_codes_of
from ..profile.check_discovery import _objective_logical_chars, _pauli_xor
from ..profile.propagation.conditional import (
    ConditionalChoiResult,
    conditional_choi_state,
)
from ..profile.propagation.frames import FrameGroup
from ..profile.propagation.isa_actions import parse_basis_index
from ..profile.propagation.pauli import Pauli, PauliCharacter
from ..profile.propagation.pauli_remap import encoding_qubit_relocation


@dataclass(frozen=True)
class ReadoutMismatch:
    name: str
    declared_positions: tuple[int, ...]
    discovered_signature: BitVector
    declared_signature: BitVector
    reason: str
    verifiable: bool = True


def readout_disagreements(gadget: qodec.Gadget) -> list[ReadoutMismatch]:
    observables, result = _realization_input_observables(gadget)
    declared = observables_as_xor_map(gadget)
    probes = _data_side_logical_probes(gadget)
    relevant_mask = _bitvector_not(_projector_random_mask(result))
    width = result.simulation.sign_matrix.column_count
    mismatches = []
    for name, positions in declared.items():
        probe = probes.get(name)
        if probe is None:
            continue
        try:
            frame = observables.frame_of(probe)
        except ValueError:
            mismatches.append(
                ReadoutMismatch(
                    name=name,
                    declared_positions=tuple(sorted(positions)),
                    discovered_signature=BitVector.zeros(width),
                    declared_signature=BitVector.zeros(width),
                    reason=(
                        "logical Pauli probe is not in the realisation's "
                        "input-side stabiliser group; cannot verify"
                    ),
                    verifiable=False,
                )
            )
            continue
        discovered = BitVector([column in frame for column in range(width)])
        declared_signature = _declared_signature(result, positions)
        if not ((discovered ^ declared_signature) & relevant_mask).is_zero:
            mismatches.append(
                ReadoutMismatch(
                    name=name,
                    declared_positions=tuple(sorted(positions)),
                    discovered_signature=discovered,
                    declared_signature=declared_signature,
                    reason=(
                        "declared XOR pattern disagrees with the realisation's "
                        "discovered signature on non-projector random columns"
                    ),
                )
            )
    return mismatches


def _realization_input_observables(
    gadget: qodec.Gadget,
) -> tuple[FrameGroup, ConditionalChoiResult]:
    channel = realization(gadget)
    program = Program(channel.instructions, channel.isa)
    code_in, _ = realization_codes_of(gadget)
    input_qubits = sorted(code_in.support)
    result = conditional_choi_state(
        program,
        input_qubits=input_qubits,
        codespace_projector=tuple(code_in.stabilizers),
    )
    physical_support = frozenset(range(program.qubit_count))
    _, input_group, _ = result.group.partition(over=physical_support)
    auxiliary = {result.aux_origin + offset for offset in range(len(input_qubits))}
    auxiliary_to_input = {
        result.aux_origin + offset: qubit for offset, qubit in enumerate(input_qubits)
    }
    observables = (
        input_group.restrict_to(auxiliary)
        .relabel(auxiliary_to_input)
        .complex_conjugated()
    )
    return observables, result


def _data_side_logical_probes(gadget: qodec.Gadget) -> dict[str, Pauli]:
    channel = realization(gadget)
    flat_map: list[tuple[Any, int]] = []
    for encoding in channel.encoding_in:
        for local in range(len(list(encoding.code.x))):
            flat_map.append((encoding, local))
    result: dict[str, Pauli] = {}
    position = 0
    for action in gadget.implements.action:
        if not isinstance(action, qodec.actions.Observe):
            continue
        for observable in action.observables:
            characters: dict[int, PauliCharacter] = {}
            for token in observable.pauli.split():
                basis, flat_index = parse_basis_index(token)
                encoding, local_index = flat_map[flat_index]
                relocation = encoding_qubit_relocation(encoding)
                for local, character in _objective_logical_chars(
                    encoding, local_index, basis
                ):
                    data_qubit = relocation[local]
                    characters[data_qubit] = _pauli_xor(
                        characters.get(data_qubit, "I"), character
                    )
            result[str(position)] = Pauli(
                {
                    qubit: character
                    for qubit, character in characters.items()
                    if character != "I"
                }
            )
            position += 1
    return result


def _declared_signature(
    result: ConditionalChoiResult, positions: Iterable[int]
) -> BitVector:
    simulation = result.simulation
    matrix = simulation.outcome_matrix
    width = matrix.column_count
    signature = BitVector.zeros(width)
    for position in positions:
        row = result.observe_outcome_rows[position]
        signature = signature ^ BitVector(
            [bool(matrix[row, column]) for column in range(width)]
        )
    return signature


def _projector_random_mask(result: ConditionalChoiResult) -> BitVector:
    simulation = result.simulation
    projector_rows = set(result.projector_outcome_rows)
    width = simulation.sign_matrix.column_count
    bits = [False] * width
    column = 0
    for row in range(simulation.outcome_count):
        if not simulation.random_outcome_indicator[row]:
            continue
        if row in projector_rows:
            bits[column] = True
        column += 1
        if column >= width:
            break
    return BitVector(bits)


def _bitvector_not(vector: BitVector) -> BitVector:
    return vector ^ BitVector.ones(len(vector))


__all__ = ["ReadoutMismatch", "readout_disagreements"]
