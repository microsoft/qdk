from __future__ import annotations

from typing import Literal, TypeVar

from pyqir import Module
from qodec import Qodec

from ..._adaptive_pass import AdaptiveProgram
from ..._types import QirInputData
from .. import NoiseConfig
from .._simulation import OutputRecordingPass, preprocess_simulation_input
from .adaptive_runtime import AdaptiveRuntime, OutputRecordValue
from .bytecode import compile
from .decoding import prepare_syndrome_decoder
from .execution_pipeline import Executor
from .protocols import PrepareDecoder, QuantumBackendFactory
from .quantum_backend import full_state_backend, stabilizer_backend

ResultT = TypeVar("ResultT")


def run_qir_with_qodec(
    qir: QirInputData | str | bytes,
    qodec: Qodec,
    noise: NoiseConfig | None,
    shots: int | None = 1,
    seed: int | None = None,
    *,
    decoder: PrepareDecoder = prepare_syndrome_decoder,
    quantum_backend_factory: QuantumBackendFactory = stabilizer_backend,
    type: Literal["clifford", "cpu", "gpu"] | None = None,
) -> list[object]:
    if type == "gpu":
        raise NotImplementedError("Qodec execution does not support the GPU simulator")
    if type not in (None, "clifford", "cpu"):
        raise ValueError(f"Invalid simulator type: {type}")
    if type == "cpu":
        quantum_backend_factory = full_state_backend
    module, shots, noise, seed = preprocess_simulation_input(qir, shots, noise, seed)
    executor = Executor[AdaptiveProgram, list[OutputRecordValue]](
        qodec, decoder, noise, AdaptiveRuntime, quantum_backend_factory
    )
    recorder = OutputRecordingPass()
    recorder.run(module)
    return [
        recorder.process_output(list(records))
        for records in run_qir_raw_records(module, executor, shots, seed)
    ]


def run_qir_raw_records(
    module: Module,
    executor: Executor[AdaptiveProgram, ResultT],
    shots: int,
    seed: int | None,
) -> list[ResultT]:
    bytecode = compile(module)
    if seed is not None:
        executor.set_seed(seed)
    return [executor.run(bytecode) for _ in range(shots)]
