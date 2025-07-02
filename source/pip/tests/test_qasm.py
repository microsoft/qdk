# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from math import pi
from textwrap import dedent
import pytest
from qsharp import (
    init,
    TargetProfile,
    set_quantum_seed,
    BitFlipNoise,
    QSharpError,
    Result,
)
from qsharp.estimator import EstimatorParams, QubitParams, QECScheme, LogicalCounts
from qsharp.openqasm import (
    import_openqasm,
    run,
    compile,
    circuit,
    estimate,
    ProgramType,
    QasmError,
)
import qsharp.code as code

# Run


def test_run_with_noise_produces_noisy_results() -> None:
    set_quantum_seed(0)
    result = run(
        """
        include "stdgates.inc";
        qubit q1;
        qubit q2;
        output int errors;
        for int i in [0:100] {
          h q1;
          cx q1, q2;
          bit[2] c;
          c[0] = measure q1;
          c[1] = measure q2;
          reset q1;
          reset q2;
          if (c[0] != c[1]) { errors += 1; }
        }
        """,
        shots=1,
        noise=BitFlipNoise(0.1),
    )
    assert result[0] > 5
    result = run(
        """
        include "stdgates.inc";
        output int errors;
        qubit q;
        for int i in [0:100] {
          bit c = measure q;
          reset q;
          if (c != 0) { errors+=1; }
        }
        """,
        shots=1,
        noise=BitFlipNoise(0.1),
    )
    assert result[0] > 5


def test_run_with_qubit_loss_produces_lossy_results() -> None:
    set_quantum_seed(0)
    result = run(
        """
        qubit q1;
        bit c1;
        c1 = measure q1;
        """,
        shots=1,
        qubit_loss=1.0,
    )
    assert result[0] == Result.Loss


def test_run_with_qubit_loss_detects_loss_with_mresetzchecked() -> None:
    set_quantum_seed(0)
    result = run(
        """
        include "qdk.inc";
        qubit q1;
        int r;
        r = mresetz_checked(q1);
        """,
        shots=1,
        qubit_loss=1.0,
    )
    assert result[0] == 2


def test_run_without_qubit_loss_does_not_detect_loss_with_mresetzchecked() -> None:
    set_quantum_seed(0)
    result = run(
        """
        include "qdk.inc";
        qubit q1;
        int r;
        r = mresetz_checked(q1);
        """,
        shots=1,
    )
    assert result[0] == 0


def test_mresetzchecked_not_present_without_qdk_inc() -> None:
    set_quantum_seed(0)
    with pytest.raises(QasmError) as excinfo:
        run(
            """
            include "stdgates.inc";
            qubit q1;
            bit[2] r;
            r = mresetz_checked(q1);
            """,
            shots=1,
        )
    assert "undefined symbol: mresetz_checked" in str(excinfo.value)


def test_run_with_result(capsys) -> None:
    results = run("output bit c;", 3)
    assert results == [Result.Zero, Result.Zero, Result.Zero]


# Import


def test_import_creates_python_callable_by_default_named_program() -> None:
    init(target_profile=TargetProfile.Base)
    import_openqasm("")
    assert code.program is not None


def test_import_python_callable_name_can_be_set() -> None:
    init(target_profile=TargetProfile.Base)
    import_openqasm("", name="Foo")
    assert code.Foo is not None


def test_import_can_process_fragments_to_modify_intereter_state() -> None:
    init(target_profile=TargetProfile.Base)
    import_openqasm("int x = 42;", program_type=ProgramType.Fragments)
    from qsharp import eval as qsharp_eval

    assert qsharp_eval("x") == 42


def test_import_can_declare_callables_from_fragments() -> None:
    init(target_profile=TargetProfile.Base)
    import_openqasm(
        "def Foo() -> int { return 42; }", program_type=ProgramType.Fragments
    )
    from qsharp import eval as qsharp_eval

    assert qsharp_eval("Foo()") == 42


def test_import_can_declare_files_with_namespaces() -> None:
    init(target_profile=TargetProfile.Adaptive_RI)
    import_openqasm("output int x; x = 42;", program_type=ProgramType.File)
    from qsharp import eval as qsharp_eval

    assert qsharp_eval("qasm_import.program()") == 42


# Import + Run


def test_run_imported_with_noise_produces_noisy_results() -> None:
    init()
    set_quantum_seed(0)
    import_openqasm(
        """
        include "stdgates.inc";
        qubit q1;
        qubit q2;
        output int errors;
        for int i in [0:100] {
          h q1;
          cx q1, q2;
          bit[2] c;
          c[0] = measure q1;
          c[1] = measure q2;
          reset q1;
          reset q2;
          if (c[0] != c[1]) { errors += 1; }
        }
        """,
        name="Program0",
    )
    result = run(code.Program0, shots=1, noise=BitFlipNoise(0.1))
    assert result[0] > 5

    result = import_openqasm(
        """
        include "stdgates.inc";
        output int errors;
        qubit q;
        for int i in [0:100] {
          bit c = measure q;
          reset q;
          if (c != 0) { errors+=1; }
        }
        """,
        name="Program1",
    )
    result = run(code.Program1, shots=1, noise=BitFlipNoise(0.1))
    assert result[0] > 5


def test_run_with_result_from_callable(capsys) -> None:
    init()
    import_openqasm("output bit c;", name="Foo")
    results = run(code.Foo, 3)
    assert results == [Result.Zero, Result.Zero, Result.Zero]


def test_run_with_result_callback(capsys) -> None:
    def on_result(result):
        nonlocal called
        called = True
        assert result["result"] == Result.Zero

    called = False
    init()
    import_openqasm("output bit c;", name="Foo")
    results = run(code.Foo, 3, on_result=on_result, save_events=True)
    assert (
        str(results)
        == "[{'result': Zero, 'events': [], 'matrices': [], 'dumps': [], 'messages': []}, {'result': Zero, 'events': [], 'matrices': [], 'dumps': [], 'messages': []}, {'result': Zero, 'events': [], 'matrices': [], 'dumps': [], 'messages': []}]"
    )
    stdout = capsys.readouterr().out
    assert stdout == ""
    assert called


def test_run_with_result_callback_from_callable_with_args(capsys) -> None:
    def on_result(result):
        nonlocal called
        called = True
        assert result["result"] == [Result.Zero, Result.Zero]

    called = False
    init()
    import_openqasm("input int a; output bit[2] c;", name="Foo")
    results = run(code.Foo, 3, 2, on_result=on_result, save_events=True)
    assert (
        str(results)
        == "[{'result': [Zero, Zero], 'events': [], 'matrices': [], 'dumps': [], 'messages': []}, {'result': [Zero, Zero], 'events': [], 'matrices': [], 'dumps': [], 'messages': []}, {'result': [Zero, Zero], 'events': [], 'matrices': [], 'dumps': [], 'messages': []}]"
    )

    assert called


def test_run_with_invalid_shots_produces_error() -> None:
    init()
    import_openqasm("output bit[2] c;", name="Foo")
    try:
        run(code.Foo, -1)
    except ValueError as e:
        assert str(e) == "The number of shots must be greater than 0."
    else:
        assert False

    try:
        run(code.Foo, 0)
    except ValueError as e:
        assert str(e) == "The number of shots must be greater than 0."
    else:
        assert False


# Compile


def test_compile_qir_input_data() -> None:
    operation = compile("qubit q; output bit c; c = measure q;")
    qir = operation._repr_qir_()
    assert isinstance(qir, bytes)


def test_compile_qir_str() -> None:
    qir = str(compile("qubit q; output bit c; c = measure q;"))
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert '"required_num_qubits"="1" "required_num_results"="1"' in qir


def test_compile_qir_str_with_single_arg_raises_error() -> None:
    init(target_profile=TargetProfile.Base)
    with pytest.raises(QSharpError) as excinfo:
        compile(
            """
            include "stdgates.inc";
            input float f;
            qubit q;
            rx(f) q;
            output bit c;
            c = measure q;
            """
        )
    assert (
        str(excinfo.value)
        == """Circuit has unbound input parameters
  help: Parameters: f: Double"""
    )


# Import + Compile


def test_compile_qir_str_from_python_callable() -> None:
    init(target_profile=TargetProfile.Base)
    import_openqasm("qubit q; output bit c; c = measure q;", name="Program")
    operation = compile(code.Program)
    qir = str(operation)
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert '"required_num_qubits"="1" "required_num_results"="1"' in qir


def test_compile_qir_str_from_python_callable_with_single_arg() -> None:
    init(target_profile=TargetProfile.Base)
    import_openqasm(
        """
        include "stdgates.inc";
        input float f;
        qubit q;
        rx(f) q;
        output bit c;
        c = measure q;
        """
    )

    operation = compile(code.program, pi)
    qir = str(operation)
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert (
        "call void @__quantum__qis__rx__body(double 3.141592653589793, %Qubit* inttoptr (i64 0 to %Qubit*))"
        in qir
    )
    assert (
        "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)"
        in qir
    )
    assert '"required_num_qubits"="1" "required_num_results"="1"' in qir


def test_compile_qir_str_from_python_callable_with_multiple_args() -> None:
    init(target_profile=TargetProfile.Base)
    import_openqasm(
        """
        include "stdgates.inc";
        input float f;
        input float d;
        qubit q;
        rx(f/d) q;
        output bit c;
        c = measure q;
        """,
        name="Program",
    )
    operation = compile(code.Program, 2 * pi, 2.0)
    qir = str(operation)
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert (
        "call void @__quantum__qis__rx__body(double 3.141592653589793, %Qubit* inttoptr (i64 0 to %Qubit*))"
        in qir
    )
    assert (
        "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)"
        in qir
    )
    assert '"required_num_qubits"="1" "required_num_results"="1"' in qir


def test_compile_qir_str_from_python_callable_with_multiple_args_passed_as_tuple() -> (
    None
):
    init(target_profile=TargetProfile.Base)
    import_openqasm(
        """
        include "stdgates.inc";
        input float f;
        input float d;
        qubit q;
        rx(f/d) q;
        output bit c;
        c = measure q;
        """,
        name="Program",
    )
    args = (2 * pi, 2.0)
    operation = compile(code.Program, args)
    qir = str(operation)
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert (
        "call void @__quantum__qis__rx__body(double 3.141592653589793, %Qubit* inttoptr (i64 0 to %Qubit*))"
        in qir
    )
    assert (
        "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)"
        in qir
    )
    assert '"required_num_qubits"="1" "required_num_results"="1"' in qir


def test_compile_qir_str_from_callable_with_mresetzchecked() -> None:
    init(target_profile=TargetProfile.Adaptive_RI)
    import_openqasm(
        """
        include "qdk.inc";
        qubit q1;
        int r;
        r = mresetz_checked(q1);
        """,
        name="Program",
    )
    operation = compile(code.Program)
    qir = str(operation)
    assert "define void @ENTRYPOINT__main()" in qir
    assert (
        "call i1 @__quantum__rt__read_loss(%Result* inttoptr (i64 0 to %Result*))"
        in qir
    )
    assert '"required_num_qubits"="1" "required_num_results"="1"' in qir
    assert "call void @__quantum__rt__int_record_output" in qir


def test_callables_exposed_into_env() -> None:
    init()
    import_openqasm(
        "def Four() -> int { return 4; }", program_type=ProgramType.Fragments
    )
    assert code.Four() == 4, "callable should be available"
    import_openqasm(
        "def Add(int a, int b) -> int { return a + b; }",
        program_type=ProgramType.Fragments,
    )
    assert code.Four() == 4, "first callable should still be available"
    assert code.Add(2, 3) == 5, "second callable should be available"
    # After init, the callables should be cleared and no longer available
    init()
    with pytest.raises(AttributeError):
        code.Four()


def test_callable_with_int_exposed_into_env_fails_incorrect_types() -> None:
    init()
    import_openqasm(
        "def Identity(int a) -> int { return a; }", program_type=ProgramType.Fragments
    )
    assert code.Identity(4) == 4
    with pytest.raises(TypeError):
        code.Identity("4")
    with pytest.raises(TypeError):
        code.Identity(4.0)
    with pytest.raises(OverflowError):
        code.Identity(4000000000000000000000)
    with pytest.raises(TypeError):
        code.Identity([4])


def test_callable_with_double_exposed_into_env_fails_incorrect_types() -> None:
    init()
    import_openqasm(
        "def Identity(float a) -> float { return a; }",
        program_type=ProgramType.Fragments,
    )
    assert code.Identity(4.0) == 4.0
    assert code.Identity(4) == 4.0
    with pytest.raises(TypeError):
        code.Identity("4")
    with pytest.raises(TypeError):
        code.Identity([4])


def test_callable_with_bigint_exposed_into_env_fails_incorrect_types() -> None:
    init()
    import_openqasm(
        "def Identity(int[128] a) -> int[128] { return a; }",
        program_type=ProgramType.Fragments,
    )
    assert code.Identity(4000000000000000000000) == 4000000000000000000000
    with pytest.raises(TypeError):
        code.Identity("4")
    with pytest.raises(TypeError):
        code.Identity(4.0)


def test_callable_with_bool_exposed_into_env_fails_incorrect_types() -> None:
    init()
    import_openqasm(
        "def Identity(bool a) -> bool { return a; }", program_type=ProgramType.Fragments
    )
    assert code.Identity(True) == True
    with pytest.raises(TypeError):
        code.Identity("4")
    with pytest.raises(TypeError):
        code.Identity(4)
    with pytest.raises(TypeError):
        code.Identity(4.0)
    with pytest.raises(TypeError):
        code.Identity([4])


def test_callable_with_array_exposed_into_env_fails_incorrect_types() -> None:
    init()
    import_openqasm(
        "def fst(readonly array[int, #dim = 1] arr_arg) -> int { return arr_arg[0]; }",
        program_type=ProgramType.Fragments,
    )
    assert code.fst([4, 5, 6]) == 4
    assert code.fst((5, 6, 7)) == 5
    with pytest.raises(QSharpError):  # out of range
        code.fst([])
    with pytest.raises(TypeError):
        code.fst((4, 5.0, 6))
    with pytest.raises(TypeError):
        code.fst(4)
    with pytest.raises(TypeError):
        code.fst("4")
    with pytest.raises(TypeError):
        code.fst(4.0)
    with pytest.raises(TypeError):
        code.fst([1, 2, 3.0])


@pytest.mark.xfail(
    reason="When compiling fragments, input/output angles should be converted to floats"
)
def test_callables_with_unsupported_types_raise_errors_on_call() -> None:
    init()
    import_openqasm("def Unsupported(angle a) { }", program_type=ProgramType.Fragments)
    with pytest.raises(QSharpError, match='unsupported input type: `UDT<"Angle":'):
        code.Unsupported()


def test_callable_with_complex_input() -> None:
    init()
    import_openqasm(
        "def ComplexInput(complex a) { }", program_type=ProgramType.Fragments
    )
    code.ComplexInput(2 + 3j)


def test_callable_with_complex_output() -> None:
    init()
    import_openqasm(
        "def ComplexOutput() -> complex { return 2 + 3im; }",
        program_type=ProgramType.Fragments,
    )
    assert code.ComplexOutput() == 2 + 3j


def test_callable_with_complex_input_output() -> None:
    init()
    import_openqasm(
        "def ComplexOutput(complex a) -> complex { return 2 * a; }",
        program_type=ProgramType.Fragments,
    )
    assert code.ComplexOutput(2 + 3j) == 4 + 6j


def test_identity_returning_complex_udt() -> None:
    init()
    import_openqasm(
        "def ComplexOutput(complex a) -> complex { return a; }",
        program_type=ProgramType.Fragments,
    )
    assert code.ComplexOutput(2 + 3j) == 2 + 3j


def test_callable_with_angle_input() -> None:
    init()
    import_openqasm("def AngleInput(angle a) { }", program_type=ProgramType.Fragments)
    code.AngleInput(3.14)


def test_callable_with_angle_output() -> None:
    init()
    import_openqasm(
        "def AngleOutput() -> angle { return pi; }",
        program_type=ProgramType.Fragments,
    )
    assert code.AngleOutput() == pi


def test_callable_with_angle_input_output() -> None:
    init()
    import_openqasm(
        "def AngleOutput(angle a) -> angle { return a; }",
        program_type=ProgramType.Fragments,
    )
    assert code.AngleOutput(pi) == pi


def test_circuit_from_program() -> None:
    init()

    c = circuit(
        """
        include "stdgates.inc";
        qubit q1;
        qubit q2;
        x q1;
        """,
    )
    assert str(c) == dedent(
        """\
        q_0    ── X ──
        q_1    ───────
        """
    )


def test_circuit_from_callable() -> None:
    init()
    import_openqasm(
        """
        include "stdgates.inc";
        qubit q1;
        qubit q2;
        x q1;
        """,
        program_type=ProgramType.Operation,
        name="Foo",
    )
    c = circuit(code.Foo)
    assert str(c) == dedent(
        """\
        q_0    ── X ──
        q_1    ───────
        """
    )


def test_circuit_from_callable_with_args() -> None:
    init()
    import_openqasm(
        """
        include "stdgates.inc";
        qubit[2] qs;
        input int nQubits;
        for int i in [0:nQubits-1] {
            x qs[i];
        }
        """,
        name="Foo",
    )
    c = circuit(code.Foo, 2)
    assert str(c) == dedent(
        """\
        q_0    ── X ──
        q_1    ── X ──
        """
    )


def test_circuit_with_measure_from_callable() -> None:
    init()
    import_openqasm(
        """include "stdgates.inc"; qubit q; h q; bit c; c = measure q;""",
        name="Foo",
    )
    c = circuit(code.Foo)
    assert str(c) == dedent(
        """\
        q_0    ── H ──── M ──
                         ╘═══
        """
    )


# Estimate


def test_qasm_estimation() -> None:
    res = estimate(
        """
        include "stdgates.inc";
        const int SIZE = 10;
        qubit[SIZE] q;
        for int i in [0:SIZE-1] {
            t q[i];
            measure q[i];
        }
        """
    )
    assert res["status"] == "success"
    assert res["physicalCounts"] is not None
    assert res.logical_counts == LogicalCounts(
        {
            "numQubits": 10,
            "tCount": 10,
            "rotationCount": 0,
            "rotationDepth": 0,
            "cczCount": 0,
            "measurementCount": 10,
        }
    )


def test_qasm_estimation_with_single_params() -> None:
    params = EstimatorParams()
    params.error_budget = 0.333
    params.qubit_params.name = QubitParams.MAJ_NS_E4
    assert params.as_dict() == {
        "qubitParams": {"name": "qubit_maj_ns_e4"},
        "errorBudget": 0.333,
    }

    res = estimate(
        """
        include "stdgates.inc";
        const int SIZE = 10;
        qubit[SIZE] q;
        for int i in [0:SIZE-1] {
            t q[i];
            measure q[i];
        }
        """,
        params=params,
    )

    assert res["status"] == "success"
    assert res["physicalCounts"] is not None
    assert res["jobParams"]["qubitParams"]["name"] == "qubit_maj_ns_e4"
    assert res.logical_counts == LogicalCounts(
        {
            "numQubits": 10,
            "tCount": 10,
            "rotationCount": 0,
            "rotationDepth": 0,
            "cczCount": 0,
            "measurementCount": 10,
        }
    )


def test_qasm_estimation_with_multiple_params() -> None:
    params = EstimatorParams(3)
    params.items[0].qubit_params.name = QubitParams.GATE_US_E3
    params.items[0].error_budget = 0.333
    params.items[1].qubit_params.name = QubitParams.GATE_US_E4
    params.items[1].error_budget = 0.333
    params.items[2].qubit_params.name = QubitParams.MAJ_NS_E6
    params.items[2].qec_scheme.name = QECScheme.FLOQUET_CODE
    params.items[2].error_budget = 0.333
    assert params.as_dict() == {
        "items": [
            {
                "qubitParams": {"name": "qubit_gate_us_e3"},
                "errorBudget": 0.333,
            },
            {
                "qubitParams": {"name": "qubit_gate_us_e4"},
                "errorBudget": 0.333,
            },
            {
                "qubitParams": {"name": "qubit_maj_ns_e6"},
                "qecScheme": {"name": "floquet_code"},
                "errorBudget": 0.333,
            },
        ],
        "resumeAfterFailedItem": True,
    }

    res = estimate(
        """
        include "stdgates.inc";
        const int SIZE = 10;
        qubit[SIZE] q;
        for int i in [0:SIZE-1] {
            t q[i];
            measure q[i];
        }
        """,
        params=params,
    )

    for idx in res:
        assert res[idx]["status"] == "success"
        assert res[idx]["physicalCounts"] is not None
        assert (
            res[idx]["jobParams"]["qubitParams"]["name"]
            == params.items[idx].qubit_params.name
        )
        assert res[idx]["logicalCounts"] == LogicalCounts(
            {
                "numQubits": 10,
                "tCount": 10,
                "rotationCount": 0,
                "rotationDepth": 0,
                "cczCount": 0,
                "measurementCount": 10,
            }
        )
    assert res[2]["jobParams"]["qecScheme"]["name"] == QECScheme.FLOQUET_CODE


def test_qasm_estimation_with_multiple_params_from_python_callable() -> None:
    init(target_profile=TargetProfile.Unrestricted)

    params = EstimatorParams(3)
    params.items[0].qubit_params.name = QubitParams.GATE_US_E3
    params.items[0].error_budget = 0.333
    params.items[1].qubit_params.name = QubitParams.GATE_US_E4
    params.items[1].error_budget = 0.333
    params.items[2].qubit_params.name = QubitParams.MAJ_NS_E6
    params.items[2].qec_scheme.name = QECScheme.FLOQUET_CODE
    params.items[2].error_budget = 0.333
    assert params.as_dict() == {
        "items": [
            {
                "qubitParams": {"name": "qubit_gate_us_e3"},
                "errorBudget": 0.333,
            },
            {
                "qubitParams": {"name": "qubit_gate_us_e4"},
                "errorBudget": 0.333,
            },
            {
                "qubitParams": {"name": "qubit_maj_ns_e6"},
                "qecScheme": {"name": "floquet_code"},
                "errorBudget": 0.333,
            },
        ],
        "resumeAfterFailedItem": True,
    }

    import_openqasm(
        """
        include "stdgates.inc";
        const int SIZE = 10;
        qubit[SIZE] q;
        for int i in [0:SIZE-1] {
            t q[i];
            measure q[i];
        }
        """,
        name="Test",
    )

    res = estimate(code.Test, params=params)

    for idx in res:
        assert res[idx]["status"] == "success"
        assert res[idx]["physicalCounts"] is not None
        assert (
            res[idx]["jobParams"]["qubitParams"]["name"]
            == params.items[idx].qubit_params.name
        )
        assert res[idx]["logicalCounts"] == LogicalCounts(
            {
                "numQubits": 10,
                "tCount": 10,
                "rotationCount": 0,
                "rotationDepth": 0,
                "cczCount": 0,
                "measurementCount": 10,
            }
        )
    assert res[2]["jobParams"]["qecScheme"]["name"] == QECScheme.FLOQUET_CODE


def test_qasm_estimation_with_multiple_params_from_python_callable_with_arg() -> None:
    init(target_profile=TargetProfile.Unrestricted)

    params = EstimatorParams(3)
    params.items[0].qubit_params.name = QubitParams.GATE_US_E3
    params.items[0].error_budget = 0.333
    params.items[1].qubit_params.name = QubitParams.GATE_US_E4
    params.items[1].error_budget = 0.333
    params.items[2].qubit_params.name = QubitParams.MAJ_NS_E6
    params.items[2].qec_scheme.name = QECScheme.FLOQUET_CODE
    params.items[2].error_budget = 0.333
    assert params.as_dict() == {
        "items": [
            {
                "qubitParams": {"name": "qubit_gate_us_e3"},
                "errorBudget": 0.333,
            },
            {
                "qubitParams": {"name": "qubit_gate_us_e4"},
                "errorBudget": 0.333,
            },
            {
                "qubitParams": {"name": "qubit_maj_ns_e6"},
                "qecScheme": {"name": "floquet_code"},
                "errorBudget": 0.333,
            },
        ],
        "resumeAfterFailedItem": True,
    }

    import_openqasm(
        """
        include "stdgates.inc";
        input int discard;
        const int SIZE = 7;
        qubit[SIZE] q;
        for int i in [0:SIZE-1] {
            t q[i];
            measure q[i];
        }
        """,
        name="Test",
    )

    res = estimate(code.Test, params, 8)

    for idx in res:
        assert res[idx]["status"] == "success"
        assert res[idx]["physicalCounts"] is not None
        assert (
            res[idx]["jobParams"]["qubitParams"]["name"]
            == params.items[idx].qubit_params.name
        )
        assert res[idx]["logicalCounts"] == LogicalCounts(
            {
                "numQubits": 7,
                "tCount": 7,
                "rotationCount": 0,
                "rotationDepth": 0,
                "cczCount": 0,
                "measurementCount": 7,
            }
        )
    assert res[2]["jobParams"]["qecScheme"]["name"] == QECScheme.FLOQUET_CODE
