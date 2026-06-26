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
# These exercise the per-gate `NoiseConfig.<gate>.on_loss` behavior.
#
# A qubit is lost deterministically by giving a single-qubit gate a loss
# probability of 1.0 and then applying that gate. The gate under test then sees
# a lost operand and applies its configured policy. All outcomes are
# deterministic, so a single shot is sufficient.
#
# Need to test: CX, CY, CZ, RXX, RYY, RZZ, SWAP
#         with: SKIP, PROPAGATE, DEGRADE, RESIDUAL_S_DAGGER
#
# TEST 0: C*, R**, and SWAP default loss policies
# TEST 1: C*, R**, and SWAP with SKIP do not apply unitary
# TEST 2: C*, R**, and SWAP with PROPAGATE lose first qubit
# TEST 3: C*, R**, and SWAP with PROPAGATE lose second qubit
# TEST 4: C* and SWAP with DEGRADE behave like skip
# TEST 5: R** with DEGRADE behave like R*
# TEST 6: C*, R**, and SWAP with RESIDUAL_S_DAGGER do not apply unitary
# TEST 7: SWAP always exchanges loss flags and qubit states

# Two-qubit gate-call fragments, grouped by how they reduce when exactly one
# operand is lost. Each entry is (NoiseConfig attribute, Q# gate call).
CONTROLLED_GATES = [
    ("cx", "CNOT(qs[0], qs[1])"),
    ("cy", "CY(qs[0], qs[1])"),
    ("cz", "CZ(qs[0], qs[1])"),
]
ROTATION_GATES = [
    ("rxx", "Rxx(Std.Math.PI(), qs[0], qs[1])"),
    ("ryy", "Ryy(Std.Math.PI(), qs[0], qs[1])"),
    ("rzz", "Rzz(Std.Math.PI(), qs[0], qs[1])"),
]
SWAP_GATE = ("swap", "SWAP(qs[0], qs[1])")
ALL_GATES = CONTROLLED_GATES + ROTATION_GATES + [SWAP_GATE]

CONTROLLED_IDS = [attr for attr, _ in CONTROLLED_GATES]
ROTATION_IDS = [attr for attr, _ in ROTATION_GATES]
SWAP_ID = SWAP_GATE[0]
ALL_IDS = [attr for attr, _ in ALL_GATES]

# Rotation gates under DEGRADE reduce to their single-qubit version on the
# survivor. With theta = PI the degraded rotation flips the survivor's measured
# bit to 1, but Rz only adds phase, so Rzz is prepared/measured in the X basis.
ROTATION_DEGRADE_RECIPES = [
    ("rxx", "Rxx(Std.Math.PI(), qs[0], qs[1])", "", ""),
    ("ryy", "Ryy(Std.Math.PI(), qs[0], qs[1])", "", ""),
    ("rzz", "Rzz(Std.Math.PI(), qs[0], qs[1])", "H(qs[1]);", "H(qs[1]);"),
]


# Allowed `on_loss` policies for each multi-qubit gate, mirroring
# `allowed_noise_policies_from_gate_name` on the Rust side. Single-qubit gate
# tables reject `on_loss` entirely (see SINGLE_QUBIT_GATE_ATTRS).
ALL_LOSS_POLICIES = [
    LossPolicy.SKIP,
    LossPolicy.PROPAGATE,
    LossPolicy.DEGRADE,
    LossPolicy.RESIDUAL_S_DAGGER,
    LossPolicy.APPLY_ANYWAY,
]

# The policies every multi-qubit gate accepts.
DEFAULT_MULTI_QUBIT_POLICIES = [
    LossPolicy.SKIP,
    LossPolicy.PROPAGATE,
    LossPolicy.RESIDUAL_S_DAGGER,
]

# NoiseConfig gate attribute -> the policies that gate accepts. Rotation gates
# additionally allow DEGRADE (reduce to the single-qubit rotation) and SWAP
# additionally allows APPLY_ANYWAY (run the swap regardless of loss).
ALLOWED_ON_LOSS_POLICIES = {
    "cx": DEFAULT_MULTI_QUBIT_POLICIES,
    "cy": DEFAULT_MULTI_QUBIT_POLICIES,
    "cz": DEFAULT_MULTI_QUBIT_POLICIES,
    "ccx": DEFAULT_MULTI_QUBIT_POLICIES,
    "rxx": DEFAULT_MULTI_QUBIT_POLICIES + [LossPolicy.DEGRADE],
    "ryy": DEFAULT_MULTI_QUBIT_POLICIES + [LossPolicy.DEGRADE],
    "rzz": DEFAULT_MULTI_QUBIT_POLICIES + [LossPolicy.DEGRADE],
    "swap": DEFAULT_MULTI_QUBIT_POLICIES + [LossPolicy.APPLY_ANYWAY],
}

# Single-qubit gate tables: `on_loss` is meaningless and rejected for every
# policy.
SINGLE_QUBIT_GATE_ATTRS = [
    "i",
    "x",
    "y",
    "z",
    "h",
    "s",
    "s_adj",
    "t",
    "t_adj",
    "sx",
    "sx_adj",
    "rx",
    "ry",
    "rz",
    "mov",
    "mz",
    "mresetz",
]


def forbidden_on_loss_policies(attr):
    """The policies *not* accepted by the gate at NoiseConfig attribute *attr*."""
    allowed = ALLOWED_ON_LOSS_POLICIES[attr]
    return [p for p in ALL_LOSS_POLICIES if p not in allowed]


def run_loss_policy_scenario(
    gate: str,
    sim_type: SimType,
    *,
    attr: str = "",
    on_loss=None,
    prep: str = "",
    post: str = "",
    lose: int = 0,
) -> str:
    """
    Lose one operand of a two-qubit gate deterministically, apply the gate, and
    measure both qubits.

    The qubit at index *lose* is taken out via a Y gate configured with
    ``loss = 1.0``; the survivor can therefore be prepared with any non-Y gate
    through *prep* (and post-processed through *post*). Returns the single
    deterministic shot as a two-character string for
    ``[MResetZ(qs[0]), MResetZ(qs[1])]``.
    """
    noise = NoiseConfig()
    noise.y.loss = 1.0
    if on_loss is not None:
        setattr(getattr(noise, attr), "on_loss", on_loss)
    source = (
        f"{{use qs = Qubit[2]; {prep} Y(qs[{lose}]); {gate}; {post} "
        f"[MResetZ(qs[0]), MResetZ(qs[1])]}}"
    )
    return compile_and_run(source, shots=1, seed=SEED, noise=noise, sim_type=sim_type)[
        0
    ]


# TEST 0: C*, R**, and SWAP default loss policies
def test_on_loss_defaults():
    noise = NoiseConfig()
    assert noise.cx.on_loss == LossPolicy.SKIP
    assert noise.cy.on_loss == LossPolicy.SKIP
    assert noise.cz.on_loss == LossPolicy.SKIP
    assert noise.rxx.on_loss == LossPolicy.DEGRADE
    assert noise.ryy.on_loss == LossPolicy.DEGRADE
    assert noise.rzz.on_loss == LossPolicy.DEGRADE
    assert noise.swap.on_loss == LossPolicy.APPLY_ANYWAY


def test_on_loss_allowed_policies():
    # Every gate accepts each of its allowed policies, and the assigned value
    # round-trips through the getter on the (shared) noise table.
    for attr, allowed in ALLOWED_ON_LOSS_POLICIES.items():
        for policy in allowed:
            noise = NoiseConfig()
            setattr(getattr(noise, attr), "on_loss", policy)
            assert (
                getattr(noise, attr).on_loss == policy
            ), f"`{attr}` should accept on_loss={policy}"

    # The default policy reported by a fresh config must itself be allowed.
    noise = NoiseConfig()
    for attr, allowed in ALLOWED_ON_LOSS_POLICIES.items():
        assert getattr(noise, attr).on_loss in allowed

    # Multi-qubit noise intrinsics accept the default multi-qubit policies.
    for policy in DEFAULT_MULTI_QUBIT_POLICIES:
        noise = NoiseConfig()
        setattr(noise.intrinsic("loss_intrinsic", num_qubits=2), "on_loss", policy)
        assert noise.intrinsic("loss_intrinsic", num_qubits=2).on_loss == policy


def test_on_loss_forbidden_policies_raise_error():
    # Each multi-qubit gate rejects every policy outside its allowed set, and a
    # rejected assignment leaves the current policy unchanged.
    for attr in ALLOWED_ON_LOSS_POLICIES:
        for policy in forbidden_on_loss_policies(attr):
            noise = NoiseConfig()
            original = getattr(noise, attr).on_loss
            with pytest.raises(
                AttributeError, match="only supports the following policies"
            ):
                setattr(getattr(noise, attr), "on_loss", policy)
            assert getattr(noise, attr).on_loss == original

    # Single-qubit gate tables reject on_loss for *every* policy: loss policies
    # only apply to multi-qubit gates.
    for attr in SINGLE_QUBIT_GATE_ATTRS:
        for policy in ALL_LOSS_POLICIES:
            noise = NoiseConfig()
            with pytest.raises(AttributeError, match="only apply to multi-qubit gates"):
                setattr(getattr(noise, attr), "on_loss", policy)

    # A single-qubit noise intrinsic likewise rejects on_loss entirely.
    for policy in ALL_LOSS_POLICIES:
        noise = NoiseConfig()
        table = noise.intrinsic("loss_intrinsic_1q", num_qubits=1)
        with pytest.raises(AttributeError, match="only apply to multi-qubit gates"):
            setattr(table, "on_loss", policy)

    # A multi-qubit noise intrinsic only allows the default multi-qubit
    # policies, so DEGRADE and APPLY_ANYWAY are rejected.
    for policy in (LossPolicy.DEGRADE, LossPolicy.APPLY_ANYWAY):
        noise = NoiseConfig()
        table = noise.intrinsic("loss_intrinsic_2q", num_qubits=2)
        with pytest.raises(
            AttributeError, match="only supports the following policies"
        ):
            setattr(table, "on_loss", policy)


@pytest.mark.parametrize("sim_type", SIM_TYPES)
@pytest.mark.parametrize(
    "attr,gate,expected",
    [(*elt, "-1") for elt in ALL_GATES],
    ids=ALL_IDS,
)
def test_on_loss_skip_does_not_apply_unitary(attr, gate, expected, sim_type):
    res = run_loss_policy_scenario(
        gate, sim_type, attr=attr, on_loss=LossPolicy.SKIP, prep="X(qs[1]);"
    )
    assert res == expected


@pytest.mark.parametrize("sim_type", SIM_TYPES)
@pytest.mark.parametrize("attr,gate", ALL_GATES, ids=ALL_IDS)
def test_on_loss_propagate_lose_first(attr, gate, sim_type):
    res = run_loss_policy_scenario(
        gate, sim_type, attr=attr, on_loss=LossPolicy.PROPAGATE, lose=0
    )
    assert res == "--"


@pytest.mark.parametrize("sim_type", SIM_TYPES)
@pytest.mark.parametrize("attr,gate", ALL_GATES, ids=ALL_IDS)
def test_on_loss_propagate_lose_second(attr, gate, sim_type):
    res = run_loss_policy_scenario(
        gate, sim_type, attr=attr, on_loss=LossPolicy.PROPAGATE, lose=1
    )
    assert res == "--"


@pytest.mark.parametrize("sim_type", SIM_TYPES)
@pytest.mark.parametrize(
    "attr,gate,prep,post", ROTATION_DEGRADE_RECIPES, ids=ROTATION_IDS
)
def test_on_loss_degrade_reduces_rotation(attr, gate, prep, post, sim_type):
    res = run_loss_policy_scenario(
        gate, sim_type, attr=attr, on_loss=LossPolicy.DEGRADE, prep=prep, post=post
    )
    assert res == "-1"


@pytest.mark.parametrize("sim_type", SIM_TYPES)
@pytest.mark.parametrize(
    "attr,gate",
    CONTROLLED_GATES + ROTATION_GATES,
    ids=CONTROLLED_IDS + ROTATION_IDS,
)
def test_on_loss_residual_s_dagger_applies_s_adjoint(attr, gate, sim_type):
    res = run_loss_policy_scenario(
        gate,
        sim_type,
        attr=attr,
        on_loss=LossPolicy.RESIDUAL_S_DAGGER,
        prep="H(qs[1]); S(qs[1]);",
        post="H(qs[1]);",
    )
    assert res == "-0"


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_on_loss_swap_residual_s_dagger_applies_s_adjoint(sim_type):
    res = run_loss_policy_scenario(
        SWAP_GATE[1],
        sim_type,
        attr=SWAP_GATE[0],
        on_loss=LossPolicy.RESIDUAL_S_DAGGER,
        prep="H(qs[1]); S(qs[1]);",
        post="H(qs[0]);",
    )
    assert res == "0-"


@pytest.mark.parametrize("sim_type", SIM_TYPES)
@pytest.mark.parametrize(
    "on_loss,expected",
    [(LossPolicy.RESIDUAL_S_DAGGER, "1-"), (LossPolicy.APPLY_ANYWAY, "1-")],
    ids=["residual_s_dagger", "apply_anyway"],
)
def test_on_loss_swap_swaps_loss_flag(on_loss, expected, sim_type):
    res = run_loss_policy_scenario(
        SWAP_GATE[1], sim_type, attr="swap", on_loss=on_loss, prep="X(qs[1]);"
    )
    assert res == expected


# ===========================================================================
# Correlated loss tests ('L' in a noise string)
# ===========================================================================


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_correlated_loss_only_entry(sim_type):
    # An "L"-only entry loses the qubit with the entry's probability, like the
    # scalar loss field, but expressed inside a correlated string.
    noise = NoiseConfig()
    noise.x.L = 0.1
    results = compile_and_run(
        "{use q = Qubit(); X(q); MResetZ(q)}",
        shots=1000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    check_histogram(results, {"-": 0.1, "1": 0.9})


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_correlated_pauli_and_loss(sim_type):
    # "XL" applies an X to the control qubit and loses the target qubit, both
    # with probability 0.1, as a single correlated event. The X on the control
    # cancels the gate's X, so the control reads 0 exactly when the target is lost.
    noise = NoiseConfig()
    noise.cx.XL = 0.1
    results = compile_and_run(
        "{use qs = Qubit[2]; X(qs[0]); CNOT(qs[0], qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=10_000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    # Without noise: control=1, target=1 => "11".
    # With the "XL" event (p=0.1): control flips to 0, target lost => "0-".
    check_histogram(results, {"11": 0.9, "0-": 0.1}, tolerance=0.03)


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_correlated_multi_qubit_loss(sim_type):
    # "LL" loses both qubits of the gate together with probability 0.1.
    noise = NoiseConfig()
    noise.cz.LL = 0.1
    results = compile_and_run(
        "{use qs = Qubit[2]; CZ(qs[0], qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=10_000,
        seed=SEED,
        noise=noise,
        sim_type=sim_type,
    )
    # Either both qubits are lost together ("--", p=0.1) or neither ("00", p=0.9).
    check_histogram(results, {"--": 0.1, "00": 0.9}, tolerance=0.03)


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
