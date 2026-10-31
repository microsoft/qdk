# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest

from qdk.qre import LOGICAL
from qdk.qre.instruction_ids import T
from qdk.qre.models import BlackBoxFactory, GateBased


def test_black_box_factory_produces_configured_t_state():
    ctx = GateBased(gate_time=50, measurement_time=100).context()

    isas = list(BlackBoxFactory.q(space=100, time=250, error_rate=1e-8).enumerate(ctx))

    assert len(isas) == 1
    assert len(isas[0]) == 1
    instruction = isas[0][T]
    assert instruction.encoding == LOGICAL
    assert instruction.arity == 1
    assert instruction.expect_space() == 100
    assert instruction.expect_time() == 250
    assert instruction.expect_error_rate() == 1e-8


@pytest.mark.parametrize(
    "kwargs, missing",
    [
        ({"time": 250, "error_rate": 1e-8}, "space"),
        ({"space": 100, "error_rate": 1e-8}, "time"),
        ({"space": 100, "time": 250}, "error_rate"),
    ],
)
def test_black_box_factory_requires_all_values(kwargs, missing):
    with pytest.raises(TypeError, match=missing):
        BlackBoxFactory(**kwargs)


@pytest.mark.parametrize(
    "kwargs, field",
    [
        ({"space": 0}, "space"),
        ({"space": -1}, "space"),
        ({"space": 1.5}, "space"),
        ({"space": True}, "space"),
        ({"time": 0}, "time"),
        ({"time": -1}, "time"),
        ({"time": 1.5}, "time"),
        ({"time": False}, "time"),
        ({"error_rate": -0.1}, "error_rate"),
        ({"error_rate": 1.5}, "error_rate"),
        ({"error_rate": float("nan")}, "error_rate"),
        ({"error_rate": float("inf")}, "error_rate"),
        ({"error_rate": True}, "error_rate"),
    ],
)
def test_black_box_factory_rejects_invalid_values(kwargs, field):
    values = {"space": 100, "time": 250, "error_rate": 1e-8}
    values.update(kwargs)

    with pytest.raises(ValueError, match=field):
        BlackBoxFactory(**values)


@pytest.mark.parametrize("error_rate", [0.0, 1.0])
def test_black_box_factory_accepts_probability_boundaries(error_rate):
    factory = BlackBoxFactory(space=100, time=250, error_rate=error_rate)
    assert factory.error_rate == error_rate
