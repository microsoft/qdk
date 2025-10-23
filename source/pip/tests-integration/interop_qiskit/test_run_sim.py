# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from concurrent.futures import ThreadPoolExecutor
import pytest
from qsharp import QSharpError, TargetProfile

from interop_qiskit import QISKIT_AVAILABLE, SKIP_REASON

if QISKIT_AVAILABLE:
    from .test_circuits import (
        generate_repro_information,
    )
    from qiskit.circuit import QuantumCircuit, Parameter
    from qiskit_aer import AerSimulator
    from qiskit.qasm3 import loads as from_qasm3
    from qiskit.providers import JobStatus
    from qiskit import ClassicalRegister
    from qiskit import transpile
    from qsharp.interop.qiskit import QSharpBackend, QirTarget
    from .test_circuits import (
        generate_repro_information,
    )


# Convert the circuit to QASM3 and back to a backend.
# Then load the QASM3 and convert it back to a circuit.
# This is to ensure that the QASM3 conversion is semantically correct.
def round_trip_circuit(circuit, backend):
    qasm = backend._qasm(circuit)
    circuit = from_qasm3(qasm)
    return circuit


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_run_smoke() -> None:
    circuit = QuantumCircuit(2, 2)
    circuit.x(0)
    circuit.cx(0, 1)
    circuit.measure_all(add_bits=False)
    backend = QSharpBackend()
    res = backend.run(circuit, shots=1).result()
    assert res is not None


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_run_state_prep_smoke() -> None:
    circuit = QuantumCircuit(1)
    circuit.initialize([0.6, 0.8])
    circuit.measure_all()
    backend = QSharpBackend(num_qubits=2)
    target = QirTarget.create_target(num_qubits=2)
    circuit = transpile(circuit, backend=backend, target=target, optimization_level=0)
    res = backend.run(circuit, shots=1).result()
    assert res is not None


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_get_counts_matches_qiskit_simulator():
    target_profile = TargetProfile.Base
    circuit = create_deterministic_test_circuit()
    backend = QSharpBackend(target_profile=target_profile)

    try:
        circuit = round_trip_circuit(circuit, backend)

        aersim = AerSimulator()
        job = aersim.run(circuit, shots=5)
        qsjob = backend.run(circuit, shots=5)
        qscounts = qsjob.result().get_counts()
        assert qscounts == job.result().get_counts()
    except AssertionError:
        raise
    except Exception as ex:
        additional_info = generate_repro_information(circuit, backend)
        raise RuntimeError(additional_info) from ex


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def create_deterministic_test_circuit():
    cr0 = ClassicalRegister(2, "first")
    cr1 = ClassicalRegister(3, "second")
    circuit = QuantumCircuit(5)
    circuit.add_register(cr0)
    circuit.add_register(cr1)
    circuit.x(0)
    circuit.id(1)
    circuit.id(2)
    circuit.x(3)
    circuit.x(4)
    circuit.measure_all(add_bits=False)
    return circuit


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_execution_works_with_threadpool_set_on_backend():
    target_profile = TargetProfile.Base
    circuit = create_deterministic_test_circuit()
    executor = ThreadPoolExecutor(max_workers=4)
    backend = QSharpBackend(target_profile=target_profile, executor=executor, shots=5)

    try:
        job = backend.run(circuit)
        qscounts = job.result().get_counts()
        assert str(qscounts) == "{'110 01': 5}"
    except AssertionError:
        raise
    except Exception as ex:
        additional_info = generate_repro_information(circuit, backend)
        raise RuntimeError(additional_info) from ex


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_execution_works_with_threadpool_set_on_run():
    target_profile = TargetProfile.Base
    circuit = create_deterministic_test_circuit()

    backend = QSharpBackend(target_profile=target_profile, shots=5)
    try:
        executor = ThreadPoolExecutor(max_workers=1)
        job = backend.run(circuit, executor=executor)
        qscounts = job.result().get_counts()
        assert str(qscounts) == "{'110 01': 5}"
    except AssertionError:
        raise
    except Exception as ex:
        additional_info = generate_repro_information(circuit, backend)
        raise RuntimeError(additional_info) from ex


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_get_counts_matches_qiskit_simulator_multiple_circuits():
    target_profile = TargetProfile.Base
    circuit = create_deterministic_test_circuit()
    circuit2 = create_deterministic_test_circuit()
    backend = QSharpBackend(target_profile=target_profile)

    try:
        circuit = round_trip_circuit(circuit, backend)

        aersim = AerSimulator()
        job = aersim.run([circuit, circuit2], shots=5)
        qsjob = backend.run([circuit, circuit2], shots=5)
        qscounts = qsjob.result().get_counts()
        assert qscounts == job.result().get_counts()
    except AssertionError:
        raise
    except Exception as ex:
        additional_info = generate_repro_information(circuit, backend)
        raise RuntimeError(additional_info) from ex


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_simulting_with_unbound_param_raises():
    theta = Parameter("theta")

    circuit = QuantumCircuit(1)
    circuit.name = "test"
    circuit.rx(theta, 0)
    circuit.measure_all()

    backend = QSharpBackend()
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
