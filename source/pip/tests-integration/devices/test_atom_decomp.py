# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest
from expecttest import assert_expected_inline

import qsharp
from qsharp._device._atom._decomp import (
    DecomposeMultiQubitToCZ,
    DecomposeSingleRotationToRz,
    DecomposeSingleQubitToRzSX,
    DecomposeRzAnglesToCliffordGates,
    ReplaceResetWithMResetZ,
)

try:
    import pyqir

    PYQIR_AVAILABLE = True
except ImportError:
    PYQIR_AVAILABLE = False

SKIP_REASON = "PyQIR is not available"


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_ccx_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use (q1, q2, q3) = (Qubit(), Qubit(), Qubit());
            CCNOT(q1, q2, q3);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeMultiQubitToCZ().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__t__adj(%Qubit* null)
  call void @__quantum__qis__t__adj(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__t__body(%Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__t__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__t__adj(%Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__t__adj(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__t__body(%Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__ccx__body(%Qubit*, %Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__t__body(%Qubit*)

declare void @__quantum__qis__t__adj(%Qubit*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_cx_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use (q1, q2) = (Qubit(), Qubit());
            CNOT(q1, q2);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeMultiQubitToCZ().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__t__body(%Qubit*)

declare void @__quantum__qis__t__adj(%Qubit*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_cy_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use (q1, q2) = (Qubit(), Qubit());
            CY(q1, q2);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeMultiQubitToCZ().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__s__adj(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__s__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__cy__body(%Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__t__body(%Qubit*)

declare void @__quantum__qis__t__adj(%Qubit*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_rxx_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use (q1, q2) = (Qubit(), Qubit());
            Rxx(1.2345, q1, q2);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeMultiQubitToCZ().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__rz__body(double 1.234500e+00, %Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__rxx__body(double, %Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__t__body(%Qubit*)

declare void @__quantum__qis__t__adj(%Qubit*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_ryy_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use (q1, q2) = (Qubit(), Qubit());
            Ryy(1.2345, q1, q2);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeMultiQubitToCZ().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__s__adj(%Qubit* null)
  call void @__quantum__qis__s__adj(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__rz__body(double 1.234500e+00, %Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__s__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__s__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__ryy__body(double, %Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__t__body(%Qubit*)

declare void @__quantum__qis__t__adj(%Qubit*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_rzz_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use (q1, q2) = (Qubit(), Qubit());
            Rzz(1.2345, q1, q2);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeMultiQubitToCZ().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__rz__body(double 1.234500e+00, %Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__rzz__body(double, %Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__t__body(%Qubit*)

declare void @__quantum__qis__t__adj(%Qubit*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_swap_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use (q1, q2) = (Qubit(), Qubit());
            SWAP(q1, q2);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeMultiQubitToCZ().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__swap__body(%Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__t__body(%Qubit*)

declare void @__quantum__qis__t__adj(%Qubit*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_rx_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            Rx(1.2345, q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeSingleRotationToRz().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__rz__body(double 1.234500e+00, %Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__rx__body(double, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_ry_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            Ry(1.2345, q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeSingleRotationToRz().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__s__adj(%Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__rz__body(double 1.234500e+00, %Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__s__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__ry__body(double, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_h_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            H(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeSingleQubitToRzSX().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__rz__body(double 0x3FF921FB54442D18, %Qubit* null)
  call void @__quantum__qis__sx__body(%Qubit* null)
  call void @__quantum__qis__rz__body(double 0x3FF921FB54442D18, %Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_s_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            S(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeSingleQubitToRzSX().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__rz__body(double 0x3FF921FB54442D18, %Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_sadj_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            Adjoint S(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeSingleQubitToRzSX().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__rz__body(double 0xBFF921FB54442D18, %Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_t_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            T(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeSingleQubitToRzSX().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__rz__body(double 0x3FE921FB54442D18, %Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__t__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_tadj_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            Adjoint T(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeSingleQubitToRzSX().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__rz__body(double 0xBFE921FB54442D18, %Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__t__adj(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_x_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            X(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeSingleQubitToRzSX().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__sx__body(%Qubit* null)
  call void @__quantum__qis__sx__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_y_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            Y(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeSingleQubitToRzSX().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__sx__body(%Qubit* null)
  call void @__quantum__qis__sx__body(%Qubit* null)
  call void @__quantum__qis__rz__body(double 0x400921FB54442D18, %Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__y__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_z_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            Z(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeSingleQubitToRzSX().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__rz__body(double 0x400921FB54442D18, %Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__z__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_rz_3pi_over_2_clifford_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            Rz(3.0 * Std.Math.PI() / 2.0, q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeRzAnglesToCliffordGates().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__s__adj(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__z__body(%Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_rz_neg_pi_over_2_clifford_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            Rz(-1.0 * Std.Math.PI() / 2.0, q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeRzAnglesToCliffordGates().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__s__adj(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__z__body(%Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_rz_pi_clifford_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            Rz(Std.Math.PI(), q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeRzAnglesToCliffordGates().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__z__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__z__body(%Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_rz_neg_pi_clifford_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            Rz(-1.0 * Std.Math.PI(), q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeRzAnglesToCliffordGates().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__z__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__z__body(%Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_rz_pi_over_2_clifford_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            Rz(Std.Math.PI() / 2.0, q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeRzAnglesToCliffordGates().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__s__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__z__body(%Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_rz_neg_3pi_over_2_clifford_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            Rz(-3.0 * Std.Math.PI() / 2.0, q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeRzAnglesToCliffordGates().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__s__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__z__body(%Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_rz_2pi_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            Rz(2.0 * Std.Math.PI(), q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeRzAnglesToCliffordGates().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__z__body(%Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_rz_neg_2pi_decomposition() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            Rz(-2.0 * Std.Math.PI(), q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    DecomposeRzAnglesToCliffordGates().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__z__body(%Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_rz_non_clifford_decomposition_fails() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            Rz(0.1, q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))

    with pytest.raises(ValueError) as ex:
        DecomposeRzAnglesToCliffordGates().run(module)

    assert_expected_inline(
        str(ex),
        """<ExceptionInfo ValueError('Angle 0.1 used in RZ is not a Clifford compatible rotation angle') tblen=9>""",
    )


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_reset_replaced_by_mresetz() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            Reset(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    ReplaceResetWithMResetZ().run(module)
    transformed_qir = str(module)

    assert_expected_inline(
        transformed_qir,
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__mresetz__body(%Qubit* null, %Result* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__reset__body(%Qubit*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
attributes #1 = { "irreversible" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !6}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !5}
!5 = !{!"i64"}
!6 = !{i32 5, !"float_computations", !7}
!7 = !{!"double"}
""",
    )
