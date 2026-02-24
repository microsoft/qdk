# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest
from expecttest import assert_expected_inline

import qsharp
from qsharp._device._atom._reorder import Reorder
from qsharp._device._atom import NeutralAtomDevice
from .validation import PerQubitOrdering, check_qubit_ordering_unchanged

try:
    import pyqir

    PYQIR_AVAILABLE = True
except ImportError:
    PYQIR_AVAILABLE = False

SKIP_REASON = "PyQIR is not available"


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_reorder_no_changes_to_simple_ordered_program() -> None:
    qir = qsharp.compile(
        """
        {
            use (q1, q2) = (Qubit(), Qubit());
            SX(q1);
            CZ(q1, q2);
            (MResetZ(q1), MResetZ(q2))
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    before = PerQubitOrdering()
    before.run(module)
    Reorder(NeutralAtomDevice()).run(module)
    after = PerQubitOrdering()
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@0 = internal constant [4 x i8] c"0_t\\00"
@1 = internal constant [6 x i8] c"1_t0r\\00"
@2 = internal constant [6 x i8] c"2_t1r\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__sx__body(%Qubit* null)
  call void @__quantum__qis__cz__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* null, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__result_record_output(%Result*, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
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
def test_reorder_groups_matching_sequential_gates_into_same_step() -> None:
    qir = qsharp.compile(
        """
        {
            use (q1, q2, q3, q4) = (Qubit(), Qubit(), Qubit(), Qubit());
            SX(q1);
            CZ(q1, q2);
            let (r1, r2) = (MResetZ(q1), MResetZ(q2));
            SX(q3);
            CZ(q3, q4);
            let (r3, r4) = (MResetZ(q3), MResetZ(q4));
            (r1, r2, r3, r4)
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    before = PerQubitOrdering()
    before.run(module)
    Reorder(NeutralAtomDevice()).run(module)
    after = PerQubitOrdering()
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@0 = internal constant [4 x i8] c"0_t\\00"
@1 = internal constant [6 x i8] c"1_t0r\\00"
@2 = internal constant [6 x i8] c"2_t1r\\00"
@3 = internal constant [6 x i8] c"3_t2r\\00"
@4 = internal constant [6 x i8] c"4_t3r\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__sx__body(%Qubit* null)
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
  call void @__quantum__rt__tuple_record_output(i64 4, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* null, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 3 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @4, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__result_record_output(%Result*, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="4" }
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
def test_reorder_moves_gates_past_measurements_that_overlap_qubit_and_result_ids() -> (
    None
):
    qir = qsharp.compile(
        """
        {
            use (q1, q2) = (Qubit(), Qubit());
            SX(q2);
            let r2 = MResetZ(q2);
            SX(q1);
            let r1 = MResetZ(q1);
            (r1, r2)
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    before = PerQubitOrdering()
    before.run(module)
    Reorder(NeutralAtomDevice()).run(module)
    after = PerQubitOrdering()
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@0 = internal constant [4 x i8] c"0_t\\00"
@1 = internal constant [6 x i8] c"1_t0r\\00"
@2 = internal constant [6 x i8] c"2_t1r\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__sx__body(%Qubit* null)
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* null, %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* null)
  call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* null, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__result_record_output(%Result*, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
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
def test_reorder_keeps_dependent_gates_in_order() -> None:
    qir = qsharp.compile(
        """
        {
            use (q1, q2) = (Qubit(), Qubit());
            SX(q1);
            CZ(q1, q2);
            SX(q1);
            CZ(q1, q2);
            (MResetZ(q1), MResetZ(q2))
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    before = PerQubitOrdering()
    before.run(module)
    Reorder(NeutralAtomDevice()).run(module)
    after = PerQubitOrdering()
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@0 = internal constant [4 x i8] c"0_t\\00"
@1 = internal constant [6 x i8] c"1_t0r\\00"
@2 = internal constant [6 x i8] c"2_t1r\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__sx__body(%Qubit* null)
  call void @__quantum__qis__cz__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* null)
  call void @__quantum__qis__cz__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* null, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__result_record_output(%Result*, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
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
def test_reorder_sorts_gates_by_qubit_id() -> None:
    qir = qsharp.compile(
        """
        {
            use qs = Qubit[5];
            SX(qs[3]);
            SX(qs[1]);
            SX(qs[4]);
            SX(qs[0]);
            SX(qs[2]);
            let r3 = MResetZ(qs[3]);
            let r1 = MResetZ(qs[1]);
            let r4 = MResetZ(qs[4]);
            let r0 = MResetZ(qs[0]);
            let r2 = MResetZ(qs[2]);
            [r0, r1, r2, r3, r4]
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    before = PerQubitOrdering()
    before.run(module)
    Reorder(NeutralAtomDevice()).run(module)
    after = PerQubitOrdering()
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@0 = internal constant [4 x i8] c"0_a\\00"
@1 = internal constant [6 x i8] c"1_a0r\\00"
@2 = internal constant [6 x i8] c"2_a1r\\00"
@3 = internal constant [6 x i8] c"3_a2r\\00"
@4 = internal constant [6 x i8] c"4_a3r\\00"
@5 = internal constant [6 x i8] c"5_a4r\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__sx__body(%Qubit* null)
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 4 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* null, %Result* inttoptr (i64 3 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* null)
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  call void @__quantum__rt__array_record_output(i64 5, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 3 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* null, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @4, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @5, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

declare void @__quantum__rt__array_record_output(i64, i8*)

declare void @__quantum__rt__result_record_output(%Result*, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="5" }
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
def test_reorder_sorts_cz_to_end_of_step() -> None:
    qir = qsharp.compile(
        """
        {
            use qs = Qubit[4];
            SX(qs[3]);
            CZ(qs[1], qs[0]);
            SX(qs[2]);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    before = PerQubitOrdering()
    before.run(module)
    Reorder(NeutralAtomDevice()).run(module)
    after = PerQubitOrdering()
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque

@0 = internal constant [4 x i8] c"0_t\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }

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
def test_reorder_respects_read_result_and_classical_compute() -> None:
    qir = qsharp.compile(
        """
        {
            use (q1, q2) = (Qubit(), Qubit());
            SX(q1);
            let r1 = MResetZ(q1);
            let r2 = MResetZ(q2);
            let angle = if r1 == One {
                if r2 == One {
                    SX(q1);
                    0.0
                } else {
                    Rz(1.0, q1);
                    1.0
                }
            } else {
                if r2 == One {
                    Rz(2.0, q1);
                    2.0
                } else {
                    Rz(3.0, q1);
                    3.0
                }
            };
            Rz(2.0 * angle, q2);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    Reorder(NeutralAtomDevice()).run(module)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@0 = internal constant [4 x i8] c"0_t\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__sx__body(%Qubit* null)
  call void @__quantum__qis__mresetz__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  %0 = call i1 @__quantum__rt__read_result(%Result* null)
  br i1 %0, label %block_1, label %block_2

block_1:                                          ; preds = %block_0
  %1 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
  br i1 %1, label %block_3, label %block_4

block_2:                                          ; preds = %block_0
  %2 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
  br i1 %2, label %block_5, label %block_6

block_3:                                          ; preds = %block_1
  call void @__quantum__qis__sx__body(%Qubit* null)
  br label %block_7

block_4:                                          ; preds = %block_1
  call void @__quantum__qis__rz__body(double 1.000000e+00, %Qubit* null)
  br label %block_7

block_5:                                          ; preds = %block_2
  call void @__quantum__qis__rz__body(double 2.000000e+00, %Qubit* null)
  br label %block_8

block_6:                                          ; preds = %block_2
  call void @__quantum__qis__rz__body(double 3.000000e+00, %Qubit* null)
  br label %block_8

block_7:                                          ; preds = %block_4, %block_3
  %3 = phi double [ 0.000000e+00, %block_3 ], [ 1.000000e+00, %block_4 ]
  br label %block_9

block_8:                                          ; preds = %block_6, %block_5
  %4 = phi double [ 2.000000e+00, %block_5 ], [ 3.000000e+00, %block_6 ]
  br label %block_9

block_9:                                          ; preds = %block_8, %block_7
  %5 = phi double [ %3, %block_7 ], [ %4, %block_8 ]
  %6 = fmul double 2.000000e+00, %5
  call void @__quantum__qis__rz__body(double %6, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

declare i1 @__quantum__rt__read_result(%Result*)

declare void @__quantum__qis__rz__body(double, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
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
def test_reorder_preserves_per_qubit_order_on_large_program() -> None:
    qir = qsharp.compile(
        """
        {
            import Std.Math.PI;
            operation IsingModel2DEvolution(
                N1 : Int,
                N2 : Int,
                J : Double,
                g : Double,
                evolutionTime : Double,
                numberOfSteps : Int
            ) : Result[] {
                use qubits = Qubit[N1 * N2];
                let qubitsAs2D = Std.Arrays.Chunks(N2, qubits);
                let dt : Double = evolutionTime / Std.Convert.IntAsDouble(numberOfSteps);
                let theta_x = - g * dt;
                let theta_zz = J * dt;
                for i in 1..numberOfSteps {
                    for q in qubits {
                        Rx(2.0 * theta_x, q);
                    }
                    for row in 0..N1-1 {
                        for col in 0..2..N2-2 {
                            Rzz(2.0 * theta_zz, qubitsAs2D[row][col], qubitsAs2D[row][col + 1]);
                        }
                        for col in 1..2..N2-2 {
                            Rzz(2.0 * theta_zz, qubitsAs2D[row][col], qubitsAs2D[row][col + 1]);
                        }
                    }
                    for col in 0..N2-1 {
                        for row in 0..2..N1-2 {
                            Rzz(2.0 * theta_zz, qubitsAs2D[row][col], qubitsAs2D[row + 1][col]);
                        }
                        for row in 1..2..N1-2 {
                            Rzz(2.0 * theta_zz, qubitsAs2D[row][col], qubitsAs2D[row + 1][col]);
                        }
                    }
                }
                MResetEachZ(qubits)
            }
            IsingModel2DEvolution(
                10,
                10,
                PI() / 2.0,
                PI() / 2.0,
                10.0,
                10
            )
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    before = PerQubitOrdering()
    before.run(module)
    Reorder(NeutralAtomDevice()).run(module)
    after = PerQubitOrdering()
    after.run(module)
    check_qubit_ordering_unchanged(after, before)
