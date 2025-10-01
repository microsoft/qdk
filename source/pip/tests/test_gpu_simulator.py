# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pathlib import Path

from qsharp import BitFlipNoise

import qsharp
from qsharp._simulation import NoiseConfig
from qsharp.passes import transform_to_clifford
from qsharp import TargetProfile

from qsharp._simulation import run_qir_gpu

current_file_path = Path(__file__)
# Get the directory of the current file
current_dir = current_file_path.parent


def read_file(file_name: str) -> str:
    return Path(file_name).read_text(encoding="utf-8")


def read_file_relative(file_name: str) -> str:
    return Path(current_dir / file_name).read_text(encoding="utf-8")


def test_smoke():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    input = qsharp.compile(
        "IsingModel2DEvolution(5, 5, PI() / 2.0, PI() / 2.0, 2.0, 2)"
    )

    # TODO: Reinstate once we figure out how to run in the CI
    output = "Test skipped to fix CI issues"
    # output = run_qir_gpu(str(input))
    print(output)


def test_smoke2():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    input = qsharp.compile(
        "IsingModel2DEvolution(5, 5, PI() / 2.0, PI() / 2.0, 10.0, 10)"
    )

    # TODO: Reinstate once we figure out how to run in the CI
    output = "Test skipped to fix CI issues"
    # output = run_qir_gpu(str(input))
    print(output)


def test_smoke_noise():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    input = qsharp.compile(
        "IsingModel2DEvolution(5, 5, PI() / 2.0, PI() / 2.0, 10.0, 10)"
    )

    output = "Test skipped to fix CI issues"
    # output = run_qir_gpu(str(input), shots=3, noise=BitFlipNoise(0.01), seed=None)
    print(output)
