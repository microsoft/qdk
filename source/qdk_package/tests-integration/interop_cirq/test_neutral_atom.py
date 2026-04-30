# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest

cirq = pytest.importorskip("cirq")

import numpy as np
from qdk.cirq import NeutralAtomCirqResult, NeutralAtomSampler
from qdk._simulation import NoiseConfig
from qdk._device._atom import NeutralAtomDevice


# ---------------------------------------------------------------------------
# Module-scoped fixtures — one device and default sampler shared across tests.
# ---------------------------------------------------------------------------


@pytest.fixture(scope="module")
def device():
    return NeutralAtomDevice()


@pytest.fixture(scope="module")
def sampler(device):
    """A NeutralAtomSampler backed by the shared device (noiseless, unseeded)."""
    return NeutralAtomSampler(device=device)


# ---------------------------------------------------------------------------
# Circuit helpers
# ---------------------------------------------------------------------------


def create_bell_circuit():
    """Two-qubit Bell state — should produce only |00⟩ or |11⟩."""
    q0, q1 = cirq.LineQubit.range(2)
    return cirq.Circuit(
        [
            cirq.H(q0),
            cirq.CNOT(q0, q1),
            cirq.measure(q0, q1, key="m"),
        ]
    )


def create_deterministic_circuit():
    """Circuit whose output is always '11' regardless of noise (before native decomp)."""
    q0, q1 = cirq.LineQubit.range(2)
    return cirq.Circuit(
        [
            cirq.X(q0),
            cirq.X(q1),
            cirq.measure(q0, q1, key="m"),
        ]
    )


def create_multi_key_circuit():
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


def test_run_smoke(sampler) -> None:
    circuit = create_bell_circuit()
    result = sampler.run(circuit, repetitions=10)
    assert result is not None


def test_returns_neutral_atom_cirq_result(sampler) -> None:
    circuit = create_bell_circuit()
    result = sampler.run(circuit, repetitions=10)
    assert isinstance(result, NeutralAtomCirqResult)


def test_returns_cirq_result_dict(sampler) -> None:
    """NeutralAtomCirqResult must be a cirq.ResultDict for full Cirq compatibility."""
    circuit = create_bell_circuit()
    result = sampler.run(circuit, repetitions=10)
    assert isinstance(result, cirq.ResultDict)


def test_run_deterministic_circuit(sampler) -> None:
    circuit = create_deterministic_circuit()
    result = sampler.run(circuit, repetitions=10)
    measurements = result.measurements["m"]
    # Every shot must be [1, 1].
    assert measurements.shape == (10, 2)
    assert np.all(measurements == 1)


# ---------------------------------------------------------------------------
# Seed / reproducibility
# ---------------------------------------------------------------------------


def test_seed_produces_reproducible_results(device) -> None:
    circuit = create_bell_circuit()
    r1 = NeutralAtomSampler(seed=42, device=device).run(circuit, repetitions=200)
    r2 = NeutralAtomSampler(seed=42, device=device).run(circuit, repetitions=200)
    assert np.array_equal(r1.measurements["m"], r2.measurements["m"])


def test_different_seeds_produce_different_results(device) -> None:
    circuit = create_bell_circuit()
    r1 = NeutralAtomSampler(seed=1, device=device).run(circuit, repetitions=500)
    r2 = NeutralAtomSampler(seed=2, device=device).run(circuit, repetitions=500)
    assert not np.array_equal(r1.measurements["m"], r2.measurements["m"])


# ---------------------------------------------------------------------------
# Bell-state outcome distribution
# ---------------------------------------------------------------------------


def test_bell_state_outcomes_are_correlated(device) -> None:
    """Bell circuit must produce only |00⟩ or |11⟩."""
    circuit = create_bell_circuit()
    result = NeutralAtomSampler(seed=99, device=device).run(circuit, repetitions=200)
    measurements = result.measurements["m"]
    for row in measurements:
        bits = tuple(int(b) for b in row)
        assert bits in ((0, 0), (1, 1)), f"Unexpected outcome: {bits}"


def test_histogram_counts_sum_to_shots(device) -> None:
    """result.histogram() must account for all accepted shots."""
    circuit = create_bell_circuit()
    repetitions = 200
    result = NeutralAtomSampler(seed=7, device=device).run(
        circuit, repetitions=repetitions
    )
    hist = result.histogram(key="m")
    assert sum(hist.values()) == repetitions


# ---------------------------------------------------------------------------
# Noise model
# ---------------------------------------------------------------------------


def test_noiseless_noiseconfig_is_identity(device) -> None:
    """An empty NoiseConfig must give the same result as no noise."""
    circuit = create_deterministic_circuit()
    noise = NoiseConfig()
    result = NeutralAtomSampler(noise=noise, device=device).run(circuit, repetitions=10)
    assert np.all(result.measurements["m"] == 1)


def test_bitflip_noise_introduces_errors(device) -> None:
    """Heavy SX bit-flip must flip some outcomes in the deterministic circuit.

    When a Cirq ``X`` gate is compiled via QASM 3.0 → QIR →
    ``NeutralAtomDevice.compile()``, the device decomposer represents X as
    SX·SX (two SX gates) rather than Rz + SX.  Noise must therefore be
    configured on ``noise.sx`` (not ``noise.rz``) to affect X-gate circuits
    on the Cirq path.
    """
    circuit = create_deterministic_circuit()
    noise = NoiseConfig()
    noise.sx.set_bitflip(0.5)  # 50% bit-flip on every SX gate
    result = NeutralAtomSampler(noise=noise, seed=42, device=device).run(
        circuit, repetitions=200
    )
    # Without noise every shot would be [1,1]. With heavy noise some must differ.
    all_ones = np.all(result.measurements["m"] == 1, axis=1)
    assert not np.all(
        all_ones
    ), "Expected some flipped shots with 50% SX bit-flip noise, but all were [1,1]"


# ---------------------------------------------------------------------------
# Multiple measurement keys
# ---------------------------------------------------------------------------


def test_multi_key_circuit_has_all_keys(sampler) -> None:
    """result.measurements must contain an entry for each measurement key."""
    circuit = create_multi_key_circuit()
    result = sampler.run(circuit, repetitions=20)
    assert "a" in result.measurements
    assert "b" in result.measurements


def test_multi_key_circuit_correct_values(sampler) -> None:
    circuit = create_multi_key_circuit()
    result = sampler.run(circuit, repetitions=20)
    assert np.all(result.measurements["a"] == 1)
    assert np.all(result.measurements["b"] == 1)


# ---------------------------------------------------------------------------
# raw_shots and raw_measurements (loss separation)
# ---------------------------------------------------------------------------


def test_raw_shots_present(sampler) -> None:
    """result.raw_shots must be populated regardless of whether loss occurred."""
    circuit = create_deterministic_circuit()
    result = sampler.run(circuit, repetitions=10)
    assert hasattr(result, "raw_shots")
    assert len(result.raw_shots) == 10


def test_raw_shots_equal_measurements_when_no_loss(device) -> None:
    """Without loss noise every raw shot must be a valid accepted shot."""
    circuit = create_deterministic_circuit()
    repetitions = 20
    result = NeutralAtomSampler(seed=0, device=device).run(
        circuit, repetitions=repetitions
    )
    assert len(result.raw_shots) == repetitions
    # All accepted — accepted row count equals total shots.
    assert result.measurements["m"].shape[0] == repetitions


def test_raw_measurements_returns_dict(sampler) -> None:
    circuit = create_deterministic_circuit()
    result = sampler.run(circuit, repetitions=10)
    raw = result.raw_measurements()
    assert isinstance(raw, dict)
    assert "m" in raw


def test_loss_shots_excluded_from_measurements(device) -> None:
    """With high loss noise, some raw shots must carry loss markers and be excluded."""
    circuit = create_bell_circuit()
    noise = NoiseConfig()
    # Use rz loss on the Bell circuit: the H gate always decomposes to Rz in
    # the {Rz, SX, CZ} native gate set, so every shot unconditionally passes
    # through at least one Rz gate.
    noise.rz.loss = 0.5
    result = NeutralAtomSampler(noise=noise, seed=42, device=device).run(
        circuit, repetitions=100
    )

    # raw_shots includes all 100 shots.
    assert len(result.raw_shots) == 100

    # raw_measurements retains loss markers (dtype "<U1").
    raw = result.raw_measurements()
    raw_m = raw["m"]
    assert raw_m.dtype == np.dtype("<U1")

    # At least some raw shots must contain a non-binary character (loss marker).
    has_loss = any(not all(ch in ("0", "1") for ch in row) for row in raw_m.tolist())
    assert has_loss, "Expected loss markers in raw_measurements with loss=0.5"

    # Accepted measurements must contain only 0/1 values.
    accepted = result.measurements["m"]
    assert accepted.dtype == np.int8
    assert np.all((accepted == 0) | (accepted == 1))

    # Accepted shot count must be fewer than total shots.
    assert accepted.shape[0] < 100


# ---------------------------------------------------------------------------
# Device injection
# ---------------------------------------------------------------------------


def test_default_device_created_when_none() -> None:
    """Passing no device should trigger lazy device creation without error."""
    circuit = create_deterministic_circuit()
    result = NeutralAtomSampler(seed=1).run(circuit, repetitions=5)
    assert result is not None


# ---------------------------------------------------------------------------
# Simulator type selection
# ---------------------------------------------------------------------------


def test_cpu_simulator_type(device) -> None:
    circuit = create_bell_circuit()
    result = NeutralAtomSampler(simulator_type="cpu", seed=7, device=device).run(
        circuit, repetitions=100
    )
    for row in result.measurements["m"]:
        bits = tuple(int(b) for b in row)
        assert bits in ((0, 0), (1, 1))


def test_clifford_simulator_type(device) -> None:
    circuit = create_bell_circuit()
    result = NeutralAtomSampler(simulator_type="clifford", seed=7, device=device).run(
        circuit, repetitions=100
    )
    for row in result.measurements["m"]:
        bits = tuple(int(b) for b in row)
        assert bits in ((0, 0), (1, 1))


# ---------------------------------------------------------------------------
# Error cases
# ---------------------------------------------------------------------------


def test_unsupported_gate_raises_value_error(sampler) -> None:
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
    with pytest.raises(ValueError, match="QASM 3.0"):
        sampler.run(circuit, repetitions=5)
