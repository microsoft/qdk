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
from qsharp._qsharp import qsharp_value_to_python_value
import pytest

# Test helpers


def check_interpret(source: str, expect: str):
    e = Interpreter(TargetProfile.Unrestricted)
    value = qsharp_value_to_python_value(e.interpret(source))
    assert str(value) == expect


def check_invoke(source: str, callable: str, expect: str):
    e = None
    f = None

    def _make_callable(callable, namespace, callable_name):
        nonlocal f
        f = callable

    e = Interpreter(TargetProfile.Unrestricted, make_callable=_make_callable)
    e.interpret(source)
    e.interpret(callable)
    value = qsharp_value_to_python_value(e.invoke(f))
    assert str(value) == expect


def check_run(entry_expr: str, expect: str):
    e = Interpreter(TargetProfile.Unrestricted)
    value = qsharp_value_to_python_value(e.run(entry_expr))
    assert str(value) == expect


def check_circuit(entry_expr: str, expect):
    e = Interpreter(TargetProfile.Unrestricted)
    value = e.circuit(entry_expr)
    assert str(value) == expect


def check_qir(source: str, entry_expr, expect):
    e = Interpreter(TargetProfile.Base)
    e.interpret(source)
    value = e.qir(entry_expr)
    assert str(value) == expect


def check_estimate(source: str):
    e = Interpreter(TargetProfile.Unrestricted)
    e.estimate("", source)


def check_logical_counts(source: str):
    e = Interpreter(TargetProfile.Unrestricted)
    e.logical_counts(source)


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


def test_value_complex() -> None:
    e = Interpreter(TargetProfile.Unrestricted)
    value = e.interpret("new Std.Math.Complex { Real = 2.0, Imag = 3.0 }")
    assert value == 2 + 3j


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


def test_value_udt() -> None:
    udt_def = "struct Data { a: Int, b: Int }"
    new_udt = "new Data { a = 2, b = 3 }"
    callable = f"function makeData() : Data {{ {new_udt} }}"
    entry_expr = f"{{ {udt_def} {new_udt} }}"
    output = "Data(a=2, b=3)"

    check_interpret(entry_expr, output)
    check_run(entry_expr, output)
    check_invoke(udt_def, callable, output)
    check_circuit(entry_expr, "")
    check_estimate(entry_expr)
    check_logical_counts(entry_expr)
    with pytest.raises(QSharpError, match="Qsc.CapabilitiesCk.UseOfAdvancedOutput"):
        check_qir(udt_def + callable, "makeData()", "")


def test_value_nested_udts() -> None:
    udt_def = """
        struct Data { a: Int, b: MoreData }
        struct MoreData { c: Int, d: Int }
    """
    new_udt = "new Data { a = 2, b = new MoreData { c = 3, d = 4 } }"
    callable = f"function makeData() : Data {{ {new_udt} }}"
    entry_expr = f"{{ {udt_def} {new_udt} }}"
    output = "Data(a=2, b=MoreData(c=3, d=4))"

    check_interpret(entry_expr, output)
    check_run(entry_expr, output)
    check_invoke(udt_def, callable, output)
    check_circuit(entry_expr, "")
    check_estimate(entry_expr)
    check_logical_counts(entry_expr)
    with pytest.raises(QSharpError, match="Qsc.CapabilitiesCk.UseOfAdvancedOutput"):
        check_qir(udt_def + callable, "makeData()", "")


def test_value_udts_with_complex_field() -> None:
    udt_def = "struct Data { a: Std.Math.Complex }"
    new_udt = "new Data { a = new Std.Math.Complex { Real = 2.0, Imag = 3.0 } }"
    callable = f"function makeData() : Data {{ {new_udt} }}"
    entry_expr = f"{{ {udt_def} {new_udt} }}"
    output = "Data(a=(2+3j))"

    check_interpret(entry_expr, output)
    check_run(entry_expr, output)
    check_invoke(udt_def, callable, output)
    check_circuit(entry_expr, "")
    check_estimate(entry_expr)
    check_logical_counts(entry_expr)
    with pytest.raises(QSharpError, match="Qsc.CapabilitiesCk.UseOfAdvancedOutput"):
        check_qir(udt_def + callable, "makeData()", "")


def test_value_udts_with_array_field() -> None:
    udt_def = "struct Data { a: Int[] }"
    new_udt = "new Data { a = [2, 3, 4] }"
    callable = f"function makeData() : Data {{ {new_udt} }}"
    entry_expr = f"{{ {udt_def} {new_udt} }}"
    output = "Data(a=[2, 3, 4])"

    check_interpret(entry_expr, output)
    check_run(entry_expr, output)
    check_invoke(udt_def, callable, output)
    check_circuit(entry_expr, "")
    check_estimate(entry_expr)
    check_logical_counts(entry_expr)
    with pytest.raises(QSharpError, match="Qsc.CapabilitiesCk.UseOfAdvancedOutput"):
        check_qir(udt_def + callable, "makeData()", "")


def test_value_udts_with_tuple_field() -> None:
    udt_def = "struct Data { a: (Int, Int, Int) }"
    new_udt = "new Data { a = (2, 3, 4) }"
    callable = f"function makeData() : Data {{ {new_udt} }}"
    entry_expr = f"{{ {udt_def} {new_udt} }}"
    output = "Data(a=(2, 3, 4))"

    check_interpret(entry_expr, output)
    check_run(entry_expr, output)
    check_invoke(udt_def, callable, output)
    check_circuit(entry_expr, "")
    check_estimate(entry_expr)
    check_logical_counts(entry_expr)
    with pytest.raises(QSharpError, match="Qsc.CapabilitiesCk.UseOfAdvancedOutput"):
        check_qir(udt_def + callable, "makeData()", "")


def test_value_array_of_udts() -> None:
    udt_def = "struct Data { a: Int }"
    new_udt = "[new Data { a = 2 }, new Data { a = 3 }]"
    callable = f"function makeData() : Data[] {{ {new_udt} }}"
    entry_expr = f"{{ {udt_def} {new_udt} }}"
    output = "[Data(a=2), Data(a=3)]"

    check_interpret(entry_expr, output)
    check_run(entry_expr, output)
    check_invoke(udt_def, callable, output)
    check_circuit(entry_expr, "")
    check_estimate(entry_expr)
    check_logical_counts(entry_expr)
    with pytest.raises(QSharpError, match="Qsc.CapabilitiesCk.UseOfAdvancedOutput"):
        check_qir(udt_def + callable, "makeData()", "")


def test_value_array_of_complex() -> None:
    new_udt = "[new Std.Math.Complex { Real = 2.0, Imag = 3.0 }]"
    callable = f"function makeData() : Std.Math.Complex[] {{ {new_udt} }}"
    entry_expr = f"{{ {new_udt} }}"
    output = "[(2+3j)]"

    check_interpret(entry_expr, output)
    check_run(entry_expr, output)
    check_invoke("", callable, output)
    check_circuit(entry_expr, "")
    check_estimate(entry_expr)
    check_logical_counts(entry_expr)
    with pytest.raises(QSharpError, match="Qsc.CapabilitiesCk.UseOfAdvancedOutput"):
        check_qir(callable, "makeData()", "")


def test_value_tuple_of_udts() -> None:
    udt_def = "struct Data { a: Int }"
    new_udt = "(new Data { a = 2 }, new Data { a = 3 })"
    callable = f"function makeData() : (Data, Data) {{ {new_udt} }}"
    entry_expr = f"{{ {udt_def} {new_udt} }}"
    output = "(Data(a=2), Data(a=3))"

    check_interpret(entry_expr, output)
    check_run(entry_expr, output)
    check_invoke(udt_def, callable, output)
    check_circuit(entry_expr, "")
    check_estimate(entry_expr)
    check_logical_counts(entry_expr)
    with pytest.raises(QSharpError, match="Qsc.CapabilitiesCk.UseOfAdvancedOutput"):
        check_qir(udt_def + callable, "makeData()", "")


def test_value_tuple_of_complex() -> None:
    new_udt = "(new Std.Math.Complex { Real = 2.0, Imag = 3.0 },)"
    callable = f"function makeData() : (Std.Math.Complex,) {{ {new_udt} }}"
    entry_expr = f"{{ {new_udt} }}"
    output = "((2+3j),)"

    check_interpret(entry_expr, output)
    check_run(entry_expr, output)
    check_invoke("", callable, output)
    check_circuit(entry_expr, "")
    check_estimate(entry_expr)
    check_logical_counts(entry_expr)
    with pytest.raises(QSharpError, match="Qsc.CapabilitiesCk.UseOfAdvancedOutput"):
        check_qir(callable, "makeData()", "")


def test_target_error() -> None:
    e = Interpreter(TargetProfile.Base)
    with pytest.raises(QSharpError) as excinfo:
        e.interpret(
            "operation Program() : Result { use q = Qubit(); if M(q) == Zero { return Zero } else { return One } }"
        )
    assert str(excinfo.value).startswith("Qsc.CapabilitiesCk.UseOfDynamicBool")


def test_qirgen_compile_error() -> None:
    e = Interpreter(TargetProfile.Base)
    e.interpret("operation Program() : Int { return 0 }")
    with pytest.raises(QSharpError) as excinfo:
        e.qir("Foo()")
    assert str(excinfo.value).startswith("Qsc.Resolve.NotFound")


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


def test_swap_label_circuit() -> None:
    e = Interpreter(TargetProfile.Unrestricted)
    e.interpret(
        "operation Foo() : Unit { use q1 = Qubit(); use q2 = Qubit(); X(q1); Relabel([q1, q2], [q2, q1]); X(q2); }"
    )
    circuit = e.circuit("Foo()")
    assert str(circuit) == dedent(
        """\
        q_0    ── X ──── X ──
        q_1    ──────────────
        """
    )


def test_callables_failing_profile_validation_are_not_registered() -> None:
    e = Interpreter(TargetProfile.Adaptive_RI)
    with pytest.raises(Exception) as excinfo:
        e.interpret(
            "operation Foo() : Double { use q = Qubit(); mutable x = 1.0; if MResetZ(q) == One { set x = 2.0; } x }"
        )
    assert "Qsc.CapabilitiesCk.UseOfDynamicDouble" in str(excinfo)
    # In this case, the callable Foo failed compilation late enough that the symbol is bound. This makes later
    # use of `Foo` valid from a name resolution standpoint, but the callable cannot be invoked because it was found
    # to be invalid for the current profile. To stay consistent with the behavior of other compilations that
    # leave unbound symbols, the call will compile but fail to run.
    with pytest.raises(Exception) as excinfo:
        e.interpret("Foo()")
    assert "Qsc.Eval.UnboundName" in str(excinfo)


def test_once_callable_fails_profile_validation_it_fails_compile_to_QIR() -> None:
    e = Interpreter(TargetProfile.Adaptive_RI)
    with pytest.raises(Exception) as excinfo:
        e.interpret(
            "operation Foo() : Double { use q = Qubit(); mutable x = 1.0; if MResetZ(q) == One { set x = 2.0; } x }"
        )
    assert "Qsc.CapabilitiesCk.UseOfDynamicDouble" in str(excinfo)
    with pytest.raises(Exception) as excinfo:
        e.qir("{Foo();}")
    assert "Qsc.PartialEval.EvaluationFailed" in str(excinfo)
    assert "name is not bound" in str(excinfo)


def test_once_rca_validation_fails_following_calls_do_not_fail() -> None:
    e = Interpreter(TargetProfile.Adaptive_RI)
    with pytest.raises(Exception) as excinfo:
        e.interpret(
            "operation Foo() : Double { use q = Qubit(); mutable x = 1.0; if MResetZ(q) == One { set x = 2.0; } x }"
        )
    assert "Qsc.CapabilitiesCk.UseOfDynamicDouble" in str(excinfo)
    value = e.interpret("let x = 5; x")
    assert value == 5


def test_adaptive_errors_are_raised_when_interpreting() -> None:
    e = Interpreter(TargetProfile.Adaptive_RI)
    with pytest.raises(Exception) as excinfo:
        e.interpret(
            "operation Foo() : Double { use q = Qubit(); mutable x = 1.0; if MResetZ(q) == One { set x = 2.0; } x }"
        )
    assert "Qsc.CapabilitiesCk.UseOfDynamicDouble" in str(excinfo)


def test_adaptive_errors_are_raised_from_entry_expr() -> None:
    e = Interpreter(TargetProfile.Adaptive_RI)
    e.interpret("use q = Qubit();")
    with pytest.raises(Exception) as excinfo:
        e.run("{mutable x = 1.0; if MResetZ(q) == One { set x = 2.0; }}")
    assert "Qsc.CapabilitiesCk.UseOfDynamicDouble" in str(excinfo)


def test_adaptive_ri_qir_can_be_generated() -> None:
    adaptive_input = """
        namespace Test {
            import Std.Math.*;
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
    e = Interpreter(TargetProfile.Adaptive_RI)
    e.interpret(adaptive_input)
    qir = e.qir("Test.Main()")
    assert qir == dedent(
        """\
        %Result = type opaque
        %Qubit = type opaque

        @empty_tag = internal constant [1 x i8] c"\\00"
        @0 = internal constant [4 x i8] c"0_r\\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__qis__rz__body(double 2.0, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.0, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 1.0, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__qis__rz__body(double, %Qubit*)

        declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        """
    )


def test_base_qir_can_be_generated() -> None:
    base_input = """
        namespace Test {
            import Std.Math.*;
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

        @empty_tag = internal constant [1 x i8] c"\\00"
        @0 = internal constant [4 x i8] c"0_r\\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__qis__rz__body(double 2.0, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.0, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 1.0, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__qis__rz__body(double, %Qubit*)

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

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


def test_results_are_comparable() -> None:
    e = Interpreter(TargetProfile.Unrestricted)
    r = e.interpret("[One, Zero]")
    assert r == [Result.One, Result.Zero]
    r.sort()
    assert r == [Result.Zero, Result.One]
