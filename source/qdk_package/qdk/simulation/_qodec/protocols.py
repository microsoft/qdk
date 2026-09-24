from __future__ import annotations

from collections.abc import Callable, Generator, Mapping, Sequence
from copy import copy
from dataclasses import dataclass, field
from types import MappingProxyType
from typing import Protocol, TypeAlias, TypeVar, runtime_checkable

from qodec import Gadget, Layer
from qodec.gadgets import Circuit
from qodec.instructions import InstructionCall

from .. import NoiseConfig
from .quantum_operations import Operation, RestoreMeasured

ProgramT = TypeVar("ProgramT", contravariant=True)
ResultT = TypeVar("ResultT", covariant=True)

Readouts: TypeAlias = tuple[bool | None, ...]
Request: TypeAlias = Operation | InstructionCall | RestoreMeasured
Requests: TypeAlias = Generator[Request, Readouts | None, ResultT]


@dataclass(frozen=True)
class Resources:
    qubits: int = 0
    blocks: Mapping[str, int] = field(default_factory=dict)

    def __post_init__(self) -> None:
        if any(count < 0 for count in (self.qubits, *self.blocks.values())):
            raise ValueError("Resource capacities must be non-negative")
        object.__setattr__(self, "blocks", MappingProxyType(dict(self.blocks)))


@dataclass(frozen=True)
class BlockReference:
    label: int | str
    generation: int
    block_type: str


@dataclass(frozen=True)
class Invocation:
    id: int
    gadget: Gadget
    call: InstructionCall
    inputs: tuple[BlockReference, ...]
    outputs: tuple[BlockReference, ...]


@dataclass(frozen=True)
class Correction:
    blocks: tuple[BlockReference, ...]
    operation: Operation


Corrections: TypeAlias = Generator[Correction, Readouts | None, ResultT]


@dataclass(frozen=True)
class Decoded:
    outcomes: Readouts
    flags: Readouts = ()

    @property
    def readouts(self) -> Readouts:
        return self.outcomes + self.flags


class ExecutionRejected(RuntimeError):
    pass


class ExecutionUnresolved(RuntimeError):
    pass


class ClassicalRuntime(Protocol[ProgramT, ResultT]):
    def required_resources(self, program: ProgramT, /) -> Resources: ...

    def run(self, program: ProgramT, /) -> Requests[ResultT]: ...


ClassicalRuntimeFactory: TypeAlias = Callable[[], ClassicalRuntime[ProgramT, ResultT]]


@dataclass(frozen=True)
class PreparedCircuit:
    labels: tuple[str, ...]
    create_runtime: ClassicalRuntimeFactory[Invocation, Readouts]
    block_types: Mapping[str, str] = field(default_factory=dict)


PrepareCircuit: TypeAlias = Callable[[Circuit], PreparedCircuit]


class ExecutionLayer(Protocol):
    def required_resources(self, upper: Resources, /) -> Resources: ...

    def handle(self, request: Request, /) -> Requests[Readouts]: ...


class OperationExecutor(Protocol):
    def execute(self, request: Operation, /) -> Readouts: ...


QuantumBackendFactory: TypeAlias = Callable[
    [NoiseConfig | None, int], OperationExecutor
]


@runtime_checkable
class Startable(Protocol):
    def start(self, resources: Resources, /) -> None: ...


@runtime_checkable
class Closable(Protocol):
    def close(self) -> None: ...


class QuantumEngine(Closable, Protocol):
    def apply(
        self, operation: str, targets: Sequence[int], *, angle: float | None = None
    ) -> None: ...

    def measure(self, target: int) -> int: ...

    def reset(self, target: int) -> None: ...


QuantumEngineFactory: TypeAlias = Callable[[int, int | None], QuantumEngine]


class DecoderSession(Closable, Protocol):
    def decode(
        self, invocation: Invocation, readouts: Readouts, /
    ) -> Corrections[Decoded]: ...


@runtime_checkable
class BeforeInvocation(Protocol):
    def before(self, invocation: Invocation, /) -> Corrections[None]: ...


@runtime_checkable
class BlockObserver(Protocol):
    def discarded(self, blocks: Sequence[BlockReference], /) -> None: ...


class ReadoutBatch(Protocol):
    """Decode independent terminal records using the supplied per-shot seeds.

    Each output depends only on its corresponding input and seed. The prepared
    evaluator must outlive the preparation session and must not emit physical
    corrections, retain inter-shot state, or change the number of records.
    Return expected decoding failures in their shot positions; unexpected
    execution errors propagate normally.
    """

    def decode_batch(
        self, readouts: Sequence[Readouts], seeds: Sequence[int], /
    ) -> Sequence[Readouts | Exception]: ...


@dataclass(frozen=True)
class ReadoutTable:
    """A seed-independent terminal decoder, indexed by little-endian input bits."""

    values: tuple[Readouts | Exception, ...]

    def __post_init__(self) -> None:
        if not self.values or len(self.values) & (len(self.values) - 1):
            raise ValueError("Readout table size must be a power of two")

    def decode_batch(
        self, readouts: Sequence[Readouts], seeds: Sequence[int], /
    ) -> Sequence[Readouts | Exception]:
        if len(readouts) != len(seeds):
            raise ValueError("Each decoder input requires a shot seed")
        width = (len(self.values) - 1).bit_length()
        results = []
        for row in readouts:
            if len(row) != width:
                raise ValueError("Readout width does not match the prepared table")
            if any(value is None for value in row):
                results.append(
                    ExecutionUnresolved("Terminal records contain unknown readouts")
                )
                continue
            value = self.values[
                sum(bool(bit) << index for index, bit in enumerate(row))
            ]
            results.append(
                copy(value).with_traceback(None)
                if isinstance(value, Exception)
                else value
            )
        return results


class BatchDecoderSession(DecoderSession, Protocol):
    """Preparation-only session for measurement-independent execution.

    Decoding with no circuit records must be seed-independent. Terminal
    preparation returns None when its inputs cannot be decoded independently
    of earlier invocations or may yield corrections. Raise BatchUnsupported
    when preparation cannot reproduce seed-independent prefix behavior.
    Deferring terminal evaluation must not change later preparation behavior.
    """

    def prepare_readouts(
        self, invocation: Invocation, record_count: int, /
    ) -> ReadoutBatch | None: ...


@runtime_checkable
class BatchDecoderFactory(Protocol):
    """Optional capability of a callable DecoderFactory, not a decoder name."""

    def __call__(self, seed: int | None, /) -> DecoderSession: ...

    def prepare_batch(self) -> BatchDecoderSession: ...


class BatchUnsupported(Exception):
    """Batch preparation cannot preserve semantics; use the layer interpreter."""


DecoderFactory: TypeAlias = Callable[[int | None], DecoderSession]
PrepareDecoder: TypeAlias = Callable[[Layer], DecoderFactory]
