from qsharp._simulation import NoiseConfig
import pytest


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
    noise.cz.set_pauli_noise("iz", 0.01)
    noise.cz.set_pauli_noise("zz", 0.02)
    assert noise.cz.iz == 0.01
    assert noise.cz.zz == 0.02


def test_setting_2q_noise_through_attr():
    noise = NoiseConfig()
    noise.cz.set_pauli_noise("iz", 0.01)
    noise.cz.set_pauli_noise("zz", 0.02)
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


def test_accessing_non_set_pauli_attr_errors():
    noise = NoiseConfig()
    with pytest.raises(AttributeError):
        noise.h.x


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
