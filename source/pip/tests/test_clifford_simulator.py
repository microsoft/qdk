# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pathlib import Path
import pyqir

import qsharp
from qsharp._simulation import run_qir_clifford, NoiseConfig
from qsharp._device._atom import NeutralAtomDevice
from qsharp._device._atom._decomp import DecomposeRzAnglesToCliffordGates
from qsharp._device._atom._validate import ValidateSingleBlock
from qsharp import TargetProfile, Result

current_file_path = Path(__file__)
# Get the directory of the current file
current_dir = current_file_path.parent

# Tests for the Q# noisy simulator.


def transform_to_clifford(input) -> str:
    native_qir = NeutralAtomDevice().compile(input)
    module = pyqir.Module.from_ir(pyqir.Context(), str(native_qir))
    ValidateSingleBlock().run(module)
    DecomposeRzAnglesToCliffordGates().run(module)
    return str(module)


def read_file(file_name: str) -> str:
    return Path(file_name).read_text(encoding="utf-8")


def read_file_relative(file_name: str) -> str:
    return Path(current_dir / file_name).read_text(encoding="utf-8")


def test_smoke():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    input = qsharp.compile(
        "IsingModel2DEvolution(5, 5, PI() / 2.0, PI() / 2.0, 5.0, 5)"
    )
    input = transform_to_clifford(input)
    output = run_qir_clifford(input, 10, NoiseConfig())
    print(output)


def test_1224_clifford_ising():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    input = qsharp.compile(
        "IsingModel2DEvolution(20, 50, PI() / 2.0, PI() / 2.0, 5.0, 5)"
    )
    qir = transform_to_clifford(input)

    output = run_qir_clifford(qir, 1, NoiseConfig())

    print(output)


def test_million():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordCalls.qs"))

    ir = qsharp.compile("Main()")
    output = run_qir_clifford(str(ir), 1, NoiseConfig())
    print(output)


def test_s_noise_inherits_from_rz():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval("operation Main() : Result { use q = Qubit(); S(q); MResetZ(q) }")
    ir = qsharp.compile("Main()")
    noise = NoiseConfig()
    noise.rz.x = 1.0
    output = run_qir_clifford(str(ir), 1, noise)
    assert output == [Result.One]


def test_z_noise_inherits_from_rz():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval("operation Main() : Result { use q = Qubit(); Z(q); MResetZ(q) }")
    ir = qsharp.compile("Main()")
    noise = NoiseConfig()
    noise.rz.x = 1.0
    output = run_qir_clifford(str(ir), 1, noise)
    assert output == [Result.One]


def test_s_adj_noise_inherits_from_rz():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(
        "operation Main() : Result { use q = Qubit(); Adjoint S(q); MResetZ(q) }"
    )
    ir = qsharp.compile("Main()")
    noise = NoiseConfig()
    noise.rz.x = 1.0
    output = run_qir_clifford(str(ir), 1, noise)
    assert output == [Result.One]
