# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from concurrent.futures import ThreadPoolExecutor

import pytest

from interop_qiskit import QISKIT_AVAILABLE, SKIP_REASON

if QISKIT_AVAILABLE:
    from qiskit import ClassicalRegister
    from qiskit.circuit import QuantumCircuit
    from qiskit.providers import JobStatus
    from qdk.qiskit import NeutralAtomBackend
    from qdk.simulation import NoiseConfig
    from qdk._device._atom import NeutralAtomDevice
    from .test_circuits import generate_repro_information


# ---------------------------------------------------------------------------
# Module-scoped fixture — one NeutralAtomDevice shared across all tests.
# This avoids paying the device setup + multi-pass compilation pipeline cost
# on every individual test.
# ---------------------------------------------------------------------------


@pytest.fixture(scope="module")
def device():
    if not QISKIT_AVAILABLE:
        pytest.skip(SKIP_REASON)
    return NeutralAtomDevice()


@pytest.fixture(scope="module")
def backend(device):
    return NeutralAtomBackend(device=device)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def create_bell_circuit() -> "QuantumCircuit":
    """Two-qubit Bell state — deterministic up to measurement basis."""
    circuit = QuantumCircuit(2)
    circuit.h(0)
    circuit.cx(0, 1)
    circuit.measure_all()
    return circuit


def create_deterministic_circuit() -> "QuantumCircuit":
    """Circuit whose output is always '11' regardless of noise model."""
    cr0 = ClassicalRegister(2, "first")
    circuit = QuantumCircuit(2)
    circuit.add_register(cr0)
    circuit.x(0)
    circuit.x(1)
    circuit.measure_all(add_bits=False)
    return circuit


# ---------------------------------------------------------------------------
# Smoke tests
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_run_smoke(backend) -> None:
    circuit = create_bell_circuit()
    job = backend.run(circuit, shots=10)
    result = job.result()
    assert result is not None


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_run_returns_completed_job(backend) -> None:
    # job.result() blocks until the future completes; only then is status DONE.
    circuit = create_bell_circuit()
    job = backend.run(circuit, shots=5)
    job.result()  # block
    assert job.status() == JobStatus.DONE


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_run_deterministic_circuit(backend) -> None:
    """Deterministic circuit must always produce the same counts."""
    circuit = create_deterministic_circuit()
    try:
        job = backend.run(circuit, shots=10)
        counts = job.result().get_counts()
        assert counts == {"11": 10}
    except AssertionError:
        raise
    except Exception as ex:
        additional_info = generate_repro_information(circuit, backend)
        raise RuntimeError(additional_info) from ex


# ---------------------------------------------------------------------------
# Seed / reproducibility
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_seed_produces_reproducible_results(backend) -> None:
    circuit = create_bell_circuit()
    try:
        counts1 = backend.run(circuit, shots=200, seed=42).result().get_counts()
        counts2 = backend.run(circuit, shots=200, seed=42).result().get_counts()
        assert counts1 == counts2
    except AssertionError:
        raise
    except Exception as ex:
        additional_info = generate_repro_information(circuit, backend)
        raise RuntimeError(additional_info) from ex


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_different_seeds_may_produce_different_results(backend) -> None:
    """Two different seeds on a random circuit should produce different histograms."""
    circuit = create_bell_circuit()
    counts1 = backend.run(circuit, shots=500, seed=1).result().get_counts()
    counts2 = backend.run(circuit, shots=500, seed=2).result().get_counts()
    assert counts1 != counts2


# ---------------------------------------------------------------------------
# Bell-state outcome distribution
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_bell_state_outcomes_are_correlated(backend) -> None:
    """Bell state should only ever produce |00⟩ or |11⟩."""
    circuit = create_bell_circuit()
    try:
        counts = backend.run(circuit, shots=200, seed=99).result().get_counts()
        for bitstring in counts:
            assert bitstring in ("00", "11"), f"Unexpected outcome: {bitstring}"
        assert sum(counts.values()) == 200
    except AssertionError:
        raise
    except Exception as ex:
        additional_info = generate_repro_information(circuit, backend)
        raise RuntimeError(additional_info) from ex


# ---------------------------------------------------------------------------
# Noise model
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_run_with_noiseless_noiseconfig(backend) -> None:
    """Passing an empty NoiseConfig should behave identically to no noise."""
    circuit = create_deterministic_circuit()
    try:
        noise = NoiseConfig()
        counts = backend.run(circuit, shots=10, noise=noise).result().get_counts()
        assert counts == {"11": 10}
    except AssertionError:
        raise
    except Exception as ex:
        additional_info = generate_repro_information(circuit, backend)
        raise RuntimeError(additional_info) from ex


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_run_with_bitflip_noise_introduces_errors(backend) -> None:
    """Heavy bit-flip noise on Rz gates should flip some outcomes.

    NeutralAtomDevice.compile() decomposes all single-qubit gates to Rz + SX
    before simulation, so X gates do not survive into the final QIR.
    Noise must be applied to a native gate (Rz, SX, or mresetz) to have any effect.
    """
    circuit = create_deterministic_circuit()
    noise = NoiseConfig()
    # p=0.5 bitflip on rz — the native gate X decomposes into — guarantees errors.
    noise.rz.set_bitflip(0.5)
    counts = backend.run(circuit, shots=200, noise=noise, seed=42).result().get_counts()
    # Without noise this circuit always returns "11". With 50% bitflip noise on every
    # Rz gate, some shots must flip — "11" can no longer account for all 200 shots.
    assert counts.get("11", 0) < 200


# ---------------------------------------------------------------------------
# Simulator type selection
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_cpu_simulator_type(backend) -> None:
    circuit = create_bell_circuit()
    try:
        counts = (
            backend.run(circuit, shots=100, simulator_type="cpu", seed=7)
            .result()
            .get_counts()
        )
        for key in counts:
            assert key in ("00", "11")
        assert sum(counts.values()) == 100
    except AssertionError:
        raise
    except Exception as ex:
        additional_info = generate_repro_information(circuit, backend)
        raise RuntimeError(additional_info) from ex


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_clifford_simulator_type(backend) -> None:
    circuit = create_bell_circuit()
    try:
        counts = (
            backend.run(circuit, shots=100, simulator_type="clifford", seed=7)
            .result()
            .get_counts()
        )
        for key in counts:
            assert key in ("00", "11")
        assert sum(counts.values()) == 100
    except AssertionError:
        raise
    except Exception as ex:
        additional_info = generate_repro_information(circuit, backend)
        raise RuntimeError(additional_info) from ex


# ---------------------------------------------------------------------------
# Device injection
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_custom_device_is_used(device) -> None:
    """A device passed at construction is reused across runs."""
    backend_with_device = NeutralAtomBackend(device=device)
    circuit = create_bell_circuit()
    job = backend_with_device.run(circuit, shots=10, seed=1)
    assert job.result() is not None
    # The same device instance should be stored, not replaced.
    assert backend_with_device._get_device() is device


# ---------------------------------------------------------------------------
# Executor / async submission
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_execution_works_with_threadpool_set_on_backend(device) -> None:
    circuit = create_deterministic_circuit()
    executor = ThreadPoolExecutor(max_workers=4)
    backend_with_executor = NeutralAtomBackend(
        device=device, executor=executor, shots=10
    )
    try:
        counts = backend_with_executor.run(circuit).result().get_counts()
        assert counts == {"11": 10}
    except AssertionError:
        raise
    except Exception as ex:
        additional_info = generate_repro_information(circuit, backend_with_executor)
        raise RuntimeError(additional_info) from ex


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_execution_works_with_threadpool_set_on_run(device) -> None:
    circuit = create_deterministic_circuit()
    backend_with_device = NeutralAtomBackend(device=device, shots=10)
    try:
        executor = ThreadPoolExecutor(max_workers=1)
        counts = (
            backend_with_device.run(circuit, executor=executor).result().get_counts()
        )
        assert counts == {"11": 10}
    except AssertionError:
        raise
    except Exception as ex:
        additional_info = generate_repro_information(circuit, backend_with_device)
        raise RuntimeError(additional_info) from ex


# ---------------------------------------------------------------------------
# Multiple circuits
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_run_multiple_circuits(backend) -> None:
    circuit1 = create_deterministic_circuit()
    circuit2 = create_deterministic_circuit()
    try:
        job = backend.run([circuit1, circuit2], shots=10)
        all_counts = job.result().get_counts()
        # get_counts() must return a list when multiple circuits are submitted.
        assert isinstance(all_counts, list)
        for counts in all_counts:
            assert counts == {"11": 10}
    except AssertionError:
        raise
    except Exception as ex:
        additional_info = generate_repro_information(circuit1, backend)
        raise RuntimeError(additional_info) from ex


# ---------------------------------------------------------------------------
# Memory field
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_get_memory_returns_per_shot_list(backend) -> None:
    """result.get_memory() should return one bitstring per shot."""
    circuit = create_deterministic_circuit()
    shots = 10
    memory = backend.run(circuit, shots=shots).result().get_memory()
    assert len(memory) == shots
    assert all(m == "11" for m in memory)


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_get_memory_is_consistent_with_counts(backend) -> None:
    """The counts histogram must be consistent with the per-shot memory list."""
    circuit = create_bell_circuit()
    shots = 100
    result = backend.run(circuit, shots=shots, seed=5).result()
    memory = result.get_memory()
    counts = result.get_counts()
    assert len(memory) == shots
    from collections import Counter

    assert dict(Counter(memory)) == dict(counts)


# ---------------------------------------------------------------------------
# Raw fields (loss separation)
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_raw_fields_are_present_in_result_data(backend) -> None:
    """result.data() must expose raw_counts, raw_probabilities, and raw_memory."""
    circuit = create_deterministic_circuit()
    data = backend.run(circuit, shots=10).result().data(0)
    assert "raw_counts" in data
    assert "raw_probabilities" in data
    assert "raw_memory" in data


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_raw_fields_equal_accepted_fields_when_no_loss(backend) -> None:
    """Without loss noise, raw_* fields must equal the accepted fields."""
    from collections import Counter

    circuit = create_deterministic_circuit()
    result = backend.run(circuit, shots=20, seed=0).result()
    data = result.data(0)
    assert data["raw_counts"] == data["counts"]
    assert data["raw_memory"] == data["memory"]
    # Probabilities should also be equal (same denominator when no loss).
    for bs in data["raw_probabilities"]:
        assert abs(data["raw_probabilities"][bs] - data["probabilities"][bs]) < 1e-9


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_loss_shots_excluded_from_accepted_fields(backend) -> None:
    """With high loss noise, raw_memory contains loss markers but memory does not."""
    circuit = create_deterministic_circuit()
    noise = NoiseConfig()
    # High loss probability on the native gate so some shots produce loss markers.
    noise.rz.loss = 0.2
    result = backend.run(circuit, shots=100, noise=noise, seed=42).result()
    data = result.data(0)

    # raw_memory includes all shots; memory filters out loss.
    assert len(data["raw_memory"]) == 100
    assert all("-" not in bs for bs in data["memory"])

    # raw_counts must contain an entry for at least one loss-bearing bitstring.
    loss_keys = [bs for bs in data["raw_counts"] if "-" in bs]
    assert len(loss_keys) > 0, "Expected loss markers in raw_counts with loss=0.2"

    # counts must not contain any loss-bearing bitstrings.
    assert all("-" not in bs for bs in data["counts"])

    # raw_memory and raw_counts must be consistent.
    from collections import Counter

    assert Counter(data["raw_memory"]) == Counter(data["raw_counts"])


# ---------------------------------------------------------------------------
# User-initiated transpilation (skip_transpilation=True)
# ---------------------------------------------------------------------------

NATIVE_GATES = {"rz", "sx", "cz", "measure", "reset"}


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_native_target_gate_set(backend) -> None:
    """backend.target must advertise exactly the NeutralAtomDevice native gate set."""
    assert set(backend.target.operation_names) == NATIVE_GATES


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_transpile_produces_native_gates_only(backend) -> None:
    """After transpiling against the backend target, no non-native gates remain."""
    from qiskit import transpile

    circuit = create_bell_circuit()
    native_circuit = transpile(circuit, backend=backend)
    ops = set(native_circuit.count_ops().keys())
    non_native = ops - NATIVE_GATES - {"barrier"}
    assert non_native == set(), f"Non-native gates after transpilation: {non_native}"


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_skip_transpilation_gives_correct_results(backend) -> None:
    """Pre-transpiled circuit run with skip_transpilation=True must produce valid Bell outcomes."""
    from qiskit import transpile

    circuit = create_bell_circuit()
    native_circuit = transpile(circuit, backend=backend)
    counts = (
        backend.run(native_circuit, shots=200, seed=77, skip_transpilation=True)
        .result()
        .get_counts()
    )
    for bitstring in counts:
        assert bitstring in ("00", "11"), f"Unexpected outcome: {bitstring}"
    assert sum(counts.values()) == 200


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_pretranspiled_matches_backend_transpiled(backend) -> None:
    """Pre-transpiled + skip_transpilation=True must give the same counts as a normal run with the same seed."""
    from qiskit import transpile

    circuit = create_bell_circuit()
    native_circuit = transpile(circuit, backend=backend)

    counts_normal = backend.run(circuit, shots=300, seed=55).result().get_counts()
    counts_pretranspiled = (
        backend.run(native_circuit, shots=300, seed=55, skip_transpilation=True)
        .result()
        .get_counts()
    )
    assert counts_normal == counts_pretranspiled


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_non_base_target_profile_raises(backend) -> None:
    """Passing a non-Base target_profile must raise ValueError immediately."""
    from qdk import TargetProfile

    circuit = create_bell_circuit()
    with pytest.raises(ValueError, match="TargetProfile.Base"):
        backend.run(circuit, shots=10, target_profile=TargetProfile.Adaptive_RI).result()
