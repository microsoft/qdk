# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pathlib import Path
import pyqir

import qsharp
from qsharp._simulation import run_qir_clifford, NoiseConfig
from qsharp._device._atom import NeutralAtomDevice
from qsharp._device._atom._decomp import DecomposeRzAnglesToCliffordGates
from qsharp._device._atom._validate import ValidateNoConditionalBranches
from qsharp import TargetProfile, Result

current_file_path = Path(__file__)
# Get the directory of the current file
current_dir = current_file_path.parent

# Tests for the Q# noisy simulator.


def transform_to_clifford(input) -> str:
    native_qir = NeutralAtomDevice().compile(input)
    module = pyqir.Module.from_ir(pyqir.Context(), str(native_qir))
    ValidateNoConditionalBranches().run(module)
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


def test_program_with_branching_fails():
    qsharp.init(target_profile=TargetProfile.Adaptive_RI)
    qsharp.eval(
        """
        operation Main() : Result {
            use q = Qubit();
            H(q);
            if (MResetZ(q) == One) {
                X(q);
            }
            return MResetZ(q);
        }
        """
    )
    ir = qsharp.compile("Main()")
    try:
        run_qir_clifford(str(ir), 1, NoiseConfig())
        assert False, "Expected ValueError for branching control flow"
    except ValueError as e:
        assert (
            "simulation of programs with branching control flow is not supported"
            in str(e)
        )


def test_program_with_unconditional_branching_succeeds():
    qir = """
%Result = type opaque
%Qubit = type opaque

@empty_tag = internal constant [1 x i8] c"\\00"
@0 = internal constant [6 x i8] c"0_a0r\\00"
@1 = internal constant [6 x i8] c"1_a1r\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  br label %block_1
block_1:
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
  br label %block_2
block_2:
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  br label %block_3
block_3:
  call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

declare void @__quantum__rt__array_record_output(i64, i8*)

declare void @__quantum__rt__result_record_output(%Result*, i8*)

declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="2" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
"""

    output = run_qir_clifford(qir, 1, NoiseConfig())
    assert output == [[Result.Zero, Result.Zero]] or output == [
        [Result.One, Result.One]
    ]


def test_cy_direct_qir():
    qir = """
%Result = type opaque
%Qubit = type opaque

@empty_tag = internal constant [1 x i8] c"\\00"
@0 = internal constant [6 x i8] c"0_a0r\\00"
@1 = internal constant [6 x i8] c"1_a1r\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
    call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
    call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
    call void @__quantum__qis__s__body(%Qubit* inttoptr (i64 1 to %Qubit*))
    call void @__quantum__qis__cy__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
    call void @__quantum__qis__s__adj(%Qubit* inttoptr (i64 1 to %Qubit*))
    call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__cy__body(%Qubit*, %Qubit*)

declare void @__quantum__rt__array_record_output(i64, i8*)

declare void @__quantum__rt__result_record_output(%Result*, i8*)

declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="2" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
"""

    # Do not go through Neutral Atom device compilation since we want to test CY.
    output = run_qir_clifford(qir, 50, NoiseConfig())
    # This test should deterministically produce Zero.
    # If CZ or CX is executed instead of CY, then some measurements will produce One.
    assert all(shot[1] == Result.Zero for shot in output)
