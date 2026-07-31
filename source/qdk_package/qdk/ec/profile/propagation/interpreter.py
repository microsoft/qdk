"""Canonical exact walker over qodec program instructions."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable, Protocol, Sequence, runtime_checkable

from binar import BitMatrix
import qodec
from paulimer import CliffordUnitary, OutcomeCompleteSimulation
from qodec.actions import (
    Clifford as CliffordAction,
    Observe,
    Pauli as PauliAction,
    Stabilize,
)
from qodec.circuits import Program

from .isa_actions import (
    block_operands,
    block_strides,
    build_clifford_images,
    build_qubit_map,
    remap_pauli,
)
from .pauli import Pauli, characters_of


@runtime_checkable
class PropagationEngine(Protocol):
    """What :func:`walk_program` requires of an ``extra_engines`` entry.

    An engine is driven alongside the primary simulation: the walker replays
    every Clifford, Pauli, conditional Pauli, and measurement onto it, so the
    engine can accumulate whatever view of the program it cares about (a Pauli
    frame per fault, a stabilizer tableau, a record of outcomes, ...).
    """

    def apply_pauli(self, pauli: Pauli) -> None: ...

    def apply_conditional_pauli(
        self,
        pauli: Pauli,
        outcomes: Sequence[int],
        parity: bool = True,
    ) -> None: ...

    def apply_clifford(
        self, clifford: CliffordUnitary, qubits: Sequence[int]
    ) -> None: ...

    def measure(self, observable: Pauli) -> int: ...


class _FramePropagator:
    """Propagate one relative Pauli frame per fault-basis element."""

    def __init__(self, shot_count: int) -> None:
        self._frames = [Pauli.identity() for _ in range(shot_count)]
        self._outcomes: list[list[bool]] = []

    def apply_pauli_to_shot(self, shot: int, pauli: Pauli) -> None:
        self._frames[shot] = abs(pauli * self._frames[shot])

    def apply_pauli(self, pauli: Pauli) -> None:
        del pauli

    def apply_conditional_pauli(
        self,
        pauli: Pauli,
        outcomes: Sequence[int],
        parity: bool = True,
    ) -> None:
        for shot, frame in enumerate(self._frames):
            condition = sum(self._outcomes[index][shot] for index in outcomes) % 2
            if bool(condition) == parity:
                self._frames[shot] = abs(pauli * frame)

    def apply_clifford(
        self,
        clifford: CliffordUnitary,
        supported_by: Sequence[int],
    ) -> None:
        local_index = {qubit: index for index, qubit in enumerate(supported_by)}
        support = set(supported_by)
        evolved = []
        for frame in self._frames:
            characters = characters_of(frame)
            local = Pauli(
                {
                    local_index[qubit]: character
                    for qubit, character in characters.items()
                    if qubit in support
                }
            )
            image = Pauli.from_dense(clifford.image_of(local))
            remapped = {
                supported_by[qubit]: character
                for qubit, character in characters_of(image).items()
            }
            remapped.update(
                {
                    qubit: character
                    for qubit, character in characters.items()
                    if qubit not in support
                }
            )
            evolved.append(Pauli(remapped))
        self._frames = evolved

    def measure(self, observable: Pauli) -> int:
        outcome = [not frame.commutes_with(observable) for frame in self._frames]
        self._outcomes.append(outcome)
        return len(self._outcomes) - 1

    @property
    def outcome_deltas(self) -> BitMatrix:
        return BitMatrix(self._outcomes)


@dataclass
class WalkResult:
    simulation: OutcomeCompleteSimulation
    hidden_count: int
    outcome_count: int
    output_stab_count: int = 0
    observe_outcomes: tuple[int, ...] = ()


def _eigenstate_correction(observable: Pauli) -> Pauli:
    qubit = observable.support[0]
    correction = Pauli.z(qubit)
    if observable.commutes_with(correction):
        correction = Pauli.x(qubit)
    return correction


def walk_program(
    program: Program,
    *,
    simulation: OutcomeCompleteSimulation | None = None,
    extra_engines: Sequence[PropagationEngine] = (),
    input_stabilizers: Sequence[Pauli] = (),
    output_stabilizers: Sequence[Pauli] = (),
    on_instruction: Callable[[int], None] | None = None,
) -> WalkResult:
    if simulation is None:
        qubit_count = program.qubit_count
        oracle = OutcomeCompleteSimulation.with_capacity(qubit_count, 100, 50)
        oracle.reserve_qubits(qubit_count)
        oracle.reserve_outcomes(50, 50)
    else:
        oracle = simulation

    hidden_count = 0
    for stabilizer in input_stabilizers:
        oracle.measure(stabilizer)
        for engine in extra_engines:
            engine.measure(stabilizer)
        hidden_count += 1

    outcome_count = 0
    observe_rows: list[int] = []
    strides = block_strides(program.isa)
    operands_flat = block_operands(program)
    operand_offset = 0
    for instruction_index, call in enumerate(program.instructions):
        instruction = program.lookup(call.mnemonic)
        operand_count = len(call.inputs)
        call_operands = operands_flat[operand_offset : operand_offset + operand_count]
        operand_offset += operand_count
        qubit_map = build_qubit_map(call, call_operands, strides)

        for action in instruction.action:
            if isinstance(action, Stabilize):
                for pauli_str in action.operators:
                    remapped = remap_pauli(pauli_str, qubit_map)
                    if oracle.is_stabilizer(remapped, ignore_sign=True):
                        continue
                    correction = _eigenstate_correction(remapped)
                    outcome = oracle.measure(remapped)
                    oracle.apply_conditional_pauli(correction, [outcome])
                    for engine in extra_engines:
                        engine_outcome = engine.measure(remapped)
                        engine.apply_conditional_pauli(correction, [engine_outcome])
                    hidden_count += 1
            elif isinstance(action, CliffordAction):
                qubits = sorted(set(qubit_map.values()))
                local_map = {qubit: index for index, qubit in enumerate(qubits)}
                images = build_clifford_images(
                    action.generators,
                    qubit_map,
                    local_map,
                    len(qubits),
                )
                clifford = CliffordUnitary.from_images(images)
                oracle.apply_clifford(clifford, qubits)
                for engine in extra_engines:
                    engine.apply_clifford(clifford, qubits)
            elif isinstance(action, PauliAction):
                remapped = remap_pauli(action.operator, qubit_map)
                oracle.apply_pauli(remapped)
                for engine in extra_engines:
                    engine.apply_pauli(remapped)
            elif isinstance(action, Observe):
                for observable in action.observables:
                    remapped = remap_pauli(observable.pauli, qubit_map)
                    observe_rows.append(oracle.outcome_count)
                    oracle.measure(remapped)
                    for engine in extra_engines:
                        engine.measure(remapped)
                    outcome_count += 1
            else:
                raise TypeError(
                    f"unrecognised action type {type(action).__name__!r} "
                    f"in instruction {call.mnemonic!r}"
                )

        if on_instruction is not None:
            on_instruction(instruction_index)

    output_count = 0
    for stabilizer in output_stabilizers:
        oracle.measure(stabilizer)
        for engine in extra_engines:
            engine.measure(stabilizer)
        output_count += 1

    return WalkResult(
        simulation=oracle,
        hidden_count=hidden_count,
        outcome_count=outcome_count,
        output_stab_count=output_count,
        observe_outcomes=tuple(observe_rows),
    )


def walk_for_outcome_code(
    program: Program,
    input_stabilizers: Sequence[Pauli] = (),
    output_stabilizers: Sequence[Pauli] = (),
) -> WalkResult:
    return walk_program(
        program,
        input_stabilizers=input_stabilizers,
        output_stabilizers=output_stabilizers,
    )


def propagate_faults(
    program: Program,
    fault_basis: Sequence[Any],
    residual_probes: Sequence[Pauli],
) -> tuple[BitMatrix, int, int]:
    propagator = _FramePropagator(len(fault_basis))
    injections: dict[int, list[tuple[int, Pauli]]] = {}
    for fault_index, fault in enumerate(fault_basis):
        for instruction_index, pauli in fault.errors.items():
            injections.setdefault(instruction_index, []).append((fault_index, pauli))

    def inject_at(instruction_index: int) -> None:
        for shot_index, pauli in injections.get(instruction_index, ()):
            propagator.apply_pauli_to_shot(shot_index, pauli)

    result = walk_program(
        program,
        extra_engines=[propagator],
        on_instruction=inject_at,
    )
    for probe in residual_probes:
        propagator.measure(probe)
    return propagator.outcome_deltas, result.hidden_count, result.outcome_count


def propagate_input_paulis(
    channel: qodec.Channel,
    paulis: Sequence[Pauli],
    *,
    residual_probes: Sequence[Pauli] = (),
) -> tuple[BitMatrix, int, int]:
    program = Program(channel.instructions, channel.isa)
    propagator = _FramePropagator(len(paulis))
    for shot_index, pauli in enumerate(paulis):
        propagator.apply_pauli_to_shot(shot_index, pauli)
    result = walk_program(program, extra_engines=[propagator])
    for probe in residual_probes:
        propagator.measure(probe)
    return propagator.outcome_deltas, result.hidden_count, result.outcome_count


__all__ = [
    "PropagationEngine",
    "WalkResult",
    "propagate_faults",
    "propagate_input_paulis",
    "walk_for_outcome_code",
    "walk_program",
]
