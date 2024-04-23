# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from textwrap import dedent
from qsharp._native import (
    Interpreter,
    Result,
    Pauli,
    QSharpError,
    TargetProfile,
)
import pytest


# Tests for the native Q# interpreter class


def test_output() -> None:
    e = Interpreter(TargetProfile.Unrestricted)

    def callback(output):
        nonlocal called
        called = True
        assert output.__repr__() == "Hello, world!"

    called = False
    value = e.interpret('Message("Hello, world!")', callback)
    assert called


def test_dump_output() -> None:
    e = Interpreter(TargetProfile.Unrestricted)

    def callback(output):
        nonlocal called
        called = True
        assert output.__repr__() == "STATE:\n|10⟩: 1.0000+0.0000𝑖"

    called = False
    value = e.interpret(
        """
    use q1 = Qubit();
    use q2 = Qubit();
    X(q1);
    Microsoft.Quantum.Diagnostics.DumpMachine();
    ResetAll([q1, q2]);
    """,
        callback,
    )
    assert called


def test_quantum_seed() -> None:
    e = Interpreter(TargetProfile.Unrestricted)
    e.set_quantum_seed(42)
    value1 = e.interpret(
        "{ use qs = Qubit[16]; for q in qs { H(q); }; Microsoft.Quantum.Measurement.MResetEachZ(qs) }"
    )
    e = Interpreter(TargetProfile.Unrestricted)
    e.set_quantum_seed(42)
    value2 = e.interpret(
        "{ use qs = Qubit[16]; for q in qs { H(q); }; Microsoft.Quantum.Measurement.MResetEachZ(qs) }"
    )
    assert value1 == value2


def test_classical_seed() -> None:
    e = Interpreter(TargetProfile.Unrestricted)
    e.set_classical_seed(42)
    value1 = e.interpret(
        "{ mutable res = []; for _ in 0..15{ set res += [Microsoft.Quantum.Random.DrawRandomInt(0, 100)]; }; res }"
    )
    e = Interpreter(TargetProfile.Unrestricted)
    e.set_classical_seed(42)
    value2 = e.interpret(
        "{ mutable res = []; for _ in 0..15{ set res += [Microsoft.Quantum.Random.DrawRandomInt(0, 100)]; }; res }"
    )
    assert value1 == value2


def test_dump_machine() -> None:
    e = Interpreter(TargetProfile.Unrestricted)

    def callback(output):
        assert output.__repr__() == "STATE:\n|10⟩: 1.0000+0.0000𝑖"

    value = e.interpret(
        """
    use q1 = Qubit();
    use q2 = Qubit();
    X(q1);
    Microsoft.Quantum.Diagnostics.DumpMachine();
    """,
        callback,
    )
    state_dump = e.dump_machine()
    assert state_dump.qubit_count == 2
    state_dump = state_dump.get_dict()
    assert len(state_dump) == 1
    assert state_dump[2].real == 1.0
    assert state_dump[2].imag == 0.0


def test_error() -> None:
    e = Interpreter(TargetProfile.Unrestricted)

    with pytest.raises(QSharpError) as excinfo:
        e.interpret("a864")
    assert str(excinfo.value).find("name error") != -1


def test_multiple_errors() -> None:
    e = Interpreter(TargetProfile.Unrestricted)

    with pytest.raises(QSharpError) as excinfo:
        e.interpret("operation Foo() : Unit { Bar(); Baz(); }")
    assert str(excinfo.value).find("`Bar` not found") != -1
    assert str(excinfo.value).find("`Baz` not found") != -1


def test_multiple_statements() -> None:
    e = Interpreter(TargetProfile.Unrestricted)
    value = e.interpret("1; Zero")
    assert value == Result.Zero


def test_value_int() -> None:
    e = Interpreter(TargetProfile.Unrestricted)
    value = e.interpret("5")
    assert value == 5


def test_value_double() -> None:
    e = Interpreter(TargetProfile.Unrestricted)
    value = e.interpret("3.1")
    assert value == 3.1


def test_value_bool() -> None:
    e = Interpreter(TargetProfile.Unrestricted)
    value = e.interpret("true")
    assert value == True


def test_value_string() -> None:
    e = Interpreter(TargetProfile.Unrestricted)
    value = e.interpret('"hello"')
    assert value == "hello"


def test_value_result() -> None:
    e = Interpreter(TargetProfile.Unrestricted)
    value = e.interpret("One")
    assert value == Result.One


def test_value_pauli() -> None:
    e = Interpreter(TargetProfile.Unrestricted)
    value = e.interpret("PauliX")
    assert value == Pauli.X


def test_value_tuple() -> None:
    e = Interpreter(TargetProfile.Unrestricted)
    value = e.interpret('(1, "hello", One)')
    assert value == (1, "hello", Result.One)


def test_value_unit() -> None:
    e = Interpreter(TargetProfile.Unrestricted)
    value = e.interpret("()")
    assert value is None


def test_value_array() -> None:
    e = Interpreter(TargetProfile.Unrestricted)
    value = e.interpret("[1, 2, 3]")
    assert value == [1, 2, 3]


def test_target_error() -> None:
    e = Interpreter(TargetProfile.Base)
    with pytest.raises(QSharpError) as excinfo:
        e.interpret("operation Program() : Result { return Zero }")
    assert str(excinfo.value).startswith("Qsc.BaseProfCk.ResultLiteral") != -1


def test_qirgen_compile_error() -> None:
    e = Interpreter(TargetProfile.Base)
    e.interpret("operation Program() : Int { return 0 }")
    with pytest.raises(QSharpError) as excinfo:
        e.qir("Foo()")
    assert str(excinfo.value).startswith("Qsc.Resolve.NotFound") != -1


def test_error_spans_from_multiple_lines() -> None:
    e = Interpreter(TargetProfile.Unrestricted)

    # Qsc.Resolve.Ambiguous is chosen as a test case
    # because it contains multiple spans which can be from different lines
    e.interpret("namespace Other { operation DumpMachine() : Unit { } }")
    e.interpret("open Other;")
    e.interpret("open Microsoft.Quantum.Diagnostics;")
    with pytest.raises(QSharpError) as excinfo:
        e.interpret("DumpMachine()")
    assert str(excinfo.value).startswith("Qsc.Resolve.Ambiguous")


def test_qirgen() -> None:
    e = Interpreter(TargetProfile.Base)
    e.interpret("operation Program() : Result { use q = Qubit(); return M(q) }")
    qir = e.qir("Program()")
    assert isinstance(qir, str)


def test_run_with_shots() -> None:
    e = Interpreter(TargetProfile.Unrestricted)

    def callback(output):
        nonlocal called
        called += 1
        assert output.__repr__() == "Hello, world!"

    called = 0
    e.interpret('operation Foo() : Unit { Message("Hello, world!"); }', callback)
    assert called == 0

    value = []
    for _ in range(5):
        value.append(e.run("Foo()", callback))
    assert called == 5

    assert value == [None, None, None, None, None]


def test_dump_circuit() -> None:
    e = Interpreter(TargetProfile.Unrestricted)
    e.interpret(
        """
    use q1 = Qubit();
    use q2 = Qubit();
    X(q1);
    """
    )
    circuit = e.dump_circuit()
    assert str(circuit) == dedent(
        """\
        q_0    ── X ──
        q_1    ───────
        """
    )

    e.interpret("X(q2);")
    circuit = e.dump_circuit()
    assert str(circuit) == dedent(
        """\
        q_0    ── X ──
        q_1    ── X ──
        """
    )


def test_entry_expr_circuit() -> None:
    e = Interpreter(TargetProfile.Unrestricted)
    e.interpret("operation Foo() : Result { use q = Qubit(); H(q); return M(q) }")
    circuit = e.circuit("Foo()")
    assert str(circuit) == dedent(
        """\
        q_0    ── H ──── M ──
                         ╘═══
        """
    )


# this is by design
def test_callables_failing_profile_validation_are_still_registered() -> None:
    e = Interpreter(TargetProfile.Quantinuum)
    with pytest.raises(Exception) as excinfo:
        e.interpret(
            "operation Foo() : Double { use q = Qubit(); mutable x = 1.0; if MResetZ(q) == One { set x = 2.0; } x }"
        )
    assert "Qsc.CapabilitiesCk.UseOfDynamicDouble" in str(excinfo)
    with pytest.raises(Exception) as excinfo:
        e.interpret("Foo()")
    assert "Qsc.CapabilitiesCk.UseOfDynamicDouble" in str(excinfo)


# this is by design
def test_once_rca_validation_fails_following_calls_also_fail() -> None:
    e = Interpreter(TargetProfile.Quantinuum)
    with pytest.raises(Exception) as excinfo:
        e.interpret(
            "operation Foo() : Double { use q = Qubit(); mutable x = 1.0; if MResetZ(q) == One { set x = 2.0; } x }"
        )
    assert "Qsc.CapabilitiesCk.UseOfDynamicDouble" in str(excinfo)
    with pytest.raises(Exception) as excinfo:
        e.interpret("let x = 5;")
    assert "Qsc.CapabilitiesCk.UseOfDynamicDouble" in str(excinfo)


def test_adaptive_errors_are_raised_when_interpreting() -> None:
    e = Interpreter(TargetProfile.Quantinuum)
    with pytest.raises(Exception) as excinfo:
        e.interpret(
            "operation Foo() : Double { use q = Qubit(); mutable x = 1.0; if MResetZ(q) == One { set x = 2.0; } x }"
        )
    assert "Qsc.CapabilitiesCk.UseOfDynamicDouble" in str(excinfo)


def test_adaptive_errors_are_raised_from_entry_expr() -> None:
    e = Interpreter(TargetProfile.Quantinuum)
    e.interpret("use q = Qubit();")
    with pytest.raises(Exception) as excinfo:
        e.run("{mutable x = 1.0; if MResetZ(q) == One { set x = 2.0; }}")
    assert "Qsc.CapabilitiesCk.UseOfDynamicDouble" in str(excinfo)


def test_quantinuum_qir_can_be_generated() -> None:
    adaptive_input = """
        namespace Test {
            open Microsoft.Quantum.Math;
            open QIR.Intrinsic;
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let pi_over_two = 4.0 / 2.0;
                __quantum__qis__rz__body(pi_over_two, q);
                mutable some_angle = ArcSin(0.0);
                __quantum__qis__rz__body(some_angle, q);
                set some_angle = ArcCos(-1.0) / PI();
                __quantum__qis__rz__body(some_angle, q);
                __quantum__qis__mresetz__body(q)
            }
        }
        """
    e = Interpreter(TargetProfile.Quantinuum)
    e.interpret(adaptive_input)
    qir = e.qir("Test.Main()")
    assert qir == dedent(
        """\
        %Result = type opaque
        %Qubit = type opaque

        define void @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__qis__rz__body(double 2.0, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.0, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 1.0, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
          ret void
        }

        declare void @__quantum__qis__rz__body(double, %Qubit*)

        declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9, !10}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 1, !"classical_ints", i1 true}
        !5 = !{i32 1, !"qubit_resetting", i1 true}
        !6 = !{i32 1, !"classical_floats", i1 false}
        !7 = !{i32 1, !"backwards_branching", i1 false}
        !8 = !{i32 1, !"classical_fixed_points", i1 false}
        !9 = !{i32 1, !"user_functions", i1 false}
        !10 = !{i32 1, !"multiple_target_branching", i1 false}
        """
    )


def test_base_qir_can_be_generated() -> None:
    base_input = """
        namespace Test {
            open Microsoft.Quantum.Math;
            open QIR.Intrinsic;
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let pi_over_two = 4.0 / 2.0;
                __quantum__qis__rz__body(pi_over_two, q);
                mutable some_angle = ArcSin(0.0);
                __quantum__qis__rz__body(some_angle, q);
                set some_angle = ArcCos(-1.0) / PI();
                __quantum__qis__rz__body(some_angle, q);
                __quantum__qis__mresetz__body(q)
            }
        }
        """
    e = Interpreter(TargetProfile.Base)
    e.interpret(base_input)
    qir = e.qir("Test.Main()")
    assert qir == dedent(
        """\
        %Result = type opaque
        %Qubit = type opaque

        define void @ENTRYPOINT__main() #0 {
          call void @__quantum__qis__rz__body(double 2.0, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.0, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 1.0, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*)) #1
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
          ret void
        }

        declare void @__quantum__qis__ccx__body(%Qubit*, %Qubit*, %Qubit*)
        declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)
        declare void @__quantum__qis__cy__body(%Qubit*, %Qubit*)
        declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)
        declare void @__quantum__qis__rx__body(double, %Qubit*)
        declare void @__quantum__qis__rxx__body(double, %Qubit*, %Qubit*)
        declare void @__quantum__qis__ry__body(double, %Qubit*)
        declare void @__quantum__qis__ryy__body(double, %Qubit*, %Qubit*)
        declare void @__quantum__qis__rz__body(double, %Qubit*)
        declare void @__quantum__qis__rzz__body(double, %Qubit*, %Qubit*)
        declare void @__quantum__qis__h__body(%Qubit*)
        declare void @__quantum__qis__s__body(%Qubit*)
        declare void @__quantum__qis__s__adj(%Qubit*)
        declare void @__quantum__qis__t__body(%Qubit*)
        declare void @__quantum__qis__t__adj(%Qubit*)
        declare void @__quantum__qis__x__body(%Qubit*)
        declare void @__quantum__qis__y__body(%Qubit*)
        declare void @__quantum__qis__z__body(%Qubit*)
        declare void @__quantum__qis__swap__body(%Qubit*, %Qubit*)
        declare void @__quantum__qis__mz__body(%Qubit*, %Result* writeonly) #1
        declare void @__quantum__rt__result_record_output(%Result*, i8*)
        declare void @__quantum__rt__array_record_output(i64, i8*)
        declare void @__quantum__rt__tuple_record_output(i64, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        """
    )


def test_operation_circuit() -> None:
    e = Interpreter(TargetProfile.Unrestricted)
    e.interpret("operation Foo(q: Qubit) : Result { H(q); return M(q) }")
    circuit = e.circuit(operation="Foo")
    assert str(circuit) == dedent(
        """\
        q_0    ── H ──── M ──
                         ╘═══
        """
    )


def test_unsupported_operation_circuit() -> None:
    e = Interpreter(TargetProfile.Unrestricted)
    e.interpret("operation Foo(n: Int) : Result { return One }")
    with pytest.raises(QSharpError) as excinfo:
        circuit = e.circuit(operation="Foo")
    assert (
        str(excinfo.value).find(
            "expression does not evaluate to an operation that takes qubit parameters"
        )
        != -1
    )
