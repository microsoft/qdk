from __future__ import annotations

from collections.abc import Callable, Generator, Mapping, Sequence
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


DecoderFactory: TypeAlias = Callable[[int | None], DecoderSession]
PrepareDecoder: TypeAlias = Callable[[Layer], DecoderFactory]
