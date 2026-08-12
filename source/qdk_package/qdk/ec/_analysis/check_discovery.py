"""Discover gadget checks and logical readouts by exact simulation."""

from __future__ import annotations

from collections.abc import Callable, Iterator, Mapping, Sequence
from dataclasses import dataclass, field
from typing import Any, cast

import qodec
from paulimer import OutcomeCompleteSimulation, UnitaryOpcode
from qodec.actions import Observe
from qodec.circuits import Program

from .._qodec_compat import (
    observables_as_xor_map,
    observe_count,
    outcome_indices,
    realization,
)
from .propagation.interpreter import walk_program
from .propagation.isa_actions import parse_basis_index
from .propagation.pauli import Pauli, PauliCharacter
from .propagation.pauli_remap import encoding_qubit_relocation


@dataclass(frozen=True)
class ProgramSimulation:
    simulation: OutcomeCompleteSimulation
    observe_outcomes: tuple[int, ...]


@dataclass(frozen=True)
class ChannelSimulation:
    simulation: OutcomeCompleteSimulation
    in_stab_outcomes: tuple[int, ...]
    program_outcomes: tuple[int, ...]
    out_stab_outcomes: tuple[int, ...]
    objective_outcomes: tuple[tuple[str, int], ...] = ()
    in_refs: tuple["StabilizerReference", ...] = field(default_factory=tuple)
    out_refs: tuple["StabilizerReference", ...] = field(default_factory=tuple)


@dataclass(frozen=True)
class Profile:
    checks: list[list[str]]
    observables: dict[str, list[int]]


@dataclass(frozen=True)
class StabilizerReference:
    encoding: qodec.gadgets.Encoding
    stabilizer_index: int


def simulate_program(
    program: Program,
    simulation: OutcomeCompleteSimulation | None = None,
    *,
    sim: OutcomeCompleteSimulation | None = None,
) -> ProgramSimulation:
    if simulation is not None and sim is not None:
        raise TypeError("pass only one of simulation or sim")
    walk = walk_program(program, simulation=simulation or sim)
    return ProgramSimulation(walk.simulation, walk.observe_outcomes)


def choi_prepare(channel: qodec.Channel) -> OutcomeCompleteSimulation:
    program = Program(channel.instructions, channel.isa)
    input_qubits = _input_data_qubits(channel)
    simulation = _fresh_sim(program.qubit_count + len(input_qubits))
    for offset, data_qubit in enumerate(input_qubits):
        simulation.apply_unitary(
            UnitaryOpcode.PrepareBell,
            [data_qubit, program.qubit_count + offset],
        )
    return simulation


def simulate_channel(
    channel: qodec.Channel | None = None,
    *,
    gadget: qodec.Gadget | None = None,
) -> ChannelSimulation:
    if (channel is None) == (gadget is None):
        raise TypeError("pass exactly one of channel or gadget")
    if gadget is not None:
        channel = realization(gadget)
    assert channel is not None
    program = Program(channel.instructions, channel.isa)
    simulation = choi_prepare(channel)
    input_stabilizers, input_refs = _stabilizer_probes(channel.encoding_in)
    output_stabilizers, output_refs = _stabilizer_probes(channel.encoding_out)
    input_outcomes = [_measure(simulation, item) for item in input_stabilizers]
    program_result = simulate_program(program, simulation)
    output_outcomes = [_measure(simulation, item) for item in output_stabilizers]
    objective_outcomes: tuple[tuple[str, int], ...] = ()
    if gadget is not None:
        objective_outcomes = tuple(
            (name, _measure(simulation, probe))
            for name, probe in _objective_observable_probes(gadget)
            if probe is not None
        )
    return ChannelSimulation(
        simulation,
        tuple(input_outcomes),
        program_result.observe_outcomes,
        tuple(output_outcomes),
        objective_outcomes,
        input_refs,
        output_refs,
    )


def checks_of(channel: qodec.Channel) -> list[list[str]]:
    result = simulate_channel(channel)
    return _emit_checks(result, _deterministic_rows(result))


def profile_of(gadget: qodec.Gadget) -> Profile:
    result = simulate_channel(gadget=gadget)
    rows = _deterministic_rows(result)
    checks = [row for row in rows if not row.objectives]
    objective_rows = [row for row in rows if row.objectives]
    observables, excluded = _emit_observables(result, gadget, objective_rows, checks)
    return Profile(
        checks=_emit_checks(result, checks, exclude=excluded),
        observables=observables,
    )


@dataclass(frozen=True)
class CheckRow:
    in_stabs: frozenset[int]
    outcomes: frozenset[int]
    out_stabs: frozenset[int]
    objectives: frozenset[int] = frozenset()

    def xor(self, other: "CheckRow") -> "CheckRow":
        return CheckRow(
            self.in_stabs ^ other.in_stabs,
            self.outcomes ^ other.outcomes,
            self.out_stabs ^ other.out_stabs,
            self.objectives ^ other.objectives,
        )


def _emit_checks(
    result: ChannelSimulation,
    rows: Sequence[CheckRow],
    *,
    exclude: Sequence[frozenset[int]] = (),
) -> list[list[str]]:
    candidates = _eliminate(rows, lambda row: row.out_stabs) + _eliminate(
        rows, lambda row: row.in_stabs
    )
    excluded = set(exclude)
    seen = set()
    emitted = []
    for row in candidates:
        if (not row.outcomes and row.in_stabs and row.out_stabs) or not (
            row.outcomes or row.in_stabs or row.out_stabs
        ):
            continue
        if row.outcomes in excluded and not row.in_stabs and not row.out_stabs:
            continue
        key = (row.in_stabs, row.outcomes, row.out_stabs)
        if key in seen:
            continue
        seen.add(key)
        emitted.append(_check_atoms(result, row))
    return emitted


def _check_atoms(result: ChannelSimulation, row: CheckRow) -> list[str]:
    atoms = [f"circuit.readouts[{index}]" for index in sorted(row.outcomes)]
    for index in sorted(row.in_stabs):
        reference = result.in_refs[index]
        atoms.append(
            f"in[{reference.encoding.operand}].stabilizers[{reference.stabilizer_index}]"
        )
    for index in sorted(row.out_stabs):
        reference = result.out_refs[index]
        atoms.append(
            f"out[{reference.encoding.operand}].stabilizers[{reference.stabilizer_index}]"
        )
    return atoms


def _eliminate(
    rows: Sequence[CheckRow], target: Callable[[CheckRow], frozenset[int]]
) -> list[CheckRow]:
    surviving = list(rows)
    while True:
        pivot_index = next(
            (index for index, row in enumerate(surviving) if target(row)),
            None,
        )
        if pivot_index is None:
            return surviving
        pivot = surviving[pivot_index]
        column = min(target(pivot))
        surviving = [
            row.xor(pivot) if column in target(row) else row
            for index, row in enumerate(surviving)
            if index != pivot_index
        ]


def _deterministic_rows(result: ChannelSimulation) -> list[CheckRow]:
    simulation = result.simulation
    matrix = simulation.outcome_matrix
    random = simulation.random_outcome_indicator
    rank_profile = [index for index in range(matrix.row_count) if random[index]]
    groups = (
        result.in_stab_outcomes,
        result.program_outcomes,
        result.out_stab_outcomes,
        tuple(row for _, row in result.objective_outcomes),
    )
    indexes = [{row: index for index, row in enumerate(group)} for group in groups]
    reportable = set().union(*(set(group) for group in groups))
    rows = []
    for row in range(matrix.row_count):
        if random[row] or row not in reportable:
            continue
        columns: list[set[int]] = [set(), set(), set(), set()]
        _classify(row, indexes, columns)
        for column, contributor in enumerate(rank_profile):
            if matrix[row, column] and contributor in reportable:
                _classify(contributor, indexes, columns)
        rows.append(CheckRow(*(frozenset(column) for column in columns)))
    return rows


def _classify(
    row: int, indexes: Sequence[dict[int, int]], columns: Sequence[set[int]]
) -> None:
    for lookup, target in zip(indexes, columns):
        if row in lookup:
            target.symmetric_difference_update({lookup[row]})
            return


def _emit_observables(
    result: ChannelSimulation,
    gadget: qodec.Gadget,
    objective_rows: Sequence[CheckRow],
    check_rows: Sequence[CheckRow],
) -> tuple[dict[str, list[int]], list[frozenset[int]]]:
    basis = _eliminate(
        _eliminate(list(objective_rows) + list(check_rows), lambda row: row.in_stabs),
        lambda row: row.out_stabs,
    )
    by_index = {
        next(iter(row.objectives)): row.outcomes
        for row in basis
        if len(row.objectives) == 1 and not row.in_stabs and not row.out_stabs
    }
    discoverable = {
        name: index for index, (name, _) in enumerate(result.objective_outcomes)
    }
    observables = {}
    flag_patterns = []
    flag_bindings = _flag_bindings_of(gadget)
    authored = observables_as_xor_map(gadget)
    for name in _objective_observable_names(gadget):
        if name in discoverable:
            index = discoverable[name]
            if index not in by_index:
                raise ValueError(
                    f"objective observable {name!r} could not be expressed "
                    "in terms of realization outcomes"
                )
            outcomes = by_index[index]
        elif name in flag_bindings:
            outcomes = flag_bindings[name]
            flag_patterns.append(outcomes)
        elif name in authored:
            outcomes = frozenset(authored[name])
            flag_patterns.append(outcomes)
        else:
            raise KeyError(f"flag {name!r} is not bound by gadget readouts")
        observables[name] = sorted(outcomes)
    return observables, flag_patterns


def _flag_bindings_of(gadget: qodec.Gadget) -> dict[str, frozenset[int]]:
    trailing = list(gadget.readouts)[observe_count(gadget) :]
    result = {}
    for name, readout in zip(gadget.implements.flags, trailing):
        equation = (
            next(iter(readout.values())) if isinstance(readout, Mapping) else readout
        )
        result[name] = frozenset(outcome_indices(map(str, equation)))
    return result


def _objective_observable_names(gadget: qodec.Gadget) -> list[str]:
    names = list(gadget.implements.flags)
    position = 0
    for action in gadget.implements.action:
        if isinstance(action, Observe):
            for _ in action.observables:
                names.append(str(position))
                position += 1
    return names


def _fresh_sim(qubit_count: int) -> OutcomeCompleteSimulation:
    simulation = OutcomeCompleteSimulation.with_capacity(qubit_count, 100, 100)
    simulation.reserve_qubits(qubit_count)
    simulation.reserve_outcomes(100, 100)
    return simulation


def _measure(simulation: OutcomeCompleteSimulation, pauli: Pauli) -> int:
    row = simulation.outcome_count
    simulation.measure(pauli)
    return row


def _input_data_qubits(channel: qodec.Channel) -> list[int]:
    qubits: set[int] = set()
    for encoding in channel.encoding_in:
        qubits.update(encoding_qubit_relocation(encoding).values())
    return sorted(qubits)


def _stabilizer_probes(
    encodings: Sequence[qodec.gadgets.Encoding],
) -> tuple[tuple[Pauli, ...], tuple[StabilizerReference, ...]]:
    paulis: list[Pauli] = []
    references: list[StabilizerReference] = []
    for encoding in encodings:
        relocation = encoding_qubit_relocation(encoding)
        for index, stabilizer in enumerate(encoding.code.stabilizers):
            sparse = Pauli(str(stabilizer))
            paulis.append(
                Pauli(
                    {
                        relocation[local]: cast(PauliCharacter, character)
                        for local, character in zip(sparse.support, sparse.characters)
                    }
                )
            )
            references.append(StabilizerReference(encoding, index))
    return tuple(paulis), tuple(references)


def _objective_observable_probes(
    gadget: qodec.Gadget,
) -> list[tuple[str, Pauli | None]]:
    channel = realization(gadget)
    flat_map = [
        (encoding, local)
        for encoding in channel.encoding_in
        for local in range(len(list(encoding.code.x)))
    ]
    program = Program(channel.instructions, channel.isa)
    partners = {
        qubit: program.qubit_count + offset
        for offset, qubit in enumerate(_input_data_qubits(channel))
    }
    specs: list[tuple[str, Pauli | None]] = [
        (name, None) for name in gadget.implements.flags
    ]
    position = 0
    for action in gadget.implements.action:
        if not isinstance(action, Observe):
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
                    target = partners[relocation[local]]
                    characters[target] = _pauli_xor(
                        characters.get(target, "I"), character
                    )
            specs.append(
                (
                    str(position),
                    Pauli(
                        {
                            qubit: character
                            for qubit, character in characters.items()
                            if character != "I"
                        }
                    ),
                )
            )
            position += 1
    return specs


def _objective_logical_chars(
    encoding: object, local_index: int, basis: str
) -> Iterator[tuple[int, PauliCharacter]]:
    code = encoding.code  # type: ignore[attr-defined]
    if basis == "X":
        operators = [list(code.x)[local_index]]
    elif basis == "Z":
        operators = [list(code.z)[local_index]]
    elif basis == "Y":
        operators = [list(code.x)[local_index], list(code.z)[local_index]]
    else:
        raise ValueError(f"unsupported objective Pauli basis {basis!r}")
    for operator in operators:
        for token in str(operator).split():
            character, index = parse_basis_index(token)
            if character != "I":
                yield index, cast(PauliCharacter, character)


def _pauli_xor(left: PauliCharacter, right: PauliCharacter) -> PauliCharacter:
    if left == "I":
        return right
    if right == "I":
        return left
    if left == right:
        return "I"
    return next(item for item in ("X", "Y", "Z") if item not in (left, right))


__all__ = [
    "ChannelSimulation",
    "Profile",
    "ProgramSimulation",
    "checks_of",
    "choi_prepare",
    "profile_of",
    "simulate_channel",
    "simulate_program",
]
