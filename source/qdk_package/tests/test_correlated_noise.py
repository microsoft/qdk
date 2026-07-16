# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import os
import pytest
from pathlib import Path
from qdk.simulation import NoiseConfig, run_qir
from qdk import Result
import qdk.openqasm
from qdk.simulation._simulation import try_create_gpu_adapter

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


QASM_WITH_CORRELATED_NOISE = """
OPENQASM 3.0;
include "stdgates.inc";

@qdk.qir.noise_intrinsic
gate test_noise_intrinsic q0, q1, q2 {}

qubit[3] qs;
x qs[1];
test_noise_intrinsic qs[0], qs[1], qs[2];
bit[3] res = measure qs;
"""

QIR_WITH_CORRELATED_NOISE = qdk.openqasm.compile(
    QASM_WITH_CORRELATED_NOISE,
    output_semantics=qdk.openqasm.OutputSemantics.OpenQasm,
    target_profile=qdk.TargetProfile.Base,
)


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_noiseless_simulation(sim_type):
    output = run_qir(QIR_WITH_CORRELATED_NOISE, shots=1, noise=None, type=sim_type)
    assert output == [[Result.Zero, Result.One, Result.Zero]]


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_noisy_simulation(sim_type):
    noise = NoiseConfig()
    table = noise.intrinsic("test_noise_intrinsic", 3)
    table.yyy = 1.0
    output = run_qir(QIR_WITH_CORRELATED_NOISE, shots=1, noise=noise, type=sim_type)
    assert output == [[Result.One, Result.Zero, Result.One]]


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_correlated_loss_only_entry(sim_type):
    noise = NoiseConfig()
    table = noise.intrinsic("test_noise_intrinsic", 3)
    table.yyl = 1.0
    output = run_qir(QIR_WITH_CORRELATED_NOISE, shots=1, noise=noise, type=sim_type)
    assert output == [[Result.One, Result.Zero, Result.Loss]]


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_load_csv_dir(sim_type):
    noise = NoiseConfig()
    noise.load_csv_dir(str(Path(__file__).parent / "csv_dir_test"))
    output = run_qir(QIR_WITH_CORRELATED_NOISE, shots=1, noise=noise, type=sim_type)
    assert output == [[Result.One, Result.Zero, Result.Loss]]


def test_noisy_simulation_with_missing_gates_fails():
    """
    This failure happens before the list of QIR instructions makes it to
    any specific simulator. So, we don't need separate tests for all of them.
    """
    noise = NoiseConfig()
    with pytest.raises(ValueError) as excinfo:
        run_qir(QIR_WITH_CORRELATED_NOISE, shots=1, noise=noise, type="cpu")
    assert "Missing noise intrinsic: test_noise_intrinsic" in str(excinfo.value)
