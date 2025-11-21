# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest
from expecttest import assert_expected_inline

import qsharp
from qsharp._device._atom import AC1000
from qsharp._device._atom._scheduler import Schedule
from .validation import (
    ValidateBeginEndParallel,
    PerQubitOrdering,
    check_qubit_ordering_unchanged,
)

try:
    import pyqir

    PYQIR_AVAILABLE = True
except ImportError:
    PYQIR_AVAILABLE = False

SKIP_REASON = "PyQIR is not available"

qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)


@pytest.mark.skipif(not PYQIR_AVAILABLE, reason=SKIP_REASON)
def test_scheduler_inserts_move_to_iz_for_single_qubit_gates():
    qir = qsharp.compile(
        """
        {
            use q = Qubit();
            SX(q);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    before = PerQubitOrdering()
    before.run(module)
    Schedule(AC1000()).run(module)
    ValidateBeginEndParallel().run(module)
    after = PerQubitOrdering()
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* null, i64 17, i64 0)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__sx__body(%Qubit* null)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* null, i64 21, i64 0)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__begin_parallel()

declare void @__quantum__rt__end_parallel()

declare void @__quantum__qis__move1__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move2__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move3__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move4__body(%Qubit*, i64, i64)

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
def test_scheduler_inserts_move_to_iz_for_two_qubit_gates():
    qir = qsharp.compile(
        """
        {
            use q = Qubit[2];
            CZ(q[0], q[1]);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    before = PerQubitOrdering()
    before.run(module)
    Schedule(AC1000()).run(module)
    ValidateBeginEndParallel().run(module)
    after = PerQubitOrdering()
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* null, i64 17, i64 0)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 1 to %Qubit*), i64 17, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__cz__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* null, i64 21, i64 0)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 1 to %Qubit*), i64 21, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__begin_parallel()

declare void @__quantum__rt__end_parallel()

declare void @__quantum__qis__move1__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move2__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move3__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move4__body(%Qubit*, i64, i64)

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
def test_scheduler_inserts_move_to_iz_for_mixed_gates_with_1q_gates_first():
    qir = qsharp.compile(
        """
        {
            use q = Qubit[2];
            SX(q[0]);
            CZ(q[0], q[1]);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    before = PerQubitOrdering()
    before.run(module)
    Schedule(AC1000()).run(module)
    ValidateBeginEndParallel().run(module)
    after = PerQubitOrdering()
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* null, i64 17, i64 0)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 1 to %Qubit*), i64 17, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__sx__body(%Qubit* null)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__cz__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* null, i64 21, i64 0)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 1 to %Qubit*), i64 21, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__begin_parallel()

declare void @__quantum__rt__end_parallel()

declare void @__quantum__qis__move1__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move2__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move3__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move4__body(%Qubit*, i64, i64)

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
def test_scheduler_parallelizes_operations_when_possible():
    qir = qsharp.compile(
        """
        {
            use q = Qubit[4];
            SX(q[0]);
            SX(q[2]);
            CZ(q[0], q[1]);
            CZ(q[2], q[3]);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    before = PerQubitOrdering()
    before.run(module)
    Schedule(AC1000()).run(module)
    after = PerQubitOrdering()
    ValidateBeginEndParallel().run(module)
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* null, i64 17, i64 0)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 2 to %Qubit*), i64 17, i64 2)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 1 to %Qubit*), i64 17, i64 1)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 3 to %Qubit*), i64 17, i64 3)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__sx__body(%Qubit* null)
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__cz__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* null, i64 21, i64 0)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 2 to %Qubit*), i64 21, i64 2)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 1 to %Qubit*), i64 21, i64 1)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 3 to %Qubit*), i64 21, i64 3)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__begin_parallel()

declare void @__quantum__rt__end_parallel()

declare void @__quantum__qis__move1__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move2__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move3__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move4__body(%Qubit*, i64, i64)

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
def test_scheduler_inserts_moves_to_mz_for_measurement():
    qir = qsharp.compile(
        """
        {
            use q = Qubit[2];
            MResetZ(q[0]);
            MResetZ(q[1]);
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    before = PerQubitOrdering()
    before.run(module)
    Schedule(AC1000()).run(module)
    after = PerQubitOrdering()
    ValidateBeginEndParallel().run(module)
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* null, i64 38, i64 0)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 1 to %Qubit*), i64 38, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__mresetz__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* null, i64 21, i64 0)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 1 to %Qubit*), i64 21, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__begin_parallel()

declare void @__quantum__rt__end_parallel()

declare void @__quantum__qis__move1__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move2__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move3__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move4__body(%Qubit*, i64, i64)

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
def test_scheduler_parallelizes_1q_gates_by_iz_row():
    device = AC1000()
    num_qubits = device.column_count * 2
    qir = qsharp.compile(
        f"""
        {{
            use qs = Qubit[{num_qubits}];
            ApplyToEach(SX, qs);
        }}
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    before = PerQubitOrdering()
    before.run(module)
    Schedule(device).run(module)
    after = PerQubitOrdering()
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* null, i64 17, i64 0)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 2 to %Qubit*), i64 17, i64 2)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 4 to %Qubit*), i64 17, i64 4)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 6 to %Qubit*), i64 17, i64 6)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 8 to %Qubit*), i64 17, i64 8)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 10 to %Qubit*), i64 17, i64 10)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 12 to %Qubit*), i64 17, i64 12)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 14 to %Qubit*), i64 17, i64 14)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 16 to %Qubit*), i64 17, i64 16)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 18 to %Qubit*), i64 17, i64 18)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 20 to %Qubit*), i64 17, i64 20)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 22 to %Qubit*), i64 17, i64 22)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 24 to %Qubit*), i64 17, i64 24)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 26 to %Qubit*), i64 17, i64 26)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 28 to %Qubit*), i64 17, i64 28)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 30 to %Qubit*), i64 17, i64 30)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 32 to %Qubit*), i64 17, i64 32)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 34 to %Qubit*), i64 17, i64 34)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 36 to %Qubit*), i64 18, i64 0)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 38 to %Qubit*), i64 18, i64 2)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 40 to %Qubit*), i64 18, i64 4)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 42 to %Qubit*), i64 18, i64 6)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 44 to %Qubit*), i64 18, i64 8)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 46 to %Qubit*), i64 18, i64 10)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 48 to %Qubit*), i64 18, i64 12)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 50 to %Qubit*), i64 18, i64 14)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 52 to %Qubit*), i64 18, i64 16)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 54 to %Qubit*), i64 18, i64 18)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 56 to %Qubit*), i64 18, i64 20)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 58 to %Qubit*), i64 18, i64 22)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 60 to %Qubit*), i64 18, i64 24)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 62 to %Qubit*), i64 18, i64 26)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 64 to %Qubit*), i64 18, i64 28)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 66 to %Qubit*), i64 18, i64 30)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 68 to %Qubit*), i64 18, i64 32)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 70 to %Qubit*), i64 18, i64 34)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 1 to %Qubit*), i64 17, i64 1)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 3 to %Qubit*), i64 17, i64 3)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 5 to %Qubit*), i64 17, i64 5)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 7 to %Qubit*), i64 17, i64 7)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 9 to %Qubit*), i64 17, i64 9)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 11 to %Qubit*), i64 17, i64 11)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 13 to %Qubit*), i64 17, i64 13)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 15 to %Qubit*), i64 17, i64 15)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 17 to %Qubit*), i64 17, i64 17)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 19 to %Qubit*), i64 17, i64 19)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 21 to %Qubit*), i64 17, i64 21)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 23 to %Qubit*), i64 17, i64 23)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 25 to %Qubit*), i64 17, i64 25)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 27 to %Qubit*), i64 17, i64 27)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 29 to %Qubit*), i64 17, i64 29)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 31 to %Qubit*), i64 17, i64 31)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 33 to %Qubit*), i64 17, i64 33)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 35 to %Qubit*), i64 17, i64 35)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 37 to %Qubit*), i64 18, i64 1)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 39 to %Qubit*), i64 18, i64 3)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 41 to %Qubit*), i64 18, i64 5)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 43 to %Qubit*), i64 18, i64 7)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 45 to %Qubit*), i64 18, i64 9)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 47 to %Qubit*), i64 18, i64 11)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 49 to %Qubit*), i64 18, i64 13)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 51 to %Qubit*), i64 18, i64 15)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 53 to %Qubit*), i64 18, i64 17)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 55 to %Qubit*), i64 18, i64 19)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 57 to %Qubit*), i64 18, i64 21)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 59 to %Qubit*), i64 18, i64 23)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 61 to %Qubit*), i64 18, i64 25)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 63 to %Qubit*), i64 18, i64 27)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 65 to %Qubit*), i64 18, i64 29)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 67 to %Qubit*), i64 18, i64 31)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 69 to %Qubit*), i64 18, i64 33)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 71 to %Qubit*), i64 18, i64 35)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__sx__body(%Qubit* null)
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 4 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 5 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 6 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 7 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 8 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 9 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 10 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 11 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 12 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 13 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 14 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 15 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 16 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 17 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 18 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 19 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 20 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 21 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 22 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 23 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 24 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 25 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 26 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 27 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 28 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 29 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 30 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 31 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 32 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 33 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 34 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 35 to %Qubit*))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 36 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 37 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 38 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 39 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 40 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 41 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 42 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 43 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 44 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 45 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 46 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 47 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 48 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 49 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 50 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 51 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 52 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 53 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 54 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 55 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 56 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 57 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 58 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 59 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 60 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 61 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 62 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 63 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 64 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 65 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 66 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 67 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 68 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 69 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 70 to %Qubit*))
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 71 to %Qubit*))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* null, i64 21, i64 0)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 2 to %Qubit*), i64 21, i64 2)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 4 to %Qubit*), i64 21, i64 4)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 6 to %Qubit*), i64 21, i64 6)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 8 to %Qubit*), i64 21, i64 8)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 10 to %Qubit*), i64 21, i64 10)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 12 to %Qubit*), i64 21, i64 12)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 14 to %Qubit*), i64 21, i64 14)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 16 to %Qubit*), i64 21, i64 16)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 18 to %Qubit*), i64 21, i64 18)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 20 to %Qubit*), i64 21, i64 20)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 22 to %Qubit*), i64 21, i64 22)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 24 to %Qubit*), i64 21, i64 24)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 26 to %Qubit*), i64 21, i64 26)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 28 to %Qubit*), i64 21, i64 28)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 30 to %Qubit*), i64 21, i64 30)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 32 to %Qubit*), i64 21, i64 32)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 34 to %Qubit*), i64 21, i64 34)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 36 to %Qubit*), i64 22, i64 0)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 38 to %Qubit*), i64 22, i64 2)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 40 to %Qubit*), i64 22, i64 4)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 42 to %Qubit*), i64 22, i64 6)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 44 to %Qubit*), i64 22, i64 8)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 46 to %Qubit*), i64 22, i64 10)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 48 to %Qubit*), i64 22, i64 12)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 50 to %Qubit*), i64 22, i64 14)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 52 to %Qubit*), i64 22, i64 16)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 54 to %Qubit*), i64 22, i64 18)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 56 to %Qubit*), i64 22, i64 20)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 58 to %Qubit*), i64 22, i64 22)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 60 to %Qubit*), i64 22, i64 24)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 62 to %Qubit*), i64 22, i64 26)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 64 to %Qubit*), i64 22, i64 28)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 66 to %Qubit*), i64 22, i64 30)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 68 to %Qubit*), i64 22, i64 32)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 70 to %Qubit*), i64 22, i64 34)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 1 to %Qubit*), i64 21, i64 1)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 3 to %Qubit*), i64 21, i64 3)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 5 to %Qubit*), i64 21, i64 5)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 7 to %Qubit*), i64 21, i64 7)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 9 to %Qubit*), i64 21, i64 9)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 11 to %Qubit*), i64 21, i64 11)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 13 to %Qubit*), i64 21, i64 13)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 15 to %Qubit*), i64 21, i64 15)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 17 to %Qubit*), i64 21, i64 17)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 19 to %Qubit*), i64 21, i64 19)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 21 to %Qubit*), i64 21, i64 21)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 23 to %Qubit*), i64 21, i64 23)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 25 to %Qubit*), i64 21, i64 25)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 27 to %Qubit*), i64 21, i64 27)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 29 to %Qubit*), i64 21, i64 29)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 31 to %Qubit*), i64 21, i64 31)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 33 to %Qubit*), i64 21, i64 33)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 35 to %Qubit*), i64 21, i64 35)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 37 to %Qubit*), i64 22, i64 1)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 39 to %Qubit*), i64 22, i64 3)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 41 to %Qubit*), i64 22, i64 5)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 43 to %Qubit*), i64 22, i64 7)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 45 to %Qubit*), i64 22, i64 9)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 47 to %Qubit*), i64 22, i64 11)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 49 to %Qubit*), i64 22, i64 13)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 51 to %Qubit*), i64 22, i64 15)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 53 to %Qubit*), i64 22, i64 17)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 55 to %Qubit*), i64 22, i64 19)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 57 to %Qubit*), i64 22, i64 21)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 59 to %Qubit*), i64 22, i64 23)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 61 to %Qubit*), i64 22, i64 25)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 63 to %Qubit*), i64 22, i64 27)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 65 to %Qubit*), i64 22, i64 29)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 67 to %Qubit*), i64 22, i64 31)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 69 to %Qubit*), i64 22, i64 33)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 71 to %Qubit*), i64 22, i64 35)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__begin_parallel()

declare void @__quantum__rt__end_parallel()

declare void @__quantum__qis__move1__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move2__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move3__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move4__body(%Qubit*, i64, i64)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="72" "required_num_results"="0" }

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
def test_scheduler_parallelizes_all_2q_in_iz():
    device = AC1000()
    num_qubits = device.column_count * 2
    qir = qsharp.compile(
        f"""
        {{
            use qs = Qubit[{num_qubits}];
            for i in 0..2..(Length(qs)-2) {{
                CZ(qs[i], qs[i+1]);
            }}
        }}
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    before = PerQubitOrdering()
    before.run(module)
    Schedule(device).run(module)
    after = PerQubitOrdering()
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* null, i64 17, i64 0)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 2 to %Qubit*), i64 17, i64 2)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 4 to %Qubit*), i64 17, i64 4)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 6 to %Qubit*), i64 17, i64 6)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 8 to %Qubit*), i64 17, i64 8)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 10 to %Qubit*), i64 17, i64 10)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 12 to %Qubit*), i64 17, i64 12)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 14 to %Qubit*), i64 17, i64 14)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 16 to %Qubit*), i64 17, i64 16)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 18 to %Qubit*), i64 17, i64 18)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 20 to %Qubit*), i64 17, i64 20)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 22 to %Qubit*), i64 17, i64 22)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 24 to %Qubit*), i64 17, i64 24)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 26 to %Qubit*), i64 17, i64 26)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 28 to %Qubit*), i64 17, i64 28)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 30 to %Qubit*), i64 17, i64 30)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 32 to %Qubit*), i64 17, i64 32)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 34 to %Qubit*), i64 17, i64 34)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 36 to %Qubit*), i64 18, i64 0)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 38 to %Qubit*), i64 18, i64 2)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 40 to %Qubit*), i64 18, i64 4)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 42 to %Qubit*), i64 18, i64 6)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 44 to %Qubit*), i64 18, i64 8)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 46 to %Qubit*), i64 18, i64 10)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 48 to %Qubit*), i64 18, i64 12)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 50 to %Qubit*), i64 18, i64 14)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 52 to %Qubit*), i64 18, i64 16)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 54 to %Qubit*), i64 18, i64 18)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 56 to %Qubit*), i64 18, i64 20)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 58 to %Qubit*), i64 18, i64 22)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 60 to %Qubit*), i64 18, i64 24)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 62 to %Qubit*), i64 18, i64 26)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 64 to %Qubit*), i64 18, i64 28)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 66 to %Qubit*), i64 18, i64 30)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 68 to %Qubit*), i64 18, i64 32)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 70 to %Qubit*), i64 18, i64 34)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 1 to %Qubit*), i64 17, i64 1)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 3 to %Qubit*), i64 17, i64 3)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 5 to %Qubit*), i64 17, i64 5)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 7 to %Qubit*), i64 17, i64 7)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 9 to %Qubit*), i64 17, i64 9)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 11 to %Qubit*), i64 17, i64 11)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 13 to %Qubit*), i64 17, i64 13)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 15 to %Qubit*), i64 17, i64 15)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 17 to %Qubit*), i64 17, i64 17)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 19 to %Qubit*), i64 17, i64 19)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 21 to %Qubit*), i64 17, i64 21)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 23 to %Qubit*), i64 17, i64 23)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 25 to %Qubit*), i64 17, i64 25)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 27 to %Qubit*), i64 17, i64 27)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 29 to %Qubit*), i64 17, i64 29)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 31 to %Qubit*), i64 17, i64 31)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 33 to %Qubit*), i64 17, i64 33)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 35 to %Qubit*), i64 17, i64 35)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 37 to %Qubit*), i64 18, i64 1)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 39 to %Qubit*), i64 18, i64 3)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 41 to %Qubit*), i64 18, i64 5)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 43 to %Qubit*), i64 18, i64 7)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 45 to %Qubit*), i64 18, i64 9)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 47 to %Qubit*), i64 18, i64 11)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 49 to %Qubit*), i64 18, i64 13)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 51 to %Qubit*), i64 18, i64 15)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 53 to %Qubit*), i64 18, i64 17)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 55 to %Qubit*), i64 18, i64 19)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 57 to %Qubit*), i64 18, i64 21)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 59 to %Qubit*), i64 18, i64 23)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 61 to %Qubit*), i64 18, i64 25)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 63 to %Qubit*), i64 18, i64 27)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 65 to %Qubit*), i64 18, i64 29)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 67 to %Qubit*), i64 18, i64 31)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 69 to %Qubit*), i64 18, i64 33)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 71 to %Qubit*), i64 18, i64 35)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__cz__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 5 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 8 to %Qubit*), %Qubit* inttoptr (i64 9 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 10 to %Qubit*), %Qubit* inttoptr (i64 11 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 12 to %Qubit*), %Qubit* inttoptr (i64 13 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 14 to %Qubit*), %Qubit* inttoptr (i64 15 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 16 to %Qubit*), %Qubit* inttoptr (i64 17 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 18 to %Qubit*), %Qubit* inttoptr (i64 19 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 20 to %Qubit*), %Qubit* inttoptr (i64 21 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 22 to %Qubit*), %Qubit* inttoptr (i64 23 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 24 to %Qubit*), %Qubit* inttoptr (i64 25 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 26 to %Qubit*), %Qubit* inttoptr (i64 27 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 28 to %Qubit*), %Qubit* inttoptr (i64 29 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 30 to %Qubit*), %Qubit* inttoptr (i64 31 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 32 to %Qubit*), %Qubit* inttoptr (i64 33 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 34 to %Qubit*), %Qubit* inttoptr (i64 35 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 36 to %Qubit*), %Qubit* inttoptr (i64 37 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 38 to %Qubit*), %Qubit* inttoptr (i64 39 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 40 to %Qubit*), %Qubit* inttoptr (i64 41 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 42 to %Qubit*), %Qubit* inttoptr (i64 43 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 44 to %Qubit*), %Qubit* inttoptr (i64 45 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 46 to %Qubit*), %Qubit* inttoptr (i64 47 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 48 to %Qubit*), %Qubit* inttoptr (i64 49 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 50 to %Qubit*), %Qubit* inttoptr (i64 51 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 52 to %Qubit*), %Qubit* inttoptr (i64 53 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 54 to %Qubit*), %Qubit* inttoptr (i64 55 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 56 to %Qubit*), %Qubit* inttoptr (i64 57 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 58 to %Qubit*), %Qubit* inttoptr (i64 59 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 60 to %Qubit*), %Qubit* inttoptr (i64 61 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 62 to %Qubit*), %Qubit* inttoptr (i64 63 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 64 to %Qubit*), %Qubit* inttoptr (i64 65 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 66 to %Qubit*), %Qubit* inttoptr (i64 67 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 68 to %Qubit*), %Qubit* inttoptr (i64 69 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 70 to %Qubit*), %Qubit* inttoptr (i64 71 to %Qubit*))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* null, i64 21, i64 0)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 2 to %Qubit*), i64 21, i64 2)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 4 to %Qubit*), i64 21, i64 4)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 6 to %Qubit*), i64 21, i64 6)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 8 to %Qubit*), i64 21, i64 8)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 10 to %Qubit*), i64 21, i64 10)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 12 to %Qubit*), i64 21, i64 12)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 14 to %Qubit*), i64 21, i64 14)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 16 to %Qubit*), i64 21, i64 16)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 18 to %Qubit*), i64 21, i64 18)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 20 to %Qubit*), i64 21, i64 20)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 22 to %Qubit*), i64 21, i64 22)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 24 to %Qubit*), i64 21, i64 24)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 26 to %Qubit*), i64 21, i64 26)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 28 to %Qubit*), i64 21, i64 28)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 30 to %Qubit*), i64 21, i64 30)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 32 to %Qubit*), i64 21, i64 32)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 34 to %Qubit*), i64 21, i64 34)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 36 to %Qubit*), i64 22, i64 0)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 38 to %Qubit*), i64 22, i64 2)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 40 to %Qubit*), i64 22, i64 4)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 42 to %Qubit*), i64 22, i64 6)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 44 to %Qubit*), i64 22, i64 8)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 46 to %Qubit*), i64 22, i64 10)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 48 to %Qubit*), i64 22, i64 12)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 50 to %Qubit*), i64 22, i64 14)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 52 to %Qubit*), i64 22, i64 16)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 54 to %Qubit*), i64 22, i64 18)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 56 to %Qubit*), i64 22, i64 20)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 58 to %Qubit*), i64 22, i64 22)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 60 to %Qubit*), i64 22, i64 24)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 62 to %Qubit*), i64 22, i64 26)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 64 to %Qubit*), i64 22, i64 28)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 66 to %Qubit*), i64 22, i64 30)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 68 to %Qubit*), i64 22, i64 32)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 70 to %Qubit*), i64 22, i64 34)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 1 to %Qubit*), i64 21, i64 1)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 3 to %Qubit*), i64 21, i64 3)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 5 to %Qubit*), i64 21, i64 5)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 7 to %Qubit*), i64 21, i64 7)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 9 to %Qubit*), i64 21, i64 9)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 11 to %Qubit*), i64 21, i64 11)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 13 to %Qubit*), i64 21, i64 13)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 15 to %Qubit*), i64 21, i64 15)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 17 to %Qubit*), i64 21, i64 17)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 19 to %Qubit*), i64 21, i64 19)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 21 to %Qubit*), i64 21, i64 21)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 23 to %Qubit*), i64 21, i64 23)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 25 to %Qubit*), i64 21, i64 25)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 27 to %Qubit*), i64 21, i64 27)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 29 to %Qubit*), i64 21, i64 29)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 31 to %Qubit*), i64 21, i64 31)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 33 to %Qubit*), i64 21, i64 33)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 35 to %Qubit*), i64 21, i64 35)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 37 to %Qubit*), i64 22, i64 1)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 39 to %Qubit*), i64 22, i64 3)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 41 to %Qubit*), i64 22, i64 5)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 43 to %Qubit*), i64 22, i64 7)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 45 to %Qubit*), i64 22, i64 9)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 47 to %Qubit*), i64 22, i64 11)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 49 to %Qubit*), i64 22, i64 13)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 51 to %Qubit*), i64 22, i64 15)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 53 to %Qubit*), i64 22, i64 17)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 55 to %Qubit*), i64 22, i64 19)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 57 to %Qubit*), i64 22, i64 21)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 59 to %Qubit*), i64 22, i64 23)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 61 to %Qubit*), i64 22, i64 25)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 63 to %Qubit*), i64 22, i64 27)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 65 to %Qubit*), i64 22, i64 29)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 67 to %Qubit*), i64 22, i64 31)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 69 to %Qubit*), i64 22, i64 33)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 71 to %Qubit*), i64 22, i64 35)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__begin_parallel()

declare void @__quantum__rt__end_parallel()

declare void @__quantum__qis__move1__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move2__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move3__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move4__body(%Qubit*, i64, i64)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="72" "required_num_results"="0" }

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
def test_scheduler_splits_large_parallel_2q_in_iz_by_iz_size():
    device = AC1000()
    num_qubits = int(
        device.column_count * device.get_interaction_zones()[0].row_count * 1.5
    )
    qir = qsharp.compile(
        f"""
        {{
            use qs = Qubit[{num_qubits}];
            for i in 0..2..(Length(qs)-2) {{
                CZ(qs[i], qs[i+1]);
            }}
        }}
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    before = PerQubitOrdering()
    before.run(module)
    Schedule(device).run(module)
    after = PerQubitOrdering()
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* null, i64 17, i64 0)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 2 to %Qubit*), i64 17, i64 2)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 4 to %Qubit*), i64 17, i64 4)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 6 to %Qubit*), i64 17, i64 6)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 8 to %Qubit*), i64 17, i64 8)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 10 to %Qubit*), i64 17, i64 10)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 12 to %Qubit*), i64 17, i64 12)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 14 to %Qubit*), i64 17, i64 14)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 16 to %Qubit*), i64 17, i64 16)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 18 to %Qubit*), i64 17, i64 18)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 20 to %Qubit*), i64 17, i64 20)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 22 to %Qubit*), i64 17, i64 22)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 24 to %Qubit*), i64 17, i64 24)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 26 to %Qubit*), i64 17, i64 26)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 28 to %Qubit*), i64 17, i64 28)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 30 to %Qubit*), i64 17, i64 30)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 32 to %Qubit*), i64 17, i64 32)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 34 to %Qubit*), i64 17, i64 34)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 36 to %Qubit*), i64 18, i64 0)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 38 to %Qubit*), i64 18, i64 2)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 40 to %Qubit*), i64 18, i64 4)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 42 to %Qubit*), i64 18, i64 6)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 44 to %Qubit*), i64 18, i64 8)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 46 to %Qubit*), i64 18, i64 10)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 48 to %Qubit*), i64 18, i64 12)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 50 to %Qubit*), i64 18, i64 14)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 52 to %Qubit*), i64 18, i64 16)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 54 to %Qubit*), i64 18, i64 18)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 56 to %Qubit*), i64 18, i64 20)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 58 to %Qubit*), i64 18, i64 22)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 60 to %Qubit*), i64 18, i64 24)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 62 to %Qubit*), i64 18, i64 26)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 64 to %Qubit*), i64 18, i64 28)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 66 to %Qubit*), i64 18, i64 30)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 68 to %Qubit*), i64 18, i64 32)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 70 to %Qubit*), i64 18, i64 34)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 1 to %Qubit*), i64 17, i64 1)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 3 to %Qubit*), i64 17, i64 3)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 5 to %Qubit*), i64 17, i64 5)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 7 to %Qubit*), i64 17, i64 7)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 9 to %Qubit*), i64 17, i64 9)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 11 to %Qubit*), i64 17, i64 11)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 13 to %Qubit*), i64 17, i64 13)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 15 to %Qubit*), i64 17, i64 15)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 17 to %Qubit*), i64 17, i64 17)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 19 to %Qubit*), i64 17, i64 19)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 21 to %Qubit*), i64 17, i64 21)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 23 to %Qubit*), i64 17, i64 23)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 25 to %Qubit*), i64 17, i64 25)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 27 to %Qubit*), i64 17, i64 27)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 29 to %Qubit*), i64 17, i64 29)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 31 to %Qubit*), i64 17, i64 31)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 33 to %Qubit*), i64 17, i64 33)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 35 to %Qubit*), i64 17, i64 35)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 37 to %Qubit*), i64 18, i64 1)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 39 to %Qubit*), i64 18, i64 3)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 41 to %Qubit*), i64 18, i64 5)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 43 to %Qubit*), i64 18, i64 7)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 45 to %Qubit*), i64 18, i64 9)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 47 to %Qubit*), i64 18, i64 11)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 49 to %Qubit*), i64 18, i64 13)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 51 to %Qubit*), i64 18, i64 15)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 53 to %Qubit*), i64 18, i64 17)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 55 to %Qubit*), i64 18, i64 19)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 57 to %Qubit*), i64 18, i64 21)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 59 to %Qubit*), i64 18, i64 23)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 61 to %Qubit*), i64 18, i64 25)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 63 to %Qubit*), i64 18, i64 27)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 65 to %Qubit*), i64 18, i64 29)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 67 to %Qubit*), i64 18, i64 31)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 69 to %Qubit*), i64 18, i64 33)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 71 to %Qubit*), i64 18, i64 35)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 72 to %Qubit*), i64 19, i64 0)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 74 to %Qubit*), i64 19, i64 2)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 76 to %Qubit*), i64 19, i64 4)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 78 to %Qubit*), i64 19, i64 6)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 80 to %Qubit*), i64 19, i64 8)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 82 to %Qubit*), i64 19, i64 10)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 84 to %Qubit*), i64 19, i64 12)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 86 to %Qubit*), i64 19, i64 14)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 88 to %Qubit*), i64 19, i64 16)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 90 to %Qubit*), i64 19, i64 18)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 92 to %Qubit*), i64 19, i64 20)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 94 to %Qubit*), i64 19, i64 22)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 96 to %Qubit*), i64 19, i64 24)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 98 to %Qubit*), i64 19, i64 26)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 100 to %Qubit*), i64 19, i64 28)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 102 to %Qubit*), i64 19, i64 30)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 104 to %Qubit*), i64 19, i64 32)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 106 to %Qubit*), i64 19, i64 34)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 108 to %Qubit*), i64 20, i64 0)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 110 to %Qubit*), i64 20, i64 2)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 112 to %Qubit*), i64 20, i64 4)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 114 to %Qubit*), i64 20, i64 6)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 116 to %Qubit*), i64 20, i64 8)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 118 to %Qubit*), i64 20, i64 10)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 120 to %Qubit*), i64 20, i64 12)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 122 to %Qubit*), i64 20, i64 14)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 124 to %Qubit*), i64 20, i64 16)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 126 to %Qubit*), i64 20, i64 18)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 128 to %Qubit*), i64 20, i64 20)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 130 to %Qubit*), i64 20, i64 22)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 132 to %Qubit*), i64 20, i64 24)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 134 to %Qubit*), i64 20, i64 26)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 136 to %Qubit*), i64 20, i64 28)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 138 to %Qubit*), i64 20, i64 30)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 140 to %Qubit*), i64 20, i64 32)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 142 to %Qubit*), i64 20, i64 34)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 73 to %Qubit*), i64 19, i64 1)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 75 to %Qubit*), i64 19, i64 3)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 77 to %Qubit*), i64 19, i64 5)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 79 to %Qubit*), i64 19, i64 7)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 81 to %Qubit*), i64 19, i64 9)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 83 to %Qubit*), i64 19, i64 11)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 85 to %Qubit*), i64 19, i64 13)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 87 to %Qubit*), i64 19, i64 15)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 89 to %Qubit*), i64 19, i64 17)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 91 to %Qubit*), i64 19, i64 19)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 93 to %Qubit*), i64 19, i64 21)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 95 to %Qubit*), i64 19, i64 23)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 97 to %Qubit*), i64 19, i64 25)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 99 to %Qubit*), i64 19, i64 27)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 101 to %Qubit*), i64 19, i64 29)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 103 to %Qubit*), i64 19, i64 31)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 105 to %Qubit*), i64 19, i64 33)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 107 to %Qubit*), i64 19, i64 35)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 109 to %Qubit*), i64 20, i64 1)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 111 to %Qubit*), i64 20, i64 3)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 113 to %Qubit*), i64 20, i64 5)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 115 to %Qubit*), i64 20, i64 7)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 117 to %Qubit*), i64 20, i64 9)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 119 to %Qubit*), i64 20, i64 11)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 121 to %Qubit*), i64 20, i64 13)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 123 to %Qubit*), i64 20, i64 15)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 125 to %Qubit*), i64 20, i64 17)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 127 to %Qubit*), i64 20, i64 19)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 129 to %Qubit*), i64 20, i64 21)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 131 to %Qubit*), i64 20, i64 23)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 133 to %Qubit*), i64 20, i64 25)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 135 to %Qubit*), i64 20, i64 27)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 137 to %Qubit*), i64 20, i64 29)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 139 to %Qubit*), i64 20, i64 31)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 141 to %Qubit*), i64 20, i64 33)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 143 to %Qubit*), i64 20, i64 35)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__cz__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 5 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 6 to %Qubit*), %Qubit* inttoptr (i64 7 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 8 to %Qubit*), %Qubit* inttoptr (i64 9 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 10 to %Qubit*), %Qubit* inttoptr (i64 11 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 12 to %Qubit*), %Qubit* inttoptr (i64 13 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 14 to %Qubit*), %Qubit* inttoptr (i64 15 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 16 to %Qubit*), %Qubit* inttoptr (i64 17 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 18 to %Qubit*), %Qubit* inttoptr (i64 19 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 20 to %Qubit*), %Qubit* inttoptr (i64 21 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 22 to %Qubit*), %Qubit* inttoptr (i64 23 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 24 to %Qubit*), %Qubit* inttoptr (i64 25 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 26 to %Qubit*), %Qubit* inttoptr (i64 27 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 28 to %Qubit*), %Qubit* inttoptr (i64 29 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 30 to %Qubit*), %Qubit* inttoptr (i64 31 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 32 to %Qubit*), %Qubit* inttoptr (i64 33 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 34 to %Qubit*), %Qubit* inttoptr (i64 35 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 36 to %Qubit*), %Qubit* inttoptr (i64 37 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 38 to %Qubit*), %Qubit* inttoptr (i64 39 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 40 to %Qubit*), %Qubit* inttoptr (i64 41 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 42 to %Qubit*), %Qubit* inttoptr (i64 43 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 44 to %Qubit*), %Qubit* inttoptr (i64 45 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 46 to %Qubit*), %Qubit* inttoptr (i64 47 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 48 to %Qubit*), %Qubit* inttoptr (i64 49 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 50 to %Qubit*), %Qubit* inttoptr (i64 51 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 52 to %Qubit*), %Qubit* inttoptr (i64 53 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 54 to %Qubit*), %Qubit* inttoptr (i64 55 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 56 to %Qubit*), %Qubit* inttoptr (i64 57 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 58 to %Qubit*), %Qubit* inttoptr (i64 59 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 60 to %Qubit*), %Qubit* inttoptr (i64 61 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 62 to %Qubit*), %Qubit* inttoptr (i64 63 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 64 to %Qubit*), %Qubit* inttoptr (i64 65 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 66 to %Qubit*), %Qubit* inttoptr (i64 67 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 68 to %Qubit*), %Qubit* inttoptr (i64 69 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 70 to %Qubit*), %Qubit* inttoptr (i64 71 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 72 to %Qubit*), %Qubit* inttoptr (i64 73 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 74 to %Qubit*), %Qubit* inttoptr (i64 75 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 76 to %Qubit*), %Qubit* inttoptr (i64 77 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 78 to %Qubit*), %Qubit* inttoptr (i64 79 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 80 to %Qubit*), %Qubit* inttoptr (i64 81 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 82 to %Qubit*), %Qubit* inttoptr (i64 83 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 84 to %Qubit*), %Qubit* inttoptr (i64 85 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 86 to %Qubit*), %Qubit* inttoptr (i64 87 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 88 to %Qubit*), %Qubit* inttoptr (i64 89 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 90 to %Qubit*), %Qubit* inttoptr (i64 91 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 92 to %Qubit*), %Qubit* inttoptr (i64 93 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 94 to %Qubit*), %Qubit* inttoptr (i64 95 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 96 to %Qubit*), %Qubit* inttoptr (i64 97 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 98 to %Qubit*), %Qubit* inttoptr (i64 99 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 100 to %Qubit*), %Qubit* inttoptr (i64 101 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 102 to %Qubit*), %Qubit* inttoptr (i64 103 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 104 to %Qubit*), %Qubit* inttoptr (i64 105 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 106 to %Qubit*), %Qubit* inttoptr (i64 107 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 108 to %Qubit*), %Qubit* inttoptr (i64 109 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 110 to %Qubit*), %Qubit* inttoptr (i64 111 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 112 to %Qubit*), %Qubit* inttoptr (i64 113 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 114 to %Qubit*), %Qubit* inttoptr (i64 115 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 116 to %Qubit*), %Qubit* inttoptr (i64 117 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 118 to %Qubit*), %Qubit* inttoptr (i64 119 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 120 to %Qubit*), %Qubit* inttoptr (i64 121 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 122 to %Qubit*), %Qubit* inttoptr (i64 123 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 124 to %Qubit*), %Qubit* inttoptr (i64 125 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 126 to %Qubit*), %Qubit* inttoptr (i64 127 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 128 to %Qubit*), %Qubit* inttoptr (i64 129 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 130 to %Qubit*), %Qubit* inttoptr (i64 131 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 132 to %Qubit*), %Qubit* inttoptr (i64 133 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 134 to %Qubit*), %Qubit* inttoptr (i64 135 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 136 to %Qubit*), %Qubit* inttoptr (i64 137 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 138 to %Qubit*), %Qubit* inttoptr (i64 139 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 140 to %Qubit*), %Qubit* inttoptr (i64 141 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 142 to %Qubit*), %Qubit* inttoptr (i64 143 to %Qubit*))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* null, i64 21, i64 0)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 2 to %Qubit*), i64 21, i64 2)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 4 to %Qubit*), i64 21, i64 4)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 6 to %Qubit*), i64 21, i64 6)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 8 to %Qubit*), i64 21, i64 8)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 10 to %Qubit*), i64 21, i64 10)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 12 to %Qubit*), i64 21, i64 12)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 14 to %Qubit*), i64 21, i64 14)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 16 to %Qubit*), i64 21, i64 16)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 18 to %Qubit*), i64 21, i64 18)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 20 to %Qubit*), i64 21, i64 20)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 22 to %Qubit*), i64 21, i64 22)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 24 to %Qubit*), i64 21, i64 24)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 26 to %Qubit*), i64 21, i64 26)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 28 to %Qubit*), i64 21, i64 28)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 30 to %Qubit*), i64 21, i64 30)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 32 to %Qubit*), i64 21, i64 32)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 34 to %Qubit*), i64 21, i64 34)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 36 to %Qubit*), i64 22, i64 0)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 38 to %Qubit*), i64 22, i64 2)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 40 to %Qubit*), i64 22, i64 4)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 42 to %Qubit*), i64 22, i64 6)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 44 to %Qubit*), i64 22, i64 8)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 46 to %Qubit*), i64 22, i64 10)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 48 to %Qubit*), i64 22, i64 12)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 50 to %Qubit*), i64 22, i64 14)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 52 to %Qubit*), i64 22, i64 16)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 54 to %Qubit*), i64 22, i64 18)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 56 to %Qubit*), i64 22, i64 20)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 58 to %Qubit*), i64 22, i64 22)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 60 to %Qubit*), i64 22, i64 24)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 62 to %Qubit*), i64 22, i64 26)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 64 to %Qubit*), i64 22, i64 28)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 66 to %Qubit*), i64 22, i64 30)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 68 to %Qubit*), i64 22, i64 32)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 70 to %Qubit*), i64 22, i64 34)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 1 to %Qubit*), i64 21, i64 1)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 3 to %Qubit*), i64 21, i64 3)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 5 to %Qubit*), i64 21, i64 5)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 7 to %Qubit*), i64 21, i64 7)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 9 to %Qubit*), i64 21, i64 9)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 11 to %Qubit*), i64 21, i64 11)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 13 to %Qubit*), i64 21, i64 13)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 15 to %Qubit*), i64 21, i64 15)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 17 to %Qubit*), i64 21, i64 17)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 19 to %Qubit*), i64 21, i64 19)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 21 to %Qubit*), i64 21, i64 21)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 23 to %Qubit*), i64 21, i64 23)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 25 to %Qubit*), i64 21, i64 25)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 27 to %Qubit*), i64 21, i64 27)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 29 to %Qubit*), i64 21, i64 29)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 31 to %Qubit*), i64 21, i64 31)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 33 to %Qubit*), i64 21, i64 33)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 35 to %Qubit*), i64 21, i64 35)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 37 to %Qubit*), i64 22, i64 1)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 39 to %Qubit*), i64 22, i64 3)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 41 to %Qubit*), i64 22, i64 5)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 43 to %Qubit*), i64 22, i64 7)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 45 to %Qubit*), i64 22, i64 9)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 47 to %Qubit*), i64 22, i64 11)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 49 to %Qubit*), i64 22, i64 13)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 51 to %Qubit*), i64 22, i64 15)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 53 to %Qubit*), i64 22, i64 17)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 55 to %Qubit*), i64 22, i64 19)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 57 to %Qubit*), i64 22, i64 21)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 59 to %Qubit*), i64 22, i64 23)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 61 to %Qubit*), i64 22, i64 25)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 63 to %Qubit*), i64 22, i64 27)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 65 to %Qubit*), i64 22, i64 29)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 67 to %Qubit*), i64 22, i64 31)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 69 to %Qubit*), i64 22, i64 33)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 71 to %Qubit*), i64 22, i64 35)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 72 to %Qubit*), i64 23, i64 0)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 74 to %Qubit*), i64 23, i64 2)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 76 to %Qubit*), i64 23, i64 4)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 78 to %Qubit*), i64 23, i64 6)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 80 to %Qubit*), i64 23, i64 8)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 82 to %Qubit*), i64 23, i64 10)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 84 to %Qubit*), i64 23, i64 12)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 86 to %Qubit*), i64 23, i64 14)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 88 to %Qubit*), i64 23, i64 16)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 90 to %Qubit*), i64 23, i64 18)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 92 to %Qubit*), i64 23, i64 20)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 94 to %Qubit*), i64 23, i64 22)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 96 to %Qubit*), i64 23, i64 24)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 98 to %Qubit*), i64 23, i64 26)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 100 to %Qubit*), i64 23, i64 28)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 102 to %Qubit*), i64 23, i64 30)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 104 to %Qubit*), i64 23, i64 32)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 106 to %Qubit*), i64 23, i64 34)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 108 to %Qubit*), i64 24, i64 0)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 110 to %Qubit*), i64 24, i64 2)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 112 to %Qubit*), i64 24, i64 4)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 114 to %Qubit*), i64 24, i64 6)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 116 to %Qubit*), i64 24, i64 8)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 118 to %Qubit*), i64 24, i64 10)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 120 to %Qubit*), i64 24, i64 12)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 122 to %Qubit*), i64 24, i64 14)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 124 to %Qubit*), i64 24, i64 16)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 126 to %Qubit*), i64 24, i64 18)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 128 to %Qubit*), i64 24, i64 20)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 130 to %Qubit*), i64 24, i64 22)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 132 to %Qubit*), i64 24, i64 24)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 134 to %Qubit*), i64 24, i64 26)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 136 to %Qubit*), i64 24, i64 28)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 138 to %Qubit*), i64 24, i64 30)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 140 to %Qubit*), i64 24, i64 32)
  call void @__quantum__qis__move3__body(%Qubit* inttoptr (i64 142 to %Qubit*), i64 24, i64 34)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 73 to %Qubit*), i64 23, i64 1)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 75 to %Qubit*), i64 23, i64 3)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 77 to %Qubit*), i64 23, i64 5)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 79 to %Qubit*), i64 23, i64 7)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 81 to %Qubit*), i64 23, i64 9)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 83 to %Qubit*), i64 23, i64 11)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 85 to %Qubit*), i64 23, i64 13)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 87 to %Qubit*), i64 23, i64 15)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 89 to %Qubit*), i64 23, i64 17)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 91 to %Qubit*), i64 23, i64 19)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 93 to %Qubit*), i64 23, i64 21)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 95 to %Qubit*), i64 23, i64 23)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 97 to %Qubit*), i64 23, i64 25)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 99 to %Qubit*), i64 23, i64 27)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 101 to %Qubit*), i64 23, i64 29)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 103 to %Qubit*), i64 23, i64 31)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 105 to %Qubit*), i64 23, i64 33)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 107 to %Qubit*), i64 23, i64 35)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 109 to %Qubit*), i64 24, i64 1)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 111 to %Qubit*), i64 24, i64 3)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 113 to %Qubit*), i64 24, i64 5)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 115 to %Qubit*), i64 24, i64 7)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 117 to %Qubit*), i64 24, i64 9)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 119 to %Qubit*), i64 24, i64 11)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 121 to %Qubit*), i64 24, i64 13)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 123 to %Qubit*), i64 24, i64 15)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 125 to %Qubit*), i64 24, i64 17)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 127 to %Qubit*), i64 24, i64 19)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 129 to %Qubit*), i64 24, i64 21)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 131 to %Qubit*), i64 24, i64 23)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 133 to %Qubit*), i64 24, i64 25)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 135 to %Qubit*), i64 24, i64 27)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 137 to %Qubit*), i64 24, i64 29)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 139 to %Qubit*), i64 24, i64 31)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 141 to %Qubit*), i64 24, i64 33)
  call void @__quantum__qis__move4__body(%Qubit* inttoptr (i64 143 to %Qubit*), i64 24, i64 35)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 144 to %Qubit*), i64 17, i64 0)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 146 to %Qubit*), i64 17, i64 2)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 148 to %Qubit*), i64 17, i64 4)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 150 to %Qubit*), i64 17, i64 6)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 152 to %Qubit*), i64 17, i64 8)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 154 to %Qubit*), i64 17, i64 10)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 156 to %Qubit*), i64 17, i64 12)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 158 to %Qubit*), i64 17, i64 14)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 160 to %Qubit*), i64 17, i64 16)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 162 to %Qubit*), i64 17, i64 18)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 164 to %Qubit*), i64 17, i64 20)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 166 to %Qubit*), i64 17, i64 22)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 168 to %Qubit*), i64 17, i64 24)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 170 to %Qubit*), i64 17, i64 26)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 172 to %Qubit*), i64 17, i64 28)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 174 to %Qubit*), i64 17, i64 30)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 176 to %Qubit*), i64 17, i64 32)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 178 to %Qubit*), i64 17, i64 34)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 180 to %Qubit*), i64 18, i64 0)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 182 to %Qubit*), i64 18, i64 2)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 184 to %Qubit*), i64 18, i64 4)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 186 to %Qubit*), i64 18, i64 6)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 188 to %Qubit*), i64 18, i64 8)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 190 to %Qubit*), i64 18, i64 10)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 192 to %Qubit*), i64 18, i64 12)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 194 to %Qubit*), i64 18, i64 14)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 196 to %Qubit*), i64 18, i64 16)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 198 to %Qubit*), i64 18, i64 18)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 200 to %Qubit*), i64 18, i64 20)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 202 to %Qubit*), i64 18, i64 22)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 204 to %Qubit*), i64 18, i64 24)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 206 to %Qubit*), i64 18, i64 26)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 208 to %Qubit*), i64 18, i64 28)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 210 to %Qubit*), i64 18, i64 30)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 212 to %Qubit*), i64 18, i64 32)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 214 to %Qubit*), i64 18, i64 34)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 145 to %Qubit*), i64 17, i64 1)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 147 to %Qubit*), i64 17, i64 3)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 149 to %Qubit*), i64 17, i64 5)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 151 to %Qubit*), i64 17, i64 7)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 153 to %Qubit*), i64 17, i64 9)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 155 to %Qubit*), i64 17, i64 11)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 157 to %Qubit*), i64 17, i64 13)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 159 to %Qubit*), i64 17, i64 15)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 161 to %Qubit*), i64 17, i64 17)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 163 to %Qubit*), i64 17, i64 19)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 165 to %Qubit*), i64 17, i64 21)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 167 to %Qubit*), i64 17, i64 23)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 169 to %Qubit*), i64 17, i64 25)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 171 to %Qubit*), i64 17, i64 27)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 173 to %Qubit*), i64 17, i64 29)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 175 to %Qubit*), i64 17, i64 31)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 177 to %Qubit*), i64 17, i64 33)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 179 to %Qubit*), i64 17, i64 35)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 181 to %Qubit*), i64 18, i64 1)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 183 to %Qubit*), i64 18, i64 3)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 185 to %Qubit*), i64 18, i64 5)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 187 to %Qubit*), i64 18, i64 7)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 189 to %Qubit*), i64 18, i64 9)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 191 to %Qubit*), i64 18, i64 11)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 193 to %Qubit*), i64 18, i64 13)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 195 to %Qubit*), i64 18, i64 15)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 197 to %Qubit*), i64 18, i64 17)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 199 to %Qubit*), i64 18, i64 19)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 201 to %Qubit*), i64 18, i64 21)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 203 to %Qubit*), i64 18, i64 23)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 205 to %Qubit*), i64 18, i64 25)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 207 to %Qubit*), i64 18, i64 27)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 209 to %Qubit*), i64 18, i64 29)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 211 to %Qubit*), i64 18, i64 31)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 213 to %Qubit*), i64 18, i64 33)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 215 to %Qubit*), i64 18, i64 35)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 144 to %Qubit*), %Qubit* inttoptr (i64 145 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 146 to %Qubit*), %Qubit* inttoptr (i64 147 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 148 to %Qubit*), %Qubit* inttoptr (i64 149 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 150 to %Qubit*), %Qubit* inttoptr (i64 151 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 152 to %Qubit*), %Qubit* inttoptr (i64 153 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 154 to %Qubit*), %Qubit* inttoptr (i64 155 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 156 to %Qubit*), %Qubit* inttoptr (i64 157 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 158 to %Qubit*), %Qubit* inttoptr (i64 159 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 160 to %Qubit*), %Qubit* inttoptr (i64 161 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 162 to %Qubit*), %Qubit* inttoptr (i64 163 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 164 to %Qubit*), %Qubit* inttoptr (i64 165 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 166 to %Qubit*), %Qubit* inttoptr (i64 167 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 168 to %Qubit*), %Qubit* inttoptr (i64 169 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 170 to %Qubit*), %Qubit* inttoptr (i64 171 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 172 to %Qubit*), %Qubit* inttoptr (i64 173 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 174 to %Qubit*), %Qubit* inttoptr (i64 175 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 176 to %Qubit*), %Qubit* inttoptr (i64 177 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 178 to %Qubit*), %Qubit* inttoptr (i64 179 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 180 to %Qubit*), %Qubit* inttoptr (i64 181 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 182 to %Qubit*), %Qubit* inttoptr (i64 183 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 184 to %Qubit*), %Qubit* inttoptr (i64 185 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 186 to %Qubit*), %Qubit* inttoptr (i64 187 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 188 to %Qubit*), %Qubit* inttoptr (i64 189 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 190 to %Qubit*), %Qubit* inttoptr (i64 191 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 192 to %Qubit*), %Qubit* inttoptr (i64 193 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 194 to %Qubit*), %Qubit* inttoptr (i64 195 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 196 to %Qubit*), %Qubit* inttoptr (i64 197 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 198 to %Qubit*), %Qubit* inttoptr (i64 199 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 200 to %Qubit*), %Qubit* inttoptr (i64 201 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 202 to %Qubit*), %Qubit* inttoptr (i64 203 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 204 to %Qubit*), %Qubit* inttoptr (i64 205 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 206 to %Qubit*), %Qubit* inttoptr (i64 207 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 208 to %Qubit*), %Qubit* inttoptr (i64 209 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 210 to %Qubit*), %Qubit* inttoptr (i64 211 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 212 to %Qubit*), %Qubit* inttoptr (i64 213 to %Qubit*))
  call void @__quantum__qis__cz__body(%Qubit* inttoptr (i64 214 to %Qubit*), %Qubit* inttoptr (i64 215 to %Qubit*))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 144 to %Qubit*), i64 25, i64 0)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 146 to %Qubit*), i64 25, i64 2)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 148 to %Qubit*), i64 25, i64 4)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 150 to %Qubit*), i64 25, i64 6)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 152 to %Qubit*), i64 25, i64 8)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 154 to %Qubit*), i64 25, i64 10)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 156 to %Qubit*), i64 25, i64 12)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 158 to %Qubit*), i64 25, i64 14)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 160 to %Qubit*), i64 25, i64 16)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 162 to %Qubit*), i64 25, i64 18)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 164 to %Qubit*), i64 25, i64 20)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 166 to %Qubit*), i64 25, i64 22)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 168 to %Qubit*), i64 25, i64 24)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 170 to %Qubit*), i64 25, i64 26)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 172 to %Qubit*), i64 25, i64 28)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 174 to %Qubit*), i64 25, i64 30)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 176 to %Qubit*), i64 25, i64 32)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 178 to %Qubit*), i64 25, i64 34)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 180 to %Qubit*), i64 26, i64 0)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 182 to %Qubit*), i64 26, i64 2)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 184 to %Qubit*), i64 26, i64 4)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 186 to %Qubit*), i64 26, i64 6)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 188 to %Qubit*), i64 26, i64 8)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 190 to %Qubit*), i64 26, i64 10)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 192 to %Qubit*), i64 26, i64 12)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 194 to %Qubit*), i64 26, i64 14)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 196 to %Qubit*), i64 26, i64 16)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 198 to %Qubit*), i64 26, i64 18)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 200 to %Qubit*), i64 26, i64 20)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 202 to %Qubit*), i64 26, i64 22)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 204 to %Qubit*), i64 26, i64 24)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 206 to %Qubit*), i64 26, i64 26)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 208 to %Qubit*), i64 26, i64 28)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 210 to %Qubit*), i64 26, i64 30)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 212 to %Qubit*), i64 26, i64 32)
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 214 to %Qubit*), i64 26, i64 34)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 145 to %Qubit*), i64 25, i64 1)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 147 to %Qubit*), i64 25, i64 3)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 149 to %Qubit*), i64 25, i64 5)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 151 to %Qubit*), i64 25, i64 7)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 153 to %Qubit*), i64 25, i64 9)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 155 to %Qubit*), i64 25, i64 11)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 157 to %Qubit*), i64 25, i64 13)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 159 to %Qubit*), i64 25, i64 15)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 161 to %Qubit*), i64 25, i64 17)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 163 to %Qubit*), i64 25, i64 19)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 165 to %Qubit*), i64 25, i64 21)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 167 to %Qubit*), i64 25, i64 23)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 169 to %Qubit*), i64 25, i64 25)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 171 to %Qubit*), i64 25, i64 27)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 173 to %Qubit*), i64 25, i64 29)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 175 to %Qubit*), i64 25, i64 31)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 177 to %Qubit*), i64 25, i64 33)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 179 to %Qubit*), i64 25, i64 35)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 181 to %Qubit*), i64 26, i64 1)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 183 to %Qubit*), i64 26, i64 3)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 185 to %Qubit*), i64 26, i64 5)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 187 to %Qubit*), i64 26, i64 7)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 189 to %Qubit*), i64 26, i64 9)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 191 to %Qubit*), i64 26, i64 11)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 193 to %Qubit*), i64 26, i64 13)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 195 to %Qubit*), i64 26, i64 15)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 197 to %Qubit*), i64 26, i64 17)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 199 to %Qubit*), i64 26, i64 19)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 201 to %Qubit*), i64 26, i64 21)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 203 to %Qubit*), i64 26, i64 23)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 205 to %Qubit*), i64 26, i64 25)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 207 to %Qubit*), i64 26, i64 27)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 209 to %Qubit*), i64 26, i64 29)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 211 to %Qubit*), i64 26, i64 31)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 213 to %Qubit*), i64 26, i64 33)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 215 to %Qubit*), i64 26, i64 35)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__begin_parallel()

declare void @__quantum__rt__end_parallel()

declare void @__quantum__qis__move1__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move2__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move3__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move4__body(%Qubit*, i64, i64)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="216" "required_num_results"="0" }

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
def test_scheduler_moves_qubits_to_iz_for_1q_gate_after_2q_gate_before_measurement():
    qir = qsharp.compile(
        """
        {
            use qs = Qubit[2];
            SX(qs[0]);
            SX(qs[1]);
            CZ(qs[0], qs[1]);
            SX(qs[1]);
            MResetZ(qs[1])
        }
        """
    )

    module = pyqir.Module.from_ir(pyqir.Context(), str(qir))
    before = PerQubitOrdering()
    before.run(module)
    Schedule(AC1000()).run(module)
    after = PerQubitOrdering()
    ValidateBeginEndParallel().run(module)
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

%Qubit = type opaque
%Result = type opaque

@empty_tag = internal constant [1 x i8] zeroinitializer
@0 = internal constant [4 x i8] c"0_r\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* null, i64 17, i64 0)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 1 to %Qubit*), i64 17, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__sx__body(%Qubit* null)
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__cz__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* null, i64 21, i64 0)
  call void @__quantum__qis__move2__body(%Qubit* inttoptr (i64 1 to %Qubit*), i64 21, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 1 to %Qubit*), i64 17, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__sx__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 1 to %Qubit*), i64 21, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 1 to %Qubit*), i64 38, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* null)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move1__body(%Qubit* inttoptr (i64 1 to %Qubit*), i64 21, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__result_record_output(%Result* null, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__sx__body(%Qubit*)

declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

declare void @__quantum__rt__result_record_output(%Result*, i8*)

declare void @__quantum__rt__begin_parallel()

declare void @__quantum__rt__end_parallel()

declare void @__quantum__qis__move1__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move2__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move3__body(%Qubit*, i64, i64)

declare void @__quantum__qis__move4__body(%Qubit*, i64, i64)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="1" }
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
