# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest

SKIP_REASON = "GPU is not available"

try:
    from qsharp._native import try_create_gpu_adapter

    try_create_gpu_adapter()

    GPU_AVAILABLE = True
except OSError as e:
    GPU_AVAILABLE = False
    SKIP_REASON = str(e)


from pathlib import Path

from qsharp import BitFlipNoise

import qsharp
from qsharp import TargetProfile

from qsharp._simulation import run_qir_gpu, run_shot_gpu

current_file_path = Path(__file__)
# Get the directory of the current file
current_dir = current_file_path.parent


def read_file(file_name: str) -> str:
    return Path(file_name).read_text(encoding="utf-8")


def read_file_relative(file_name: str) -> str:
    return Path(current_dir / file_name).read_text(encoding="utf-8")


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_smoke():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    input = qsharp.compile(
        "IsingModel2DEvolution(5, 5, PI() / 2.0, PI() / 2.0, 2.0, 2)"
    )

    output = run_qir_gpu(str(input))
    print(output)


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_smoke2():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    input = qsharp.compile(
        "IsingModel2DEvolution(5, 5, PI() / 2.0, PI() / 2.0, 10.0, 10)"
    )

    output = run_qir_gpu(str(input))
    print(output)


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_smoke_noise():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    input = qsharp.compile(
        "IsingModel2DEvolution(5, 5, PI() / 2.0, PI() / 2.0, 10.0, 10)"
    )

    output = run_qir_gpu(str(input), shots=3, noise=BitFlipNoise(0.01), seed=None)
    print(output)


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_gpu_sampling():
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

    results = [run_shot_gpu(qir, None, seed) for seed in range(12)]

    # Results will be an array of 12 tuples of (bit_string, probability)
    # Each result string should be "00" or "11"
    # Running with fixed seeds of 0..11 gives 6 results of each
    # Note: 0.5 probability returned is actually about 0.49999997

    # Verify we have 6 of each result
    count_00 = sum(1 for r, p in results if r == "00")
    count_11 = sum(1 for r, p in results if r == "11")
    assert count_00 == 6
    assert count_11 == 6

    # Verify probabilities are all about 0.5
    for r, p in results:
        assert abs(p - 0.5) < 0.00001
