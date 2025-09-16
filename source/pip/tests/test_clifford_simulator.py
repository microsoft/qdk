# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pathlib import Path
import qsharp

current_file_path = Path(__file__)
# Get the directory of the current file
current_dir = current_file_path.parent

# Tests for the Q# noisy simulator.


def read_file(file_name: str) -> str:
    return Path(file_name).read_text(encoding="utf-8")


def read_file_relative(file_name: str) -> str:
    return Path(current_dir / file_name).read_text(encoding="utf-8")


def test_smoke():
    from qsharp._simulation import run_qir, NoiseConfig
    from qsharp import TargetProfile

    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    ir = qsharp.compile("IsingModel2DEvolution(5, 5, PI() / 2.0, PI() / 2.0, 40.0, 40)")
    output = run_qir(str(ir), 10, NoiseConfig())
    print(output)


def test_1224_clifford_ising_1MM_calls():
    from qsharp._simulation import run_qir, NoiseConfig
    from qsharp import TargetProfile

    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    ir = qsharp.compile(
        "IsingModel2DEvolution(34, 36, PI() / 2.0, PI() / 2.0, 300.0, 300)"
    )
    output = run_qir(str(ir), 1, NoiseConfig())
    print(output)


def test_million():
    from qsharp._simulation import run_qir, NoiseConfig
    from qsharp import TargetProfile

    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordCalls.qs"))

    ir = qsharp.compile("Main()")
    output = run_qir(str(ir), 1, NoiseConfig())
    print(output)
