# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest
import sys
from qsharp._simulation import NoiseConfig, run_qir
from qsharp import Result
import qsharp.openqasm

SKIP_REASON = "GPU is not available"

gpu_info = "Unknown"

try:
    from qsharp._native import try_create_gpu_adapter

    gpu_info = try_create_gpu_adapter()
    # Printing to stderr so that it is visible if CI run fails
    print(f"*** USING GPU: {gpu_info}", file=sys.stderr)
    GPU_AVAILABLE = True
except OSError as e:
    GPU_AVAILABLE = False
    SKIP_REASON = str(e)


CPU_SIMULATORS = ("clifford", "cpu")
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

QIR_WITH_CORRELATED_NOISE = qsharp.openqasm.compile(
    QASM_WITH_CORRELATED_NOISE,
    output_semantics=qsharp.openqasm.OutputSemantics.OpenQasm,
    target_profile=qsharp.TargetProfile.Base,
)


def test_noiseless_simulation():
    for type in CPU_SIMULATORS:
        output = run_qir(QIR_WITH_CORRELATED_NOISE, shots=1, noise=None, type=type)
        assert output == [[Result.Zero, Result.One, Result.Zero]]


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_noiseless_simulation_gpu():
    output = run_qir(QIR_WITH_CORRELATED_NOISE, shots=1, noise=None, type="gpu")
    assert output == [[Result.Zero, Result.One, Result.Zero]]


def test_noisy_simulation():
    noise = NoiseConfig()
    table = noise.intrinsic("test_noise_intrinsic", 3)
    table.yyy = 1.0
    for type in CPU_SIMULATORS:
        output = run_qir(QIR_WITH_CORRELATED_NOISE, shots=1, noise=noise, type=type)
        assert output == [[Result.One, Result.Zero, Result.One]]


def test_load_csv_dir():
    noise = NoiseConfig()
    noise.load_csv_dir("./csv_dir_test")
    for type in CPU_SIMULATORS:
        output = run_qir(QIR_WITH_CORRELATED_NOISE, shots=1, noise=noise, type=type)
        assert output == [[Result.One, Result.Zero, Result.One]]


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_noisy_simulation_gpu():
    noise = NoiseConfig()
    noise.load_csv_dir("./csv_dir_test")
    output = run_qir(QIR_WITH_CORRELATED_NOISE, shots=1, noise=noise, type="gpu")
    assert output == [[Result.One, Result.Zero, Result.One]]


def test_noisy_simulation_with_missing_gates_fails():
    noise = NoiseConfig()
    with pytest.raises(ValueError) as excinfo:
        run_qir(QIR_WITH_CORRELATED_NOISE, shots=1, noise=noise)
    assert "Missing noise intrinsic: test_noise_intrinsic" in str(excinfo.value)
