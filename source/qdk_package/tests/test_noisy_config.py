# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from qdk.simulation import NoiseConfig, LossPolicy
import pytest


def test_accessing_unset_valid_pauli():
    noise = NoiseConfig()
    assert noise.h.x == 0


def test_setting_1q_noise():
    noise = NoiseConfig()
    noise.h.set_pauli_noise("X", 0.01)
    assert noise.h.x == 0.01


def test_setting_1q_noise_through_attr():
    noise = NoiseConfig()
    noise.h.x = 0.01
    assert noise.h.x == 0.01


def test_setting_2q_noise():
    noise = NoiseConfig()
    noise.cz.set_pauli_noise("IZ", 0.01)
    noise.cz.set_pauli_noise("ZZ", 0.02)
    assert noise.cz.iz == 0.01
    assert noise.cz.zz == 0.02


def test_setting_2q_noise_through_attr():
    noise = NoiseConfig()
    noise.cz.set_pauli_noise("IZ", 0.01)
    noise.cz.set_pauli_noise("ZZ", 0.02)
    assert noise.cz.iz == 0.01
    assert noise.cz.zz == 0.02


def test_setting_1q_depolarizing_noise():
    noise = NoiseConfig()
    noise.h.set_depolarizing(0.3)
    p = 0.3 / 3
    assert noise.h.x == p
    assert noise.h.y == p
    assert noise.h.z == p


def test_setting_2q_depolarizing_noise():
    noise = NoiseConfig()
    noise.cz.set_depolarizing(0.15)
    p = 0.15 / 15
    assert noise.cz.ix == p
    assert noise.cz.iy == p
    assert noise.cz.iz == p
    assert noise.cz.xx == p
    assert noise.cz.xy == p
    assert noise.cz.xz == p
    assert noise.cz.yx == p
    assert noise.cz.yy == p
    assert noise.cz.yz == p
    assert noise.cz.zx == p
    assert noise.cz.zy == p
    assert noise.cz.zz == p


def test_setting_2q_noise_on_1q_op_errors():
    noise = NoiseConfig()
    with pytest.raises(AttributeError):
        noise.h.set_pauli_noise("ZZ", 0.01)


def test_setting_2q_noise_on_1q_op_through_attr_errors():
    noise = NoiseConfig()
    with pytest.raises(AttributeError):
        noise.h.zz = 0.01


def test_setting_1q_noise_on_2q_op_errors():
    noise = NoiseConfig()
    with pytest.raises(AttributeError):
        noise.cz.set_pauli_noise("Z", 0.01)


def test_setting_1q_noise_on_2q_op_through_attr_errors():
    noise = NoiseConfig()
    with pytest.raises(AttributeError):
        noise.cz.z = 0.01


def test_setting_non_valid_pauli_errors():
    noise = NoiseConfig()
    with pytest.raises(AttributeError):
        noise.h.set_pauli_noise("W", 0.01)


def test_setting_non_valid_pauli_through_attr_errors():
    noise = NoiseConfig()
    with pytest.raises(AttributeError):
        noise.h.w = 0.01


def test_accessing_invalid_pauli_attr_errors():
    noise = NoiseConfig()
    with pytest.raises(AttributeError):
        noise.h.w


def test_accessing_non_valid_pauli_attr_errors():
    noise = NoiseConfig()
    with pytest.raises(AttributeError):
        noise.h.w


def test_setting_bitflip_on_1q_op():
    noise = NoiseConfig()
    noise.h.set_bitflip(0.01)
    assert noise.h.x == 0.01


def test_setting_bitflip_on_2q_op_errors():
    noise = NoiseConfig()
    with pytest.raises(AttributeError):
        noise.cz.set_bitflip(0.01)


def test_setting_phaseflip_on_1q_op():
    noise = NoiseConfig()
    noise.h.set_phaseflip(0.01)
    assert noise.h.z == 0.01


def test_setting_phaseflip_on_2q_op_errors():
    noise = NoiseConfig()
    with pytest.raises(AttributeError):
        noise.cz.set_phaseflip(0.01)


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
