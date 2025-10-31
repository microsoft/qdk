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
    from qiskit.circuit import QuantumCircuit, Parameter
    from qiskit.circuit.library import RGQFTMultiplier
    from qsharp.interop.qiskit import ResourceEstimatorBackend
    from qiskit.version import __version__ as QISKIT_VERSION


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
    sim = ResourceEstimatorBackend(skip_transpilation=True)
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
    params = EstimatorParams()
    sim = ResourceEstimatorBackend()
    job = sim.run(circuit, params=params, optimization_level=0)
    res = job.result()
    assert res["status"] == "success"

    if QISKIT_VERSION.startswith("1."):
        assert res.logical_counts == LogicalCounts(
            {
                "numQubits": 16,
                "tCount": 90,
                "rotationCount": 972,
                "rotationDepth": 666,
                "cczCount": 0,
                "ccixCount": 0,
                "measurementCount": 0,
            }
        )
    elif QISKIT_VERSION.startswith("2."):
        assert res.logical_counts == LogicalCounts(
            {
                "numQubits": 16,
                "tCount": 154,
                "rotationCount": 574,
                "rotationDepth": 374,
                "cczCount": 0,
                "ccixCount": 0,
                "measurementCount": 0,
            }
        )
    else:
        assert False, f"Unsupported Qiskit version {QISKIT_VERSION}."


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_estimate_qiskit_rgqft_multiplier_without_tranpspile() -> None:
    bitwidth = 4
    circuit = RGQFTMultiplier(num_state_qubits=bitwidth)
    params = EstimatorParams()
    sim = ResourceEstimatorBackend(skip_transpilation=True)
    job = sim.run(circuit, params=params)
    res = job.result()
    assert res["status"] == "success"

    if QISKIT_VERSION.startswith("1."):
        assert res.logical_counts == LogicalCounts(
            {
                "numQubits": 16,
                "tCount": 76,
                "rotationCount": 936,
                "rotationDepth": 665,
                "cczCount": 0,
                "ccixCount": 0,
                "measurementCount": 0,
            }
        )
    elif QISKIT_VERSION.startswith("2."):
        assert res.logical_counts == LogicalCounts(
            {
                "numQubits": 16,
                "tCount": 140,
                "rotationCount": 532,
                "rotationDepth": 369,
                "cczCount": 0,
                "ccixCount": 0,
                "measurementCount": 0,
            }
        )
    else:
        assert False, f"Unsupported Qiskit version {QISKIT_VERSION}."


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_estimate_qiskit_rgqft_multiplier_in_threadpool() -> None:
    bitwidth = 4
    circuit = RGQFTMultiplier(num_state_qubits=bitwidth)
    params = EstimatorParams()
    executor = ThreadPoolExecutor(max_workers=1)
    sim = ResourceEstimatorBackend(executor=executor, skip_transpilation=True)
    job = sim.run(circuit, params=params)
    res = job.result()
    assert res["status"] == "success"

    if QISKIT_VERSION.startswith("1."):
        assert res.logical_counts == LogicalCounts(
            {
                "numQubits": 16,
                "tCount": 76,
                "rotationCount": 936,
                "rotationDepth": 665,
                "cczCount": 0,
                "ccixCount": 0,
                "measurementCount": 0,
            }
        )
    elif QISKIT_VERSION.startswith("2."):
        assert res.logical_counts == LogicalCounts(
            {
                "numQubits": 16,
                "tCount": 140,
                "rotationCount": 532,
                "rotationDepth": 369,
                "cczCount": 0,
                "ccixCount": 0,
                "measurementCount": 0,
            }
        )
    else:
        assert False, f"Unsupported Qiskit version {QISKIT_VERSION}."


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_estimating_with_unbound_param_raises():
    theta = Parameter("theta")

    circuit = QuantumCircuit(1)
    circuit.name = "test"
    circuit.rx(theta, 0)
    circuit.measure_all()

    backend = ResourceEstimatorBackend()
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
