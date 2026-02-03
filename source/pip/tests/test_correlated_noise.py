# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest
from qsharp._simulation import NoiseConfig, run_qir
from qsharp import Result
import qsharp.openqasm

# Tests for the Q# noisy simulator.
QASM_WITH_CORRELATED_NOISE = """
OPENQASM 3.0;
include "stdgates.inc";

@qdk.qir.noise_intrinsic
gate test_noise_intrinsic q0, q1 {}

qubit[2] qs;
x qs[1];
test_noise_intrinsic qs[0], qs[1];
bit[2] res = measure qs;
"""

QIR_WITH_CORRELATED_NOISE = qsharp.openqasm.compile(
    QASM_WITH_CORRELATED_NOISE,
    output_semantics=qsharp.openqasm.OutputSemantics.OpenQasm,
    target_profile=qsharp.TargetProfile.Base,
)


def test_noiseless_simulation():
    output = run_qir(QIR_WITH_CORRELATED_NOISE, shots=1, noise=None)
    assert output == [[Result.Zero, Result.One]]


def test_noisy_simulation():
    noise = NoiseConfig()
    table = noise.intrinsic("test_noise_intrinsic", 2)
    table.xx = 1.0
    output = run_qir(QIR_WITH_CORRELATED_NOISE, shots=1, noise=noise)
    assert output == [[Result.One, Result.Zero]]


def test_noisy_simulation_with_missing_gates_fails():
    noise = NoiseConfig()
    with pytest.raises(ValueError) as excinfo:
        run_qir(QIR_WITH_CORRELATED_NOISE, shots=1, noise=noise)
    assert "Missing noise intrinsic: test_noise_intrinsic" in str(excinfo.value)
