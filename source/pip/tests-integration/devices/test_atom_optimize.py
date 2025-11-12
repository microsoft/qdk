# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest
from expecttest import assert_expected_inline

import qsharp
from qsharp._device._atom._optimize import (
    PruneUnusedFunctions,
    OptimizeSingleQubitGates,
)

try:
    import pyqir

    PYQIR_AVAILABLE = True
except ImportError:
    PYQIR_AVAILABLE = False

SKIP_REASON = "PyQIR is not available"


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_prune_init_handled_by_unused_functions_pass() -> None:
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
    PruneUnusedFunctions().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

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
def test_optimize_removes_h_h_gates() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            H(q);
            H(q);
            X(q);
            H(q);
            H(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

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
def test_optimize_removes_s_sadj_gates() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            S(q);
            Adjoint S(q);
            X(q);
            Adjoint S(q);
            S(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

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
def test_optimize_removes_t_tadj_gates() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            T(q);
            Adjoint T(q);
            X(q);
            Adjoint T(q);
            T(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__t__body(%Qubit*)

declare void @__quantum__qis__t__adj(%Qubit*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

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
def test_optimize_combines_h_s_h_gates() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            H(q);
            S(q);
            H(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__sx__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

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
def test_optimize_removes_x_x_gates() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            X(q);
            X(q);
            Z(q);
            X(q);
            X(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__z__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__z__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

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
def test_optimize_removes_y_y_gates() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            Y(q);
            Y(q);
            Z(q);
            Y(q);
            Y(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__z__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__y__body(%Qubit*)

declare void @__quantum__qis__z__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

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
def test_optimize_removes_z_z_gates() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            Z(q);
            Z(q);
            X(q);
            Z(q);
            Z(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__z__body(%Qubit*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

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
def test_optimize_combines_rx_rotation_angles() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            import Std.Math.PI;
            use q = Qubit();
            Rx(PI() / 2.0, q);
            Rx(PI() / 2.0, q);
            X(q);
            Rx(PI() / -2.0, q);
            Rx(PI() / 2.0, q);
            Y(q);
            Rx(PI(), q);
            Rx(PI(), q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__rx__body(double 0x400921FB54442D18, %Qubit* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__y__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__rx__body(double, %Qubit*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__y__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

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
def test_optimize_combines_ry_rotation_angles() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            import Std.Math.PI;
            use q = Qubit();
            Ry(PI() / 2.0, q);
            Ry(PI() / 2.0, q);
            X(q);
            Ry(PI() / -2.0, q);
            Ry(PI() / 2.0, q);
            Y(q);
            Ry(PI(), q);
            Ry(PI(), q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__ry__body(double 0x400921FB54442D18, %Qubit* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__y__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__ry__body(double, %Qubit*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__y__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

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
def test_optimize_combines_rz_rotation_angles() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            import Std.Math.PI;
            use q = Qubit();
            Rz(PI() / 2.0, q);
            Rz(PI() / 2.0, q);
            X(q);
            Rz(PI() / -2.0, q);
            Rz(PI() / 2.0, q);
            Y(q);
            Rz(PI(), q);
            Rz(PI(), q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__rz__body(double 0x400921FB54442D18, %Qubit* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__y__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__y__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

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
def test_optimize_removes_adjoint_gates_after_removing_other_adjoint_gates() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            S(q);
            X(q);
            H(q);

            H(q);
            X(q);
            Adjoint S(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

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
def test_optimize_leaves_gates_with_intervening_gates() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            H(q);
            S(q);
            Adjoint S(q);
            X(q);
            H(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

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
def test_optimize_treats_rxx_as_barrier() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q1 = Qubit();
            use q2 = Qubit();
            X(q1);
            Rxx(0.5, q1, q2);
            X(q1);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__rxx__body(double 5.000000e-01, %Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__rxx__body(double, %Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

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
def test_optimize_treats_ryy_as_barrier() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q1 = Qubit();
            use q2 = Qubit();
            X(q1);
            Ryy(0.5, q1, q2);
            X(q1);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__ryy__body(double 5.000000e-01, %Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__ryy__body(double, %Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

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
def test_optimize_treats_rzz_as_barrier() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q1 = Qubit();
            use q2 = Qubit();
            X(q1);
            Rzz(0.5, q1, q2);
            X(q1);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__rzz__body(double 5.000000e-01, %Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__rzz__body(double, %Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

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
def test_optimize_treats_ccx_as_barrier() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q1 = Qubit();
            use q2 = Qubit();
            use q3 = Qubit();
            X(q1);
            CCNOT(q1, q2, q3);
            X(q1);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__ccx__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__ccx__body(%Qubit*, %Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

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
def test_optimize_treats_cx_as_barrier() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q1 = Qubit();
            use q2 = Qubit();
            X(q1);
            CX(q1, q2);
            X(q1);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__cx__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

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
def test_optimize_treats_cy_as_barrier() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q1 = Qubit();
            use q2 = Qubit();
            X(q1);
            CY(q1, q2);
            X(q1);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__cy__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__cy__body(%Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

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
def test_optimize_treats_cz_as_barrier() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q1 = Qubit();
            use q2 = Qubit();
            X(q1);
            CZ(q1, q2);
            X(q1);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__cz__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

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
def test_optimize_treats_swap_as_barrier() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q1 = Qubit();
            use q2 = Qubit();
            X(q1);
            SWAP(q1, q2);
            X(q1);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__swap__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__swap__body(%Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

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
def test_optimize_treats_m_as_barrier() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            X(q);
            M(q);
            X(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__m__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_optimize_treats_mresetz_as_barrier() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            X(q);
            MResetZ(q);
            X(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__mresetz__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_optimize_treats_reset_as_barrier() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            X(q);
            Reset(q);
            X(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__reset__body(%Qubit* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__reset__body(%Qubit*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

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


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_optimize_works_within_blocks_not_across_blocks() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            X(q);
            if MResetZ(q) == One {
                H(q);
                H(q);
                X(q);
            } else {
                X(q);
                Z(q);
                Z(q);
                Y(q);
                X(q);
            }
            X(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__mresetz__body(%Qubit* null, %Result* null)
  %var_0 = call i1 @__quantum__rt__read_result(%Result* null)
  br i1 %var_0, label %block_1, label %block_2

block_1:                                          ; preds = %block_0
  call void @__quantum__qis__x__body(%Qubit* null)
  br label %block_3

block_2:                                          ; preds = %block_0
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__y__body(%Qubit* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  br label %block_3

block_3:                                          ; preds = %block_2, %block_1
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

declare i1 @__quantum__rt__read_result(%Result*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__z__body(%Qubit*)

declare void @__quantum__qis__y__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_optimize_combines_m_and_reset_into_mresetz() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            X(q);
            M(q);
            Reset(q);
            X(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__mresetz__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

declare void @__quantum__qis__reset__body(%Qubit*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_optimize_removes_mresetz_and_reset_into_mresetz() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            X(q);
            MResetZ(q);
            Reset(q);
            X(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__mresetz__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

declare void @__quantum__qis__reset__body(%Qubit*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_optimize_removes_reset_of_unused_qubits() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q1 = Qubit();
            use q2 = Qubit();
            X(q1);
            Reset(q1);
            Reset(q2);
            X(q1);
            X(q2);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__reset__body(%Qubit* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__reset__body(%Qubit*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }
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


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_optimize_turns_final_m_into_mresetz() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            X(q);
            M(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__mresetz__body(%Qubit* null, %Result* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_optimize_removes_reset_after_reset() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            X(q);
            Reset(q);
            Reset(q);
            Y(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__reset__body(%Qubit* null)
  call void @__quantum__qis__y__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__reset__body(%Qubit*) #1

declare void @__quantum__qis__y__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

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


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_optimize_removes_final_reset() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            X(q);
            Reset(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    OptimizeSingleQubitGates().run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__reset__body(%Qubit*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

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
