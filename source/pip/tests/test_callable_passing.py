# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import qsharp
from expecttest import assert_expected_inline
from textwrap import dedent


def test_python_callable_passed_to_python_callable() -> None:
    qsharp.init()
    qsharp.eval("""
        function InvokeWithFive(f : Int -> Int) : Int {
            f(5)
        }
        function AddOne(x : Int) : Int {
            x + 1
        }
    """)
    from qsharp.code import InvokeWithFive, AddOne
    assert InvokeWithFive(AddOne) == 6


def test_python_callable_passed_to_qsharp_callable() -> None:
    qsharp.init()
    qsharp.eval("""
        function InvokeWithFive(f : Int -> Int) : Int {
            f(5)
        }
        function AddOne(x : Int) : Int {
            x + 1
        }
    """)
    from qsharp.code import InvokeWithFive
    f = qsharp.eval("AddOne")
    assert InvokeWithFive(f) == 6


def test_run_qsharp_callable_passed_to_qsharp_callable() -> None:
    qsharp.init()
    qsharp.eval("""
        function InvokeWithFive(f : Int -> Int) : Int {
            f(5)
        }
        function AddOne(x : Int) : Int {
            x + 1
        }
    """)
    invoke_with_five = qsharp.eval("InvokeWithFive")
    add_one = qsharp.eval("AddOne")
    res = qsharp.run(invoke_with_five, 1, add_one)[0]
    assert res == 6


def test_run_qsharp_callable_passed_to_python_callable() -> None:
    qsharp.init()
    qsharp.eval("""
        function InvokeWithFive(f : Int -> Int) : Int {
            f(5)
        }
        function AddOne(x : Int) : Int {
            x + 1
        }
    """)
    from qsharp.code import InvokeWithFive
    add_one = qsharp.eval("AddOne")
    res = qsharp.run(InvokeWithFive, 1, add_one)[0]
    assert res == 6


def test_python_callable_with_unsupported_types_passed_to_python_callable() -> None:
    qsharp.init()
    qsharp.eval("""
        function MakeRange() : Range {
            1..10
        }
        function SumRangeFromMaker(maker : Unit -> Range) : Int {
            mutable sum = 0;
            for v in maker() {
                sum += v;
            }
            sum
        }
    """)
    from qsharp.code import MakeRange, SumRangeFromMaker
    assert SumRangeFromMaker(MakeRange) == 55


def test_qsharp_closure_from_python_callable_passed_to_python_callable() -> None:
    qsharp.init()
    qsharp.eval("""
        function InvokeWithFive(f : Int -> Int) : Int {
            f(5)
        }
        function MakeAdd(inc : Int) : Int -> Int {
            x -> x + inc
        }
    """)
    from qsharp.code import InvokeWithFive, MakeAdd
    assert InvokeWithFive(MakeAdd(1)) == 6


def test_qir_from_python_callable_passed_to_python_callable() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("""
        operation InvokeWithQubits(nQubits : Int, f : Qubit[] => Unit) : Unit {
            use qs = Qubit[nQubits];
            f(qs)
        }
        operation AllH(qs : Qubit[]) : Unit {
            ApplyToEach(H, qs);
        }
    """)
    from qsharp.code import InvokeWithQubits, AllH
    qir = qsharp.compile(InvokeWithQubits, 3, AllH)
    assert_expected_inline(str(qir), """\
%Result = type opaque
%Qubit = type opaque

@empty_tag = internal constant [1 x i8] c"\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="3" "required_num_results"="0" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
""")


def test_qir_from_qsharp_callable_passed_to_python_callable() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("""
        operation InvokeWithQubits(nQubits : Int, f : Qubit[] => Unit) : Unit {
            use qs = Qubit[nQubits];
            f(qs)
        }
        operation AllH(qs : Qubit[]) : Unit {
            ApplyToEach(H, qs);
        }
    """)
    from qsharp.code import InvokeWithQubits
    all_h = qsharp.eval("AllH")
    qir = qsharp.compile(InvokeWithQubits, 3, all_h)
    assert_expected_inline(str(qir), """\
%Result = type opaque
%Qubit = type opaque

@empty_tag = internal constant [1 x i8] c"\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="3" "required_num_results"="0" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
""")


def test_qir_from_qsharp_closure_passed_to_python_callable() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("""
        operation InvokeWithQubits(nQubits : Int, f : Qubit[] => Unit) : Unit {
            use qs = Qubit[nQubits];
            f(qs)
        }
    """)
    from qsharp.code import InvokeWithQubits
    apply_h = qsharp.eval("ApplyToEach(H, _)")
    qir = qsharp.compile(InvokeWithQubits, 3, apply_h)
    assert_expected_inline(str(qir), """\
%Result = type opaque
%Qubit = type opaque

@empty_tag = internal constant [1 x i8] c"\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="3" "required_num_results"="0" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
""")


def test_circuit_from_python_callable_passed_to_python_callable() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("""
        operation InvokeWithQubits(nQubits : Int, f : Qubit[] => Unit) : Unit {
            use qs = Qubit[nQubits];
            f(qs)
        }
        operation AllH(qs : Qubit[]) : Unit {
            ApplyToEach(H, qs);
        }
    """)
    from qsharp.code import InvokeWithQubits, AllH
    circuit = qsharp.circuit(InvokeWithQubits, 3, AllH)
    assert_expected_inline(str(circuit), """q_0    ── H ──
q_1    ── H ──
q_2    ── H ──
""")


def test_circuit_from_qsharp_callable_passed_to_python_callable() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("""
        operation InvokeWithQubits(nQubits : Int, f : Qubit[] => Unit) : Unit {
            use qs = Qubit[nQubits];
            f(qs)
        }
        operation AllH(qs : Qubit[]) : Unit {
            ApplyToEach(H, qs);
        }
    """)
    from qsharp.code import InvokeWithQubits
    all_h = qsharp.eval("AllH")
    circuit = qsharp.circuit(InvokeWithQubits, 3, all_h)
    assert_expected_inline(str(circuit), """q_0    ── H ──
q_1    ── H ──
q_2    ── H ──
""")


def test_circuit_from_qsharp_closure_passed_to_python_callable() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("""
        operation InvokeWithQubits(nQubits : Int, f : Qubit[] => Unit) : Unit {
            use qs = Qubit[nQubits];
            f(qs)
        }
    """)
    from qsharp.code import InvokeWithQubits
    apply_h = qsharp.eval("ApplyToEach(H, _)")
    circuit = qsharp.circuit(InvokeWithQubits, 3, apply_h)
    assert_expected_inline(str(circuit), """q_0    ── H ──
q_1    ── H ──
q_2    ── H ──
""")


