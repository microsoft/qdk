# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest

from interop_cirq import CIRQ_AVAILABLE, SKIP_REASON

if CIRQ_AVAILABLE:
    import cirq
    import numpy as np
    from qsharp.interop.cirq import NeutralAtomCirqResult, simulate_with_neutral_atom
    from qsharp._simulation import NoiseConfig
    from qsharp._device._atom import NeutralAtomDevice


# ---------------------------------------------------------------------------
# Module-scoped fixture — one NeutralAtomDevice shared across all tests.
# ---------------------------------------------------------------------------


@pytest.fixture(scope="module")
def device():
    if not CIRQ_AVAILABLE:
        pytest.skip(SKIP_REASON)
    return NeutralAtomDevice()


# ---------------------------------------------------------------------------
# Circuit helpers
# ---------------------------------------------------------------------------


def create_bell_circuit() -> "cirq.Circuit":
    """Two-qubit Bell state — should produce only |00⟩ or |11⟩."""
    q0, q1 = cirq.LineQubit.range(2)
    return cirq.Circuit(
        [
            cirq.H(q0),
            cirq.CNOT(q0, q1),
            cirq.measure(q0, q1, key="m"),
        ]
    )


def create_deterministic_circuit() -> "cirq.Circuit":
    """Circuit whose output is always '11' regardless of noise (before native decomp)."""
    q0, q1 = cirq.LineQubit.range(2)
    return cirq.Circuit(
        [
            cirq.X(q0),
            cirq.X(q1),
            cirq.measure(q0, q1, key="m"),
        ]
    )


def create_multi_key_circuit() -> "cirq.Circuit":
    """Circuit with two separate measurement keys."""
    q0, q1 = cirq.LineQubit.range(2)
    return cirq.Circuit(
        [
            cirq.X(q0),
            cirq.X(q1),
            cirq.measure(q0, key="a"),
            cirq.measure(q1, key="b"),
        ]
    )


# ---------------------------------------------------------------------------
# Smoke tests
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_run_smoke(device) -> None:
    circuit = create_bell_circuit()
    result = simulate_with_neutral_atom(circuit, shots=10, device=device)
    assert result is not None


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_returns_neutral_atom_cirq_result(device) -> None:
    circuit = create_bell_circuit()
    result = simulate_with_neutral_atom(circuit, shots=10, device=device)
    assert isinstance(result, NeutralAtomCirqResult)


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_returns_cirq_result_dict(device) -> None:
    """NeutralAtomCirqResult must be a cirq.ResultDict for full Cirq compatibility."""
    circuit = create_bell_circuit()
    result = simulate_with_neutral_atom(circuit, shots=10, device=device)
    assert isinstance(result, cirq.ResultDict)


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_run_deterministic_circuit(device) -> None:
    circuit = create_deterministic_circuit()
    result = simulate_with_neutral_atom(circuit, shots=10, device=device)
    measurements = result.measurements["m"]
    # Every shot must be [1, 1].
    assert measurements.shape == (10, 2)
    assert np.all(measurements == 1)


# ---------------------------------------------------------------------------
# Seed / reproducibility
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_seed_produces_reproducible_results(device) -> None:
    circuit = create_bell_circuit()
    r1 = simulate_with_neutral_atom(circuit, shots=200, seed=42, device=device)
    r2 = simulate_with_neutral_atom(circuit, shots=200, seed=42, device=device)
    assert np.array_equal(r1.measurements["m"], r2.measurements["m"])


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_different_seeds_produce_different_results(device) -> None:
    circuit = create_bell_circuit()
    r1 = simulate_with_neutral_atom(circuit, shots=500, seed=1, device=device)
    r2 = simulate_with_neutral_atom(circuit, shots=500, seed=2, device=device)
    assert not np.array_equal(r1.measurements["m"], r2.measurements["m"])


# ---------------------------------------------------------------------------
# Bell-state outcome distribution
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_bell_state_outcomes_are_correlated(device) -> None:
    """Bell circuit must produce only |00⟩ or |11⟩."""
    circuit = create_bell_circuit()
    result = simulate_with_neutral_atom(circuit, shots=200, seed=99, device=device)
    measurements = result.measurements["m"]
    for row in measurements:
        bits = tuple(int(b) for b in row)
        assert bits in ((0, 0), (1, 1)), f"Unexpected outcome: {bits}"


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_histogram_counts_sum_to_shots(device) -> None:
    """result.histogram() must account for all accepted shots."""
    circuit = create_bell_circuit()
    shots = 200
    result = simulate_with_neutral_atom(circuit, shots=shots, seed=7, device=device)
    hist = result.histogram(key="m")
    assert sum(hist.values()) == shots


# ---------------------------------------------------------------------------
# Noise model
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_noiseless_noiseconfig_is_identity(device) -> None:
    """An empty NoiseConfig must give the same result as no noise."""
    circuit = create_deterministic_circuit()
    noise = NoiseConfig()
    result = simulate_with_neutral_atom(circuit, shots=10, noise=noise, device=device)
    assert np.all(result.measurements["m"] == 1)


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_bitflip_noise_introduces_errors(device) -> None:
    """Heavy SX bit-flip must flip some outcomes in the deterministic circuit.

    When a Cirq ``X`` gate is compiled via QASM 2.0 → QIR →
    ``NeutralAtomDevice.compile()``, the device decomposer represents X as
    SX·SX (two SX gates) rather than Rz + SX.  Noise must therefore be
    configured on ``noise.sx`` (not ``noise.rz``) to affect X-gate circuits
    on the Cirq path.
    """
    circuit = create_deterministic_circuit()
    noise = NoiseConfig()
    noise.sx.set_bitflip(0.5)  # 50% bit-flip on every SX gate
    result = simulate_with_neutral_atom(
        circuit, shots=200, noise=noise, seed=42, device=device
    )
    # Without noise every shot would be [1,1]. With heavy noise some must differ.
    all_ones = np.all(result.measurements["m"] == 1, axis=1)
    assert not np.all(
        all_ones
    ), "Expected some flipped shots with 50% SX bit-flip noise, but all were [1,1]"


# ---------------------------------------------------------------------------
# Multiple measurement keys
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_multi_key_circuit_has_all_keys(device) -> None:
    """result.measurements must contain an entry for each measurement key."""
    circuit = create_multi_key_circuit()
    result = simulate_with_neutral_atom(circuit, shots=20, device=device)
    assert "a" in result.measurements
    assert "b" in result.measurements


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_multi_key_circuit_correct_values(device) -> None:
    circuit = create_multi_key_circuit()
    result = simulate_with_neutral_atom(circuit, shots=20, device=device)
    assert np.all(result.measurements["a"] == 1)
    assert np.all(result.measurements["b"] == 1)


# ---------------------------------------------------------------------------
# raw_shots and raw_measurements (loss separation)
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_raw_shots_present(device) -> None:
    """result.raw_shots must be populated regardless of whether loss occurred."""
    circuit = create_deterministic_circuit()
    result = simulate_with_neutral_atom(circuit, shots=10, device=device)
    assert hasattr(result, "raw_shots")
    assert len(result.raw_shots) == 10


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_raw_shots_equal_measurements_when_no_loss(device) -> None:
    """Without loss noise every raw shot must be a valid accepted shot."""
    circuit = create_deterministic_circuit()
    shots = 20
    result = simulate_with_neutral_atom(circuit, shots=shots, seed=0, device=device)
    assert len(result.raw_shots) == shots
    # All accepted — accepted row count equals total shots.
    assert result.measurements["m"].shape[0] == shots


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_raw_measurements_returns_dict(device) -> None:
    circuit = create_deterministic_circuit()
    result = simulate_with_neutral_atom(circuit, shots=10, device=device)
    raw = result.raw_measurements()
    assert isinstance(raw, dict)
    assert "m" in raw


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_loss_shots_excluded_from_measurements(device) -> None:
    """With high loss noise, some raw shots must carry loss markers and be excluded."""
    circuit = create_deterministic_circuit()
    noise = NoiseConfig()
    # Use mresetz loss: every shot goes through an mresetz gate (measurement
    # + reset) so this reliably triggers loss regardless of which gates X
    # decomposes into on the Cirq path.
    noise.mresetz.loss = 0.5
    result = simulate_with_neutral_atom(
        circuit, shots=100, noise=noise, seed=42, device=device
    )

    # raw_shots includes all 100 shots.
    assert len(result.raw_shots) == 100

    # raw_measurements retains loss markers (dtype "<U1").
    raw = result.raw_measurements()
    raw_m = raw["m"]
    assert raw_m.dtype == np.dtype("<U1")

    # At least some raw shots must contain a non-binary character (loss marker).
    has_loss = any(not all(ch in ("0", "1") for ch in row) for row in raw_m.tolist())
    assert has_loss, "Expected loss markers in raw_measurements with loss=0.2"

    # accepted measurements must contain only 0/1 values.
    accepted = result.measurements["m"]
    assert accepted.dtype == np.int8
    assert np.all((accepted == 0) | (accepted == 1))

    # Accepted shot count must be fewer than total shots.
    assert accepted.shape[0] < 100


# ---------------------------------------------------------------------------
# Device injection
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_default_device_created_when_none() -> None:
    """Passing device=None should trigger lazy device creation without error."""
    circuit = create_deterministic_circuit()
    result = simulate_with_neutral_atom(circuit, shots=5, seed=1, device=None)
    assert result is not None


# ---------------------------------------------------------------------------
# Simulator type selection
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_cpu_simulator_type(device) -> None:
    circuit = create_bell_circuit()
    result = simulate_with_neutral_atom(
        circuit, shots=100, simulator_type="cpu", seed=7, device=device
    )
    for row in result.measurements["m"]:
        bits = tuple(int(b) for b in row)
        assert bits in ((0, 0), (1, 1))


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_clifford_simulator_type(device) -> None:
    circuit = create_bell_circuit()
    result = simulate_with_neutral_atom(
        circuit, shots=100, simulator_type="clifford", seed=7, device=device
    )
    for row in result.measurements["m"]:
        bits = tuple(int(b) for b in row)
        assert bits in ((0, 0), (1, 1))


# ---------------------------------------------------------------------------
# Error cases
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_unsupported_gate_raises_value_error(device) -> None:
    """A circuit containing a gate that cannot be serialized to QASM must raise ValueError."""

    class _NoQasmGate(cirq.Gate):
        """A custom gate with no QASM serialization."""

        def _num_qubits_(self) -> int:
            return 1

    q0 = cirq.LineQubit(0)
    circuit = cirq.Circuit(
        [
            _NoQasmGate()(q0),
            cirq.measure(q0, key="m"),
        ]
    )
    with pytest.raises(ValueError, match="QASM"):
        simulate_with_neutral_atom(circuit, shots=5, device=device)
