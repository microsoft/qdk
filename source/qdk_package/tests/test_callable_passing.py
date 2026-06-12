# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest
from qdk import qsharp
from qdk._native import QSharpError
from expecttest import assert_expected_inline


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
    from qdk.code import InvokeWithFive, AddOne

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
    from qdk.code import InvokeWithFive

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
    from qdk.code import InvokeWithFive

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
    from qdk.code import MakeRange, SumRangeFromMaker

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
    from qdk.code import InvokeWithFive, MakeAdd

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
    from qdk.code import InvokeWithQubits, AllH

    qir = qsharp.compile(InvokeWithQubits, 3, AllH)
    assert_expected_inline(
        str(qir),
        """\
%Result = type opaque
%Qubit = type opaque

@0 = internal constant [4 x i8] c"0_t\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
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
""",
    )


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
    from qdk.code import InvokeWithQubits

    all_h = qsharp.eval("AllH")
    qir = qsharp.compile(InvokeWithQubits, 3, all_h)
    assert_expected_inline(
        str(qir),
        """\
%Result = type opaque
%Qubit = type opaque

@0 = internal constant [4 x i8] c"0_t\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
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
""",
    )


def test_qir_from_qsharp_closure_passed_to_python_callable() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("""
        operation InvokeWithQubits(nQubits : Int, f : Qubit[] => Unit) : Unit {
            use qs = Qubit[nQubits];
            f(qs)
        }
    """)
    from qdk.code import InvokeWithQubits

    apply_h = qsharp.eval("ApplyToEach(H, _)")
    qir = qsharp.compile(InvokeWithQubits, 3, apply_h)
    assert_expected_inline(
        str(qir),
        """\
%Result = type opaque
%Qubit = type opaque

@0 = internal constant [4 x i8] c"0_t\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
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
""",
    )


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
    from qdk.code import InvokeWithQubits, AllH

    circuit = qsharp.circuit(InvokeWithQubits, 3, AllH)
    assert_expected_inline(
        str(circuit),
        """q_0    ── H ──
q_1    ── H ──
q_2    ── H ──
""",
    )


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
    from qdk.code import InvokeWithQubits

    all_h = qsharp.eval("AllH")
    circuit = qsharp.circuit(InvokeWithQubits, 3, all_h)
    assert_expected_inline(
        str(circuit),
        """q_0    ── H ──
q_1    ── H ──
q_2    ── H ──
""",
    )


def test_circuit_from_qsharp_closure_passed_to_python_callable() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("""
        operation InvokeWithQubits(nQubits : Int, f : Qubit[] => Unit) : Unit {
            use qs = Qubit[nQubits];
            f(qs)
        }
    """)
    from qdk.code import InvokeWithQubits

    apply_h = qsharp.eval("ApplyToEach(H, _)")
    circuit = qsharp.circuit(InvokeWithQubits, 3, apply_h)
    assert_expected_inline(
        str(circuit),
        """q_0    ── H ──
q_1    ── H ──
q_2    ── H ──
""",
    )


def test_qir_from_callable_returning_closure_passed_to_qsharp_callable() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIFLA)
    qsharp.eval("""
        function First<'T>(arr : 'T[]) : 'T {
            arr[0]
        }
        function MakeOp(qs : Qubit[]) : Unit => Unit is Adj + Ctl {
            () => H(First(qs))
        }
        operation DoOp(make : Qubit[] -> Unit => Unit is Adj + Ctl) : Unit is Adj + Ctl {
            use qs = Qubit[1];
            let op = make(qs);
            op();
        }
    """)
    from qdk.code import DoOp, MakeOp

    qir = qsharp.compile(DoOp, MakeOp)
    assert "__quantum__qis__h__body" in str(qir)


@pytest.mark.xfail(
    raises=QSharpError,
    strict=True,
    reason=(
        "End-to-end gap: the codegen unlock (return_unify on a pinned "
        "ReinvokeOriginal target body) runs inside qir() codegen, but the public Python "
        "eval/init path eagerly runs RCA capability checking on every defined callable "
        "(source/compiler/qsc/src/interpret.rs `with_compiler` + `run_fir_passes`). That "
        "gate rejects the `ReturnWithinDynamicScope` target body at definition time, before "
        "any codegen routing, so neither the SyntheticEntry nor the ReinvokeOriginal variant "
        "is reachable from `qsharp.eval` under an Adaptive profile. The unlock itself is "
        "covered at the codegen layer by the Rust regression "
        "`early_return_in_dynamic_branch_synthetic_and_reinvoke_both_compile_parity` in "
        "source/compiler/qsc/src/codegen/tests.rs. This strict xfail flips (and fails) once "
        "the eager interpreter RCA gains a pinned-body carve-out, at which point the body "
        "below becomes a real passing parity assertion."
    ),
)
def test_qir_early_return_in_dynamic_branch_synthetic_and_reinvoke_parity() -> None:
    # End-to-end parity: a target operation whose body early-returns inside a
    # measurement-dependent branch should compile to QIR under an Adaptive profile for
    # BOTH closure-arg routes. A classical capture is FIR-lowerable and flows through the
    # self-contained synthetic entry; a captured allocated qubit is a runtime identity
    # that forces the pin-based ReinvokeOriginal route, where the body-only
    # signature-preserving sub-pipeline return-unifies the pinned body so the early
    # return becomes flag-guarded forward control flow. Mirrors the Rust regression
    # `early_return_in_dynamic_branch_synthetic_and_reinvoke_both_compile_parity` in
    # source/compiler/qsc/src/codegen/tests.rs.
    #
    # Today this raises QSharpError at the `qsharp.eval` below: the interpreter's eager
    # RCA capability check rejects the `ReturnWithinDynamicScope` body of `RunOp` before
    # codegen routing is ever reached (see the xfail reason above).
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIFLA)
    qsharp.eval("""
        import Std.Measurement.*;

        operation RunOp(op : (Qubit => Unit)) : Int {
            let r = {
                use q = Qubit();
                op(q);
                MResetZ(q)
            };
            if r == One {
                return 1;
            }
            return 2;
        }

        operation Rotate(reps : Int, target : Qubit) : Unit {
            for _ in 1..reps {
                X(target);
            }
        }
        operation MakeRotation(reps : Int) : (Qubit => Unit) {
            return target => Rotate(reps, target);
        }

        operation Entangle(control : Qubit, target : Qubit) : Unit is Adj + Ctl {
            CNOT(control, target);
        }
        operation MakeEntangler(control : Qubit) : (Qubit => Unit) {
            return target => Entangle(control, target);
        }
    """)
    from qdk.code import RunOp

    # SyntheticEntry variant: the closure captures a classical Int, which is
    # FIR-lowerable, so the target routes through the synthetic entry.
    rotation = qsharp.eval("MakeRotation(1)")
    synthetic_qir = str(qsharp.compile(RunOp, rotation))
    assert "__quantum__qis__x__body" in synthetic_qir

    # ReinvokeOriginal variant: the closure captures an allocated qubit (a runtime
    # identity that is NOT FIR-lowerable), forcing the pin-based ReinvokeOriginal
    # route. The allocation is a top-level statement so the captured qubit stays
    # alive for the duration of the closure value.
    entangler = qsharp.eval("use control = Qubit(); MakeEntangler(control)")
    reinvoke_qir = str(qsharp.compile(RunOp, entangler))
    assert (
        "__quantum__qis__cnot__body" in reinvoke_qir
        or "__quantum__qis__cx__body" in reinvoke_qir
    )
