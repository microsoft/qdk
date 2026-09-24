"""Prepare terminal Clifford Qodec runs for the native shot loop.

Tracing uses the ordinary layer and physical instruction lowering without
sampling a quantum state. One encoded layer with a batch-capable decoder and
measurement-independent execution is eligible. Decoders supply their own
terminal-readout evaluators. Intermediate measurements, loss, and reset/readout
noise retain the interpreter path. Native sampling uses its own seeded stream.
"""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass
from itertools import product
from random import Random
from typing import Literal, cast

from qdk import Result

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
from .layer_runtime import LayerPlan, LayerRuntime
from .logical_qubits import LogicalQubits
from .protocols import (
    BatchDecoderFactory,
    BatchDecoderSession,
    BatchUnsupported,
    BeforeInvocation,
    BlockObserver,
    BlockReference,
    Corrections,
    Decoded,
    ExecutionRejected,
    ExecutionUnresolved,
    Invocation,
    ReadoutBatch,
    Readouts,
    Resources,
)
from .quantum_backend import stabilizer_backend
from .quantum_operations import Operation, local_indices
from .readout_equations import InconsistentParity

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
        rng = Random(seed)
        seeds = []
        for _ in range(shots):
            components = Random(rng.getrandbits(64))
            components.getrandbits(64)
            seeds.append(components.getrandbits(64))
        logical: list[list[OutputRecordValue]] = [[] for _ in physical]
        failures: dict[int, Exception] = {}
        for decoder in self.decoders:
            rows = [
                tuple(
                    value == Result.One
                    for value in shot[decoder.start : decoder.start + decoder.width]
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
        self.num_qubits = 0
        self.num_measurements = 0
        self.checked_noise: set[tuple[str, int]] = set()

    def start(self, resources: Resources) -> None:
        self.num_qubits = resources.qubits

    def execute(self, request: Operation) -> Readouts:
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
        self.result_sources: dict[int, int] = {}
        self.output_sources: list[int | None] = []

    def _apply(
        self, operation: str, targets: tuple[int, ...], angle: float | None = None
    ) -> None:
        if self.measured:
            raise _NotBatchable
        super()._apply(operation, targets, angle)

    def mz(self, target: int, result_id: int) -> None:
        if target in self.measured:
            raise _NotBatchable
        self.result_sources[result_id] = len(self.measured)
        self.measured.add(target)
        super().mz(target, result_id)

    def result(self, result_id: int) -> Result:
        self.output_sources.append(self.result_sources.get(result_id))
        return cast(Result, Result.Zero)


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

    def before(self, invocation: Invocation) -> Corrections[None]:
        gadget = invocation.gadget
        if gadget.implements.flags or gadget.implements.parameters:
            raise _NotBatchable
        body = self.plan.gadgets[invocation.call.mnemonic].body.create_runtime()
        if not isinstance(body, CallListRuntime):
            raise _NotBatchable
        for prepared in body.calls:
            if (
                prepared.call.arguments
                or prepared.call.select
                or prepared.declaration.flags
                or isinstance(self.operations[prepared.call.mnemonic], PreparedAction)
            ):
                raise _NotBatchable
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


def prepare_batch(
    program: AdaptiveProgram,
    factory: ExecutionPipelineFactory[AdaptiveProgram, list[OutputRecordValue]],
) -> NativeBatch | None:
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
    if not isinstance(create_session, BatchDecoderFactory):
        return None
    backend = _RecordingBackend(factory.noise)
    runtime = _RecordingRuntime()
    try:
        session = create_session.prepare_batch()
    except BatchUnsupported:
        return None
    decoder = _RecordingDecoder(session, plan, backend, factory)
    pipeline = ExecutionPipeline(
        runtime,
        [
            LogicalQubits(),
            LayerRuntime(plan, decoder),
            InstructionRuntime(
                factory.physical, operations=factory.physical_operations
            ),
        ],
        backend,
    )
    try:
        records = pipeline.run(program)
    except _NotBatchable:
        return None
    sources = iter(runtime.output_sources)
    outputs = []
    for value in records:
        source = next(sources) if isinstance(value, Result) else None
        outputs.append(value if source is None else _Readout(source))
    return NativeBatch(
        tuple(backend.instructions),
        backend.num_qubits,
        backend.num_measurements,
        tuple(decoder.decoders),
        tuple(outputs),
    )
