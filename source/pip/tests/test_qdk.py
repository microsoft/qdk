# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from textwrap import dedent
import pytest
import qdk
import qdk.code
import qdk.utils
from contextlib import redirect_stdout
import io

# Tests for the Python library for Q#


def test_stdout() -> None:
    qdk.init(target_profile=qdk.TargetProfile.Unrestricted)
    f = io.StringIO()
    with redirect_stdout(f):
        result = qdk.eval('Message("Hello, world!")')

    assert result is None
    assert f.getvalue() == "Hello, world!\n"


def test_stdout_multiple_lines() -> None:
    qdk.init(target_profile=qdk.TargetProfile.Unrestricted)
    f = io.StringIO()
    with redirect_stdout(f):
        qdk.eval(
            """
        use q = Qubit();
        Microsoft.Quantum.Diagnostics.DumpMachine();
        Message("Hello!");
        """
        )

    assert f.getvalue() == "STATE:\n|0⟩: 1.0000+0.0000𝑖\nHello!\n"


def test_captured_stdout() -> None:
    qdk.init(target_profile=qdk.TargetProfile.Unrestricted)
    f = io.StringIO()
    with redirect_stdout(f):
        result = qdk.eval(
            '{Message("Hello, world!"); Message("Goodbye!")}', save_events=True
        )
    assert f.getvalue() == ""
    assert len(result["messages"]) == 2
    assert result["messages"][0] == "Hello, world!"
    assert result["messages"][1] == "Goodbye!"


def test_captured_matrix() -> None:
    qdk.init(target_profile=qdk.TargetProfile.Unrestricted)
    f = io.StringIO()
    with redirect_stdout(f):
        result = qdk.eval(
            "Std.Diagnostics.DumpOperation(1, qs => H(qs[0]))",
            save_events=True,
        )
    assert f.getvalue() == ""
    assert len(result["matrices"]) == 1
    assert (
        str(result["matrices"][0])
        == "MATRIX:\n 0.7071+0.0000𝑖 0.7071+0.0000𝑖\n 0.7071+0.0000𝑖 −0.7071+0.0000𝑖"
    )


def test_quantum_seed() -> None:
    qdk.init(target_profile=qdk.TargetProfile.Unrestricted)
    qdk.set_quantum_seed(42)
    value1 = qdk.eval(
        "{ use qs = Qubit[32]; for q in qs { H(q); }; Microsoft.Quantum.Measurement.MResetEachZ(qs) }"
    )
    qdk.init(target_profile=qdk.TargetProfile.Unrestricted)
    qdk.set_quantum_seed(42)
    value2 = qdk.eval(
        "{ use qs = Qubit[32]; for q in qs { H(q); }; Microsoft.Quantum.Measurement.MResetEachZ(qs) }"
    )
    assert value1 == value2
    qdk.set_quantum_seed(None)
    value3 = qdk.eval(
        "{ use qs = Qubit[32]; for q in qs { H(q); }; Microsoft.Quantum.Measurement.MResetEachZ(qs) }"
    )
    assert value1 != value3


def test_classical_seed() -> None:
    qdk.init(target_profile=qdk.TargetProfile.Unrestricted)
    qdk.set_classical_seed(42)
    value1 = qdk.eval(
        "{ mutable res = []; for _ in 0..15{ set res += [(Microsoft.Quantum.Random.DrawRandomInt(0, 100), Microsoft.Quantum.Random.DrawRandomDouble(0.0, 1.0))]; }; res }"
    )
    qdk.init(target_profile=qdk.TargetProfile.Unrestricted)
    qdk.set_classical_seed(42)
    value2 = qdk.eval(
        "{ mutable res = []; for _ in 0..15{ set res += [(Microsoft.Quantum.Random.DrawRandomInt(0, 100), Microsoft.Quantum.Random.DrawRandomDouble(0.0, 1.0))]; }; res }"
    )
    assert value1 == value2
    qdk.init(target_profile=qdk.TargetProfile.Unrestricted)
    qdk.set_classical_seed(None)
    value3 = qdk.eval(
        "{ mutable res = []; for _ in 0..15{ set res += [(Microsoft.Quantum.Random.DrawRandomInt(0, 100), Microsoft.Quantum.Random.DrawRandomDouble(0.0, 1.0))]; }; res }"
    )
    assert value1 != value3


def test_dump_machine() -> None:
    qdk.init(target_profile=qdk.TargetProfile.Unrestricted)
    qdk.eval(
        """
    use q1 = Qubit();
    use q2 = Qubit();
    X(q1);
    """
    )
    state_dump = qdk.dump_machine()
    assert state_dump.qubit_count == 2
    assert len(state_dump) == 1
    assert state_dump[2] == complex(1.0, 0.0)
    assert state_dump.as_dense_state() == [0, 0, 1, 0]
    qdk.eval("X(q2);")
    state_dump = qdk.dump_machine()
    assert state_dump.qubit_count == 2
    assert len(state_dump) == 1
    assert state_dump[3] == complex(1.0, 0.0)
    assert state_dump.as_dense_state() == [0, 0, 0, 1]
    qdk.eval("H(q1);")
    state_dump = qdk.dump_machine()
    assert state_dump.qubit_count == 2
    assert len(state_dump) == 2
    # Check that the state dump correctly supports iteration and membership checks
    for idx in state_dump:
        assert idx in state_dump
    # Check that the state dump is correct and equivalence check ignores global phase, allowing passing
    # in of different, potentially unnormalized states. The state should be
    # |01⟩: 0.7071+0.0000𝑖, |11⟩: −0.7071+0.0000𝑖
    assert state_dump.check_eq({1: complex(0.7071, 0.0), 3: complex(-0.7071, 0.0)})
    assert state_dump.as_dense_state() == [
        0,
        0.7071067811865476,
        0,
        -0.7071067811865476,
    ]
    assert state_dump.check_eq({1: complex(0.0, 0.7071), 3: complex(0.0, -0.7071)})
    assert state_dump.check_eq({1: complex(0.5, 0.0), 3: complex(-0.5, 0.0)})
    assert state_dump.check_eq(
        {1: complex(0.7071, 0.0), 3: complex(-0.7071, 0.0), 0: complex(0.0, 0.0)}
    )
    assert state_dump.check_eq([0.0, 0.5, 0.0, -0.5])
    assert state_dump.check_eq([0.0, 0.5001, 0.00001, -0.5], tolerance=1e-3)
    assert state_dump.check_eq(
        [complex(0.0, 0.0), complex(0.0, -0.5), complex(0.0, 0.0), complex(0.0, 0.5)]
    )
    assert not state_dump.check_eq({1: complex(0.7071, 0.0), 3: complex(0.7071, 0.0)})
    assert not state_dump.check_eq({1: complex(0.5, 0.0), 3: complex(0.0, 0.5)})
    assert not state_dump.check_eq({2: complex(0.5, 0.0), 3: complex(-0.5, 0.0)})
    assert not state_dump.check_eq([0.0, 0.5001, 0.0, -0.5], tolerance=1e-6)
    # Reset the qubits and apply a small rotation to q1, to confirm that tolerance applies to the dump
    # itself and not just the state.
    qdk.eval("ResetAll([q1, q2]);")
    qdk.eval("Ry(0.0001, q1);")
    state_dump = qdk.dump_machine()
    assert state_dump.qubit_count == 2
    assert len(state_dump) == 2
    assert not state_dump.check_eq([1.0])
    assert state_dump.check_eq([0.99999999875, 0.0, 4.999999997916667e-05])
    assert state_dump.check_eq([1.0], tolerance=1e-4)


def test_dump_operation() -> None:
    qdk.init(target_profile=qdk.TargetProfile.Unrestricted)
    res = qdk.utils.dump_operation("qs => ()", 1)
    assert res == [
        [complex(1.0, 0.0), complex(0.0, 0.0)],
        [complex(0.0, 0.0), complex(1.0, 0.0)],
    ]
    res = qdk.utils.dump_operation("qs => H(qs[0])", 1)
    assert res == [
        [complex(0.707107, 0.0), complex(0.707107, 0.0)],
        [complex(0.707107, 0.0), complex(-0.707107, 0.0)],
    ]
    res = qdk.utils.dump_operation("qs => CNOT(qs[0], qs[1])", 2)
    assert res == [
        [complex(1.0, 0.0), complex(0.0, 0.0), complex(0.0, 0.0), complex(0.0, 0.0)],
        [complex(0.0, 0.0), complex(1.0, 0.0), complex(0.0, 0.0), complex(0.0, 0.0)],
        [complex(0.0, 0.0), complex(0.0, 0.0), complex(0.0, 0.0), complex(1.0, 0.0)],
        [complex(0.0, 0.0), complex(0.0, 0.0), complex(1.0, 0.0), complex(0.0, 0.0)],
    ]
    res = qdk.utils.dump_operation("qs => CCNOT(qs[0], qs[1], qs[2])", 3)
    assert res == [
        [
            complex(1.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
        ],
        [
            complex(0.0, 0.0),
            complex(1.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
        ],
        [
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(1.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
        ],
        [
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(1.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
        ],
        [
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(1.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
        ],
        [
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(1.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
        ],
        [
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(1.0, 0.0),
        ],
        [
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(0.0, 0.0),
            complex(1.0, 0.0),
            complex(0.0, 0.0),
        ],
    ]
    qdk.eval(
        "operation ApplySWAP(qs : Qubit[]) : Unit is Ctl + Adj { SWAP(qs[0], qs[1]); }"
    )
    res = qdk.utils.dump_operation("ApplySWAP", 2)
    assert res == [
        [complex(1.0, 0.0), complex(0.0, 0.0), complex(0.0, 0.0), complex(0.0, 0.0)],
        [complex(0.0, 0.0), complex(0.0, 0.0), complex(1.0, 0.0), complex(0.0, 0.0)],
        [complex(0.0, 0.0), complex(1.0, 0.0), complex(0.0, 0.0), complex(0.0, 0.0)],
        [complex(0.0, 0.0), complex(0.0, 0.0), complex(0.0, 0.0), complex(1.0, 0.0)],
    ]
    res = qdk.utils.dump_operation("qs => ()", 8)
    for i in range(8):
        for j in range(8):
            if i == j:
                assert res[i][j] == complex(1.0, 0.0)
            else:
                assert res[i][j] == complex(0.0, 0.0)


def test_run_with_noise_produces_noisy_results() -> None:
    qdk.init()
    result = qdk.run(
        "{ mutable errors=0; for _ in 0..100 { use q1=Qubit(); use q2=Qubit(); H(q1); CNOT(q1, q2); if MResetZ(q1) != MResetZ(q2) { set errors+=1; } } errors }",
        shots=1,
        noise=qdk.BitFlipNoise(0.1),
        seed=0,
    )
    assert result[0] > 5
    result = qdk.run(
        "{ mutable errors=0; for _ in 0..100 { use q=Qubit(); if MResetZ(q) != Zero { set errors+=1; } } errors }",
        shots=1,
        noise=qdk.BitFlipNoise(0.1),
        seed=0,
    )
    assert result[0] > 5


def test_run_with_loss_produces_lossy_results() -> None:
    qdk.init()
    result = qdk.run(
        "{ use q = Qubit(); X(q); MResetZ(q) }",
        shots=1,
        qubit_loss=1.0,
        seed=0,
    )
    assert result[0] == qdk.Result.Loss


def test_run_with_callable_and_seed_produces_deterministic_shot_results() -> None:
    qdk.init()
    # Uses an operation that verifies both quantum and classical randomness
    qdk.eval(
        "operation Rand() : (Int, Int) { use qs = Qubit[32]; for q in qs { H(q); }; (MeasureInteger(qs), Std.Random.DrawRandomInt(0, 1_000_000)) }"
    )
    result1 = qdk.run(
        qdk.code.Rand,
        shots=10,
        seed=42,
    )
    result2 = qdk.run(
        qdk.code.Rand,
        shots=10,
        seed=42,
    )
    assert (
        result1[0] != result1[2]
    )  # Check that we actually got some randomness in the results
    assert result1 == result2
    result3 = qdk.run(
        qdk.code.Rand,
        shots=10,
        seed=None,
    )
    assert result1 != result3


def test_compile_qir_input_data() -> None:
    qdk.init(target_profile=qdk.TargetProfile.Base)
    qdk.eval("operation Program() : Result { use q = Qubit(); return M(q) }")
    operation = qdk.compile("Program()")
    qir = operation._repr_qir_()
    assert isinstance(qir, bytes)


def test_compile_qir_str() -> None:
    qdk.init(target_profile=qdk.TargetProfile.Base)
    qdk.eval("operation Program() : Result { use q = Qubit(); return MResetZ(q); }")
    operation = qdk.compile("Program()")
    qir = str(operation)
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert '"required_num_qubits"="1" "required_num_results"="1"' in qir


def test_compile_qir_str_from_python_callable() -> None:
    qdk.init(target_profile=qdk.TargetProfile.Base)
    qdk.eval("operation Program() : Result { use q = Qubit(); return MResetZ(q); }")
    operation = qdk.compile(qdk.code.Program)
    qir = str(operation)
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert '"required_num_qubits"="1" "required_num_results"="1"' in qir


def test_compile_qir_str_from_qsharp_callable() -> None:
    qdk.init(target_profile=qdk.TargetProfile.Base)
    qdk.eval("operation Program() : Result { use q = Qubit(); return MResetZ(q); }")
    program = qdk.eval("Program")
    operation = qdk.compile(program)
    qir = str(operation)
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert '"required_num_qubits"="1" "required_num_results"="1"' in qir


def test_compile_qir_str_from_python_callable_with_single_arg() -> None:
    qdk.init(target_profile=qdk.TargetProfile.Base)
    qdk.eval(
        "operation Program(nQubits : Int) : Result[] { use qs = Qubit[nQubits]; MResetEachZ(qs) }"
    )
    operation = qdk.compile(qdk.code.Program, 5)
    qir = str(operation)
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert (
        "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @5, i64 0, i64 0))"
        in qir
    )
    assert '"required_num_qubits"="5" "required_num_results"="5"' in qir


def test_compile_qir_str_from_qsharp_callable_with_single_arg() -> None:
    qdk.init(target_profile=qdk.TargetProfile.Base)
    qdk.eval(
        "operation Program(nQubits : Int) : Result[] { use qs = Qubit[nQubits]; MResetEachZ(qs) }"
    )
    program = qdk.eval("Program")
    operation = qdk.compile(program, 5)
    qir = str(operation)
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert (
        "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @5, i64 0, i64 0))"
        in qir
    )
    assert '"required_num_qubits"="5" "required_num_results"="5"' in qir


def test_compile_qir_str_from_python_callable_with_array_arg() -> None:
    qdk.init(target_profile=qdk.TargetProfile.Base)
    qdk.eval(
        "operation Program(nQubits : Int[]) : Result[] { use qs = Qubit[nQubits[1]]; MResetEachZ(qs) }"
    )
    operation = qdk.compile(qdk.code.Program, [5, 3])
    qir = str(operation)
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert (
        "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))"
        in qir
    )
    assert (
        "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @5, i64 0, i64 0))"
        not in qir
    )
    assert '"required_num_qubits"="3" "required_num_results"="3"' in qir


def test_compile_qir_str_from_qsharp_callable_with_array_arg() -> None:
    qdk.init(target_profile=qdk.TargetProfile.Base)
    qdk.eval(
        "operation Program(nQubits : Int[]) : Result[] { use qs = Qubit[nQubits[1]]; MResetEachZ(qs) }"
    )
    program = qdk.eval("Program")
    operation = qdk.compile(program, [5, 3])
    qir = str(operation)
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert (
        "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))"
        in qir
    )
    assert (
        "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @5, i64 0, i64 0))"
        not in qir
    )
    assert '"required_num_qubits"="3" "required_num_results"="3"' in qir


def test_compile_qir_str_from_python_callable_with_multiple_args() -> None:
    qdk.init(target_profile=qdk.TargetProfile.Base)
    qdk.eval(
        "operation Program(nQubits : Int, nResults : Int) : Result[] { use qs = Qubit[nQubits]; MResetEachZ(qs)[...nResults-1] }"
    )
    operation = qdk.compile(qdk.code.Program, 5, 3)
    qir = str(operation)
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert (
        "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))"
        in qir
    )
    assert (
        "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @5, i64 0, i64 0))"
        not in qir
    )
    assert '"required_num_qubits"="5" "required_num_results"="5"' in qir


def test_compile_qir_str_from_qsharp_callable_with_multiple_args() -> None:
    qdk.init(target_profile=qdk.TargetProfile.Base)
    qdk.eval(
        "operation Program(nQubits : Int, nResults : Int) : Result[] { use qs = Qubit[nQubits]; MResetEachZ(qs)[...nResults-1] }"
    )
    program = qdk.eval("Program")
    operation = qdk.compile(program, 5, 3)
    qir = str(operation)
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert (
        "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))"
        in qir
    )
    assert (
        "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @5, i64 0, i64 0))"
        not in qir
    )
    assert '"required_num_qubits"="5" "required_num_results"="5"' in qir


def test_compile_qir_str_from_python_callable_with_multiple_args_passed_as_tuple() -> (
    None
):
    qdk.init(target_profile=qdk.TargetProfile.Base)
    qdk.eval(
        "operation Program(nQubits : Int, nResults : Int) : Result[] { use qs = Qubit[nQubits]; MResetEachZ(qs)[...nResults-1] }"
    )
    args = (5, 3)
    operation = qdk.compile(qdk.code.Program, args)
    qir = str(operation)
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert (
        "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))"
        in qir
    )
    assert (
        "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @5, i64 0, i64 0))"
        not in qir
    )
    assert '"required_num_qubits"="5" "required_num_results"="5"' in qir


def test_compile_qir_str_from_qsharp_callable_with_multiple_args_passed_as_tuple() -> (
    None
):
    qdk.init(target_profile=qdk.TargetProfile.Base)
    qdk.eval(
        "operation Program(nQubits : Int, nResults : Int) : Result[] { use qs = Qubit[nQubits]; MResetEachZ(qs)[...nResults-1] }"
    )
    args = (5, 3)
    program = qdk.eval("Program")
    operation = qdk.compile(program, args)
    qir = str(operation)
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert (
        "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))"
        in qir
    )
    assert (
        "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @5, i64 0, i64 0))"
        not in qir
    )
    assert '"required_num_qubits"="5" "required_num_results"="5"' in qir


def test_init_from_provider_name() -> None:
    config = qdk.init(target_name="ionq.simulator")
    assert config._config["targetProfile"] == "base"
    config = qdk.init(target_name="rigetti.sim.qvm")
    assert config._config["targetProfile"] == "base"
    config = qdk.init(target_name="quantinuum.sim")
    assert config._config["targetProfile"] == "adaptive_ri"
    config = qdk.init(target_name="Quantinuum")
    assert config._config["targetProfile"] == "adaptive_ri"
    config = qdk.init(target_name="IonQ")
    assert config._config["targetProfile"] == "base"


def test_run_with_result(capsys) -> None:
    qdk.init()
    qdk.eval('operation Foo() : Result { Message("Hello, world!"); Zero }')
    results = qdk.run("Foo()", 3)
    assert results == [qdk.Result.Zero, qdk.Result.Zero, qdk.Result.Zero]
    stdout = capsys.readouterr().out
    assert stdout == "Hello, world!\nHello, world!\nHello, world!\n"


def test_run_with_result_from_python_callable(capsys) -> None:
    qdk.init()
    qdk.eval(
        'operation Foo() : Result { Message("Hello, world!"); use q = Qubit(); M(q) }'
    )
    results = qdk.run(qdk.code.Foo, 3)
    assert results == [qdk.Result.Zero, qdk.Result.Zero, qdk.Result.Zero]
    stdout = capsys.readouterr().out
    assert stdout == "Hello, world!\nHello, world!\nHello, world!\n"


def test_run_with_result_from_qsharp_callable(capsys) -> None:
    qdk.init()
    qdk.eval(
        'operation Foo() : Result { Message("Hello, world!"); use q = Qubit(); M(q) }'
    )
    foo = qdk.eval("Foo")
    results = qdk.run(foo, 3)
    assert results == [qdk.Result.Zero, qdk.Result.Zero, qdk.Result.Zero]
    stdout = capsys.readouterr().out
    assert stdout == "Hello, world!\nHello, world!\nHello, world!\n"


def test_run_with_result_from_python_callable_while_global_qubits_allocated(
    capsys,
) -> None:
    qdk.init()
    qdk.eval("use q = Qubit();")
    qdk.eval(
        'operation Foo() : Result { Message("Hello, world!"); use q = Qubit(); M(q) }'
    )
    results = qdk.run(qdk.code.Foo, 3)
    assert results == [qdk.Result.Zero, qdk.Result.Zero, qdk.Result.Zero]
    stdout = capsys.readouterr().out
    assert stdout == "Hello, world!\nHello, world!\nHello, world!\n"


def test_run_with_result_from_qsharp_callable_while_global_qubits_allocated(
    capsys,
) -> None:
    qdk.init()
    qdk.eval("use q = Qubit();")
    qdk.eval(
        'operation Foo() : Result { Message("Hello, world!"); use q = Qubit(); M(q) }'
    )
    foo = qdk.eval("Foo")
    results = qdk.run(foo, 3)
    assert results == [qdk.Result.Zero, qdk.Result.Zero, qdk.Result.Zero]
    stdout = capsys.readouterr().out
    assert stdout == "Hello, world!\nHello, world!\nHello, world!\n"


def test_run_with_result_callback(capsys) -> None:
    def on_result(result):
        nonlocal called
        called = True
        assert result["result"] == qdk.Result.Zero
        assert str(result["events"]) == "[Hello, world!]"

    called = False
    qdk.init()
    qdk.eval('operation Foo() : Result { Message("Hello, world!"); Zero }')
    results = qdk.run("Foo()", 3, on_result=on_result, save_events=True)
    assert (
        str(results)
        == "[{'result': Zero, 'events': [Hello, world!], 'messages': ['Hello, world!'], 'matrices': [], 'dumps': []}, {'result': Zero, 'events': [Hello, world!], 'messages': ['Hello, world!'], 'matrices': [], 'dumps': []}, {'result': Zero, 'events': [Hello, world!], 'messages': ['Hello, world!'], 'matrices': [], 'dumps': []}]"
    )
    stdout = capsys.readouterr().out
    assert stdout == ""
    assert called


def test_run_with_result_callback_from_python_callable_with_args(capsys) -> None:
    def on_result(result):
        nonlocal called
        called = True
        assert result["result"] == [qdk.Result.Zero, qdk.Result.Zero]
        assert str(result["events"]) == "[Hello, world!]"

    called = False
    qdk.init()
    qdk.eval(
        'operation Foo(nResults : Int) : Result[] { Message("Hello, world!"); Repeated(Zero, nResults) }'
    )
    results = qdk.run(qdk.code.Foo, 3, 2, on_result=on_result, save_events=True)
    assert (
        str(results)
        == "[{'result': [Zero, Zero], 'events': [Hello, world!], 'messages': ['Hello, world!'], 'matrices': [], 'dumps': []}, {'result': [Zero, Zero], 'events': [Hello, world!], 'messages': ['Hello, world!'], 'matrices': [], 'dumps': []}, {'result': [Zero, Zero], 'events': [Hello, world!], 'messages': ['Hello, world!'], 'matrices': [], 'dumps': []}]"
    )
    stdout = capsys.readouterr().out
    assert stdout == ""
    assert called


def test_run_with_result_callback_from_qsharp_callable_with_args(capsys) -> None:
    def on_result(result):
        nonlocal called
        called = True
        assert result["result"] == [qdk.Result.Zero, qdk.Result.Zero]
        assert str(result["events"]) == "[Hello, world!]"

    called = False
    qdk.init()
    qdk.eval(
        'operation Foo(nResults : Int) : Result[] { Message("Hello, world!"); Repeated(Zero, nResults) }'
    )
    foo = qdk.eval("Foo")
    results = qdk.run(foo, 3, 2, on_result=on_result, save_events=True)
    assert (
        str(results)
        == "[{'result': [Zero, Zero], 'events': [Hello, world!], 'messages': ['Hello, world!'], 'matrices': [], 'dumps': []}, {'result': [Zero, Zero], 'events': [Hello, world!], 'messages': ['Hello, world!'], 'matrices': [], 'dumps': []}, {'result': [Zero, Zero], 'events': [Hello, world!], 'messages': ['Hello, world!'], 'matrices': [], 'dumps': []}]"
    )
    stdout = capsys.readouterr().out
    assert stdout == ""
    assert called


def test_run_with_invalid_shots_produces_error() -> None:
    qdk.init()
    qdk.eval('operation Foo() : Result { Message("Hello, world!"); Zero }')
    try:
        qdk.run("Foo()", -1)
    except ValueError as e:
        assert str(e) == "The number of shots must be greater than 0."
    else:
        assert False

    try:
        qdk.run("Foo()", 0)
    except ValueError as e:
        assert str(e) == "The number of shots must be greater than 0."
    else:
        assert False


def test_run_with_complex_udt() -> None:
    qdk.init()
    val = qdk.run(
        """
        {
            new Std.Math.Complex { Real = 2., Imag = 3. }
        }
        """,
        2,
    )[0]
    assert val == 2 + 3j


def test_identity_returning_complex_udt() -> None:
    qdk.init()
    qdk.eval("function Identity(a : Std.Math.Complex) : Std.Math.Complex { a }")
    assert qdk.code.Identity(2 + 3j) == 2 + 3j


def test_run_with_udt() -> None:
    qdk.init()
    val = qdk.run(
        """
        {
            struct Data { a : Int, b : Int }
            new Data { a = 2, b = 3 }
        }
        """,
        2,
    )[0]
    assert val.a == 2 and val.b == 3


def test_callables_exposed_into_env() -> None:
    qdk.init()
    qdk.eval("function Four() : Int { 4 }")
    assert qdk.code.Four() == 4, "callable should be available"
    qdk.eval("function Add(a : Int, b : Int) : Int { a + b }")
    assert qdk.code.Four() == 4, "first callable should still be available"
    assert qdk.code.Add(2, 3) == 5, "second callable should be available"
    # After init, the callables should be cleared and no longer available
    qdk.init()
    with pytest.raises(AttributeError):
        qdk.code.Four()


def test_callable_exposed_into_env_complex_types() -> None:
    qdk.eval(
        "function Complicated(a : Int, b : (Double, BigInt)) : ((Double, BigInt), Int) { (b, a) }"
    )
    assert qdk.code.Complicated(2, (3.0, 4000000000000000000000)) == (
        (3.0, 4000000000000000000000),
        2,
    ), "callables that take complex types should marshall them correctly"


def test_callable_exposed_into_env_with_array() -> None:
    qdk.init()
    qdk.eval("function Smallest(a : Int[]) : Int { Std.Math.Min(a)}")
    assert (
        qdk.code.Smallest([1, 2, 3, 0, 4, 5]) == 0
    ), "callable that takes array should work"


def test_callable_with_int_exposed_into_env_fails_incorrect_types() -> None:
    qdk.init()
    qdk.eval("function Identity(a : Int) : Int { a }")
    assert qdk.code.Identity(4) == 4
    with pytest.raises(TypeError):
        qdk.code.Identity("4")
    with pytest.raises(TypeError):
        qdk.code.Identity(4.0)
    with pytest.raises(OverflowError):
        qdk.code.Identity(4000000000000000000000)
    with pytest.raises(TypeError):
        qdk.code.Identity([4])


def test_callable_with_double_exposed_into_env_fails_incorrect_types() -> None:
    qdk.init()
    qdk.eval("function Identity(a : Double) : Double { a }")
    assert qdk.code.Identity(4.0) == 4.0
    assert qdk.code.Identity(4) == 4.0
    with pytest.raises(TypeError):
        qdk.code.Identity("4")
    with pytest.raises(TypeError):
        qdk.code.Identity([4])


def test_callable_with_bigint_exposed_into_env_fails_incorrect_types() -> None:
    qdk.init()
    qdk.eval("function Identity(a : BigInt) : BigInt { a }")
    assert qdk.code.Identity(4000000000000000000000) == 4000000000000000000000
    with pytest.raises(TypeError):
        qdk.code.Identity("4")
    with pytest.raises(TypeError):
        qdk.code.Identity(4.0)


def test_callable_with_string_exposed_into_env_fails_incorrect_types() -> None:
    qdk.init()
    qdk.eval("function Identity(a : String) : String { a }")
    assert qdk.code.Identity("4") == "4"
    with pytest.raises(TypeError):
        qdk.code.Identity(4)
    with pytest.raises(TypeError):
        qdk.code.Identity(4.0)
    with pytest.raises(TypeError):
        qdk.code.Identity([4])


def test_callable_with_bool_exposed_into_env_fails_incorrect_types() -> None:
    qdk.init()
    qdk.eval("function Identity(a : Bool) : Bool { a }")
    assert qdk.code.Identity(True) == True
    with pytest.raises(TypeError):
        qdk.code.Identity("4")
    with pytest.raises(TypeError):
        qdk.code.Identity(4)
    with pytest.raises(TypeError):
        qdk.code.Identity(4.0)
    with pytest.raises(TypeError):
        qdk.code.Identity([4])


def test_callable_with_array_exposed_into_env_fails_incorrect_types() -> None:
    qdk.init()
    qdk.eval("function Identity(a : Int[]) : Int[] { a }")
    assert qdk.code.Identity([4, 5, 6]) == [4, 5, 6]
    assert qdk.code.Identity([]) == []
    assert qdk.code.Identity((4, 5, 6)) == [4, 5, 6]
    # This assert tests Iterables, numpy arrays fall under this category.
    assert qdk.code.Identity((elt for elt in range(4, 7))) == [4, 5, 6]
    with pytest.raises(TypeError):
        qdk.code.Identity(4)
    with pytest.raises(TypeError):
        qdk.code.Identity("4")
    with pytest.raises(TypeError):
        qdk.code.Identity(4.0)
    with pytest.raises(TypeError):
        qdk.code.Identity([1, 2, 3.0])


def test_callable_with_tuple_exposed_into_env_fails_incorrect_types() -> None:
    qdk.init()
    qdk.eval("function Identity(a : (Int, Double)) : (Int, Double) { a }")
    assert qdk.code.Identity((4, 5.0)) == (4, 5.0)
    assert qdk.code.Identity((4, 5)) == (4, 5.0)
    assert qdk.code.Identity([4, 5.0]) == (4, 5.0)
    with pytest.raises(TypeError):
        qdk.code.Identity((4, 5, 6))
    with pytest.raises(TypeError):
        qdk.code.Identity(4)
    with pytest.raises(TypeError):
        qdk.code.Identity("4")
    with pytest.raises(TypeError):
        qdk.code.Identity(4.0)
    with pytest.raises(TypeError):
        qdk.code.Identity([4.0, 5])


def test_callables_in_namespaces_exposed_into_env_submodules_and_removed_on_reinit() -> (
    None
):
    qdk.init()
    # callables should be created with their namespaces
    qdk.eval("namespace Test { function Four() : Int { 4 } }")
    qdk.eval("function Identity(a : Int) : Int { a }")
    # should be able to import callables from env and namespace submodule
    from qdk.code import Identity
    from qdk.code.Test import Four

    assert Identity(4) == 4
    assert Four() == 4
    qdk.init()
    # namespaces should be removed
    with pytest.raises(AttributeError):
        qdk.code.Test
    with pytest.raises(AttributeError):
        qdk.code.Identity()
    # imported callables should fail gracefully
    with pytest.raises(qdk.QSharpError):
        Four()


def test_callables_with_unsupported_types_raise_errors_on_call() -> None:
    qdk.init()
    qdk.eval("function Unsupported(a : Int, q : Qubit) : Unit { }")
    with pytest.raises(qdk.QSharpError, match="unsupported input type: `Qubit`"):
        qdk.code.Unsupported()


def test_callables_with_unsupported_types_in_tuples_raise_errors_on_call() -> None:
    qdk.init()
    qdk.eval("function Unsupported(q : (Int, Qubit)[]) : Unit { }")
    with pytest.raises(qdk.QSharpError, match="unsupported input type: `Qubit`"):
        qdk.code.Unsupported()


def test_callables_with_unsupported_return_types_raise_errors_on_call() -> None:
    qdk.init()
    qdk.eval('function Unsupported() : Qubit { fail "won\'t be called" }')
    with pytest.raises(qdk.QSharpError, match="unsupported output type: `Qubit`"):
        qdk.code.Unsupported()


def test_callable_with_unsupported_udt_type_raises_error_on_call() -> None:
    qdk.init()
    qdk.eval(
        """
        newtype Data = (Int, Double);
        function Unsupported(a : Data) : Unit { }
        """
    )
    with pytest.raises(
        qdk.QSharpError, match='unsupported input type: `UDT<"Data":'
    ):
        qdk.code.Unsupported()


def test_callable_with_unsupported_udt_return_type_raises_error_on_call() -> None:
    qdk.init()
    qdk.eval(
        """
        newtype Data = (Int, Double);
        function Unsupported() : Data { fail "won\'t be called" }
        """
    )
    with pytest.raises(
        qdk.QSharpError, match='unsupported output type: `UDT<"Data":'
    ):
        qdk.code.Unsupported()


def test_returning_unsupported_udt_from_eval_raises_error_on_call() -> None:
    qdk.init()
    with pytest.raises(
        TypeError, match="structs with anonymous fields are not supported: Data"
    ):
        qdk.eval(
            """
            {
                newtype Data = (Int, Double);
                Data(2, 3.0)
            }
            """
        )


def test_struct_call_constructor_exposed_into_env() -> None:
    qdk.init()
    qdk.eval("struct CustomUDT { a : Int }")
    val = qdk.code.CustomUDT(2)
    assert val.a == 2


def test_udts_are_accepted_as_input() -> None:
    qdk.init()
    qdk.eval(
        """
        struct Data { a : Int, b : Int }
        function SwapData(data : Data) : Data {
            new Data { a = data.b, b = data.a }
        }
        """
    )
    # Dict
    val = qdk.code.SwapData({"a": 2, "b": 3})
    assert val.a == 3 and val.b == 2

    # qdk.code class
    val = qdk.code.SwapData(qdk.code.Data(2, 3))
    assert val.a == 3 and val.b == 2

    # Custom class
    class CustomData:
        def __init__(self, a, b):
            self.a = a
            self.b = b

    val = qdk.code.SwapData(CustomData(2, 3))
    assert val.a == 3 and val.b == 2

    # Custom class with slots
    class CustomDataWithSlots:
        __slots__ = ["a", "b"]

        def __init__(self, a, b):
            self.a = a
            self.b = b

    val = qdk.code.SwapData(CustomDataWithSlots(2, 3))
    assert val.a == 3 and val.b == 2

    # Custom class with slots and dynamic values
    class CustomDataWithSlotsAndDynValues:
        __slots__ = ["a", "__dict__"]

        def __init__(self, a):
            self.a = a

    data = CustomDataWithSlotsAndDynValues(2)
    data.b = 3
    val = qdk.code.SwapData(data)
    assert val.a == 3 and val.b == 2


def test_lambdas_not_exposed_into_env() -> None:
    qdk.init()
    qdk.eval("a -> a + 1")
    assert not hasattr(qdk.code, "<lambda>")
    qdk.eval("q => I(q)")
    assert not hasattr(qdk.code, "<lambda>")


def test_function_defined_before_namespace_keeps_both_accessible() -> None:
    qdk.init()
    qdk.eval("function Four() : Int { 4 }")
    qdk.eval("namespace Four { function Two() : Int { 42 } }")
    assert qdk.code.Four() == 4
    assert qdk.code.Four.Two() == 42
    from qdk.code import Four
    from qdk.code.Four import Two

    assert Four() == 4
    assert Two() == 42


def test_namespace_defined_before_function_keeps_both_accessible() -> None:
    qdk.init()
    qdk.eval("namespace Four { function Two() : Int { 42 } }")
    qdk.eval("function Four() : Int { 4 }")
    assert qdk.code.Four() == 4
    assert qdk.code.Four.Two() == 42
    from qdk.code import Four
    from qdk.code.Four import Two

    assert Four() == 4
    assert Two() == 42


def test_callables_can_be_shadowed() -> None:
    qdk.init()
    qdk.eval("function Foo() : Int { 1 }")
    assert qdk.code.Foo() == 1
    qdk.eval("function Foo() : Int { 2 }")
    assert qdk.code.Foo() == 2


def test_qsharp_callables_are_not_shadowed() -> None:
    qdk.init()
    qdk.eval("function Foo() : Int { 1 }")
    foo_orig = qdk.eval("Foo")
    assert qdk.code.Foo() == 1
    qdk.eval("function Foo() : Int { 2 }")
    foo_new = qdk.eval("Foo")
    assert qdk.code.Foo() == 2
    assert qdk.run(foo_orig, 1)[0] == 1
    assert qdk.run(foo_new, 1)[0] == 2


def test_circuit_from_python_callable() -> None:
    qdk.init()
    qdk.eval(
        """
    operation Foo() : Unit {
        use q1 = Qubit();
        use q2 = Qubit();
        X(q1);
    }
    """
    )
    circuit = qdk.circuit(qdk.code.Foo)
    assert str(circuit) == dedent(
        """\
        q_0    ── X ──
        q_1    ───────
        """
    )


def test_circuit_from_qsharp_callable() -> None:
    qdk.init()
    qdk.eval(
        """
    operation Foo() : Unit {
        use q1 = Qubit();
        use q2 = Qubit();
        X(q1);
    }
    """
    )
    foo = qdk.eval("Foo")
    circuit = qdk.circuit(foo)
    assert str(circuit) == dedent(
        """\
        q_0    ── X ──
        q_1    ───────
        """
    )


def test_circuit_with_generation_method() -> None:
    qdk.init()
    qdk.eval(
        """
    operation Foo() : Unit {
        use q1 = Qubit();
        use q2 = Qubit();
        X(q1);
        Reset(q1);
    }
    """
    )
    circuit = qdk.circuit(
        qdk.code.Foo, generation_method=qdk.CircuitGenerationMethod.Simulate
    )
    assert str(circuit) == dedent(
        """\
        q_0    ── X ──── |0〉 ──
        q_1    ────────────────
        """
    )


def test_circuit_with_static_generation_method() -> None:
    qdk.init(target_profile=qdk.TargetProfile.Adaptive_RIF)
    qdk.eval(
        """
    operation Foo() : Result {
        use q = Qubit();
        H(q);
        let r = M(q);
        if r == One { X(q); }
        Reset(q);
        r
    }
    """
    )
    circuit = qdk.circuit(
        "Foo()", generation_method=qdk.CircuitGenerationMethod.Static
    )
    assert str(circuit) == dedent(
        """\
        q_0    ── H ──── M ──── if: c_0 = |1〉 ──── |0〉 ──
                         ╘═══════════ ● ═════════════════
        """
    )


def test_circuit_from_qsharp_callable_static() -> None:
    qdk.init(target_profile=qdk.TargetProfile.Adaptive_RIF)
    qdk.eval(
        """
    operation Foo() : Unit {
        use q = Qubit();
        H(q);
        let r = M(q);
        if r == One { X(q); }
        Reset(q);
    }
    """
    )
    circuit = qdk.circuit(
        qdk.code.Foo, generation_method=qdk.CircuitGenerationMethod.Static
    )
    assert str(circuit) == dedent(
        """\
        q_0    ── H ──── M ──── if: c_0 = |1〉 ──── |0〉 ──
                         ╘═══════════ ● ═════════════════
        """
    )


def test_circuit_from_python_callable_with_args() -> None:
    qdk.init()
    qdk.eval(
        """
    operation Foo(nQubits : Int) : Unit {
        use qs = Qubit[nQubits];
        ApplyToEach(X, qs);
    }
    """
    )
    circuit = qdk.circuit(qdk.code.Foo, 2)
    assert str(circuit) == dedent(
        """\
        q_0    ── X ──
        q_1    ── X ──
        """
    )


def test_circuit_from_qsharp_callable_with_args() -> None:
    qdk.init()
    qdk.eval(
        """
    operation Foo(nQubits : Int) : Unit {
        use qs = Qubit[nQubits];
        ApplyToEach(X, qs);
    }
    """
    )
    foo = qdk.eval("Foo")
    circuit = qdk.circuit(foo, 2)
    assert str(circuit) == dedent(
        """\
        q_0    ── X ──
        q_1    ── X ──
        """
    )


def test_circuit_with_measure_from_callable() -> None:
    qdk.init()
    qdk.eval("operation Foo() : Result { use q = Qubit(); H(q); return M(q) }")
    circuit = qdk.circuit(qdk.code.Foo)
    assert str(circuit) == dedent(
        """\
        q_0    ── H ──── M ──
                         ╘═══
        """
    )


def test_swap_label_circuit_from_callable() -> None:
    qdk.init()
    qdk.eval(
        "operation Foo() : Unit { use q1 = Qubit(); use q2 = Qubit(); X(q1); Relabel([q1, q2], [q2, q1]); X(q2); }"
    )
    circuit = qdk.circuit(qdk.code.Foo)
    assert str(circuit) == dedent(
        """\
        q_0    ── X ──── X ──
        q_1    ──────────────
        """
    )
