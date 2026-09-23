from __future__ import annotations

import sys
from typing import cast, Literal, TypeAlias, TypeVar

from pyqir import Module
from qodec import Qodec

from ..._adaptive_pass import AdaptiveProgram
from ..._types import QirInputData
from .. import NoiseConfig
from .._simulation import OutputRecordingPass, preprocess_simulation_input
from .adaptive_runtime import AdaptiveRuntime, OutputRecordValue
from .bytecode import compile
from .decoding import prepare_syndrome_decoder
from .executor import Executor
from .protocols import (
    ExecutionRejected,
    ExecutionUnresolved,
    PrepareDecoder,
    QuantumBackendFactory,
)
from .quantum_backend import full_state_backend, stabilizer_backend
from .readout_equations import InconsistentParity

ResultT = TypeVar("ResultT")
ShotFailurePolicy: TypeAlias = Literal["raise", "discard", "retry"]


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
    on_shot_failure: ShotFailurePolicy = "raise",
    max_retries: int = 3,
) -> list[object]:
    match type:
        case "cpu":
            quantum_backend_factory = full_state_backend
        case "clifford":
            quantum_backend_factory = stabilizer_backend
        case "gpu":
            raise NotImplementedError("Qodec execution does not support GPU simulation")
        case None:
            pass
        case _:
            raise ValueError(f"Invalid simulator type: {type}")
    module, shots, noise, seed = preprocess_simulation_input(qir, shots, noise, seed)
    executor = Executor[AdaptiveProgram, list[OutputRecordValue]](
        qodec, decoder, noise, AdaptiveRuntime, quantum_backend_factory
    )
    executor.set_seed(seed)
    recorder = OutputRecordingPass()
    recorder.run(module)
    return [
        recorder.process_output(cast(list[object], records))
        for records in run_qir_raw_records(
            module,
            executor,
            shots,
            on_shot_failure=on_shot_failure,
            max_retries=max_retries,
        )
    ]


def run_qir_raw_records(
    module: Module,
    executor: Executor[AdaptiveProgram, ResultT],
    shots: int,
    *,
    on_shot_failure: ShotFailurePolicy = "raise",
    max_retries: int = 3,
) -> list[ResultT]:
    if on_shot_failure not in ("raise", "discard", "retry"):
        raise ValueError("on_shot_failure must be 'raise', 'discard', or 'retry'")
    if type(max_retries) is not int or max_retries < 0:
        raise ValueError("max_retries must be a non-negative integer")
    bytecode = compile(module)
    records: list[ResultT] = []
    for shot_index in range(shots):
        for attempt in range(max_retries + 1):
            try:
                records.append(executor.run(bytecode))
                break
            except (
                ExecutionRejected,
                ExecutionUnresolved,
                InconsistentParity,
            ) as error:
                if on_shot_failure == "raise":
                    raise
                if on_shot_failure == "discard":
                    break
                if attempt == max_retries:
                    note = (
                        f"Shot {shot_index + 1} failed after {attempt + 1} attempts "
                        f"(max_retries={max_retries})."
                    )
                    if sys.version_info >= (3, 11):
                        error.add_note(note)
                    else:
                        error.args = (*error.args, note)
                    raise
    return records
