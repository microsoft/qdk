"""Tests for raw qdk.ec execution targets."""
from __future__ import annotations

import numpy as np
import pytest

stim = pytest.importorskip("stim")

import qodec  # noqa: E402
from qodec.circuits import Program  # noqa: E402
from ec_tests.testing.qodecs import c4  # noqa: E402
from qdk.ec.targets import (  # noqa: E402
    StimSampler,
    Target,
    detector_error_model_of,
)


@pytest.fixture
def c4_codec() -> qodec.Qodec:
    return c4()


@pytest.fixture
def c4_source_isa(c4_codec: qodec.Qodec) -> qodec.InstructionSet:
    return c4_codec.layers[0].isa


def _program(isa: qodec.InstructionSet, *mnemonics: str) -> Program:
    return Program([_call(isa, m) for m in mnemonics], isa)


def _call(isa: qodec.InstructionSet, mnemonic: str) -> qodec.instructions.InstructionCall:
    instruction = isa.instruction(mnemonic)
    inputs = {str(i): "q" for i in range(len(list(instruction.inputs)))}
    outputs = {str(i): "q" for i in range(len(list(instruction.outputs)))}
    if not inputs and not outputs:
        return qodec.instructions.InstructionCall(mnemonic)
    return qodec.instructions.InstructionCall(mnemonic, inputs=inputs, outputs=outputs)


@pytest.fixture
def c4_sampler(c4_codec: qodec.Codec) -> StimSampler:
    return StimSampler(c4_codec)


def test_stim_sampler_is_target(c4_sampler: "StimSampler") -> None:
    assert isinstance(c4_sampler, Target)


def test_noiseless_idle_has_no_detections(c4_sampler: "StimSampler", c4_source_isa: qodec.InstructionSet) -> None:
    program = _program(c4_source_isa, "prepare_zz", "idle")
    result = c4_sampler.execute(program, shots=100)
    events = c4_sampler.emitter.detection_events(program, np.asarray(result))
    assert events.shape[1] > 0
    assert events.sum() == 0


def test_noisy_idle_has_some_detections(c4_codec: qodec.Codec, c4_source_isa: qodec.InstructionSet) -> None:
    sampler = StimSampler(
        c4_codec, noise={"p_data": 0.1, "p_meas": 0.1}
    )
    program = _program(c4_source_isa, "prepare_zz", "idle")
    result = sampler.execute(program, shots=1000)
    events = sampler.emitter.detection_events(program, np.asarray(result))
    assert events.sum() > 0


def test_detector_error_model_uses_target_noise(
    c4_codec: qodec.Codec,
    c4_source_isa: qodec.InstructionSet,
) -> None:
    program = _program(c4_source_isa, "prepare_zz", "idle")
    dem = detector_error_model_of(
        c4_codec,
        program,
        {"p_data": 0.01, "p_meas": 0.01},
    )
    assert "error(" in str(dem)


def test_prepare_measure_noiseless(c4_sampler: "StimSampler", c4_source_isa: qodec.InstructionSet) -> None:
    program = _program(c4_source_isa, "prepare_zz", "measure_zz")
    result = c4_sampler.execute(program, shots=100)
    flips = c4_sampler.emitter.observable_flips(program, np.asarray(result))
    assert flips.shape == (100, 3)
    assert flips.sum() == 0


def test_prepare_measure_noisy(c4_codec: qodec.Codec, c4_source_isa: qodec.InstructionSet) -> None:
    sampler = StimSampler(
        c4_codec, noise={"p_data": 0.05, "p_meas": 0.05}
    )
    program = _program(c4_source_isa, "prepare_zz", "measure_zz")
    result = sampler.execute(program, shots=10_000)
    flips = sampler.emitter.observable_flips(program, np.asarray(result))
    error_rate = flips.mean()
    assert 0 < error_rate < 0.5


def test_sample_result_attributes(c4_sampler: "StimSampler", c4_source_isa: qodec.InstructionSet) -> None:
    program = _program(c4_source_isa, "prepare_zz", "measure_zz")
    result = c4_sampler.execute(program, shots=10)
    assert len(result) == 10
    assert np.asarray(result).shape[0] == 10

