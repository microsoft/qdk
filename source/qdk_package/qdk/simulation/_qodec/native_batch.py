"""Prepare Clifford Qodec runs for the native shot loop.

Tracing uses the ordinary layer and physical instruction lowering without
sampling a quantum state. One encoded layer with measurement-independent
execution is eligible. When only terminal gadgets record measurements, a
batch-capable decoder supplies seed-independent readout tables. Otherwise every
shot replays its decoder callbacks on the native records. Frame updates never
enter the native circuit; each flips the later records its Pauli reaches, so
batches track the same noiseless Pauli frame as the interpreter. Loss,
reset/readout noise, and program-level feedback retain the interpreter path.
Native sampling uses its own seeded stream.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from contextlib import closing
from dataclasses import dataclass
from itertools import product
from random import Random
from typing import Literal, TypeVar, cast

from qdk import Result
from qodec.instructions import InstructionCall

from ..._adaptive_bytecode import (
    OP_PEEK_LOSS,
    OP_READ_LOSS,
    OP_READ_RESULT,
    OP_READOUT_NOISE,
    OP_WRITE_RESULT,
)
from ..._adaptive_pass import AdaptiveProgram
from ..._native import NoiseConfig, QirInstruction, QirInstructionId, run_clifford
from .adaptive_runtime import AdaptiveRuntime, OutputRecordValue
from .circuit_runtime import CallListRuntime
from .execution_pipeline import ExecutionPipeline
from .executor import ExecutionPipelineFactory
from .instruction_set import InstructionRuntime, PreparedAction
from .layer_layout import LayerLayout
from .layer_runtime import LayerPlan, LayerRuntime
from .logical_qubits import LogicalQubits
from .protocols import (
    BatchDecoderFactory,
    BatchDecoderSession,
    BatchUnsupported,
    BeforeInvocation,
    BlockObserver,
    BlockReference,
    Correction,
    Corrections,
    Decoded,
    DecoderFactory,
    ExecutionRejected,
    ExecutionUnresolved,
    Invocation,
    ReadoutBatch,
    Readouts,
    Request,
    Requests,
    Resources,
)
from .quantum_backend import stabilizer_backend
from .quantum_operations import (
    FrameUpdate,
    LogicalSlot,
    Operation,
    RestoreMeasured,
    local_indices,
)
from .readout_equations import InconsistentParity
from .selection import Selection, prepare_selection

ResultT = TypeVar("ResultT")

_GATES = {
    "x": QirInstructionId.X,
    "y": QirInstructionId.Y,
    "z": QirInstructionId.Z,
    "h": QirInstructionId.H,
    "s": QirInstructionId.S,
    "s_adj": QirInstructionId.SAdj,
    "sx": QirInstructionId.SX,
    "sx_adj": QirInstructionId.SXAdj,
    "cx": QirInstructionId.CX,
    "cy": QirInstructionId.CY,
    "cz": QirInstructionId.CZ,
    "swap": QirInstructionId.SWAP,
    "mov": QirInstructionId.Move,
}


_NotBatchable = BatchUnsupported


@dataclass(frozen=True)
class _Readout:
    index: int


@dataclass(frozen=True)
class _DecodingTable:
    start: int
    width: int
    output_count: int
    decoder: ReadoutBatch


@dataclass(frozen=True)
class NativeBatch:
    instructions: tuple[tuple[object, ...], ...]
    num_qubits: int
    num_measurements: int
    decoders: tuple[_DecodingTable, ...]
    outputs: tuple[OutputRecordValue | _Readout, ...]
    frames: tuple[tuple[int, int, str], ...] = ()

    def run(
        self,
        shots: int,
        noise: NoiseConfig | None,
        *,
        seed: int,
        on_shot_failure: Literal["raise", "discard"] = "discard",
    ) -> list[list[OutputRecordValue]]:
        physical = cast(
            list[list[Result]],
            run_clifford(
                cast(list[QirInstruction], list(self.instructions)),
                self.num_qubits,
                self.num_measurements,
                shots,
                noise,
                seed,
            ),
        )
        seeds = _shot_seeds(seed, shots)
        flips = _static_flips(self.instructions, self.frames)
        logical: list[list[OutputRecordValue]] = [[] for _ in physical]
        failures: dict[int, Exception] = {}
        for decoder in self.decoders:
            rows = [
                tuple(
                    (value == Result.One) != bool((flips >> record) & 1)
                    for record, value in enumerate(
                        shot[decoder.start : decoder.start + decoder.width],
                        decoder.start,
                    )
                )
                for shot in physical
            ]
            decoded = decoder.decoder.decode_batch(rows, seeds)
            if len(decoded) != shots:
                raise ValueError("Batch decoder returned the wrong number of shots")
            for index, (destination, row) in enumerate(zip(logical, decoded)):
                if isinstance(row, Exception):
                    failures.setdefault(index, row)
                    continue
                if len(row) != decoder.output_count or any(
                    type(value) is not bool for value in row
                ):
                    raise ValueError(
                        "Batch decoder must return resolved terminal readouts"
                    )
                destination.extend(
                    cast(Result, Result.One if value else Result.Zero) for value in row
                )
        records = []
        for index, shot in enumerate(logical):
            if index in failures:
                error = failures[index]
                if on_shot_failure == "discard" and isinstance(
                    error, (ExecutionRejected, ExecutionUnresolved, InconsistentParity)
                ):
                    continue
                raise error
            records.append(
                [
                    shot[value.index] if isinstance(value, _Readout) else value
                    for value in self.outputs
                ]
            )
        return records


class _RecordingBackend:
    def __init__(self, noise: NoiseConfig | None) -> None:
        self.noise = noise
        self.instructions: list[tuple[object, ...]] = []
        # Frame updates reached during tracing, as (position, qubit, pauli).
        self.frames: list[tuple[int, int, str]] = []
        self.num_qubits = 0
        self.num_measurements = 0
        self.checked_noise: set[tuple[str, int]] = set()
        # While probing, a correction's physical qubit is captured, not traced.
        self.probing = False
        self.probed: int | None = None

    def start(self, resources: Resources) -> None:
        self.num_qubits = resources.qubits

    def execute(self, request: Operation | FrameUpdate) -> Readouts:
        if isinstance(request, FrameUpdate):
            if not isinstance(request.target, int):
                raise _NotBatchable
            if self.probing:
                self.probed = request.target
            else:
                self.frames.append(
                    (len(self.instructions), request.target, request.pauli)
                )
            return ()
        if self.probing:
            raise _NotBatchable
        if len(self.instructions) >= 100_000 or request.angle is not None:
            raise _NotBatchable
        targets = local_indices(request)
        if request.name in ("prepare", "discard"):
            self.instructions.append((QirInstructionId.RESET, targets[0]))
        elif request.name == "measure":
            self.instructions.append(
                (QirInstructionId.MZ, targets[0], self.num_measurements)
            )
            self.num_measurements += 1
            return (False,)
        else:
            if request.name not in _GATES:
                raise _NotBatchable
            self._check_noise(request.name, len(targets))
            self.instructions.append((_GATES[request.name], *targets))
        return ()

    def _check_noise(self, name: str, width: int) -> None:
        if self.noise is None or (name, width) in self.checked_noise:
            return
        table = getattr(self.noise, name)
        if not table.is_noiseless() and any(
            getattr(table, "".join(fault).lower())
            for fault in product("IXYZL", repeat=width)
            if "L" in fault
        ):
            raise _NotBatchable
        self.checked_noise.add((name, width))


class _RecordingRuntime(AdaptiveRuntime):
    def __init__(self) -> None:
        super().__init__()
        self.measured: set[int] = set()
        self.measurements = 0
        self.result_sources: dict[int, int] = {}
        self.output_sources: list[int | None] = []

    def _apply(
        self, operation: str, targets: tuple[int, ...], angle: float | None = None
    ) -> None:
        if operation in ("prepare", "discard"):
            # A reset or release makes the qubit independent of its measured value.
            self.measured.difference_update(targets)
        elif self.measured.intersection(targets):
            # Reusing a measured qubit restores its measured value first.
            raise _NotBatchable
        super()._apply(operation, targets, angle)

    def mz(self, target: int, result_id: int) -> None:
        if target in self.measured:
            raise _NotBatchable
        self.result_sources[result_id] = self.measurements
        self.measurements += 1
        self.measured.add(target)
        super().mz(target, result_id)

    def instruction(self, call: InstructionCall, results: Sequence[int]) -> None:
        for result_id in results:
            self.result_sources[result_id] = self.measurements
            self.measurements += 1
        super().instruction(call, results)

    def result(self, result_id: int) -> Result:
        self.output_sources.append(self.result_sources.get(result_id))
        return cast(Result, Result.Zero)


def _require_static_circuit(
    plan: LayerPlan,
    operations: Mapping[str, object],
    invocation: Invocation,
) -> None:
    """Reject gadget bodies whose physical trace could depend on a shot."""
    if invocation.gadget.implements.parameters:
        raise _NotBatchable
    body = plan.gadgets[invocation.call.mnemonic].body.create_runtime()
    if not isinstance(body, CallListRuntime):
        raise _NotBatchable
    for prepared in body.calls:
        if (
            prepared.call.arguments
            or prepared.call.select
            or prepared.declaration.flags
            or isinstance(operations[prepared.call.mnemonic], PreparedAction)
        ):
            raise _NotBatchable


class _RecordingLayer(LayerRuntime):
    """Note which decoded gadget output each program measurement reports."""

    def __init__(
        self, plan: LayerPlan, decoder: _RecordingDecoder | _DeferringDecoder
    ) -> None:
        super().__init__(plan, decoder)
        self.recorder = decoder
        self.sources: list[tuple[int, int]] = []

    def handle(self, request: Request) -> Requests[Readouts]:
        if isinstance(request, RestoreMeasured):
            # Restoring a measured qubit makes the trace depend on its outcome.
            raise _NotBatchable
        readouts = yield from super().handle(request)
        if isinstance(request, InstructionCall):
            binding = self.plan.bindings[request.mnemonic]
            # A call that selects no flags receives them after its outcomes.
            count = binding.observe_count + (
                0 if request.select else len(binding.flags)
            )
            self.sources.extend((self.recorder.latest, index) for index in range(count))
        return readouts

    def measure(self, target: int | str | LogicalSlot) -> Requests[bool | None]:
        readouts, index = yield from self._measure_readouts(target)
        self.sources.append((self.recorder.latest, index))
        return readouts[index]


class _RecordingDecoder:
    def __init__(
        self,
        session: BatchDecoderSession,
        plan: LayerPlan,
        backend: _RecordingBackend,
        factory: ExecutionPipelineFactory[AdaptiveProgram, list[OutputRecordValue]],
    ) -> None:
        self.session = session
        self.plan = plan
        self.backend = backend
        self.operations = factory.physical_operations
        self.decoders: list[_DecodingTable] = []

    @property
    def latest(self) -> int:
        return len(self.decoders) - 1

    def before(self, invocation: Invocation) -> Corrections[None]:
        if invocation.gadget.implements.flags:
            raise _NotBatchable
        _require_static_circuit(self.plan, self.operations, invocation)
        if isinstance(self.session, BeforeInvocation):
            yield from self.session.before(invocation)

    def decode(
        self, invocation: Invocation, readouts: Readouts
    ) -> Corrections[Decoded]:
        if not invocation.gadget.outputs:
            prepared = self.session.prepare_readouts(invocation, len(readouts))
            if prepared is None:
                raise _NotBatchable
            count = invocation.gadget.implements.observe_count
            self.decoders.append(
                _DecodingTable(
                    self.backend.num_measurements - len(readouts),
                    len(readouts),
                    count,
                    prepared,
                )
            )
            yield from ()
            return Decoded((False,) * count)
        if readouts:
            raise _NotBatchable
        return (yield from self.session.decode(invocation, readouts))

    def discarded(self, blocks: Sequence[BlockReference]) -> None:
        if isinstance(self.session, BlockObserver):
            self.session.discarded(blocks)

    def close(self) -> None:
        self.session.close()


@dataclass(frozen=True)
class _Before:
    invocation: Invocation
    position: int
    qubits: Mapping[BlockReference, tuple[int, ...]]


@dataclass(frozen=True)
class _Decode:
    invocation: Invocation
    position: int
    qubits: Mapping[BlockReference, tuple[int, ...]]
    start: int
    width: int
    selection: Selection


@dataclass(frozen=True)
class _Discard:
    blocks: tuple[BlockReference, ...]


class _DeferringDecoder:
    """Trace decoder callbacks so every shot can replay them on native records.

    No correction reaches the traced circuit. Instead, one probe correction per
    code qubit of each block a callback may correct records the physical qubit
    it would reach; replay then carries corrections as a Pauli frame.
    """

    def __init__(
        self,
        plan: LayerPlan,
        backend: _RecordingBackend,
        factory: ExecutionPipelineFactory[AdaptiveProgram, list[OutputRecordValue]],
    ) -> None:
        self.plan = plan
        self.backend = backend
        self.operations = factory.physical_operations
        self.layout: LayerLayout | None = None
        self.events: list[_Before | _Decode | _Discard] = []
        self.latest = -1

    def before(self, invocation: Invocation) -> Corrections[None]:
        _require_static_circuit(self.plan, self.operations, invocation)
        position = len(self.backend.instructions)
        qubits = yield from self._probe(invocation.inputs)
        self.events.append(_Before(invocation, position, qubits))

    def decode(
        self, invocation: Invocation, readouts: Readouts
    ) -> Corrections[Decoded]:
        gadget = invocation.gadget
        position = len(self.backend.instructions)
        start = self.backend.num_measurements - len(readouts)
        qubits = yield from self._probe(invocation.outputs)
        selection = prepare_selection(gadget.implements.flags, invocation.call.select)
        self.latest = len(self.events)
        self.events.append(
            _Decode(invocation, position, qubits, start, len(readouts), selection)
        )
        return Decoded(
            (False,) * gadget.implements.observe_count,
            (False,) * len(gadget.implements.flags),
        )

    def discarded(self, blocks: Sequence[BlockReference]) -> None:
        self.events.append(_Discard(tuple(blocks)))

    def close(self) -> None:
        pass

    def _probe(
        self, blocks: Sequence[BlockReference]
    ) -> Corrections[dict[BlockReference, tuple[int, ...]]]:
        assert self.layout is not None
        mapping: dict[BlockReference, tuple[int, ...]] = {}
        self.backend.probing = True
        try:
            for block in blocks:
                live = self.layout.blocks.get(block.label)
                if live is None or live.reference != block:
                    continue
                qubits = []
                for index in range(len(live.qubits)):
                    yield Correction((block,), Operation("x", (index,)))
                    if self.backend.probed is None:
                        raise _NotBatchable
                    qubits.append(self.backend.probed)
                    self.backend.probed = None
                mapping[block] = tuple(qubits)
        finally:
            self.backend.probing = False
        return mapping


class _FrameMasks:
    """Record flips caused by deferring a Pauli at a position of the trace."""

    def __init__(self, instructions: Sequence[tuple[object, ...]]) -> None:
        self.instructions = instructions
        self.cache: dict[tuple[int, int, str], int] = {}

    def mask(self, position: int, qubit: int, pauli: str) -> int:
        key = (position, qubit, pauli)
        if key not in self.cache:
            self.cache[key] = self._propagate(position, qubit, pauli)
        return self.cache[key]

    def _propagate(self, position: int, qubit: int, pauli: str) -> int:
        xs = {qubit} if pauli in ("x", "y") else set()
        zs = {qubit} if pauli in ("y", "z") else set()
        flips = 0
        for instruction in self.instructions[position:]:
            opcode, *operands = instruction
            targets = cast(list[int], operands)
            if opcode in _PAULI_OPCODES:
                continue
            if opcode == QirInstructionId.MZ:
                if targets[0] in xs:
                    flips ^= 1 << targets[1]
            elif opcode == QirInstructionId.RESET:
                xs.discard(targets[0])
                zs.discard(targets[0])
            elif opcode == QirInstructionId.H:
                (target,) = targets
                in_x, in_z = target in xs, target in zs
                _assign(xs, target, in_z)
                _assign(zs, target, in_x)
            elif opcode in (QirInstructionId.S, QirInstructionId.SAdj):
                if targets[0] in xs:
                    zs.symmetric_difference_update({targets[0]})
            elif opcode in (QirInstructionId.SX, QirInstructionId.SXAdj):
                if targets[0] in zs:
                    xs.symmetric_difference_update({targets[0]})
            elif opcode == QirInstructionId.CX:
                control, target = targets
                if control in xs:
                    xs.symmetric_difference_update({target})
                if target in zs:
                    zs.symmetric_difference_update({control})
            elif opcode == QirInstructionId.CY:
                control, target = targets
                target_x, target_z = target in xs, target in zs
                if control in xs:
                    xs.symmetric_difference_update({target})
                    zs.symmetric_difference_update({target})
                if target_x != target_z:
                    zs.symmetric_difference_update({control})
            elif opcode == QirInstructionId.CZ:
                first, second = targets
                first_x, second_x = first in xs, second in xs
                if second_x:
                    zs.symmetric_difference_update({first})
                if first_x:
                    zs.symmetric_difference_update({second})
            elif opcode == QirInstructionId.SWAP:
                first, second = targets
                for group in (xs, zs):
                    first_in, second_in = first in group, second in group
                    _assign(group, first, second_in)
                    _assign(group, second, first_in)
            elif opcode != QirInstructionId.Move:
                raise _NotBatchable
        return flips


def _assign(group: set[int], qubit: int, present: bool) -> None:
    if present:
        group.add(qubit)
    else:
        group.discard(qubit)


def _static_flips(
    instructions: Sequence[tuple[object, ...]],
    frames: Sequence[tuple[int, int, str]],
    masks: _FrameMasks | None = None,
) -> int:
    """Record flips from frame updates that every shot applies identically."""
    masks = _FrameMasks(instructions) if masks is None else masks
    flips = 0
    for position, qubit, pauli in frames:
        flips ^= masks.mask(position, qubit, pauli)
    return flips


_PAULI_OPCODES = (QirInstructionId.X, QirInstructionId.Y, QirInstructionId.Z)


def _shot_seeds(seed: int, shots: int) -> list[int]:
    """Decoder seeds matching the interpreter's per-shot pipeline seeds."""
    rng = Random(seed)
    seeds = []
    for _ in range(shots):
        components = Random(rng.getrandbits(64))
        components.getrandbits(64)
        seeds.append(components.getrandbits(64))
    return seeds


@dataclass(frozen=True)
class ReplayBatch:
    """Sample the traced circuit natively, then decode every shot in order.

    Decoder corrections are Pauli frame updates, as in the interpreter; each
    one flips the later measurement records its Pauli reaches.
    """

    instructions: tuple[tuple[object, ...], ...]
    num_qubits: int
    num_measurements: int
    events: tuple[_Before | _Decode | _Discard, ...]
    sources: tuple[tuple[int, int], ...]
    outputs: tuple[OutputRecordValue | _Readout, ...]
    create_session: DecoderFactory
    frames: tuple[tuple[int, int, str], ...] = ()

    def run(
        self,
        shots: int,
        noise: NoiseConfig | None,
        *,
        seed: int,
        on_shot_failure: Literal["raise", "discard"] = "discard",
    ) -> list[list[OutputRecordValue]]:
        physical = cast(
            list[list[Result]],
            run_clifford(
                cast(list[QirInstruction], list(self.instructions)),
                self.num_qubits,
                self.num_measurements,
                shots,
                noise,
                seed,
            ),
        )
        frame = _FrameMasks(self.instructions)
        static = _static_flips(self.instructions, self.frames, frame)
        records = []
        for shot, shot_seed in zip(physical, _shot_seeds(seed, shots)):
            try:
                measured = self._decode_shot(shot, shot_seed, frame, static)
            except (ExecutionRejected, ExecutionUnresolved, InconsistentParity):
                if on_shot_failure == "discard":
                    continue
                raise
            records.append(
                [
                    measured[value.index] if isinstance(value, _Readout) else value
                    for value in self.outputs
                ]
            )
        return records

    def _decode_shot(
        self, shot: Sequence[Result], seed: int, frame: _FrameMasks, flips: int
    ) -> list[OutputRecordValue]:
        bits = [value == Result.One for value in shot]
        outcomes: dict[int, Readouts] = {}
        with closing(self.create_session(seed)) as session:
            for index, event in enumerate(self.events):
                if isinstance(event, _Discard):
                    if isinstance(session, BlockObserver):
                        session.discarded(event.blocks)
                elif isinstance(event, _Before):
                    if isinstance(session, BeforeInvocation):
                        _, flips = _defer(
                            session.before(event.invocation), event, frame, flips
                        )
                else:
                    readouts = tuple(
                        bits[record] != bool((flips >> record) & 1)
                        for record in range(event.start, event.start + event.width)
                    )
                    decoded, flips = _defer(
                        session.decode(event.invocation, readouts), event, frame, flips
                    )
                    implements = event.invocation.gadget.implements
                    if len(decoded.outcomes) != implements.observe_count or len(
                        decoded.flags
                    ) != len(implements.flags):
                        raise ValueError(
                            f"Decoder returned the wrong number of readouts for {implements.mnemonic!r}"
                        )
                    event.selection.require(decoded.flags)
                    outcomes[index] = decoded.readouts
        measured: list[OutputRecordValue] = []
        for event_index, outcome in self.sources:
            value = outcomes[event_index][outcome]
            if value is None:
                raise ExecutionUnresolved("Logical measurement could not be decoded")
            measured.append(cast(Result, Result.One if value else Result.Zero))
        return measured


def _defer(
    corrections: Corrections[ResultT],
    event: _Before | _Decode,
    frame: _FrameMasks,
    flips: int,
) -> tuple[ResultT, int]:
    """Drive a decoder callback, folding its Pauli corrections into ``flips``."""
    with closing(corrections):
        reply: Readouts | None = None
        while True:
            try:
                correction = corrections.send(reply)
            except StopIteration as completed:
                return cast(ResultT, completed.value), flips
            operation = correction.operation
            if operation.name not in ("x", "y", "z") or len(operation.targets) != 1:
                raise BatchUnsupported
            try:
                qubits = [q for block in correction.blocks for q in event.qubits[block]]
            except KeyError as error:
                raise BatchUnsupported from error
            (target,) = local_indices(operation)
            flips ^= frame.mask(event.position, qubits[target], operation.name)
            reply = ()


def prepare_batch(
    program: AdaptiveProgram,
    factory: ExecutionPipelineFactory[AdaptiveProgram, list[OutputRecordValue]],
) -> NativeBatch | ReplayBatch | None:
    if (
        factory.quantum_backend_factory is not stabilizer_backend
        or factory.classical_runtime_factory is not AdaptiveRuntime
        or len(factory.prepared) != 1
        or factory.noise is not None
        and not factory.noise.mresetz.is_noiseless()
        or any(
            instruction.opcode & 0xFF
            in (
                OP_READ_RESULT,
                OP_READ_LOSS,
                OP_PEEK_LOSS,
                OP_READOUT_NOISE,
                OP_WRITE_RESULT,
            )
            for instruction in program.instructions
        )
    ):
        return None
    plan, create_session = factory.prepared[0]
    if isinstance(create_session, BatchDecoderFactory):
        try:
            session = create_session.prepare_batch()
        except BatchUnsupported:
            session = None
        if session is not None:
            batch = _trace_tables(program, factory, plan, session)
            if batch is not None:
                return batch
    return _trace_replay(program, factory, plan, create_session)


def _trace(
    program: AdaptiveProgram,
    factory: ExecutionPipelineFactory[AdaptiveProgram, list[OutputRecordValue]],
    layer: _RecordingLayer,
    backend: _RecordingBackend,
) -> tuple[list[OutputRecordValue], list[int | None]] | None:
    runtime = _RecordingRuntime()
    pipeline = ExecutionPipeline(
        runtime,
        [
            LogicalQubits(factory.program_instructions),
            layer,
            InstructionRuntime(
                factory.physical, operations=factory.physical_operations
            ),
        ],
        backend,
    )
    try:
        records = pipeline.run(program)
    except (_NotBatchable, ExecutionRejected, ExecutionUnresolved, InconsistentParity):
        return None
    return records, runtime.output_sources


def _outputs(
    records: Sequence[OutputRecordValue],
    output_sources: Sequence[int | None],
    resolve: Sequence[int],
) -> tuple[OutputRecordValue | _Readout, ...]:
    sources = iter(output_sources)
    outputs: list[OutputRecordValue | _Readout] = []
    for value in records:
        source = next(sources) if isinstance(value, Result) else None
        outputs.append(value if source is None else _Readout(resolve[source]))
    return tuple(outputs)


def _trace_tables(
    program: AdaptiveProgram,
    factory: ExecutionPipelineFactory[AdaptiveProgram, list[OutputRecordValue]],
    plan: LayerPlan,
    session: BatchDecoderSession,
) -> NativeBatch | None:
    backend = _RecordingBackend(factory.noise)
    decoder = _RecordingDecoder(session, plan, backend, factory)
    layer = _RecordingLayer(plan, decoder)
    traced = _trace(program, factory, layer, backend)
    if traced is None:
        return None
    offsets = [0]
    for table in decoder.decoders:
        offsets.append(offsets[-1] + table.output_count)
    resolve = [offsets[table] + index for table, index in layer.sources]
    return NativeBatch(
        tuple(backend.instructions),
        backend.num_qubits,
        backend.num_measurements,
        tuple(decoder.decoders),
        _outputs(*traced, resolve),
        tuple(backend.frames),
    )


def _trace_replay(
    program: AdaptiveProgram,
    factory: ExecutionPipelineFactory[AdaptiveProgram, list[OutputRecordValue]],
    plan: LayerPlan,
    create_session: DecoderFactory,
) -> ReplayBatch | None:
    backend = _RecordingBackend(factory.noise)
    decoder = _DeferringDecoder(plan, backend, factory)
    layer = _RecordingLayer(plan, decoder)
    decoder.layout = layer.layout
    traced = _trace(program, factory, layer, backend)
    if traced is None:
        return None
    return ReplayBatch(
        tuple(backend.instructions),
        backend.num_qubits,
        backend.num_measurements,
        tuple(decoder.events),
        tuple(layer.sources),
        _outputs(*traced, range(len(layer.sources))),
        create_session,
        tuple(backend.frames),
    )
