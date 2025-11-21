# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pathlib import Path
from typing import Sequence, cast
import math
import os

import pytest

from qsharp._native import Result

# Skip all tests in this module if QDK_GPU_TESTS is not set
if not os.environ.get("QDK_GPU_TESTS"):
    pytest.skip("Skipping GPU tests (QDK_GPU_TESTS not set)", allow_module_level=True)

SKIP_REASON = "GPU is not available"

gpu_name = "Unknown"

try:
    from qsharp._native import try_create_gpu_adapter

    gpu_name = try_create_gpu_adapter()

    GPU_AVAILABLE = True
except OSError as e:
    GPU_AVAILABLE = False
    SKIP_REASON = str(e)


import qsharp
from qsharp import TargetProfile
from qsharp import openqasm

from qsharp._simulation import run_qir_gpu, NoiseConfig

current_file_path = Path(__file__)
# Get the directory of the current file
current_dir = current_file_path.parent


def read_file(file_name: str) -> str:
    return Path(file_name).read_text(encoding="utf-8")


def read_file_relative(file_name: str) -> str:
    return Path(current_dir / file_name).read_text(encoding="utf-8")


def result_array_to_string(results: Sequence[Result]) -> str:
    chars = []
    for value in results:
        if value == Result.Zero:
            chars.append("0")
        elif value == Result.One:
            chars.append("1")
        else:
            chars.append("-")
    return "".join(chars)


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_gpu_seeding_no_noise():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(
        """
        operation BellTest() : Result[] {
            use qs = Qubit[2];
            H(qs[0]);
            CNOT(qs[0], qs[1]);
            MResetEachZ(qs)
        }
        """
    )

    qir = str(qsharp.compile("BellTest()"))

    results = [run_qir_gpu(qir, 1, None, seed)[0] for seed in range(12)]
    print(results)

    # Results will be an array of 12 lists [Result, Result]
    # Each result should be [Zero, Zero] or [One, One]
    # As evident from a manual experiment running with the seeds of 0..11
    # gives 6 results of each. Experiment should be repeatable for fixed seeds.

    # Verify we have 6 of each result
    count_00 = sum(1 for r in results if r == [Result.Zero, Result.Zero])
    count_11 = sum(1 for r in results if r == [Result.One, Result.One])
    assert count_00 == 6
    assert count_11 == 6


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_gpu_no_noise():
    """Simple test that GPU simulator works without noise."""
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    input = qsharp.compile(
        "IsingModel2DEvolution(5, 5, PI() / 2.0, PI() / 2.0, 10.0, 10)"
    )

    output = run_qir_gpu(str(input))
    print(gpu_name)
    print(output)
    # Expecting deterministic output, no randomization seed needed.
    assert output == [[Result.Zero] * 25], "Expected result of 0s with pi/2 angles."


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_gpu_bitflip_noise():
    """Bitflip noise for GPU simulator."""
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    input = qsharp.compile(
        "IsingModel2DEvolution(5, 5, PI() / 2.0, PI() / 2.0, 10.0, 10)"
    )

    p_noise = 0.005
    noise = NoiseConfig()
    noise.rx.set_bitflip(p_noise)
    noise.rzz.set_bitflip(p_noise)
    noise.mresetz.set_bitflip(p_noise)

    output = run_qir_gpu(str(input), shots=3, noise=noise, seed=17)
    result = [result_array_to_string(cast(Sequence[Result], x)) for x in output]
    print(result)
    # Reasonable results obtained from manual run
    assert result == [
        "0000000000011100001001110",
        "0001001000000000000100100",
        "0000001000110000000100011",
    ]


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_gpu_mixed_noise():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    input = qsharp.compile(
        "IsingModel2DEvolution(5, 5, PI() / 2.0, PI() / 2.0, 4.0, 4)"
    )

    noise = NoiseConfig()
    noise.rz.set_bitflip(0.005)
    noise.rz.loss = 0.003
    noise.rzz.set_depolarizing(0.005)
    noise.rzz.loss = 0.003

    output = run_qir_gpu(str(input), shots=3, noise=noise, seed=53)
    result = [result_array_to_string(cast(Sequence[Result], x)) for x in output]
    print(result)
    # Reasonable results obtained from manual run
    assert result == [
        "00000-00010000-0000000001",
        "00000000000-0000000000-00",
        "000000000000001-000000000",
    ]


def build_x_chain_qir(n_instances: int, n_x: int) -> str:
    # Construct multiple instances of x gate chains
    prefix = f"""
        OPENQASM 3.0;
        include "stdgates.inc";
        bit[{n_instances}] c;
        qubit[{n_instances}] q;
    """

    infix = """
        x q;
    """

    suffix = """
        c = measure q;
    """

    src_parallel = prefix + infix * n_x + suffix

    # Compile resulting program
    qsharp.init(target_profile=TargetProfile.Base)
    qir_parallel = openqasm.compile(src_parallel)
    return str(qir_parallel)


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
@pytest.mark.parametrize(
    "p_noise, n_x, n_instances, n_shots, max_percent",
    [
        (0.0005, 500, 10, 8192, 1.0),
        (0.005, 500, 10, 4096, 2.0),
        (0.0005, 20, 20, 100, 4.0),  # Only 100 shots produces imprecise results
    ],
)
def test_gpu_x_chain(
    p_noise: float, n_x: int, n_instances: int, n_shots: int, max_percent: float
):
    """
    Simulate multi-instance X-chain with bitflip noise many times
    Compare result frequencies with analytically computed probabilities
    """
    # Use the GPU simulator with noise
    noise = NoiseConfig()
    noise.x.set_bitflip(p_noise)

    qir = build_x_chain_qir(n_instances, n_x)
    output = run_qir_gpu(qir, shots=n_shots, noise=noise, seed=18)
    histogram = [0 for _ in range(n_instances + 1)]
    for shot in output:
        shot_results = cast(Sequence[Result], shot)
        count_1 = shot_results.count(Result.One)
        histogram[count_1] += 1

    # Probability of obtaining 0 and 1 at the end of the X chain.
    p_0 = ((2.0 * p_noise - 1.0) ** n_x + 1.0) / 2.0
    p_1 = 1.0 - p_0

    # Number of results with k ones that should be there.
    p_N = [
        p_0 ** ((n_instances - k)) * (p_1**k) * math.comb(n_instances, k) * n_shots
        for k in range(n_instances + 1)
    ]

    # Error % for deviation from analytical value
    error_percent = [abs(a - b) * 100.0 / n_shots for (a, b) in zip(histogram, p_N)]
    print(", ".join(f"{a} (Δ≈{b:.1f}%)" for (a, b) in zip(histogram, error_percent)))

    # We tolerate configured percentage error.
    assert all(
        err < max_percent for err in error_percent
    ), f"Error percent too high: {error_percent}"
