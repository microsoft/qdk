from __future__ import annotations

from collections.abc import Iterable
from contextlib import ExitStack, closing
from typing import Generic, TypeVar, cast

from .protocols import (
    ClassicalRuntime,
    Closable,
    ExecutionLayer,
    OperationExecutor,
    Readouts,
    Requests,
    Resources,
    Startable,
)
from .quantum_operations import Operation

ProgramT = TypeVar("ProgramT")
ResultT = TypeVar("ResultT")
ValueT = TypeVar("ValueT")


class ExecutionPipeline(Generic[ProgramT, ResultT]):
    def __init__(
        self,
        classical_runtime: ClassicalRuntime[ProgramT, ResultT],
        layers: Iterable[ExecutionLayer],
        quantum_backend: OperationExecutor,
    ) -> None:
        self.classical_runtime = classical_runtime
        self.layers = tuple(layers)
        self.quantum_backend = quantum_backend
        self.closed = False

    def run(self, bytecode: ProgramT) -> ResultT:
        self._check_pipeline_is_open()
        try:
            self._start(self.classical_runtime.required_resources(bytecode))
            return self._drive(self.classical_runtime.run(bytecode), 0)
        finally:
            self._close()

    def resource_counts(self, upper: Resources) -> tuple[Resources, ...]:
        counts = [upper]
        for layer in self.layers:
            counts.append(layer.required_resources(counts[-1]))
        return tuple(counts)

    def _check_pipeline_is_open(self) -> None:
        if self.closed:
            raise RuntimeError("Execution pipeline is closed")

    def _drive(self, requests: Requests[ValueT], depth: int) -> ValueT:
        with closing(requests):
            reply: Readouts | None = None
            while True:
                try:
                    request = requests.send(reply)
                except StopIteration as completed:
                    return cast(ValueT, completed.value)
                if depth == len(self.layers):
                    if not isinstance(request, Operation):
                        raise TypeError(
                            "The quantum backend requires a lowered Operation"
                        )
                    reply = self.quantum_backend.execute(request)
                else:
                    reply = self._drive(self.layers[depth].handle(request), depth + 1)

    def _start(self, upper: Resources) -> None:
        self._check_pipeline_is_open()
        counts = self.resource_counts(upper)
        for layer, lower in zip(self.layers, counts[1:]):
            if isinstance(layer, Startable):
                layer.start(lower)
        if isinstance(self.quantum_backend, Startable):
            self.quantum_backend.start(counts[-1])

    def _close(self) -> None:
        self._check_pipeline_is_open()
        self.closed = True
        with ExitStack() as resources:
            for component in reversed(
                (self.classical_runtime, *self.layers, self.quantum_backend)
            ):
                if isinstance(component, Closable):
                    resources.callback(component.close)
