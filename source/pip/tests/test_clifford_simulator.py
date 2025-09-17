# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pathlib import Path
import time
import sys

import qsharp
from qsharp._simulation import run_qir, NoiseConfig
from qsharp.passes import transform_to_clifford
from qsharp import TargetProfile

current_file_path = Path(__file__)
# Get the directory of the current file
current_dir = current_file_path.parent

# Tests for the Q# noisy simulator.


def read_file(file_name: str) -> str:
    return Path(file_name).read_text(encoding="utf-8")


def read_file_relative(file_name: str) -> str:
    return Path(current_dir / file_name).read_text(encoding="utf-8")


def test_smoke():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    input = qsharp.compile(
        "IsingModel2DEvolution(5, 5, PI() / 2.0, PI() / 2.0, 40.0, 40)"
    )
    input = transform_to_clifford(input)
    output = run_qir(str(input), 10, NoiseConfig())
    print(output)


def test_1224_clifford_ising():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    input = qsharp.compile(
        "IsingModel2DEvolution(34, 36, PI() / 2.0, PI() / 2.0, 40.0, 40)"
    )
    input = transform_to_clifford(input)
    qir = str(input)

    output = run_qir(qir, 1, NoiseConfig())

    print(output)


def test_million():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordCalls.qs"))

    ir = qsharp.compile("Main()")
    output = run_qir(str(ir), 1, NoiseConfig())
    print(output)
