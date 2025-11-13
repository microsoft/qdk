# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest

from qsharp._native import Result

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

from qsharp._simulation import run_qir_gpu, NoiseConfig

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

    p_noise = 0.01
    noise = NoiseConfig()
    noise.rx.set_bitflip(p_noise)
    noise.rzz.set_bitflip(p_noise)
    noise.mresetz.set_bitflip(p_noise)
    
    output = run_qir_gpu(str(input), shots=3, noise=noise, seed=None)
    print(output)

@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_smoke_noise_2():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    input = qsharp.compile(
        "IsingModel2DEvolution(5, 5, PI() / 2.0, PI() / 2.0, 4.0, 4)"
    )

    noise = NoiseConfig()
    noise.rz.set_bitflip(0.1)
    noise.rz.loss = 0.03
    noise.rzz.set_depolarizing(0.1)
    noise.rzz.loss = 0.03

    output = run_qir_gpu(str(input), shots=3, noise=noise, seed=None)
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
