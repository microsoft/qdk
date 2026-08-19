"""Discover gadget checks and logical readouts by exact simulation."""

from __future__ import annotations

from collections.abc import Callable, Sequence
from dataclasses import dataclass, field
from typing import cast

import qodec as qc
from paulimer import OutcomeCompleteSimulation, UnitaryOpcode
from qodec.actions import Observe
from qodec.circuits import Program

from .._layout import ProgramLayout
from .._readouts import flag_slots, observables_as_xor_map, observe_count_of
from .._references import Atom, Equation, Outcome, StabilizerSign, outcomes_of
from .propagation.interpreter import program_of, walk_program
from .propagation.pauli import Pauli, PauliCharacter, relabel
from .propagation.pauli_remap import declared_pauli_of, encoding_qubit_relocation


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
    declared_outcomes: tuple[tuple[str, int], ...] = ()
    in_refs: tuple["StabilizerReference", ...] = field(default_factory=tuple)
    out_refs: tuple["StabilizerReference", ...] = field(default_factory=tuple)


@dataclass(frozen=True)
class Profile:
    checks: list[Equation]
    observables: dict[str, list[int]]


@dataclass(frozen=True)
class StabilizerReference:
    entry: int
    stabilizer_index: int


def simulate_program(
    program: Program,
    simulation: OutcomeCompleteSimulation | None = None,
) -> ProgramSimulation:
    walk = walk_program(program, simulation=simulation)
    return ProgramSimulation(walk.simulation, walk.observe_outcomes)


def choi_prepare(gadget: qc.Gadget) -> OutcomeCompleteSimulation:
    program = program_of(gadget)
    input_qubits = _input_data_qubits(gadget)
    qubit_count = ProgramLayout.of(program).total_qubits
    simulation = _fresh_sim(qubit_count + len(input_qubits))
    for offset, data_qubit in enumerate(input_qubits):
        simulation.apply_unitary(
            UnitaryOpcode.PrepareBell,
            [data_qubit, qubit_count + offset],
        )
    return simulation


def simulate_channel(
    gadget: qc.Gadget, *, with_declared: bool = False
) -> ChannelSimulation:
    program = program_of(gadget)
    simulation = choi_prepare(gadget)
    input_stabilizers, input_refs = _stabilizer_probes(gadget.inputs)
    output_stabilizers, output_refs = _stabilizer_probes(gadget.outputs)
    input_outcomes = [_measure(simulation, item) for item in input_stabilizers]
    program_result = simulate_program(program, simulation)
    output_outcomes = [_measure(simulation, item) for item in output_stabilizers]
    declared_outcomes: tuple[tuple[str, int], ...] = ()
    if with_declared:
        declared_outcomes = tuple(
            (name, _measure(simulation, probe))
            for name, probe in _declared_observable_probes(gadget)
            if probe is not None
        )
    return ChannelSimulation(
        simulation,
        tuple(input_outcomes),
        program_result.observe_outcomes,
        tuple(output_outcomes),
        declared_outcomes,
        input_refs,
        output_refs,
    )


def checks_of(gadget: qc.Gadget) -> list[Equation]:
    result = simulate_channel(gadget)
    return _emit_checks(result, _deterministic_rows(result))


def profile_of(gadget: qc.Gadget) -> Profile:
    result = simulate_channel(gadget, with_declared=True)
    rows = _deterministic_rows(result)
    checks = [row for row in rows if not row.declared]
    declared_rows = [row for row in rows if row.declared]
    observables, excluded = _emit_observables(result, gadget, declared_rows, checks)
    return Profile(
        checks=_emit_checks(result, checks, exclude=excluded),
        observables=observables,
    )


@dataclass(frozen=True)
class CheckRow:
    in_stabs: frozenset[int]
    outcomes: frozenset[int]
    out_stabs: frozenset[int]
    declared: frozenset[int] = frozenset()

    def xor(self, other: "CheckRow") -> "CheckRow":
        return CheckRow(
            self.in_stabs ^ other.in_stabs,
            self.outcomes ^ other.outcomes,
            self.out_stabs ^ other.out_stabs,
            self.declared ^ other.declared,
        )


def _emit_checks(
    result: ChannelSimulation,
    rows: Sequence[CheckRow],
    *,
    exclude: Sequence[frozenset[int]] = (),
) -> list[Equation]:
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
        emitted.append(_check_equation(result, row))
    return emitted


def _check_equation(result: ChannelSimulation, row: CheckRow) -> Equation:
    atoms: list[Atom] = [Outcome(index) for index in sorted(row.outcomes)]
    for index in sorted(row.in_stabs):
        reference = result.in_refs[index]
        atoms.append(StabilizerSign("in", reference.entry, reference.stabilizer_index))
    for index in sorted(row.out_stabs):
        reference = result.out_refs[index]
        atoms.append(StabilizerSign("out", reference.entry, reference.stabilizer_index))
    return tuple(atoms)


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
        tuple(row for _, row in result.declared_outcomes),
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
    gadget: qc.Gadget,
    declared_rows: Sequence[CheckRow],
    check_rows: Sequence[CheckRow],
) -> tuple[dict[str, list[int]], list[frozenset[int]]]:
    basis = _eliminate(
        _eliminate(list(declared_rows) + list(check_rows), lambda row: row.in_stabs),
        lambda row: row.out_stabs,
    )
    by_index = {
        next(iter(row.declared)): row.outcomes
        for row in basis
        if len(row.declared) == 1 and not row.in_stabs and not row.out_stabs
    }
    discoverable = {
        name: index for index, (name, _) in enumerate(result.declared_outcomes)
    }
    observables = {}
    flag_patterns = []
    flag_bindings = _flag_bindings_of(gadget)
    authored = observables_as_xor_map(gadget)
    for name in _declared_observable_names(gadget):
        if name in discoverable:
            index = discoverable[name]
            if index not in by_index:
                raise ValueError(
                    f"declared observable {name!r} could not be expressed "
                    "in terms of realized outcomes"
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


def _flag_bindings_of(gadget: qc.Gadget) -> dict[str, frozenset[int]]:
    return {
        slot.name: frozenset(outcomes_of(slot.equation)) for slot in flag_slots(gadget)
    }


def _declared_observable_names(gadget: qc.Gadget) -> list[str]:
    """Every readout the instruction declares: its flags, then its observe outcomes."""
    instruction = gadget.implements
    return [
        *instruction.flags,
        *(str(position) for position in range(observe_count_of(instruction))),
    ]


def _fresh_sim(qubit_count: int) -> OutcomeCompleteSimulation:
    simulation = OutcomeCompleteSimulation.with_capacity(qubit_count, 100, 100)
    simulation.reserve_qubits(qubit_count)
    simulation.reserve_outcomes(100, 100)
    return simulation


def _measure(simulation: OutcomeCompleteSimulation, pauli: Pauli) -> int:
    row = simulation.outcome_count
    simulation.measure(pauli)
    return row


def _input_data_qubits(gadget: qc.Gadget) -> list[int]:
    qubits: set[int] = set()
    for encoding in gadget.inputs:
        qubits.update(encoding_qubit_relocation(encoding).values())
    return sorted(qubits)


def _stabilizer_probes(
    encodings: Sequence[qc.gadgets.Encoding],
) -> tuple[tuple[Pauli, ...], tuple[StabilizerReference, ...]]:
    paulis: list[Pauli] = []
    references: list[StabilizerReference] = []
    for entry, encoding in enumerate(encodings):
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
            references.append(StabilizerReference(entry, index))
    return tuple(paulis), tuple(references)


def _declared_observable_probes(
    gadget: qc.Gadget,
) -> list[tuple[str, Pauli | None]]:
    program = program_of(gadget)
    qubit_count = ProgramLayout.of(program).total_qubits
    partners = {
        qubit: qubit_count + offset
        for offset, qubit in enumerate(_input_data_qubits(gadget))
    }
    specs: list[tuple[str, Pauli | None]] = [
        (name, None) for name in gadget.implements.flags
    ]
    position = 0
    for action in gadget.implements.action:
        if not isinstance(action, Observe):
            continue
        for observable in action.observables:
            probe = declared_pauli_of(gadget.inputs, observable.pauli)
            specs.append((str(position), relabel(probe, partners)))
            position += 1
    return specs


__all__ = [
    "ChannelSimulation",
    "Profile",
    "checks_of",
    "choi_prepare",
    "profile_of",
    "simulate_channel",
    "simulate_program",
]
