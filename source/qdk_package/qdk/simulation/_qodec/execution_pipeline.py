from __future__ import annotations

from contextlib import ExitStack
from random import Random
from typing import Generic, TypeVar

from qodec import Qodec

from .. import NoiseConfig
from ._pipeline import ExecutionPipeline
from .instruction_set import InstructionRuntime, InstructionSet, prepare_operations
from .layer_runtime import LayerPlan, LayerRuntime
from .logical_qubits import LogicalQubits
from .protocols import (
    ClassicalRuntimeFactory,
    Closable,
    DecoderFactory,
    ExecutionLayer,
    PrepareDecoder,
    QuantumBackendFactory,
)

ProgramT = TypeVar("ProgramT")
ResultT = TypeVar("ResultT")


class Executor(Generic[ProgramT, ResultT]):
    def __init__(
        self,
        qodec: Qodec,
        decoder: PrepareDecoder,
        noise: NoiseConfig | None,
        classical_runtime_factory: ClassicalRuntimeFactory[ProgramT, ResultT],
        quantum_backend_factory: QuantumBackendFactory,
    ) -> None:
        self.pipeline_factory = ExecutionPipelineFactory(
            qodec, decoder, noise, classical_runtime_factory, quantum_backend_factory
        )

    def set_seed(self, seed: int | None) -> None:
        self.pipeline_factory.set_seed(seed)

    def run(self, bytecode: ProgramT) -> ResultT:
        return self.pipeline_factory.build_pipeline().run(bytecode)


class ExecutionPipelineFactory(Generic[ProgramT, ResultT]):
    def __init__(
        self,
        qodec: Qodec,
        decoder: PrepareDecoder,
        noise: NoiseConfig | None,
        classical_runtime_factory: ClassicalRuntimeFactory[ProgramT, ResultT],
        quantum_backend_factory: QuantumBackendFactory,
    ) -> None:
        self.noise = noise
        self.classical_runtime_factory = classical_runtime_factory
        self.quantum_backend_factory = quantum_backend_factory
        self.rng = Random()
        qodec.validate()
        if not qodec.layers:
            raise ValueError("A Qodec must contain a physical instruction set")
        self.physical = InstructionSet(qodec.layers[-1].instruction_set)
        self.physical_operations = prepare_operations(self.physical)
        self.prepared: tuple[tuple[LayerPlan, DecoderFactory], ...] = ()
        if len(qodec.layers) > 1:
            if not callable(decoder):
                raise TypeError("decoder must be a callable that prepares a layer")
            self.prepared = tuple(
                (LayerPlan(layer), decoder(layer)) for layer in qodec.layers[:-1]
            )

    def set_seed(self, seed: int | None) -> None:
        self.rng.seed(seed)

    def build_pipeline(self) -> ExecutionPipeline[ProgramT, ResultT]:
        seeds = Random(self.rng.getrandbits(64))
        backend_seed = seeds.getrandbits(64)
        decoder_seeds = [seeds.getrandbits(64) for _ in self.prepared]
        with ExitStack() as resources:
            backend = self.quantum_backend_factory(self.noise, backend_seed)
            if isinstance(backend, Closable):
                resources.callback(backend.close)
            layers: list[ExecutionLayer] = []
            for (plan, decoder_factory), decoder_seed in zip(
                self.prepared, decoder_seeds
            ):
                session = decoder_factory(decoder_seed)
                resources.callback(session.close)
                layers.append(LayerRuntime(plan, session))
            pipeline = ExecutionPipeline(
                self.classical_runtime_factory(),
                [
                    LogicalQubits(),
                    *layers,
                    InstructionRuntime(
                        self.physical, operations=self.physical_operations
                    ),
                ],
                backend,
            )
            resources.pop_all()
            return pipeline
