# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from .telemetry import log_telemetry
import math
from typing import Union

# For metrics such as duration, we want to capture things like how many shots or qubits in
# the additional properties. However properties shouldn't be 'continuous' values, as they
# create new 'dimensions' on the backend, which is limited, thus we want to bucket these properties.

# See some of the notes at: https://learn.microsoft.com/en-us/azure/azure-monitor/essentials/metrics-custom-overview#design-limitations-and-considerations


def get_next_power_of_ten_bucket(value: int) -> int:
    if value <= 1:
        return 1
    elif value >= 1000000:
        # Limit the buckets upper bound
        return 1000000
    else:
        # Bucket into nearest (rounded up) power of 10, e.g. 75 -> 100, 450 -> 1000, etc.
        return 10 ** math.ceil(math.log10(value))


# gets the order of magnitude for the number of qubits
def get_qubits_bucket(qubits: Union[str, int]) -> str:
    if qubits == "unknown":
        return "unknown"
    qubits = int(qubits)
    if qubits <= 1:
        return "1"
    elif qubits >= 50:
        return "50"
    else:
        # integer divide by 5 to get nearest 5
        return str(qubits // 5 * 5)


def on_import() -> None:
    log_telemetry("qsharp.import", 1)


def on_qdk_import() -> None:
    log_telemetry("qdk.import", 1)


def on_run(shots: int, noise: bool, qubit_loss: bool) -> None:
    log_telemetry(
        "qsharp.run",
        1,
        properties={
            "shots": get_next_power_of_ten_bucket(shots),
            "noise": noise,
            "qubit_loss": qubit_loss,
        },
    )


def on_run_end(durationMs: float, shots: int) -> None:
    log_telemetry(
        "qsharp.run.durationMs",
        durationMs,
        properties={"shots": get_next_power_of_ten_bucket(shots)},
        type="histogram",
    )


def on_run_qasm(shots: int, noise: bool, qubit_loss: bool) -> None:
    log_telemetry(
        "qsharp.run_qasm",
        1,
        properties={
            "shots": get_next_power_of_ten_bucket(shots),
            "noise": noise,
            "qubit_loss": qubit_loss,
        },
    )


def on_run_qasm_end(durationMs: float, shots: int) -> None:
    log_telemetry(
        "qsharp.run_qasm.durationMs",
        durationMs,
        properties={"shots": get_next_power_of_ten_bucket(shots)},
        type="histogram",
    )


def on_eval() -> None:
    log_telemetry(
        "qsharp.eval",
        1,
    )


def on_eval_end(durationMs: float) -> None:
    log_telemetry(
        "qsharp.eval.durationMs",
        durationMs,
        type="histogram",
    )


def on_import_qasm() -> None:
    log_telemetry(
        "qsharp.import_qasm",
        1,
    )


def on_import_qasm_end(durationMs: float) -> None:
    log_telemetry(
        "qsharp.import_qasm.durationMs",
        durationMs,
        type="histogram",
    )


def on_run_cell() -> None:
    log_telemetry(
        "qsharp.run.cell",
        1,
    )


def on_run_cell_end(durationMs: float) -> None:
    log_telemetry(
        "qsharp.run.cell.durationMs",
        durationMs,
        type="histogram",
    )


def on_compile(profile: str) -> None:
    log_telemetry("qsharp.compile", 1, properties={"profile": profile})


def on_compile_end(durationMs: float, profile: str) -> None:
    log_telemetry(
        "qsharp.compile.durationMs",
        durationMs,
        properties={"profile": profile},
        type="histogram",
    )


def on_compile_qasm(profile: str) -> None:
    log_telemetry("qsharp.compile_qasm", 1, properties={"profile": profile})


def on_compile_qasm_end(durationMs: float, profile: str) -> None:
    log_telemetry(
        "qsharp.compile_qasm.durationMs",
        durationMs,
        properties={"profile": profile},
        type="histogram",
    )


def on_estimate() -> None:
    log_telemetry(
        "qsharp.estimate",
        1,
    )


def on_estimate_end(durationMs: float, qubits: Union[str, int]) -> None:
    log_telemetry(
        "qsharp.estimate.durationMs",
        durationMs,
        properties={"qubits": get_qubits_bucket(qubits)},
        type="histogram",
    )


def on_estimate_qasm() -> None:
    log_telemetry(
        "qsharp.estimate_qasm",
        1,
    )


def on_estimate_qasm_end(durationMs: float, qubits: Union[str, int]) -> None:
    log_telemetry(
        "qsharp.estimate_qasm.durationMs",
        durationMs,
        properties={"qubits": get_qubits_bucket(qubits)},
        type="histogram",
    )


def on_circuit() -> None:
    log_telemetry(
        "qsharp.circuit",
        1,
    )


def on_circuit_end(durationMs: float) -> None:
    log_telemetry(
        "qsharp.circuit.durationMs",
        durationMs,
        type="histogram",
    )


def on_circuit_qasm() -> None:
    log_telemetry(
        "qsharp.circuit_qasm",
        1,
    )


def on_circuit_qasm_end(durationMs: float) -> None:
    log_telemetry(
        "qsharp.circuit_qasm.durationMs",
        durationMs,
        type="histogram",
    )


# Qiskit telemetry events


def on_qiskit_run(shots: int, num_circuits: int) -> None:
    log_telemetry(
        "qiskit.run",
        1,
        properties={
            "shots": get_next_power_of_ten_bucket(shots),
            "circuits": get_next_power_of_ten_bucket(num_circuits),
        },
    )


def on_qiskit_run_end(shots: int, num_circuits: int, duration_ms: float) -> None:
    log_telemetry(
        "qiskit.run.durationMs",
        duration_ms,
        properties={
            "shots": get_next_power_of_ten_bucket(shots),
            "circuits": get_next_power_of_ten_bucket(num_circuits),
        },
        type="histogram",
    )


def on_qiskit_run_re() -> None:
    log_telemetry(
        "qiskit.run.re",
        1,
    )


def on_qiskit_run_re_end(duration_ms: float) -> None:
    log_telemetry(
        "qiskit.run.re.durationMs",
        duration_ms,
        type="histogram",
    )
