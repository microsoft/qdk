# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest
from expecttest import assert_expected_inline

import qsharp
from qsharp._device._atom import NeutralAtomDevice, NoiseConfig

try:
    import pyqir

    PYQIR_AVAILABLE = True
except ImportError:
    PYQIR_AVAILABLE = False

SKIP_REASON = "PyQIR is not available"


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_device_compile() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qir = qsharp.compile(
        """
        {
            use qs = Qubit[2];
            H(qs[0]);
            CNOT(qs[0], qs[1]);
            MResetEachZ(qs)
        }
        """
    )

    device = NeutralAtomDevice()
    compiled = device.compile(qir)
    compiled_qir = str(compiled)

    assert_expected_inline(
        compiled_qir,
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer
@0 = internal constant [6 x i8] c"0_a0r\\00"
@1 = internal constant [6 x i8] c"1_a1r\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__qis__rz__body(double 0x3FF921FB54442D18, %Qubit* null)
  call void @__quantum__qis__rz__body(double 0x3FF921FB54442D18, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* null)
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rz__body(double 0x3FF921FB54442D18, %Qubit* null)
  call void @__quantum__qis__rz__body(double 0x3FF921FB54442D18, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rz__body(double 0x3FF921FB54442D18, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__rz__body(double 0x3FF921FB54442D18, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* null, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__array_record_output(i64, i8*)

declare void @__quantum__rt__result_record_output(%Result*, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="2" }

!llvm.module.flags = !{!0, !1, !2, !3}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_device_simulate_with_cpu() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qir = qsharp.compile(
        """
        {
            use qs = Qubit[2];
            H(qs[0]);
            CNOT(qs[0], qs[1]);
            MResetEachZ(qs)
        }
        """
    )

    device = NeutralAtomDevice()
    compiled = device.compile(qir)
    result = device.simulate(compiled, type="cpu")

    assert result == [[qsharp.Result.Zero, qsharp.Result.Zero]] or result == [
        [qsharp.Result.One, qsharp.Result.One]
    ]


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_device_simlate_with_clifford() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qir = qsharp.compile(
        """
        {
            use qs = Qubit[2];
            H(qs[0]);
            CNOT(qs[0], qs[1]);
            MResetEachZ(qs)
        }
        """
    )

    device = NeutralAtomDevice()
    compiled = device.compile(qir)
    result = device.simulate(compiled, type="clifford")

    assert result == [[qsharp.Result.Zero, qsharp.Result.Zero]] or result == [
        [qsharp.Result.One, qsharp.Result.One]
    ]


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_device_simulate_with_loss() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qir = qsharp.compile(
        """
        {
            use qs = Qubit[2];
            H(qs[0]);
            CNOT(qs[0], qs[1]);
            MResetEachZ(qs)
        }
        """
    )

    device = NeutralAtomDevice()
    noise = NoiseConfig()
    noise.mov.loss = 1.0  # Ensure loss occurs
    result = device.simulate(qir, noise=noise, type="cpu")
    result2 = device.simulate(qir, noise=noise, type="clifford")

    assert result == [[qsharp.Result.Loss, qsharp.Result.Loss]]
    assert result2 == [[qsharp.Result.Loss, qsharp.Result.Loss]]
