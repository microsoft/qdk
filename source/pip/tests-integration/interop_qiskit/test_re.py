# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from concurrent.futures import ThreadPoolExecutor
import pytest

from qsharp import QSharpError
from qsharp.estimator import (
    EstimatorParams,
    QubitParams,
    LogicalCounts,
)


from interop_qiskit import QISKIT_AVAILABLE, SKIP_REASON

if QISKIT_AVAILABLE:
    from .test_circuits import (
        generate_repro_information,
    )
    from qiskit import transpile
    from qiskit.circuit import QuantumCircuit, Parameter
    from qiskit.circuit.library import RGQFTMultiplier
    from qsharp.interop.qiskit import ResourceEstimatorBackend

from qsharp.interop.qiskit import estimate


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qsharp_estimation_with_single_params() -> None:
    params = EstimatorParams()
    params.error_budget = 0.333
    params.qubit_params.name = QubitParams.MAJ_NS_E4
    assert params.as_dict() == {
        "qubitParams": {"name": "qubit_maj_ns_e4"},
        "errorBudget": 0.333,
    }
    circuit = QuantumCircuit(10, 10)
    for index in range(10):
        circuit.t(index)
        circuit.measure(index, index)
    sim = ResourceEstimatorBackend(circuit.num_qubits)
    res = sim.run(circuit, params=params).result()

    assert res["status"] == "success"
    assert res["physicalCounts"] is not None
    assert res["jobParams"]["qubitParams"]["name"] == "qubit_maj_ns_e4"
    assert res.logical_counts == LogicalCounts(
        {
            "numQubits": 10,
            "tCount": 10,
            "rotationCount": 0,
            "rotationDepth": 0,
            "cczCount": 0,
            "measurementCount": 10,
        }
    )


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_estimate_qiskit_rgqft_multiplier() -> None:
    bitwidth = 4
    circuit = RGQFTMultiplier(num_state_qubits=bitwidth)
    sim = ResourceEstimatorBackend(circuit.num_qubits)
    circuit = transpile(circuit, sim)
    params = EstimatorParams()
    job = sim.run(circuit, params=params)
    res = job.result()
    assert res["status"] == "success"
    assert res.logical_counts == LogicalCounts(
        {
            "numQubits": 16,
            "tCount": 138,
            "rotationCount": 514,
            "rotationDepth": 323,
            "cczCount": 0,
            "ccixCount": 0,
            "measurementCount": 0,
        }
    )


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_estimate_qiskit_rgqft_multiplier_in_threadpool() -> None:
    bitwidth = 4
    circuit = RGQFTMultiplier(num_state_qubits=bitwidth)
    executor = ThreadPoolExecutor(max_workers=1)
    sim = ResourceEstimatorBackend(circuit.num_qubits, executor=executor)
    circuit = transpile(circuit, sim)
    params = EstimatorParams()
    job = sim.run(circuit, params=params)
    res = job.result()
    assert res["status"] == "success"
    assert res.logical_counts == LogicalCounts(
        {
            "numQubits": 16,
            "tCount": 138,
            "rotationCount": 514,
            "rotationDepth": 323,
            "cczCount": 0,
            "ccixCount": 0,
            "measurementCount": 0,
        }
    )


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_estimating_with_unbound_param_raises():
    theta = Parameter("theta")

    circuit = QuantumCircuit(1)
    circuit.name = "test"
    circuit.rx(theta, 0)
    circuit.measure_all()

    backend = ResourceEstimatorBackend(circuit.num_qubits)
    try:
        with pytest.raises(QSharpError) as ex:
            _ = backend.run(circuit).result()
        message = str(ex.value)
        assert "Circuit has unbound input parameters" in message
        assert "help: Parameters: theta: Double" in message
    except AssertionError:
        raise
    except Exception as ex:
        additional_info = generate_repro_information(circuit, backend)
        raise RuntimeError(additional_info) from ex
