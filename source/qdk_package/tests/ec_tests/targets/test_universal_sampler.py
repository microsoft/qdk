"""Tests for `UniversalSampler` — the minimal end-to-end POC sampler.

These exercise the single-translation (runtime-only) path on the in-repo
``c4`` codec: paulimer outcome-specific physical simulation plus the trivial
readout-parity decode. The layered (multi-translation) path is demonstrated in
``examples/universal_sampler.ipynb`` on the ``c4c6`` concatenation.
"""
from __future__ import annotations

import warnings

import numpy as np
import pytest

pytest.importorskip("paulimer")

import qodec  # noqa: E402

from qodec.circuits import Program  # noqa: E402
from ec_tests.testing.qodecs import c4  # noqa: E402
from qdk.ec.targets import (  # noqa: E402
    AssumeViolation,
    Sampler,
    UniversalSampler,
    UnsupportedFeatureWarning,
)


@pytest.fixture(scope="module")
def c4_codec() -> qodec.Qodec:
    return c4()


def _call(mnemonic: str, **operands: str) -> qodec.instructions.InstructionCall:
    side = "outputs" if mnemonic.startswith("prepare") else "inputs"
    return qodec.instructions.InstructionCall(
        mnemonic,
        inputs=operands if side == "inputs" else {},
        outputs=operands if side == "outputs" else {},
    )


def test_satisfies_sampler_protocol(c4_codec: qodec.Qodec) -> None:
    assert isinstance(UniversalSampler(c4_codec), Sampler)


def test_only_construction_parameter_is_the_codec(c4_codec: qodec.Qodec) -> None:
    sampler = UniversalSampler(c4_codec)
    assert sampler.codec is c4_codec


def test_z_memory_is_noiseless(c4_codec: qodec.Qodec) -> None:
    program = Program(
        [
            _call("prepare_zz", block="data"),
            _call("idle", block="data"),
            _call("measure_zz", block="data"),
        ],
        c4_codec.layers[0].isa,
    )
    batch = UniversalSampler(c4_codec).execute(program, shots=200)
    bits = np.asarray(batch, dtype=bool)
    # C4 encodes two logical qubits; |00> measured in Z is deterministically 0.
    assert bits.shape == (200, 2)
    assert not bits.any()


def test_x_memory_is_noiseless(c4_codec: qodec.Qodec) -> None:
    program = Program(
        [
            _call("prepare_xx", block="data"),
            _call("measure_xx", block="data"),
        ],
        c4_codec.layers[0].isa,
    )
    bits = np.asarray(UniversalSampler(c4_codec).execute(program, shots=100), bool)
    assert not bits.any()


def test_transversal_cx_correlates_logical_outcomes(c4_codec: qodec.Qodec) -> None:
    """A transversal CX from |+>_L onto |0>_L makes the two blocks' Z
    readouts perfectly correlated — a genuine physical Clifford lowering."""
    program = Program(
        [
            _call("prepare_zz", block="a"),
            _call("prepare_xx", block="b"),
            qodec.instructions.InstructionCall(
                "transversal_cx",
                inputs={"control": "b", "target": "a"},
                outputs={"control": "b", "target": "a"},
            ),
            _call("measure_zz", block="a"),
            _call("measure_zz", block="b"),
        ],
        c4_codec.layers[0].isa,
    )
    bits = np.asarray(UniversalSampler(c4_codec).execute(program, shots=200), bool)
    assert (bits[:, :2] == bits[:, 2:4]).all()


def test_shots_independent_trajectories(c4_codec: qodec.Qodec) -> None:
    program = Program([_call("prepare_zz", block="data")], c4_codec.layers[0].isa)
    batch = UniversalSampler(c4_codec).execute(program, shots=8)
    # prepare_zz declares no observe outcomes, so each shot is an empty row.
    assert len(batch) == 8
    assert all(len(row) == 0 for row in batch)


def test_assume_satisfied_passes(c4_codec: qodec.Qodec) -> None:
    # The verified prep's `reject` flag is deterministically 0 at zero noise,
    # so asserting `reject == 0` holds on every shot and the run completes.
    program = Program(
        [
            qodec.instructions.InstructionCall(
                "prepare_zz", outputs={"block": "data"}, assume=[{"reject": 0}]
            )
        ],
        c4_codec.layers[0].isa,
    )
    batch = UniversalSampler(c4_codec).execute(program, shots=100)
    assert len(batch) == 100


def test_assume_violation_raises(c4_codec: qodec.Qodec) -> None:
    # `reject` is 0 at zero noise, so asserting `reject == 1` is violated on
    # every shot and aborts the run.
    program = Program(
        [
            qodec.instructions.InstructionCall(
                "prepare_zz", outputs={"block": "data"}, assume=[{"reject": 1}]
            )
        ],
        c4_codec.layers[0].isa,
    )
    with pytest.raises(AssumeViolation):
        UniversalSampler(c4_codec).execute(program, shots=100)


def test_no_spurious_warnings_for_supported_program(c4_codec: qodec.Qodec) -> None:
    program = Program(
        [_call("prepare_zz", block="data"), _call("measure_zz", block="data")],
        c4_codec.layers[0].isa,
    )
    with warnings.catch_warnings():
        warnings.simplefilter("error", UnsupportedFeatureWarning)
        UniversalSampler(c4_codec).execute(program, shots=10)
