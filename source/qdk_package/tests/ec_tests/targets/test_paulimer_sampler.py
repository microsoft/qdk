"""Tests for `PaulimerSampler` — logical-level noiseless Sampler."""
from __future__ import annotations

import numpy as np
import pytest

pytest.importorskip("paulimer")

import qodec  # noqa: E402

from qodec.circuits import Program  # noqa: E402
from ec_tests.testing.qodecs import c4  # noqa: E402
from qdk.ec.targets import PaulimerSampler, Sampler  # noqa: E402


@pytest.fixture(scope="module")
def c4_codec() -> qodec.Qodec:
    return c4()


def test_satisfies_sampler_protocol(c4_codec: qodec.Codec) -> None:
    sampler = PaulimerSampler(c4_codec)
    assert isinstance(sampler, Sampler)


def test_physical_readouts_shape(c4_codec: qodec.Codec) -> None:
    sampler = PaulimerSampler(c4_codec)
    program = Program(
        [
            qodec.instructions.InstructionCall("prepare_zz", outputs={"block": "data"}),
            qodec.instructions.InstructionCall("measure_zz", inputs={"block": "data"}),
        ],
        c4_codec.layers[0].isa,
    )
    result = sampler.execute(program, shots=10)
    # measure_zz declares 2 observables (c4 encodes 2 logicals per block).
    assert np.asarray(result).shape == (10, 2)


def test_memory_experiment_is_noiseless(c4_codec: qodec.Codec) -> None:
    sampler = PaulimerSampler(c4_codec)
    program = Program(
        [
            qodec.instructions.InstructionCall("prepare_zz", outputs={"block": "data"}),
            qodec.instructions.InstructionCall("idle", inputs={"block": "data"}, outputs={"block": "data"}),
            qodec.instructions.InstructionCall("measure_zz", inputs={"block": "data"}),
        ],
        c4_codec.layers[0].isa,
    )
    result = sampler.execute(program, shots=100)
    assert not np.asarray(result).any()


def test_bell_pair_perfect_correlation(c4_codec: qodec.Codec) -> None:
    """transversal_cx between |+...+> and |0...0>, then measure both
    in Z: outcomes must be perfectly correlated."""
    sampler = PaulimerSampler(c4_codec)
    program = Program(
        [
            qodec.instructions.InstructionCall("prepare_zz", outputs={"block": "a"}),
            qodec.instructions.InstructionCall("prepare_xx", outputs={"block": "b"}),
            qodec.instructions.InstructionCall(
                "transversal_cx",
                inputs={"control": "b", "target": "a"},
                outputs={"control": "b", "target": "a"},
            ),
            qodec.instructions.InstructionCall("measure_zz", inputs={"block": "a"}),
            qodec.instructions.InstructionCall("measure_zz", inputs={"block": "b"}),
        ],
        c4_codec.layers[0].isa,
    )
    result = sampler.execute(program, shots=200)
    a_logicals = np.asarray(result)[:, :2]
    b_logicals = np.asarray(result)[:, 2:4]
    assert (a_logicals == b_logicals).all()


def test_xx_prep_then_xx_measure_is_noiseless(c4_codec: qodec.Codec) -> None:
    sampler = PaulimerSampler(c4_codec)
    program = Program(
        [
            qodec.instructions.InstructionCall("prepare_xx", outputs={"block": "data"}),
            qodec.instructions.InstructionCall("measure_xx", inputs={"block": "data"}),
        ],
        c4_codec.layers[0].isa,
    )
    result = sampler.execute(program, shots=50)
    assert not np.asarray(result).any()
