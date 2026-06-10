# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from collections import Counter
import os
import pytest
from qdk import qsharp
from qdk._interpreter import compile
from qdk import Result, TargetProfile
from qdk.simulation import run_qir as _run_qir, NoiseConfig, LossPolicy
from qdk.simulation._simulation import try_create_gpu_adapter
from typing import Literal, List, Optional, TypeAlias


@pytest.fixture(autouse=True, scope="module")
def _init_base_profile():
    """
    Initialize the Q# interpreter once per module.

    We need a pytest.fixture instead of just a global statement
    because global statements are evaluated at test-collection time,
    which means this file would inherit the interpreter state of
    another file.
    """
    qsharp.init(target_profile=TargetProfile.Base)


SEED = 42

# ---------------------------------------------------------------------------
# Simulator-type parametrization
# ---------------------------------------------------------------------------


def gpu_param():
    skip_reason = ""
    try:
        try_create_gpu_adapter()
        if not os.environ.get("QDK_GPU_TESTS"):
            skip_reason = "Env variable QDK_GPU_TESTS is not set"
    except Exception:
        skip_reason = "No GPU available"

    return pytest.param(
        "gpu",
        marks=pytest.mark.skipif(bool(skip_reason), reason=skip_reason),
    )


SIM_TYPES = ["cpu", "clifford", gpu_param()]
NON_CLIFFORD_SIM_TYPES = ["cpu", gpu_param()]


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


SimType: TypeAlias = Literal["clifford", "cpu", "gpu"]


def result_to_str(r: Result) -> str:
    match r:
        case Result.Zero:
            return "0"
        case Result.One:
            return "1"
        case Result.Loss:
            return "-"
        case _:
            raise ValueError(f"Invalid Result: {r}")


def result_list_to_str(result_list):
    if isinstance(result_list, (list, tuple)):
        return "".join(result_to_str(r) for r in result_list)
    return result_to_str(result_list)


def run_qir(
    input,
    shots: int,
    noise: Optional[NoiseConfig],
    seed: Optional[int],
    type: SimType,
) -> List:
    results = _run_qir(input, shots, noise, seed, type)
    return [result_list_to_str(r) for r in results]


def compile_and_run(source, shots=1, noise=None, seed=None, sim_type: SimType = "cpu"):
    """Compile a Q# expression and run it through run_qir."""
    qir = compile(source)
    return run_qir(qir, shots=shots, noise=noise, seed=seed, type=sim_type)


def compile_and_run_with_declarations(
    declarations, entry_expr, shots=1, noise=None, seed=None, sim_type: SimType = "cpu"
):
    """Register top-level Q# declarations, then compile and run an entry expression."""
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(declarations)
    qir = compile(entry_expr)
    return run_qir(qir, shots=shots, noise=noise, seed=seed, type=sim_type)


def histogram(results):
    """Build a {str_key: count} histogram from a list of shot results."""
    return Counter(results)


def check_histogram(results, expected_probs, tolerance=0.05):
    """
    Assert that the probability distribution of *results* matches
    *expected_probs* (a dict mapping str keys to float probabilities)
    within *tolerance*.
    """
    n = len(results)
    assert n > 0, "No results to check"
    hist = histogram(results)
    all_keys = set(expected_probs.keys()) | set(hist.keys())
    for key in all_keys:
        actual_prob = hist.get(key, 0) / n
        expected_prob = expected_probs.get(key, 0.0)
        assert abs(actual_prob - expected_prob) <= tolerance, (
            f"Key '{key}': expected ~{expected_prob:.2f}, got {actual_prob:.3f} "
            f"({hist.get(key, 0)}/{n})"
        )


# ===========================================================================
# Generic noisy simulator tests
# ===========================================================================


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_simulator_completes_all_shots(sim_type):
    results = compile_and_run(
        "{use q = Qubit(); X(q); MResetZ(q)}",
        shots=50,
        sim_type=sim_type,
    )
    assert len(results) == 50


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_noiseless_config_produces_clean_results(sim_type):
    noise = NoiseConfig()
    results = compile_and_run(
        "{use q = Qubit(); X(q); MResetZ(q)}",
        shots=100,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"1": 1.0})


# ===========================================================================
# X Noise (bit-flip) tests
# ===========================================================================


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_x_noise_on_x_gate_causes_bit_flips(sim_type):
    noise = NoiseConfig()
    noise.x.set_pauli_noise("X", 0.1)
    results = compile_and_run(
        "{use q = Qubit(); X(q); MResetZ(q)}",
        shots=1000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"0": 0.1, "1": 0.9})


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_x_noise_on_h_gate_does_not_affect_outcome(sim_type):
    noise = NoiseConfig()
    noise.h.set_pauli_noise("X", 0.3)
    results = compile_and_run(
        "{use q = Qubit(); H(q); MResetZ(q)}",
        shots=1000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"0": 0.5, "1": 0.5})


# ===========================================================================
# Z Noise (phase-flip) tests
# ===========================================================================


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_z_noise_does_not_affect_computational_basis(sim_type):
    noise = NoiseConfig()
    noise.x.set_pauli_noise("Z", 0.5)
    results = compile_and_run(
        "{use q = Qubit(); X(q); MResetZ(q)}",
        shots=100,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"1": 1.0})


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_z_noise_on_superposition_affects_interference(sim_type):
    noise = NoiseConfig()
    noise.h.set_pauli_noise("Z", 0.2)
    results = compile_and_run(
        "{use q = Qubit(); H(q); H(q); MResetZ(q)}",
        shots=1000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"0": 0.8, "1": 0.2})


# ===========================================================================
# Loss noise tests
# ===========================================================================


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_loss_noise_produces_loss_marker(sim_type):
    noise = NoiseConfig()
    noise.x.loss = 0.1
    results = compile_and_run(
        "{use q = Qubit(); X(q); MResetZ(q)}",
        shots=1000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"-": 0.1, "1": 0.9})


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_two_qubit_loss(sim_type):
    noise = NoiseConfig()
    noise.cz.loss = 0.1
    results = compile_and_run(
        "{use qs = Qubit[2]; CZ(qs[0], qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=100_000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(
        results, {"--": 0.01, "-0": 0.09, "0-": 0.09, "00": 0.81}, tolerance=0.02
    )


# ===========================================================================
# Loss-policy (on_loss) tests
# ===========================================================================
#
# These exercise the per-gate `NoiseConfig.<gate>.on_loss` behavior. The
# `on_loss` policy is honored by the cpu (full-state) and clifford (stabilizer)
# simulators, so these tests are parametrized over just those two.
#
# A qubit is lost deterministically by giving a single-qubit gate a loss
# probability of 1.0 and then applying that gate. The gate under test then sees
# a lost operand and applies its configured policy. All outcomes are
# deterministic, so a single shot is sufficient.


LOSS_POLICY_SIM_TYPES = ["cpu", "clifford", gpu_param()]


@pytest.mark.parametrize("sim_type", LOSS_POLICY_SIM_TYPES)
def test_on_loss_default_controlled_gate_skips(sim_type):
    # `cz.on_loss` defaults to SKIP: the lost control means CZ is skipped, so
    # the surviving target qubit is left untouched in |0>.
    noise = NoiseConfig()
    noise.x.loss = 1.0  # deterministically lose qs[0] after X
    results = compile_and_run(
        "{use qs = Qubit[2]; X(qs[0]); CZ(qs[0], qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=1,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"-0": 1.0})


@pytest.mark.parametrize("sim_type", LOSS_POLICY_SIM_TYPES)
def test_on_loss_propagate_marks_other_operand_lost(sim_type):
    # PROPAGATE: a lost operand propagates the loss to the other operand, so
    # both qubits measure as Loss.
    noise = NoiseConfig()
    noise.x.loss = 1.0
    noise.cz.on_loss = LossPolicy.PROPAGATE
    results = compile_and_run(
        "{use qs = Qubit[2]; X(qs[0]); CZ(qs[0], qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=1,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"--": 1.0})


@pytest.mark.parametrize("sim_type", LOSS_POLICY_SIM_TYPES)
def test_on_loss_rxx_degrade_reduces_to_single_qubit(sim_type):
    # `rxx.on_loss` defaults to DEGRADE: with one operand lost, Rxx reduces to
    # Rx on the survivor. Rx(PI) flips qs[1] to |1>.
    noise = NoiseConfig()
    noise.x.loss = 1.0
    results = compile_and_run(
        "{use qs = Qubit[2]; X(qs[0]); Rxx(Std.Math.PI(), qs[0], qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=1,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"-1": 1.0})


@pytest.mark.parametrize("sim_type", LOSS_POLICY_SIM_TYPES)
def test_on_loss_rxx_skip_leaves_survivor_untouched(sim_type):
    # Overriding `rxx.on_loss` to SKIP leaves the surviving qubit in |0>.
    noise = NoiseConfig()
    noise.x.loss = 1.0
    noise.rxx.on_loss = LossPolicy.SKIP
    results = compile_and_run(
        "{use qs = Qubit[2]; X(qs[0]); Rxx(Std.Math.PI(), qs[0], qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=1,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"-0": 1.0})


@pytest.mark.parametrize("sim_type", LOSS_POLICY_SIM_TYPES)
def test_on_loss_residual_s_dagger_applies_s_adjoint(sim_type):
    # RESIDUAL_S_DAGGER: the gate is skipped but an S-dagger is applied to each
    # surviving operand. qs[1] is prepared in |+i> = S H |0>; the residual
    # S-dagger maps it back to |+>, and a final H rotates it to |0>.
    noise = NoiseConfig()
    noise.x.loss = 1.0
    noise.cx.on_loss = LossPolicy.RESIDUAL_S_DAGGER
    results = compile_and_run(
        "{use qs = Qubit[2]; H(qs[1]); S(qs[1]); X(qs[0]); CNOT(qs[0], qs[1]); H(qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=1,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"-0": 1.0})


@pytest.mark.parametrize("sim_type", LOSS_POLICY_SIM_TYPES)
def test_on_loss_swap_apply_anyway_exchanges_state(sim_type):
    # `swap.on_loss` defaults to APPLY_ANYWAY: the SWAP unitary still runs, so
    # qs[1]'s |1> moves into qs[0]. The loss flag is always exchanged, so qs[1]
    # becomes the lost qubit. qs[0] is lost via Y so X-prepared qs[1] is intact.
    noise = NoiseConfig()
    noise.y.loss = 1.0
    results = compile_and_run(
        "{use qs = Qubit[2]; X(qs[1]); Y(qs[0]); SWAP(qs[0], qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=1,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"1-": 1.0})


@pytest.mark.parametrize("sim_type", LOSS_POLICY_SIM_TYPES)
def test_on_loss_swap_skip_keeps_state_but_swaps_loss_flag(sim_type):
    # Overriding `swap.on_loss` to SKIP skips the SWAP unitary, but the loss
    # flag is still exchanged. qs[0] keeps its reset |0> and qs[1] becomes lost.
    noise = NoiseConfig()
    noise.y.loss = 1.0
    noise.swap.on_loss = LossPolicy.SKIP
    results = compile_and_run(
        "{use qs = Qubit[2]; X(qs[1]); Y(qs[0]); SWAP(qs[0], qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=1,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"0-": 1.0})


# ===========================================================================
# Two-qubit gate noise tests
# ===========================================================================


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_cx_xi_noise_flips_control_qubit(sim_type):
    noise = NoiseConfig()
    noise.cx.set_pauli_noise("XI", 0.1)
    results = compile_and_run(
        "{use qs = Qubit[2]; X(qs[0]); CNOT(qs[0], qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=1000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"01": 0.1, "11": 0.9})


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_cx_ix_noise_flips_target_qubit(sim_type):
    noise = NoiseConfig()
    noise.cx.set_pauli_noise("IX", 0.1)
    results = compile_and_run(
        "{use qs = Qubit[2]; X(qs[0]); CNOT(qs[0], qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=1000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"10": 0.1, "11": 0.9})


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_cx_xx_noise_flips_both_qubits(sim_type):
    noise = NoiseConfig()
    noise.cx.set_pauli_noise("XX", 0.1)
    results = compile_and_run(
        "{use qs = Qubit[2]; X(qs[0]); CNOT(qs[0], qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=1000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"00": 0.1, "11": 0.9})


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_cz_noise_affects_outcome(sim_type):
    noise = NoiseConfig()
    noise.cz.set_pauli_noise("XI", 0.1)
    results = compile_and_run(
        "{use qs = Qubit[2]; CZ(qs[0], qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=1000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"00": 0.9, "10": 0.1})


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_swap_noise_affects_swapped_qubits(sim_type):
    noise = NoiseConfig()
    noise.swap.set_pauli_noise("IX", 0.1)
    results = compile_and_run(
        "{use qs = Qubit[2]; X(qs[0]); SWAP(qs[0], qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=1000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"00": 0.1, "01": 0.9})


# ===========================================================================
# Gate-specific noise tests
# ===========================================================================


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_different_gates_have_different_noise(sim_type):
    noise = NoiseConfig()
    noise.z.set_pauli_noise("X", 0.2)
    results = compile_and_run(
        "{use q = Qubit(); Z(q); X(q); MResetZ(q)}",
        shots=1000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"0": 0.2, "1": 0.8})


# ===========================================================================
# Multiple gates / accumulated noise tests
# ===========================================================================


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_noise_accumulates_across_multiple_gates(sim_type):
    noise = NoiseConfig()
    noise.x.set_pauli_noise("X", 0.1)
    results = compile_and_run(
        "{use q = Qubit(); X(q); X(q); MResetZ(q)}",
        shots=10_000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    # Two X gates each with 10% X noise: P(flip) = 2*0.1*0.9 = 0.18
    check_histogram(results, {"0": 0.82, "1": 0.18})


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_bell_state_with_combined_noise(sim_type):
    noise = NoiseConfig()
    noise.h.loss = 0.1
    noise.cx.set_pauli_noise("XI", 0.02)
    noise.cx.set_pauli_noise("IX", 0.02)
    results = compile_and_run(
        "{use qs = Qubit[2]; H(qs[0]); CNOT(qs[0], qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=200_000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(
        results,
        {"-0": 0.10, "-1": 0.00, "00": 0.43, "01": 0.02, "10": 0.02, "11": 0.43},
        tolerance=0.02,
    )


# ===========================================================================
# Rotation gate noise tests
# ===========================================================================


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_rx_gate_with_noise(sim_type):
    noise = NoiseConfig()
    noise.rx.set_pauli_noise("X", 0.1)
    results = compile_and_run(
        "{use q = Qubit(); Rx(Std.Math.PI(), q); MResetZ(q)}",
        shots=1000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"0": 0.1, "1": 0.9})


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_rz_gate_with_z_noise_no_effect_on_basis(sim_type):
    noise = NoiseConfig()
    noise.rz.set_pauli_noise("Z", 0.5)
    results = compile_and_run(
        "{use q = Qubit(); Rz(Std.Math.PI(), q); MResetZ(q)}",
        shots=100,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"0": 1.0})


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_rxx_gate_with_noise(sim_type):
    noise = NoiseConfig()
    noise.rxx.set_pauli_noise("XI", 0.1)
    results = compile_and_run(
        "{use qs = Qubit[2]; Rxx(Std.Math.PI(), qs[0], qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=1000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"01": 0.1, "11": 0.9})


# ===========================================================================
# Correlated noise intrinsic tests
# ===========================================================================

# Q# source fragments for declaring noise intrinsic operations.
# Each distinct intrinsic ID in the Rust tests maps to a separate Q# operation
# decorated with @NoiseIntrinsic().

NOISE_INTRINSIC_1Q_DECL = """
@NoiseIntrinsic()
operation noise_intrinsic_0(q : Qubit) : Unit {
    body intrinsic;
}
"""

NOISE_INTRINSIC_2Q_DECL = """
@NoiseIntrinsic()
operation noise_intrinsic_0(q0 : Qubit, q1 : Qubit) : Unit {
    body intrinsic;
}
"""

NOISE_INTRINSIC_3Q_DECL = """
@NoiseIntrinsic()
operation noise_intrinsic_0(q0 : Qubit, q1 : Qubit, q2 : Qubit) : Unit {
    body intrinsic;
}
"""

NOISE_INTRINSIC_MULTI_ID_DECL = """
@NoiseIntrinsic()
operation noise_intrinsic_0(q : Qubit) : Unit {
    body intrinsic;
}
@NoiseIntrinsic()
operation noise_intrinsic_1(q : Qubit) : Unit {
    body intrinsic;
}
"""


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_noise_intrinsic_single_qubit_x_noise(sim_type):
    noise = NoiseConfig()
    table = noise.intrinsic("noise_intrinsic_0", num_qubits=1)
    table.x = 0.1
    results = compile_and_run_with_declarations(
        NOISE_INTRINSIC_1Q_DECL,
        "{use q = Qubit(); noise_intrinsic_0(q); MResetZ(q)}",
        shots=1000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"0": 0.9, "1": 0.1})


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_noise_intrinsic_single_qubit_z_noise_no_effect(sim_type):
    noise = NoiseConfig()
    table = noise.intrinsic("noise_intrinsic_0", num_qubits=1)
    table.z = 0.5
    results = compile_and_run_with_declarations(
        NOISE_INTRINSIC_1Q_DECL,
        "{use q = Qubit(); noise_intrinsic_0(q); MResetZ(q)}",
        shots=100,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"0": 1.0})


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_noise_intrinsic_two_qubit_correlated_xx_noise(sim_type):
    noise = NoiseConfig()
    table = noise.intrinsic("noise_intrinsic_0", num_qubits=2)
    table.xx = 0.1
    results = compile_and_run_with_declarations(
        NOISE_INTRINSIC_2Q_DECL,
        "{use qs = Qubit[2]; noise_intrinsic_0(qs[0], qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=1000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"00": 0.9, "11": 0.1})


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_noise_intrinsic_two_qubit_independent_noise(sim_type):
    noise = NoiseConfig()
    table = noise.intrinsic("noise_intrinsic_0", num_qubits=2)
    table.xi = 0.1
    table.ix = 0.1
    results = compile_and_run_with_declarations(
        NOISE_INTRINSIC_2Q_DECL,
        "{use qs = Qubit[2]; noise_intrinsic_0(qs[0], qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=1000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"00": 0.8, "01": 0.1, "10": 0.1})


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_noise_intrinsic_multiple_ids(sim_type):
    noise = NoiseConfig()
    table0 = noise.intrinsic("noise_intrinsic_0", num_qubits=1)
    table0.x = 0.1
    table1 = noise.intrinsic("noise_intrinsic_1", num_qubits=1)
    table1.x = 0.5
    results = compile_and_run_with_declarations(
        NOISE_INTRINSIC_MULTI_ID_DECL,
        "{use qs = Qubit[2]; noise_intrinsic_0(qs[0]); noise_intrinsic_1(qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=10_000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"00": 0.45, "01": 0.45, "10": 0.05, "11": 0.05})


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_noise_intrinsic_three_qubit_correlated(sim_type):
    noise = NoiseConfig()
    table = noise.intrinsic("noise_intrinsic_0", num_qubits=3)
    table.xxx = 0.1
    results = compile_and_run_with_declarations(
        NOISE_INTRINSIC_3Q_DECL,
        "{use qs = Qubit[3]; noise_intrinsic_0(qs[0], qs[1], qs[2]); [MResetZ(qs[0]), MResetZ(qs[1]), MResetZ(qs[2])]}",
        shots=1000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"000": 0.9, "111": 0.1})


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_noise_intrinsic_combined_with_gate_noise(sim_type):
    noise = NoiseConfig()
    noise.x.set_pauli_noise("X", 0.1)
    table = noise.intrinsic("noise_intrinsic_0", num_qubits=1)
    table.x = 0.1
    results = compile_and_run_with_declarations(
        NOISE_INTRINSIC_1Q_DECL,
        "{use q = Qubit(); X(q); noise_intrinsic_0(q); MResetZ(q)}",
        shots=10_000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"0": 0.18, "1": 0.82})
