"""Canonical exact walker over qodec program instructions."""

from __future__ import annotations

from dataclasses import dataclass
from typing import (
    TYPE_CHECKING,
    Callable,
    Mapping,
    Protocol,
    Sequence,
    runtime_checkable,
)

from binar import BitMatrix
import qodec as qc
from paulimer import CliffordUnitary, OutcomeCompleteSimulation
from qodec.actions import (
    Clifford as CliffordAction,
    Observe,
    Pauli as PauliAction,
    Stabilize,
)
from qodec.gadgets import Circuit

from ..._layout import ProgramLayout
from ..._readouts import observe_count_of
from .isa_actions import (
    build_clifford_images,
    remap_pauli,
)
from .pauli import Pauli, PauliCharacter, characters_of

if TYPE_CHECKING:
    from ..._faults import FaultEvent


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
        del parity
        for shot, frame in enumerate(self._frames):
            condition = sum(self._outcomes[index][shot] for index in outcomes) % 2
            if condition:
                self._frames[shot] = abs(pauli * frame)

    def apply_clifford(
        self,
        clifford: CliffordUnitary,
        qubits: Sequence[int],
    ) -> None:
        local_index = {qubit: index for index, qubit in enumerate(qubits)}
        support = set(qubits)
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
            remapped: dict[int, PauliCharacter] = {
                qubits[qubit]: character
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


def _condition_indices(
    condition: qc.actions.Condition, arguments: Mapping[str, object], record_size: int
) -> tuple[list[int], bool]:
    indices = []
    parity = not condition.invert
    for predicate in condition.predicates:
        value = arguments.get(predicate, predicate)
        if isinstance(value, (bool, int)) and value in (0, 1):
            parity ^= bool(value)
        elif isinstance(value, str):
            reference = qc.gadgets.Reference(value)
            if reference.kind != "circuit_readout" or reference.index >= record_size:
                raise ValueError(
                    f"condition {predicate!r} must reference a preceding circuit readout"
                )
            indices.append(reference.index)
        else:
            raise ValueError(f"condition {predicate!r} has no bit argument")
    return indices, parity


def _apply_guarded_pauli(
    engine: PropagationEngine | OutcomeCompleteSimulation,
    pauli: Pauli,
    indices: Sequence[int],
    parity: bool,
    rows: Sequence[int | None],
) -> None:
    if indices:
        selected = [rows[index] for index in indices]
        if any(row is None for row in selected):
            raise NotImplementedError(
                "conditions on circuit flag bits are not simulated"
            )
        engine.apply_conditional_pauli(
            pauli, [row for row in selected if row is not None], parity
        )
    elif not parity:
        engine.apply_pauli(pauli)


def walk_program(
    program: Circuit,
    *,
    simulation: OutcomeCompleteSimulation | None = None,
    extra_engines: Sequence[PropagationEngine] = (),
    input_stabilizers: Sequence[Pauli] = (),
    output_stabilizers: Sequence[Pauli] = (),
    on_instruction: Callable[[int], None] | None = None,
) -> WalkResult:
    if simulation is None:
        qubit_count = ProgramLayout.of(program).total_qubits
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
    record_rows: list[int | None] = []
    engine_record_rows: list[list[int | None]] = [[] for _ in extra_engines]
    layout = ProgramLayout.of(program)
    for instruction_index, call in enumerate(program.calls):
        instruction = program.instruction_set.instructions[call.mnemonic]
        qubit_map = layout.call_qubit_map(call)

        for action in instruction.action:
            if isinstance(action, Stabilize):
                for pauli_str in action.operators:
                    remapped = remap_pauli(pauli_str, qubit_map)
                    if not remapped.weight:
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
                if action.condition is None:
                    oracle.apply_pauli(remapped)
                    for engine in extra_engines:
                        engine.apply_pauli(remapped)
                else:
                    indices, parity = _condition_indices(
                        action.condition, call.arguments, len(record_rows)
                    )
                    _apply_guarded_pauli(oracle, remapped, indices, parity, record_rows)
                    for engine, rows in zip(extra_engines, engine_record_rows):
                        _apply_guarded_pauli(engine, remapped, indices, parity, rows)
            elif isinstance(action, Observe):
                for observable in action.observables:
                    remapped = remap_pauli(observable, qubit_map)
                    observe_rows.append(oracle.outcome_count)
                    record_rows.append(oracle.outcome_count)
                    oracle.measure(remapped)
                    for engine, rows in zip(extra_engines, engine_record_rows):
                        rows.append(engine.measure(remapped))
                    outcome_count += 1
            else:
                raise TypeError(
                    f"unrecognised action type {type(action).__name__!r} "
                    f"in instruction {call.mnemonic!r}"
                )

        record_rows.extend([None] * len(instruction.flags))
        for rows in engine_record_rows:
            rows.extend([None] * len(instruction.flags))
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
    program: Circuit,
    input_stabilizers: Sequence[Pauli] = (),
    output_stabilizers: Sequence[Pauli] = (),
) -> WalkResult:
    return walk_program(
        program,
        input_stabilizers=input_stabilizers,
        output_stabilizers=output_stabilizers,
    )


def propagate_faults(
    program: Circuit,
    fault_basis: Sequence[FaultEvent],
    residual_probes: Sequence[Pauli],
    *,
    residual_frames: Sequence[frozenset[int]] | None = None,
) -> tuple[BitMatrix, int, int]:
    """Propagate quantum and recorded-bit errors in one batch.

    Readout flips change only Observe rows, not hidden reset outcomes or
    physical residuals. Signed probes use those changed rows when evaluating
    their circuit-walk outcome frames.
    """
    calls = program.calls
    readout_ranges = []
    readout_offset = 0
    for call in calls:
        instruction = program.instruction_set.instructions[call.mnemonic]
        if (
            call.predicates
            or call.select
            or any(
                getattr(action, "condition", None) is not None
                for action in instruction.action
            )
        ):
            raise NotImplementedError(
                "conditional or selected circuits are not supported by fault propagation"
            )
        if instruction.flags:
            raise NotImplementedError(
                "circuit instruction flags are not supported by fault propagation"
            )
        readout_count = observe_count_of(instruction)
        readout_ranges.append(range(readout_offset, readout_offset + readout_count))
        readout_offset += readout_count
    propagator = _FramePropagator(len(fault_basis))
    injections: dict[int, list[tuple[int, Pauli]]] = {}
    readout_injections: list[tuple[int, int]] = []
    for fault_index, fault in enumerate(fault_basis):
        for instruction_index, (pauli, readouts) in fault._locations.items():
            if not 0 <= instruction_index < len(calls):
                raise ValueError(
                    f"fault call index {instruction_index} is out of bounds for {len(calls)} calls"
                )
            if pauli.weight:
                injections.setdefault(instruction_index, []).append(
                    (fault_index, pauli)
                )
            call_readouts = readout_ranges[instruction_index]
            for readout in readouts:
                if not 0 <= readout < len(call_readouts):
                    raise ValueError(
                        f"fault readout index {readout} is out of bounds for call {instruction_index} "
                        f"with {len(call_readouts)} readouts"
                    )
                readout_injections.append((fault_index, call_readouts[readout]))

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
    deltas = propagator.outcome_deltas
    for fault_index, readout in readout_injections:
        deltas[result.observe_outcomes[readout], fault_index] ^= True
    observed = set(result.observe_outcomes)
    circuit_rows = result.hidden_count + result.outcome_count
    if residual_frames is not None:
        for probe, frame in zip(
            range(len(residual_probes)), residual_frames, strict=True
        ):
            for outcome in frame:
                if not 0 <= outcome < circuit_rows:
                    raise ValueError(f"probe outcome index {outcome} is out of bounds")
                for fault in range(len(fault_basis)):
                    deltas[circuit_rows + probe, fault] ^= deltas[outcome, fault]
    row_order = [
        *(row for row in range(circuit_rows) if row not in observed),
        *result.observe_outcomes,
        *range(circuit_rows, circuit_rows + len(residual_probes)),
    ]
    if row_order != list(range(len(row_order))):
        deltas = BitMatrix(
            [
                [bool(deltas[row, fault]) for fault in range(len(fault_basis))]
                for row in row_order
            ]
        )
    return deltas, result.hidden_count, result.outcome_count


def program_of(gadget: qc.Gadget) -> Circuit:
    """The gadget's source-backed circuit."""
    return gadget.circuit


def propagate_input_paulis(
    gadget: qc.Gadget,
    paulis: Sequence[Pauli],
    *,
    residual_probes: Sequence[Pauli] = (),
) -> tuple[BitMatrix, int, int]:
    program = program_of(gadget)
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
    "program_of",
    "propagate_faults",
    "propagate_input_paulis",
    "walk_for_outcome_code",
    "walk_program",
]
