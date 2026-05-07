# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest
from expecttest import assert_expected_inline

import qdk as qsharp
from qdk._device._atom import NeutralAtomDevice
from qdk._device._atom._scheduler import Schedule
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
    Schedule(NeutralAtomDevice()).run(module)
    ValidateBeginEndParallel().run(module)
    after = PerQubitOrdering()
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

@0 = internal constant [4 x i8] c"0_t\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr null, i64 25, i64 0)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__sx__body(ptr null)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr null, i64 24, i64 0)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__sx__body(ptr)

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__begin_parallel()

declare void @__quantum__rt__end_parallel()

declare void @__quantum__qis__move__body(ptr, i64, i64)

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
    Schedule(NeutralAtomDevice()).run(module)
    ValidateBeginEndParallel().run(module)
    after = PerQubitOrdering()
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

@0 = internal constant [4 x i8] c"0_t\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr null, i64 25, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 1 to ptr), i64 25, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__cz__body(ptr null, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr null, i64 24, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 1 to ptr), i64 24, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__cz__body(ptr, ptr)

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__begin_parallel()

declare void @__quantum__rt__end_parallel()

declare void @__quantum__qis__move__body(ptr, i64, i64)

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
    Schedule(NeutralAtomDevice()).run(module)
    ValidateBeginEndParallel().run(module)
    after = PerQubitOrdering()
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

@0 = internal constant [4 x i8] c"0_t\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr null, i64 25, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 1 to ptr), i64 25, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__sx__body(ptr null)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__cz__body(ptr null, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr null, i64 24, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 1 to ptr), i64 24, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__sx__body(ptr)

declare void @__quantum__qis__cz__body(ptr, ptr)

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__begin_parallel()

declare void @__quantum__rt__end_parallel()

declare void @__quantum__qis__move__body(ptr, i64, i64)

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
    Schedule(NeutralAtomDevice()).run(module)
    after = PerQubitOrdering()
    ValidateBeginEndParallel().run(module)
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

@0 = internal constant [4 x i8] c"0_t\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr null, i64 25, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 1 to ptr), i64 25, i64 1)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 2 to ptr), i64 25, i64 2)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 3 to ptr), i64 25, i64 3)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__sx__body(ptr null)
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__cz__body(ptr null, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr null, i64 24, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 1 to ptr), i64 24, i64 1)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 2 to ptr), i64 24, i64 2)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 3 to ptr), i64 24, i64 3)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__sx__body(ptr)

declare void @__quantum__qis__cz__body(ptr, ptr)

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__begin_parallel()

declare void @__quantum__rt__end_parallel()

declare void @__quantum__qis__move__body(ptr, i64, i64)

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
    Schedule(NeutralAtomDevice()).run(module)
    after = PerQubitOrdering()
    ValidateBeginEndParallel().run(module)
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

@0 = internal constant [4 x i8] c"0_t\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr null, i64 27, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 1 to ptr), i64 27, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__mresetz__body(ptr null, ptr null)
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr null, i64 24, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 1 to ptr), i64 24, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__begin_parallel()

declare void @__quantum__rt__end_parallel()

declare void @__quantum__qis__move__body(ptr, i64, i64)

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
    device = NeutralAtomDevice()
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

@0 = internal constant [4 x i8] c"0_t\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr null, i64 26, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 1 to ptr), i64 26, i64 1)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 2 to ptr), i64 26, i64 2)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 3 to ptr), i64 26, i64 3)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 4 to ptr), i64 26, i64 4)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 5 to ptr), i64 26, i64 5)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 6 to ptr), i64 26, i64 6)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 7 to ptr), i64 26, i64 7)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 8 to ptr), i64 26, i64 8)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 9 to ptr), i64 26, i64 9)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 10 to ptr), i64 26, i64 10)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 11 to ptr), i64 26, i64 11)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 12 to ptr), i64 26, i64 12)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 13 to ptr), i64 26, i64 13)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 14 to ptr), i64 26, i64 14)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 15 to ptr), i64 26, i64 15)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 16 to ptr), i64 26, i64 16)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 17 to ptr), i64 26, i64 17)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 18 to ptr), i64 26, i64 18)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 19 to ptr), i64 26, i64 19)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 20 to ptr), i64 26, i64 20)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 21 to ptr), i64 26, i64 21)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 22 to ptr), i64 26, i64 22)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 23 to ptr), i64 26, i64 23)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 24 to ptr), i64 26, i64 24)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 25 to ptr), i64 26, i64 25)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 26 to ptr), i64 26, i64 26)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 27 to ptr), i64 26, i64 27)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 28 to ptr), i64 26, i64 28)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 29 to ptr), i64 26, i64 29)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 30 to ptr), i64 26, i64 30)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 31 to ptr), i64 26, i64 31)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 32 to ptr), i64 26, i64 32)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 33 to ptr), i64 26, i64 33)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 34 to ptr), i64 26, i64 34)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 35 to ptr), i64 26, i64 35)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 36 to ptr), i64 26, i64 36)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 37 to ptr), i64 26, i64 37)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 38 to ptr), i64 26, i64 38)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 39 to ptr), i64 26, i64 39)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 40 to ptr), i64 25, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 41 to ptr), i64 25, i64 1)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 42 to ptr), i64 25, i64 2)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 43 to ptr), i64 25, i64 3)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 44 to ptr), i64 25, i64 4)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 45 to ptr), i64 25, i64 5)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 46 to ptr), i64 25, i64 6)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 47 to ptr), i64 25, i64 7)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 48 to ptr), i64 25, i64 8)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 49 to ptr), i64 25, i64 9)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 50 to ptr), i64 25, i64 10)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 51 to ptr), i64 25, i64 11)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 52 to ptr), i64 25, i64 12)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 53 to ptr), i64 25, i64 13)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 54 to ptr), i64 25, i64 14)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 55 to ptr), i64 25, i64 15)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 56 to ptr), i64 25, i64 16)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 57 to ptr), i64 25, i64 17)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 58 to ptr), i64 25, i64 18)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 59 to ptr), i64 25, i64 19)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 60 to ptr), i64 25, i64 20)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 61 to ptr), i64 25, i64 21)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 62 to ptr), i64 25, i64 22)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 63 to ptr), i64 25, i64 23)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 64 to ptr), i64 25, i64 24)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 65 to ptr), i64 25, i64 25)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 66 to ptr), i64 25, i64 26)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 67 to ptr), i64 25, i64 27)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 68 to ptr), i64 25, i64 28)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 69 to ptr), i64 25, i64 29)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 70 to ptr), i64 25, i64 30)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 71 to ptr), i64 25, i64 31)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 72 to ptr), i64 25, i64 32)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 73 to ptr), i64 25, i64 33)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 74 to ptr), i64 25, i64 34)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 75 to ptr), i64 25, i64 35)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 76 to ptr), i64 25, i64 36)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 77 to ptr), i64 25, i64 37)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 78 to ptr), i64 25, i64 38)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 79 to ptr), i64 25, i64 39)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 40 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 41 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 42 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 43 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 44 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 45 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 46 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 47 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 48 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 49 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 50 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 51 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 52 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 53 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 54 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 55 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 56 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 57 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 58 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 59 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 60 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 61 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 62 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 63 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 64 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 65 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 66 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 67 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 68 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 69 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 70 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 71 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 72 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 73 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 74 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 75 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 76 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 77 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 78 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 79 to ptr))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__sx__body(ptr null)
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 2 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 4 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 6 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 8 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 9 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 10 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 11 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 12 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 13 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 14 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 15 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 16 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 17 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 18 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 19 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 20 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 21 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 22 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 23 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 24 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 25 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 26 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 27 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 28 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 29 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 30 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 31 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 32 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 33 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 34 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 35 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 36 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 37 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 38 to ptr))
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 39 to ptr))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr null, i64 24, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 1 to ptr), i64 24, i64 1)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 2 to ptr), i64 24, i64 2)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 3 to ptr), i64 24, i64 3)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 4 to ptr), i64 24, i64 4)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 5 to ptr), i64 24, i64 5)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 6 to ptr), i64 24, i64 6)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 7 to ptr), i64 24, i64 7)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 8 to ptr), i64 24, i64 8)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 9 to ptr), i64 24, i64 9)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 10 to ptr), i64 24, i64 10)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 11 to ptr), i64 24, i64 11)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 12 to ptr), i64 24, i64 12)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 13 to ptr), i64 24, i64 13)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 14 to ptr), i64 24, i64 14)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 15 to ptr), i64 24, i64 15)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 16 to ptr), i64 24, i64 16)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 17 to ptr), i64 24, i64 17)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 18 to ptr), i64 24, i64 18)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 19 to ptr), i64 24, i64 19)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 20 to ptr), i64 24, i64 20)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 21 to ptr), i64 24, i64 21)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 22 to ptr), i64 24, i64 22)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 23 to ptr), i64 24, i64 23)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 24 to ptr), i64 24, i64 24)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 25 to ptr), i64 24, i64 25)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 26 to ptr), i64 24, i64 26)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 27 to ptr), i64 24, i64 27)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 28 to ptr), i64 24, i64 28)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 29 to ptr), i64 24, i64 29)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 30 to ptr), i64 24, i64 30)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 31 to ptr), i64 24, i64 31)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 32 to ptr), i64 24, i64 32)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 33 to ptr), i64 24, i64 33)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 34 to ptr), i64 24, i64 34)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 35 to ptr), i64 24, i64 35)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 36 to ptr), i64 24, i64 36)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 37 to ptr), i64 24, i64 37)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 38 to ptr), i64 24, i64 38)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 39 to ptr), i64 24, i64 39)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 40 to ptr), i64 23, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 41 to ptr), i64 23, i64 1)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 42 to ptr), i64 23, i64 2)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 43 to ptr), i64 23, i64 3)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 44 to ptr), i64 23, i64 4)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 45 to ptr), i64 23, i64 5)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 46 to ptr), i64 23, i64 6)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 47 to ptr), i64 23, i64 7)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 48 to ptr), i64 23, i64 8)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 49 to ptr), i64 23, i64 9)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 50 to ptr), i64 23, i64 10)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 51 to ptr), i64 23, i64 11)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 52 to ptr), i64 23, i64 12)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 53 to ptr), i64 23, i64 13)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 54 to ptr), i64 23, i64 14)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 55 to ptr), i64 23, i64 15)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 56 to ptr), i64 23, i64 16)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 57 to ptr), i64 23, i64 17)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 58 to ptr), i64 23, i64 18)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 59 to ptr), i64 23, i64 19)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 60 to ptr), i64 23, i64 20)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 61 to ptr), i64 23, i64 21)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 62 to ptr), i64 23, i64 22)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 63 to ptr), i64 23, i64 23)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 64 to ptr), i64 23, i64 24)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 65 to ptr), i64 23, i64 25)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 66 to ptr), i64 23, i64 26)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 67 to ptr), i64 23, i64 27)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 68 to ptr), i64 23, i64 28)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 69 to ptr), i64 23, i64 29)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 70 to ptr), i64 23, i64 30)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 71 to ptr), i64 23, i64 31)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 72 to ptr), i64 23, i64 32)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 73 to ptr), i64 23, i64 33)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 74 to ptr), i64 23, i64 34)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 75 to ptr), i64 23, i64 35)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 76 to ptr), i64 23, i64 36)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 77 to ptr), i64 23, i64 37)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 78 to ptr), i64 23, i64 38)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 79 to ptr), i64 23, i64 39)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__sx__body(ptr)

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__begin_parallel()

declare void @__quantum__rt__end_parallel()

declare void @__quantum__qis__move__body(ptr, i64, i64)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="80" "required_num_results"="0" }

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
    device = NeutralAtomDevice()
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

@0 = internal constant [4 x i8] c"0_t\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr null, i64 26, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 1 to ptr), i64 26, i64 1)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 2 to ptr), i64 26, i64 2)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 3 to ptr), i64 26, i64 3)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 4 to ptr), i64 26, i64 4)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 5 to ptr), i64 26, i64 5)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 6 to ptr), i64 26, i64 6)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 7 to ptr), i64 26, i64 7)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 8 to ptr), i64 26, i64 8)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 9 to ptr), i64 26, i64 9)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 10 to ptr), i64 26, i64 10)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 11 to ptr), i64 26, i64 11)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 12 to ptr), i64 26, i64 12)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 13 to ptr), i64 26, i64 13)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 14 to ptr), i64 26, i64 14)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 15 to ptr), i64 26, i64 15)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 16 to ptr), i64 26, i64 16)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 17 to ptr), i64 26, i64 17)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 18 to ptr), i64 26, i64 18)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 19 to ptr), i64 26, i64 19)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 20 to ptr), i64 26, i64 20)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 21 to ptr), i64 26, i64 21)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 22 to ptr), i64 26, i64 22)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 23 to ptr), i64 26, i64 23)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 24 to ptr), i64 26, i64 24)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 25 to ptr), i64 26, i64 25)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 26 to ptr), i64 26, i64 26)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 27 to ptr), i64 26, i64 27)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 28 to ptr), i64 26, i64 28)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 29 to ptr), i64 26, i64 29)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 30 to ptr), i64 26, i64 30)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 31 to ptr), i64 26, i64 31)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 32 to ptr), i64 26, i64 32)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 33 to ptr), i64 26, i64 33)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 34 to ptr), i64 26, i64 34)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 35 to ptr), i64 26, i64 35)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 36 to ptr), i64 26, i64 36)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 37 to ptr), i64 26, i64 37)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 38 to ptr), i64 26, i64 38)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 39 to ptr), i64 26, i64 39)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 40 to ptr), i64 25, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 41 to ptr), i64 25, i64 1)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 42 to ptr), i64 25, i64 2)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 43 to ptr), i64 25, i64 3)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 44 to ptr), i64 25, i64 4)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 45 to ptr), i64 25, i64 5)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 46 to ptr), i64 25, i64 6)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 47 to ptr), i64 25, i64 7)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 48 to ptr), i64 25, i64 8)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 49 to ptr), i64 25, i64 9)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 50 to ptr), i64 25, i64 10)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 51 to ptr), i64 25, i64 11)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 52 to ptr), i64 25, i64 12)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 53 to ptr), i64 25, i64 13)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 54 to ptr), i64 25, i64 14)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 55 to ptr), i64 25, i64 15)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 56 to ptr), i64 25, i64 16)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 57 to ptr), i64 25, i64 17)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 58 to ptr), i64 25, i64 18)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 59 to ptr), i64 25, i64 19)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 60 to ptr), i64 25, i64 20)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 61 to ptr), i64 25, i64 21)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 62 to ptr), i64 25, i64 22)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 63 to ptr), i64 25, i64 23)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 64 to ptr), i64 25, i64 24)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 65 to ptr), i64 25, i64 25)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 66 to ptr), i64 25, i64 26)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 67 to ptr), i64 25, i64 27)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 68 to ptr), i64 25, i64 28)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 69 to ptr), i64 25, i64 29)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 70 to ptr), i64 25, i64 30)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 71 to ptr), i64 25, i64 31)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 72 to ptr), i64 25, i64 32)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 73 to ptr), i64 25, i64 33)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 74 to ptr), i64 25, i64 34)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 75 to ptr), i64 25, i64 35)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 76 to ptr), i64 25, i64 36)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 77 to ptr), i64 25, i64 37)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 78 to ptr), i64 25, i64 38)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 79 to ptr), i64 25, i64 39)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__cz__body(ptr null, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 6 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 8 to ptr), ptr inttoptr (i64 9 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 10 to ptr), ptr inttoptr (i64 11 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 12 to ptr), ptr inttoptr (i64 13 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 14 to ptr), ptr inttoptr (i64 15 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 16 to ptr), ptr inttoptr (i64 17 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 18 to ptr), ptr inttoptr (i64 19 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 20 to ptr), ptr inttoptr (i64 21 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 22 to ptr), ptr inttoptr (i64 23 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 24 to ptr), ptr inttoptr (i64 25 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 26 to ptr), ptr inttoptr (i64 27 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 28 to ptr), ptr inttoptr (i64 29 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 30 to ptr), ptr inttoptr (i64 31 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 32 to ptr), ptr inttoptr (i64 33 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 34 to ptr), ptr inttoptr (i64 35 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 36 to ptr), ptr inttoptr (i64 37 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 38 to ptr), ptr inttoptr (i64 39 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 40 to ptr), ptr inttoptr (i64 41 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 42 to ptr), ptr inttoptr (i64 43 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 44 to ptr), ptr inttoptr (i64 45 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 46 to ptr), ptr inttoptr (i64 47 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 48 to ptr), ptr inttoptr (i64 49 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 50 to ptr), ptr inttoptr (i64 51 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 52 to ptr), ptr inttoptr (i64 53 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 54 to ptr), ptr inttoptr (i64 55 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 56 to ptr), ptr inttoptr (i64 57 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 58 to ptr), ptr inttoptr (i64 59 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 60 to ptr), ptr inttoptr (i64 61 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 62 to ptr), ptr inttoptr (i64 63 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 64 to ptr), ptr inttoptr (i64 65 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 66 to ptr), ptr inttoptr (i64 67 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 68 to ptr), ptr inttoptr (i64 69 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 70 to ptr), ptr inttoptr (i64 71 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 72 to ptr), ptr inttoptr (i64 73 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 74 to ptr), ptr inttoptr (i64 75 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 76 to ptr), ptr inttoptr (i64 77 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 78 to ptr), ptr inttoptr (i64 79 to ptr))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr null, i64 24, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 1 to ptr), i64 24, i64 1)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 2 to ptr), i64 24, i64 2)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 3 to ptr), i64 24, i64 3)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 4 to ptr), i64 24, i64 4)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 5 to ptr), i64 24, i64 5)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 6 to ptr), i64 24, i64 6)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 7 to ptr), i64 24, i64 7)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 8 to ptr), i64 24, i64 8)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 9 to ptr), i64 24, i64 9)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 10 to ptr), i64 24, i64 10)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 11 to ptr), i64 24, i64 11)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 12 to ptr), i64 24, i64 12)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 13 to ptr), i64 24, i64 13)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 14 to ptr), i64 24, i64 14)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 15 to ptr), i64 24, i64 15)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 16 to ptr), i64 24, i64 16)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 17 to ptr), i64 24, i64 17)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 18 to ptr), i64 24, i64 18)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 19 to ptr), i64 24, i64 19)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 20 to ptr), i64 24, i64 20)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 21 to ptr), i64 24, i64 21)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 22 to ptr), i64 24, i64 22)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 23 to ptr), i64 24, i64 23)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 24 to ptr), i64 24, i64 24)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 25 to ptr), i64 24, i64 25)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 26 to ptr), i64 24, i64 26)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 27 to ptr), i64 24, i64 27)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 28 to ptr), i64 24, i64 28)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 29 to ptr), i64 24, i64 29)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 30 to ptr), i64 24, i64 30)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 31 to ptr), i64 24, i64 31)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 32 to ptr), i64 24, i64 32)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 33 to ptr), i64 24, i64 33)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 34 to ptr), i64 24, i64 34)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 35 to ptr), i64 24, i64 35)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 36 to ptr), i64 24, i64 36)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 37 to ptr), i64 24, i64 37)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 38 to ptr), i64 24, i64 38)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 39 to ptr), i64 24, i64 39)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 40 to ptr), i64 23, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 41 to ptr), i64 23, i64 1)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 42 to ptr), i64 23, i64 2)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 43 to ptr), i64 23, i64 3)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 44 to ptr), i64 23, i64 4)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 45 to ptr), i64 23, i64 5)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 46 to ptr), i64 23, i64 6)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 47 to ptr), i64 23, i64 7)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 48 to ptr), i64 23, i64 8)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 49 to ptr), i64 23, i64 9)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 50 to ptr), i64 23, i64 10)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 51 to ptr), i64 23, i64 11)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 52 to ptr), i64 23, i64 12)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 53 to ptr), i64 23, i64 13)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 54 to ptr), i64 23, i64 14)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 55 to ptr), i64 23, i64 15)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 56 to ptr), i64 23, i64 16)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 57 to ptr), i64 23, i64 17)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 58 to ptr), i64 23, i64 18)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 59 to ptr), i64 23, i64 19)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 60 to ptr), i64 23, i64 20)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 61 to ptr), i64 23, i64 21)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 62 to ptr), i64 23, i64 22)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 63 to ptr), i64 23, i64 23)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 64 to ptr), i64 23, i64 24)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 65 to ptr), i64 23, i64 25)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 66 to ptr), i64 23, i64 26)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 67 to ptr), i64 23, i64 27)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 68 to ptr), i64 23, i64 28)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 69 to ptr), i64 23, i64 29)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 70 to ptr), i64 23, i64 30)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 71 to ptr), i64 23, i64 31)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 72 to ptr), i64 23, i64 32)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 73 to ptr), i64 23, i64 33)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 74 to ptr), i64 23, i64 34)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 75 to ptr), i64 23, i64 35)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 76 to ptr), i64 23, i64 36)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 77 to ptr), i64 23, i64 37)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 78 to ptr), i64 23, i64 38)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 79 to ptr), i64 23, i64 39)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__cz__body(ptr, ptr)

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__begin_parallel()

declare void @__quantum__rt__end_parallel()

declare void @__quantum__qis__move__body(ptr, i64, i64)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="80" "required_num_results"="0" }

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
    device = NeutralAtomDevice()
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

@0 = internal constant [4 x i8] c"0_t\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr null, i64 26, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 1 to ptr), i64 26, i64 1)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 2 to ptr), i64 26, i64 2)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 3 to ptr), i64 26, i64 3)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 4 to ptr), i64 26, i64 4)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 5 to ptr), i64 26, i64 5)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 6 to ptr), i64 26, i64 6)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 7 to ptr), i64 26, i64 7)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 8 to ptr), i64 26, i64 8)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 9 to ptr), i64 26, i64 9)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 10 to ptr), i64 26, i64 10)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 11 to ptr), i64 26, i64 11)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 12 to ptr), i64 26, i64 12)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 13 to ptr), i64 26, i64 13)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 14 to ptr), i64 26, i64 14)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 15 to ptr), i64 26, i64 15)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 16 to ptr), i64 26, i64 16)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 17 to ptr), i64 26, i64 17)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 18 to ptr), i64 26, i64 18)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 19 to ptr), i64 26, i64 19)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 20 to ptr), i64 26, i64 20)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 21 to ptr), i64 26, i64 21)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 22 to ptr), i64 26, i64 22)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 23 to ptr), i64 26, i64 23)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 24 to ptr), i64 26, i64 24)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 25 to ptr), i64 26, i64 25)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 26 to ptr), i64 26, i64 26)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 27 to ptr), i64 26, i64 27)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 28 to ptr), i64 26, i64 28)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 29 to ptr), i64 26, i64 29)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 30 to ptr), i64 26, i64 30)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 31 to ptr), i64 26, i64 31)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 32 to ptr), i64 26, i64 32)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 33 to ptr), i64 26, i64 33)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 34 to ptr), i64 26, i64 34)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 35 to ptr), i64 26, i64 35)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 36 to ptr), i64 26, i64 36)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 37 to ptr), i64 26, i64 37)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 38 to ptr), i64 26, i64 38)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 39 to ptr), i64 26, i64 39)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 40 to ptr), i64 25, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 41 to ptr), i64 25, i64 1)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 42 to ptr), i64 25, i64 2)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 43 to ptr), i64 25, i64 3)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 44 to ptr), i64 25, i64 4)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 45 to ptr), i64 25, i64 5)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 46 to ptr), i64 25, i64 6)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 47 to ptr), i64 25, i64 7)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 48 to ptr), i64 25, i64 8)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 49 to ptr), i64 25, i64 9)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 50 to ptr), i64 25, i64 10)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 51 to ptr), i64 25, i64 11)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 52 to ptr), i64 25, i64 12)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 53 to ptr), i64 25, i64 13)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 54 to ptr), i64 25, i64 14)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 55 to ptr), i64 25, i64 15)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 56 to ptr), i64 25, i64 16)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 57 to ptr), i64 25, i64 17)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 58 to ptr), i64 25, i64 18)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 59 to ptr), i64 25, i64 19)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 60 to ptr), i64 25, i64 20)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 61 to ptr), i64 25, i64 21)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 62 to ptr), i64 25, i64 22)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 63 to ptr), i64 25, i64 23)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 64 to ptr), i64 25, i64 24)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 65 to ptr), i64 25, i64 25)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 66 to ptr), i64 25, i64 26)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 67 to ptr), i64 25, i64 27)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 68 to ptr), i64 25, i64 28)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 69 to ptr), i64 25, i64 29)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 70 to ptr), i64 25, i64 30)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 71 to ptr), i64 25, i64 31)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 72 to ptr), i64 25, i64 32)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 73 to ptr), i64 25, i64 33)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 74 to ptr), i64 25, i64 34)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 75 to ptr), i64 25, i64 35)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 76 to ptr), i64 25, i64 36)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 77 to ptr), i64 25, i64 37)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 78 to ptr), i64 25, i64 38)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 79 to ptr), i64 25, i64 39)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__cz__body(ptr null, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 5 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 6 to ptr), ptr inttoptr (i64 7 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 8 to ptr), ptr inttoptr (i64 9 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 10 to ptr), ptr inttoptr (i64 11 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 12 to ptr), ptr inttoptr (i64 13 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 14 to ptr), ptr inttoptr (i64 15 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 16 to ptr), ptr inttoptr (i64 17 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 18 to ptr), ptr inttoptr (i64 19 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 20 to ptr), ptr inttoptr (i64 21 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 22 to ptr), ptr inttoptr (i64 23 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 24 to ptr), ptr inttoptr (i64 25 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 26 to ptr), ptr inttoptr (i64 27 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 28 to ptr), ptr inttoptr (i64 29 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 30 to ptr), ptr inttoptr (i64 31 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 32 to ptr), ptr inttoptr (i64 33 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 34 to ptr), ptr inttoptr (i64 35 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 36 to ptr), ptr inttoptr (i64 37 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 38 to ptr), ptr inttoptr (i64 39 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 40 to ptr), ptr inttoptr (i64 41 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 42 to ptr), ptr inttoptr (i64 43 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 44 to ptr), ptr inttoptr (i64 45 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 46 to ptr), ptr inttoptr (i64 47 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 48 to ptr), ptr inttoptr (i64 49 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 50 to ptr), ptr inttoptr (i64 51 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 52 to ptr), ptr inttoptr (i64 53 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 54 to ptr), ptr inttoptr (i64 55 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 56 to ptr), ptr inttoptr (i64 57 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 58 to ptr), ptr inttoptr (i64 59 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 60 to ptr), ptr inttoptr (i64 61 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 62 to ptr), ptr inttoptr (i64 63 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 64 to ptr), ptr inttoptr (i64 65 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 66 to ptr), ptr inttoptr (i64 67 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 68 to ptr), ptr inttoptr (i64 69 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 70 to ptr), ptr inttoptr (i64 71 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 72 to ptr), ptr inttoptr (i64 73 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 74 to ptr), ptr inttoptr (i64 75 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 76 to ptr), ptr inttoptr (i64 77 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 78 to ptr), ptr inttoptr (i64 79 to ptr))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr null, i64 24, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 1 to ptr), i64 24, i64 1)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 2 to ptr), i64 24, i64 2)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 3 to ptr), i64 24, i64 3)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 4 to ptr), i64 24, i64 4)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 5 to ptr), i64 24, i64 5)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 6 to ptr), i64 24, i64 6)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 7 to ptr), i64 24, i64 7)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 8 to ptr), i64 24, i64 8)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 9 to ptr), i64 24, i64 9)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 10 to ptr), i64 24, i64 10)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 11 to ptr), i64 24, i64 11)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 12 to ptr), i64 24, i64 12)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 13 to ptr), i64 24, i64 13)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 14 to ptr), i64 24, i64 14)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 15 to ptr), i64 24, i64 15)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 16 to ptr), i64 24, i64 16)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 17 to ptr), i64 24, i64 17)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 18 to ptr), i64 24, i64 18)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 19 to ptr), i64 24, i64 19)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 20 to ptr), i64 24, i64 20)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 21 to ptr), i64 24, i64 21)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 22 to ptr), i64 24, i64 22)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 23 to ptr), i64 24, i64 23)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 24 to ptr), i64 24, i64 24)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 25 to ptr), i64 24, i64 25)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 26 to ptr), i64 24, i64 26)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 27 to ptr), i64 24, i64 27)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 28 to ptr), i64 24, i64 28)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 29 to ptr), i64 24, i64 29)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 30 to ptr), i64 24, i64 30)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 31 to ptr), i64 24, i64 31)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 32 to ptr), i64 24, i64 32)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 33 to ptr), i64 24, i64 33)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 34 to ptr), i64 24, i64 34)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 35 to ptr), i64 24, i64 35)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 36 to ptr), i64 24, i64 36)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 37 to ptr), i64 24, i64 37)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 38 to ptr), i64 24, i64 38)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 39 to ptr), i64 24, i64 39)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 40 to ptr), i64 23, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 41 to ptr), i64 23, i64 1)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 42 to ptr), i64 23, i64 2)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 43 to ptr), i64 23, i64 3)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 44 to ptr), i64 23, i64 4)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 45 to ptr), i64 23, i64 5)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 46 to ptr), i64 23, i64 6)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 47 to ptr), i64 23, i64 7)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 48 to ptr), i64 23, i64 8)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 49 to ptr), i64 23, i64 9)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 50 to ptr), i64 23, i64 10)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 51 to ptr), i64 23, i64 11)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 52 to ptr), i64 23, i64 12)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 53 to ptr), i64 23, i64 13)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 54 to ptr), i64 23, i64 14)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 55 to ptr), i64 23, i64 15)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 56 to ptr), i64 23, i64 16)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 57 to ptr), i64 23, i64 17)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 58 to ptr), i64 23, i64 18)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 59 to ptr), i64 23, i64 19)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 60 to ptr), i64 23, i64 20)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 61 to ptr), i64 23, i64 21)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 62 to ptr), i64 23, i64 22)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 63 to ptr), i64 23, i64 23)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 64 to ptr), i64 23, i64 24)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 65 to ptr), i64 23, i64 25)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 66 to ptr), i64 23, i64 26)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 67 to ptr), i64 23, i64 27)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 68 to ptr), i64 23, i64 28)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 69 to ptr), i64 23, i64 29)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 70 to ptr), i64 23, i64 30)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 71 to ptr), i64 23, i64 31)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 72 to ptr), i64 23, i64 32)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 73 to ptr), i64 23, i64 33)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 74 to ptr), i64 23, i64 34)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 75 to ptr), i64 23, i64 35)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 76 to ptr), i64 23, i64 36)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 77 to ptr), i64 23, i64 37)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 78 to ptr), i64 23, i64 38)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 79 to ptr), i64 23, i64 39)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr inttoptr (i64 80 to ptr), i64 25, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 81 to ptr), i64 25, i64 1)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 82 to ptr), i64 25, i64 2)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 83 to ptr), i64 25, i64 3)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 84 to ptr), i64 25, i64 4)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 85 to ptr), i64 25, i64 5)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 86 to ptr), i64 25, i64 6)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 87 to ptr), i64 25, i64 7)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 88 to ptr), i64 25, i64 8)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 89 to ptr), i64 25, i64 9)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 90 to ptr), i64 25, i64 10)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 91 to ptr), i64 25, i64 11)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 92 to ptr), i64 25, i64 12)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 93 to ptr), i64 25, i64 13)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 94 to ptr), i64 25, i64 14)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 95 to ptr), i64 25, i64 15)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 96 to ptr), i64 25, i64 16)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 97 to ptr), i64 25, i64 17)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 98 to ptr), i64 25, i64 18)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 99 to ptr), i64 25, i64 19)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 100 to ptr), i64 25, i64 20)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 101 to ptr), i64 25, i64 21)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 102 to ptr), i64 25, i64 22)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 103 to ptr), i64 25, i64 23)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 104 to ptr), i64 25, i64 24)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 105 to ptr), i64 25, i64 25)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 106 to ptr), i64 25, i64 26)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 107 to ptr), i64 25, i64 27)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 108 to ptr), i64 25, i64 28)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 109 to ptr), i64 25, i64 29)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 110 to ptr), i64 25, i64 30)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 111 to ptr), i64 25, i64 31)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 112 to ptr), i64 25, i64 32)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 113 to ptr), i64 25, i64 33)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 114 to ptr), i64 25, i64 34)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 115 to ptr), i64 25, i64 35)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 116 to ptr), i64 25, i64 36)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 117 to ptr), i64 25, i64 37)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 118 to ptr), i64 25, i64 38)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 119 to ptr), i64 25, i64 39)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 80 to ptr), ptr inttoptr (i64 81 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 82 to ptr), ptr inttoptr (i64 83 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 84 to ptr), ptr inttoptr (i64 85 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 86 to ptr), ptr inttoptr (i64 87 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 88 to ptr), ptr inttoptr (i64 89 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 90 to ptr), ptr inttoptr (i64 91 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 92 to ptr), ptr inttoptr (i64 93 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 94 to ptr), ptr inttoptr (i64 95 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 96 to ptr), ptr inttoptr (i64 97 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 98 to ptr), ptr inttoptr (i64 99 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 100 to ptr), ptr inttoptr (i64 101 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 102 to ptr), ptr inttoptr (i64 103 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 104 to ptr), ptr inttoptr (i64 105 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 106 to ptr), ptr inttoptr (i64 107 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 108 to ptr), ptr inttoptr (i64 109 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 110 to ptr), ptr inttoptr (i64 111 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 112 to ptr), ptr inttoptr (i64 113 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 114 to ptr), ptr inttoptr (i64 115 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 116 to ptr), ptr inttoptr (i64 117 to ptr))
  call void @__quantum__qis__cz__body(ptr inttoptr (i64 118 to ptr), ptr inttoptr (i64 119 to ptr))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr inttoptr (i64 80 to ptr), i64 22, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 81 to ptr), i64 22, i64 1)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 82 to ptr), i64 22, i64 2)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 83 to ptr), i64 22, i64 3)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 84 to ptr), i64 22, i64 4)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 85 to ptr), i64 22, i64 5)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 86 to ptr), i64 22, i64 6)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 87 to ptr), i64 22, i64 7)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 88 to ptr), i64 22, i64 8)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 89 to ptr), i64 22, i64 9)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 90 to ptr), i64 22, i64 10)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 91 to ptr), i64 22, i64 11)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 92 to ptr), i64 22, i64 12)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 93 to ptr), i64 22, i64 13)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 94 to ptr), i64 22, i64 14)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 95 to ptr), i64 22, i64 15)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 96 to ptr), i64 22, i64 16)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 97 to ptr), i64 22, i64 17)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 98 to ptr), i64 22, i64 18)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 99 to ptr), i64 22, i64 19)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 100 to ptr), i64 22, i64 20)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 101 to ptr), i64 22, i64 21)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 102 to ptr), i64 22, i64 22)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 103 to ptr), i64 22, i64 23)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 104 to ptr), i64 22, i64 24)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 105 to ptr), i64 22, i64 25)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 106 to ptr), i64 22, i64 26)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 107 to ptr), i64 22, i64 27)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 108 to ptr), i64 22, i64 28)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 109 to ptr), i64 22, i64 29)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 110 to ptr), i64 22, i64 30)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 111 to ptr), i64 22, i64 31)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 112 to ptr), i64 22, i64 32)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 113 to ptr), i64 22, i64 33)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 114 to ptr), i64 22, i64 34)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 115 to ptr), i64 22, i64 35)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 116 to ptr), i64 22, i64 36)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 117 to ptr), i64 22, i64 37)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 118 to ptr), i64 22, i64 38)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 119 to ptr), i64 22, i64 39)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__cz__body(ptr, ptr)

declare void @__quantum__rt__tuple_record_output(i64, ptr)

declare void @__quantum__rt__begin_parallel()

declare void @__quantum__rt__end_parallel()

declare void @__quantum__qis__move__body(ptr, i64, i64)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="120" "required_num_results"="0" }

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
    Schedule(NeutralAtomDevice()).run(module)
    after = PerQubitOrdering()
    ValidateBeginEndParallel().run(module)
    after.run(module)
    check_qubit_ordering_unchanged(after, before)

    assert_expected_inline(
        str(module),
        """\

@0 = internal constant [4 x i8] c"0_r\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(ptr null)
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr null, i64 25, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 1 to ptr), i64 25, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__sx__body(ptr null)
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__cz__body(ptr null, ptr inttoptr (i64 1 to ptr))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr null, i64 24, i64 0)
  call void @__quantum__qis__move__body(ptr inttoptr (i64 1 to ptr), i64 24, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr inttoptr (i64 1 to ptr), i64 25, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__sx__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr inttoptr (i64 1 to ptr), i64 24, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr inttoptr (i64 1 to ptr), i64 27, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr null)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__begin_parallel()
  call void @__quantum__qis__move__body(ptr inttoptr (i64 1 to ptr), i64 24, i64 1)
  call void @__quantum__rt__end_parallel()
  call void @__quantum__rt__result_record_output(ptr null, ptr @0)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)

declare void @__quantum__qis__sx__body(ptr)

declare void @__quantum__qis__cz__body(ptr, ptr)

declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

declare void @__quantum__rt__result_record_output(ptr, ptr)

declare void @__quantum__rt__begin_parallel()

declare void @__quantum__rt__end_parallel()

declare void @__quantum__qis__move__body(ptr, i64, i64)

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
