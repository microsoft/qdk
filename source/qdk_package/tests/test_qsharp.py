# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from textwrap import dedent
import pytest
from qdk import qsharp
from qdk._native import QSharpError
from qdk.simulation import run_qir
import qdk.code
from contextlib import redirect_stdout
import io
import warnings
import sys


ADAPTIVE_FALLBACK_SOURCE = """
operation Count(q : Qubit) : Int {
    mutable n = 0;
    for _ in 1..3 { X(q); set n += 1; }
    n
}
operation Main() : (Int, Result[]) {
    use q = Qubit(); let n = Count(q); use qs = Qubit[n];
    let rs = MResetEachZ(qs);
    (n + (MResetZ(q) == One ? 10 | 0), rs)
}
"""


def test_adaptive_fallback_expression_warning_and_cpu_semantics() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive)
    qsharp.eval(ADAPTIVE_FALLBACK_SOURCE)
    expected = (13, [qsharp.Result.Zero] * 3)
    assert qsharp.eval("Main()") == expected
    with pytest.warns(UserWarning, match="Qdk.Qsc.Codegen.AdaptiveProfileFallback") as caught:
        program = str(qsharp.compile("Main()"))
    assert len(caught) == 1
    assert "QIR 1.0" in str(caught[0].message)
    assert run_qir(program, shots=1, type="cpu") == [expected]
    assert program.count("call void @__quantum__qis__x__body(") == 3
    reference = ADAPTIVE_FALLBACK_SOURCE.replace(
        "let n = Count(q);", "X(q); X(q); X(q); let n = 3;"
    )
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qsharp.eval(reference)
    assert qsharp.eval("Main()") == expected
    assert run_qir(str(qsharp.compile("Main()")), shots=1, type="cpu") == [expected]

def test_adaptive_fallback_warning_can_be_promoted_to_error() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive)
    qsharp.eval(ADAPTIVE_FALLBACK_SOURCE)
    with warnings.catch_warnings():
        warnings.simplefilter("error", UserWarning)
        with pytest.raises(UserWarning, match="AdaptiveProfileFallback"):
            qsharp.compile("Main()")


def test_adaptive_fallback_global_warning_points_to_user_call() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive)
    qsharp.eval(ADAPTIVE_FALLBACK_SOURCE)
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        line = sys._getframe().f_lineno + 1
        qsharp.compile("Main()")
    assert len(caught) == 1
    assert caught[0].filename == __file__
    assert caught[0].lineno == line


def test_adaptive_fallback_context_warning_points_to_user_call() -> None:
    ctx = qdk.Context(target_profile=qsharp.TargetProfile.Adaptive)
    ctx.eval(ADAPTIVE_FALLBACK_SOURCE)
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        line = sys._getframe().f_lineno + 1
        ctx.compile("Main()")
    assert len(caught) == 1
    assert caught[0].filename == __file__
    assert caught[0].lineno == line


def test_adaptive_fallback_default_filter_distinguishes_user_call_sites() -> None:
    ctx = qdk.Context(target_profile=qsharp.TargetProfile.Adaptive)
    ctx.eval(ADAPTIVE_FALLBACK_SOURCE)
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("default")
        ctx.compile("Main()")
        ctx.compile("Main()")
    assert len(caught) == 2
    assert caught[0].lineno != caught[1].lineno


def test_adaptive_fallback_warning_respects_user_module_filter() -> None:
    ctx = qdk.Context(target_profile=qsharp.TargetProfile.Adaptive)
    ctx.eval(ADAPTIVE_FALLBACK_SOURCE)
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        warnings.filterwarnings("ignore", category=UserWarning, module=f"^{__name__}$")
        ctx.compile("Main()")
    assert not caught


def test_adaptive_fallback_native_details_do_not_present_warnings() -> None:
    from qdk._interpreter import get_interpreter

    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive)
    qsharp.eval(ADAPTIVE_FALLBACK_SOURCE)
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        qir, diagnostics = get_interpreter().qir_with_diagnostics("Main()")
    assert not caught
    assert diagnostics[0][0] == "Qdk.Qsc.Codegen.AdaptiveProfileFallback"
    assert len(diagnostics) == 1
    assert run_qir(qir, shots=1, type="cpu") == [(13, [qsharp.Result.Zero] * 3)]


def test_adaptive_fallback_native_compatibility_warning_uses_python_caller_policy() -> None:
    from qdk._interpreter import get_interpreter

    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive)
    qsharp.eval(ADAPTIVE_FALLBACK_SOURCE)
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        line = sys._getframe().f_lineno + 1
        qir = get_interpreter().qir("Main()")
    assert len(caught) == 1
    assert caught[0].filename == __file__
    assert caught[0].lineno == line
    assert "Qdk.Qsc.Codegen.AdaptiveProfileFallback" in str(caught[0].message)
    assert run_qir(qir, shots=1, type="cpu") == [(13, [qsharp.Result.Zero] * 3)]


def test_adaptive_fallback_normal_success_has_no_warning() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive)
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        qsharp.compile("{ use q = Qubit(); MResetZ(q) }")
    assert not caught

# Tests for the Python library for Q#


def test_stdout() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)
    f = io.StringIO()
    with redirect_stdout(f):
        result = qsharp.eval('Message("Hello, world!")')

    assert result is None
    assert f.getvalue() == "Hello, world!\n"


def test_stdout_multiple_lines() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)
    f = io.StringIO()
    with redirect_stdout(f):
        qsharp.eval("""
        use q = Qubit();
        Microsoft.Quantum.Diagnostics.DumpMachine();
        Message("Hello!");
        """)

    assert f.getvalue() == "STATE:\n|0⟩: 1.0000+0.0000𝑖\nHello!\n"


def test_captured_stdout() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)
    f = io.StringIO()
    with redirect_stdout(f):
        result = qsharp.eval(
            '{Message("Hello, world!"); Message("Goodbye!")}', save_events=True
        )
    assert f.getvalue() == ""
    assert len(result["messages"]) == 2
    assert result["messages"][0] == "Hello, world!"
    assert result["messages"][1] == "Goodbye!"


def test_captured_matrix() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)
    f = io.StringIO()
    with redirect_stdout(f):
        result = qsharp.eval(
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
    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)
    qsharp.set_quantum_seed(42)
    value1 = qsharp.eval(
        "{ use qs = Qubit[32]; for q in qs { H(q); }; Microsoft.Quantum.Measurement.MResetEachZ(qs) }"
    )
    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)
    qsharp.set_quantum_seed(42)
    value2 = qsharp.eval(
        "{ use qs = Qubit[32]; for q in qs { H(q); }; Microsoft.Quantum.Measurement.MResetEachZ(qs) }"
    )
    assert value1 == value2
    qsharp.set_quantum_seed(None)
    value3 = qsharp.eval(
        "{ use qs = Qubit[32]; for q in qs { H(q); }; Microsoft.Quantum.Measurement.MResetEachZ(qs) }"
    )
    assert value1 != value3


def test_classical_seed() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)
    qsharp.set_classical_seed(42)
    value1 = qsharp.eval(
        "{ mutable res = []; for _ in 0..15{ res += [(Microsoft.Quantum.Random.DrawRandomInt(0, 100), Microsoft.Quantum.Random.DrawRandomDouble(0.0, 1.0))]; }; res }"
    )
    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)
    qsharp.set_classical_seed(42)
    value2 = qsharp.eval(
        "{ mutable res = []; for _ in 0..15{ res += [(Microsoft.Quantum.Random.DrawRandomInt(0, 100), Microsoft.Quantum.Random.DrawRandomDouble(0.0, 1.0))]; }; res }"
    )
    assert value1 == value2
    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)
    qsharp.set_classical_seed(None)
    value3 = qsharp.eval(
        "{ mutable res = []; for _ in 0..15{ res += [(Microsoft.Quantum.Random.DrawRandomInt(0, 100), Microsoft.Quantum.Random.DrawRandomDouble(0.0, 1.0))]; }; res }"
    )
    assert value1 != value3


def test_dump_machine() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)
    qsharp.eval("""
    use q1 = Qubit();
    use q2 = Qubit();
    X(q1);
    """)
    state_dump = qsharp.dump_machine()
    assert state_dump.qubit_count == 2
    assert len(state_dump) == 1
    assert state_dump[2] == complex(1.0, 0.0)
    assert state_dump.as_dense_state() == [0, 0, 1, 0]
    qsharp.eval("X(q2);")
    state_dump = qsharp.dump_machine()
    assert state_dump.qubit_count == 2
    assert len(state_dump) == 1
    assert state_dump[3] == complex(1.0, 0.0)
    assert state_dump.as_dense_state() == [0, 0, 0, 1]
    qsharp.eval("H(q1);")
    state_dump = qsharp.dump_machine()
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
    qsharp.eval("ResetAll([q1, q2]);")
    qsharp.eval("Ry(0.0001, q1);")
    state_dump = qsharp.dump_machine()
    assert state_dump.qubit_count == 2
    assert len(state_dump) == 2
    assert not state_dump.check_eq([1.0])
    assert state_dump.check_eq([0.99999999875, 0.0, 4.999999997916667e-05])
    assert state_dump.check_eq([1.0], tolerance=1e-4)


def test_dump_operation() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)
    res = qsharp.dump_operation("qs => ()", 1)
    assert res == [
        [complex(1.0, 0.0), complex(0.0, 0.0)],
        [complex(0.0, 0.0), complex(1.0, 0.0)],
    ]
    res = qsharp.dump_operation("qs => H(qs[0])", 1)
    assert res == [
        [complex(0.707107, 0.0), complex(0.707107, 0.0)],
        [complex(0.707107, 0.0), complex(-0.707107, 0.0)],
    ]
    res = qsharp.dump_operation("qs => CNOT(qs[0], qs[1])", 2)
    assert res == [
        [complex(1.0, 0.0), complex(0.0, 0.0), complex(0.0, 0.0), complex(0.0, 0.0)],
        [complex(0.0, 0.0), complex(1.0, 0.0), complex(0.0, 0.0), complex(0.0, 0.0)],
        [complex(0.0, 0.0), complex(0.0, 0.0), complex(0.0, 0.0), complex(1.0, 0.0)],
        [complex(0.0, 0.0), complex(0.0, 0.0), complex(1.0, 0.0), complex(0.0, 0.0)],
    ]
    res = qsharp.dump_operation("qs => CCNOT(qs[0], qs[1], qs[2])", 3)
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
    qsharp.eval(
        "operation ApplySWAP(qs : Qubit[]) : Unit is Ctl + Adj { SWAP(qs[0], qs[1]); }"
    )
    res = qsharp.dump_operation("ApplySWAP", 2)
    assert res == [
        [complex(1.0, 0.0), complex(0.0, 0.0), complex(0.0, 0.0), complex(0.0, 0.0)],
        [complex(0.0, 0.0), complex(0.0, 0.0), complex(1.0, 0.0), complex(0.0, 0.0)],
        [complex(0.0, 0.0), complex(1.0, 0.0), complex(0.0, 0.0), complex(0.0, 0.0)],
        [complex(0.0, 0.0), complex(0.0, 0.0), complex(0.0, 0.0), complex(1.0, 0.0)],
    ]
    res = qsharp.dump_operation("qs => ()", 8)
    for i in range(8):
        for j in range(8):
            if i == j:
                assert res[i][j] == complex(1.0, 0.0)
            else:
                assert res[i][j] == complex(0.0, 0.0)


def test_run_with_noise_produces_noisy_results() -> None:
    qsharp.init()
    result = qsharp.run(
        "{ mutable errors=0; for _ in 0..100 { use q1=Qubit(); use q2=Qubit(); H(q1); CNOT(q1, q2); if MResetZ(q1) != MResetZ(q2) { errors+=1; } } errors }",
        shots=1,
        noise=qsharp.BitFlipNoise(0.1),
        seed=0,
    )
    assert result[0] > 5
    result = qsharp.run(
        "{ mutable errors=0; for _ in 0..100 { use q=Qubit(); if MResetZ(q) != Zero { errors+=1; } } errors }",
        shots=1,
        noise=qsharp.BitFlipNoise(0.1),
        seed=0,
    )
    assert result[0] > 5


def test_run_with_loss_produces_lossy_results() -> None:
    qsharp.init()
    result = qsharp.run(
        "{ use q = Qubit(); X(q); MResetZ(q) }",
        shots=1,
        qubit_loss=1.0,
        seed=0,
    )
    assert result[0] == qsharp.Result.Loss


def test_run_with_callable_and_seed_produces_deterministic_shot_results() -> None:
    qsharp.init()
    # Uses an operation that verifies both quantum and classical randomness
    qsharp.eval(
        "operation Rand() : (Int, Int) { use qs = Qubit[32]; for q in qs { H(q); }; (MeasureInteger(qs), Std.Random.DrawRandomInt(0, 1_000_000)) }"
    )
    result1 = qsharp.run(
        qdk.code.Rand,
        shots=10,
        seed=42,
    )
    result2 = qsharp.run(
        qdk.code.Rand,
        shots=10,
        seed=42,
    )
    assert (
        result1[0] != result1[2]
    )  # Check that we actually got some randomness in the results
    assert result1 == result2
    result3 = qsharp.run(
        qdk.code.Rand,
        shots=10,
        seed=None,
    )
    assert result1 != result3


def test_compile_qir_input_data() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("operation Program() : Result { use q = Qubit(); return M(q) }")
    operation = qsharp.compile("Program()")
    qir = operation._repr_qir_()
    assert isinstance(qir, bytes)


def test_compile_qir_str() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("operation Program() : Result { use q = Qubit(); return MResetZ(q); }")
    operation = qsharp.compile("Program()")
    qir = str(operation)
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert '"required_num_qubits"="1" "required_num_results"="1"' in qir


def test_compiled_array_update_scalar_captures_replacement_before_container() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive)
    qsharp.eval("""
        operation Main() : Bool[] {
            use q = Qubit();
            mutable value = MResetZ(q) == One;
            let updated = { value = not value; [false] } w/ 0 <- value;
            updated
        }
    """)

    assert qsharp.eval("Main()") == [False]
    compiled = run_qir(str(qsharp.compile("Main()")), shots=1, type="cpu")
    assert compiled == [[False]]


def test_compiled_array_update_range_captures_replacement_before_container() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive)
    qsharp.eval("""
        operation Main() : Bool[] {
            use q = Qubit();
            mutable value = MResetZ(q) == One;
            let updated = { value = not value; [false] } w/ 0..0 <- [value];
            updated
        }
    """)

    assert qsharp.eval("Main()") == [False]
    compiled = run_qir(str(qsharp.compile("Main()")), shots=1, type="cpu")
    assert compiled == [[False]]


def test_compiled_array_update_assignment_preserves_index_effects() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive)
    qsharp.eval("""
        operation Main() : Bool[] {
            use q = Qubit();
            mutable values = [false, false];
            values w/= { values w/= 1 <- MResetZ(q) == Zero; 0 } <- false;
            values
        }
    """)

    assert qsharp.eval("Main()") == [False, True]
    compiled = run_qir(str(qsharp.compile("Main()")), shots=1, type="cpu")
    assert compiled == [[False, True]]


@pytest.mark.parametrize(
    "target_profile",
    [qsharp.TargetProfile.Adaptive, qsharp.TargetProfile.Adaptive_RIF],
)
def test_curried_allocation_preserves_live_caller_qubit(
    target_profile: qsharp.TargetProfile,
) -> None:
    qsharp.init(target_profile=target_profile)
    qsharp.eval("""
        operation Inc(x : Int) : Int {
            use q = Qubit();
            H(q);
            Reset(q);
            x + 1
        }
        function Curried2(f : (Int => Int)) : (Int -> (Int => Int)) {
            a1 -> (a2 => f(a1 + 2 * a2))
        }
        operation Main() : (Int, Int, Result) {
            mutable a = 0;
            mutable b = 0;
            a = Curried2(Inc)(a)(2);
            use r = Qubit();
            X(r);
            b = Curried2(Inc)(b)(2);
            (a, b, MResetZ(r))
        }
    """)

    assert qsharp.eval("Main()") == (5, 5, qsharp.Result.One)
    compiled = run_qir(str(qsharp.compile("Main()")), shots=1, type="cpu")
    assert compiled == [(5, 5, qsharp.Result.One)]


@pytest.mark.parametrize(
    "target_profile",
    [qsharp.TargetProfile.Adaptive, qsharp.TargetProfile.Adaptive_RIF],
)
def test_curried_transitive_allocation_preserves_live_caller_qubit(
    target_profile: qsharp.TargetProfile,
) -> None:
    qsharp.init(target_profile=target_profile)
    qsharp.eval("""
        operation Inner() : Unit {
            use q = Qubit();
            H(q);
            Reset(q);
        }
        operation Inc(x : Int) : Int {
            Inner();
            x + 1
        }
        function Curried2(f : (Int => Int)) : (Int -> (Int => Int)) {
            a1 -> (a2 => f(a1 + 2 * a2))
        }
        operation Main() : (Int, Int, Result) {
            mutable a = 0;
            mutable b = 0;
            a = Curried2(Inc)(a)(2);
            use r = Qubit();
            X(r);
            b = Curried2(Inc)(b)(2);
            (a, b, MResetZ(r))
        }
    """)

    assert qsharp.eval("Main()") == (5, 5, qsharp.Result.One)
    compiled = run_qir(str(qsharp.compile("Main()")), shots=1, type="cpu")
    assert compiled == [(5, 5, qsharp.Result.One)]


@pytest.mark.parametrize(
    "target_profile",
    [qsharp.TargetProfile.Adaptive, qsharp.TargetProfile.Adaptive_RIF],
)
def test_curried_nonallocating_closure_preserves_results_and_caller_qubit(
    target_profile: qsharp.TargetProfile,
) -> None:
    qsharp.init(target_profile=target_profile)
    qsharp.eval("""
        operation Inc(q : Qubit, x : Int) : Int {
            H(q);
            Reset(q);
            x + 1
        }
        function Curried2(f : (Int => Int)) : (Int -> (Int => Int)) {
            a1 -> (a2 => f(a1 + 2 * a2))
        }
        operation Main() : (Int, Int, Result) {
            use work = Qubit();
            mutable a = 0;
            mutable b = 0;
            a = Curried2(Inc(work, _))(a)(2);
            use r = Qubit();
            X(r);
            b = Curried2(Inc(work, _))(b)(2);
            (a, b, MResetZ(r))
        }
    """)

    assert qsharp.eval("Main()") == (5, 5, qsharp.Result.One)
    compiled = run_qir(str(qsharp.compile("Main()")), shots=1, type="cpu")
    assert compiled == [(5, 5, qsharp.Result.One)]


@pytest.mark.parametrize(
    "target_profile",
    [
        qsharp.TargetProfile.Base,
        qsharp.TargetProfile.Adaptive_RIF,
        qsharp.TargetProfile.Adaptive,
    ],
)
def test_returned_scalar_callable_in_mutable_loop_returns_one(
    target_profile: qsharp.TargetProfile,
) -> None:
    qsharp.init(target_profile=target_profile)
    qsharp.eval("""
        operation Main() : Result {
            use r = Qubit();
            mutable a = 0;
            let g = a1 -> (a2 => a1 + 2 * a2);
            for _ in 0..0 { a = g(a)(2); }
            if a == 4 { X(r); }
            MResetZ(r)
        }
    """)

    assert qsharp.eval("Main()") == qsharp.Result.One
    compiled = run_qir(str(qsharp.compile("Main()")), shots=1, type="cpu")
    assert compiled == [qsharp.Result.One]


@pytest.mark.parametrize(
    "target_profile",
    [qsharp.TargetProfile.Adaptive_RIF, qsharp.TargetProfile.Adaptive],
)
def test_conditional_returned_callable_captures_true_branch_without_false_branch_effects(
    target_profile: qsharp.TargetProfile,
) -> None:
    qsharp.init(target_profile=target_profile)
    qsharp.eval("""
        function Make(x : Int) : Int => Int { y => x + 2 * y }
        operation Main() : (Int, Int, Int) {
            use q = Qubit();
            mutable a = 0;
            mutable calls = 0;
            let value = (
                if MResetZ(q) == Zero { Make({ calls += 1; a }) }
                else { Make({ calls += 100; a + 1 }) }
            )({ a = 9; 2 });
            (value, calls, a)
        }
    """)

    assert qsharp.eval("Main()") == (4, 1, 9)
    compiled = run_qir(str(qsharp.compile("Main()")), shots=1, type="cpu")
    assert compiled == [(4, 1, 9)]


@pytest.mark.parametrize(
    "target_profile",
    [qsharp.TargetProfile.Adaptive_RIF, qsharp.TargetProfile.Adaptive],
)
def test_conditional_returned_callable_captures_false_branch_without_true_branch_effects(
    target_profile: qsharp.TargetProfile,
) -> None:
    qsharp.init(target_profile=target_profile)
    qsharp.eval("""
        function Make(x : Int) : Int => Int { y => x + 2 * y }
        operation Main() : (Int, Int, Int) {
            use q = Qubit();
            X(q);
            mutable a = 0;
            mutable calls = 0;
            let value = (
                if MResetZ(q) == Zero { Make({ calls += 1; a }) }
                else { Make({ calls += 100; a + 1 }) }
            )({ a = 9; 2 });
            (value, calls, a)
        }
    """)

    assert qsharp.eval("Main()") == (5, 100, 9)
    compiled = run_qir(str(qsharp.compile("Main()")), shots=1, type="cpu")
    assert compiled == [(5, 100, 9)]


def test_returned_scalar_callable_captures_fresh_value_each_runtime_iteration() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive)
    qsharp.eval("""
        operation Main() : Result {
            use r = Qubit();
            let count = if MResetZ(r) == Zero { 3 } else { 0 };
            mutable a = 0;
            mutable i = 0;
            let g = a1 -> (a2 => a1 + 2 * a2);
            while i < count {
                a = g(a)(2);
                i += 1;
            }
            if a == 12 { X(r); }
            MResetZ(r)
        }
    """)

    assert qsharp.eval("Main()") == qsharp.Result.One
    compiled = run_qir(str(qsharp.compile("Main()")), shots=1, type="cpu")
    assert compiled == [qsharp.Result.One]


@pytest.mark.parametrize(
    "target_profile",
    [
        qsharp.TargetProfile.Base,
        qsharp.TargetProfile.Adaptive_RIF,
        qsharp.TargetProfile.Adaptive,
    ],
)
def test_returned_scalar_callable_captures_before_invocation_argument_mutation(
    target_profile: qsharp.TargetProfile,
) -> None:
    qsharp.init(target_profile=target_profile)
    qsharp.eval("""
        operation Main() : Result {
            use r = Qubit();
            mutable a = 0;
            let g = a1 -> (a2 => a1 + 2 * a2);
            for _ in 0..0 {
                a = g(a)({ a = 9; 2 });
            }
            if a == 4 { X(r); }
            MResetZ(r)
        }
    """)

    assert qsharp.eval("Main()") == qsharp.Result.One
    compiled = run_qir(str(qsharp.compile("Main()")), shots=1, type="cpu")
    assert compiled == [qsharp.Result.One]


@pytest.mark.parametrize(
    "target_profile",
    [
        qsharp.TargetProfile.Base,
        qsharp.TargetProfile.Adaptive_RIF,
        qsharp.TargetProfile.Adaptive,
    ],
)
def test_returned_scalar_callable_captures_tuple_leaves_in_source_order(
    target_profile: qsharp.TargetProfile,
) -> None:
    qsharp.init(target_profile=target_profile)
    qsharp.eval("""
        operation Main() : Result {
            use r = Qubit();
            mutable a = 0;
            let g = (x, y) -> (z => x + 10 * y + z);
            for _ in 0..0 {
                a = g(a, { a = 3; 2 })({ a = 9; 1 });
            }
            if a == 21 { X(r); }
            MResetZ(r)
        }
    """)

    assert qsharp.eval("Main()") == qsharp.Result.One
    compiled = run_qir(str(qsharp.compile("Main()")), shots=1, type="cpu")
    assert compiled == [qsharp.Result.One]


@pytest.mark.parametrize(
    "target_profile",
    [
        qsharp.TargetProfile.Base,
        qsharp.TargetProfile.Adaptive_RIF,
        qsharp.TargetProfile.Adaptive,
    ],
)
def test_returned_scalar_callable_evaluates_producer_argument_exactly_once(
    target_profile: qsharp.TargetProfile,
) -> None:
    qsharp.init(target_profile=target_profile)
    qsharp.eval("""
        operation Main() : Result[] {
            use marker = Qubit();
            use check = Qubit();
            mutable a = 0;
            mutable calls = 0;
            let g = a1 -> (a2 => a1 + 2 * a2);
            for _ in 0..0 {
                a = g({ calls += 1; X(marker); a })(2);
            }
            if a == 4 and calls == 1 { X(check); }
            [MResetZ(marker), MResetZ(check)]
        }
    """)

    assert qsharp.eval("Main()") == [qsharp.Result.One, qsharp.Result.One]
    compiled = run_qir(str(qsharp.compile("Main()")), shots=1, type="cpu")
    assert compiled == [[qsharp.Result.One, qsharp.Result.One]]


@pytest.mark.parametrize(
    "target_profile",
    [
        qsharp.TargetProfile.Base,
        qsharp.TargetProfile.Adaptive_RIF,
        qsharp.TargetProfile.Adaptive,
    ],
)
def test_returned_scalar_callable_reports_producer_argument_failure_first(
    target_profile: qsharp.TargetProfile,
) -> None:
    qsharp.init(target_profile=target_profile)
    qsharp.eval("""
        operation Main() : Unit {
            mutable a = 0;
            let g = a1 -> (a2 => a1 + 2 * a2);
            for _ in 0..0 {
                a = g({ a += 1; fail "producer-first"; a })(
                    { fail "invocation-second"; 2 }
                );
            }
        }
    """)

    with pytest.raises(QSharpError, match="program failed: producer-first"):
        qsharp.eval("Main()")
    with pytest.raises(QSharpError, match="program failed: producer-first"):
        qsharp.compile("Main()")


def test_compile_qir_str_from_python_callable() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("operation Program() : Result { use q = Qubit(); return MResetZ(q); }")
    operation = qsharp.compile(qdk.code.Program)
    qir = str(operation)
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert '"required_num_qubits"="1" "required_num_results"="1"' in qir


def test_compile_qir_str_from_qsharp_callable() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval("operation Program() : Result { use q = Qubit(); return MResetZ(q); }")
    program = qsharp.eval("Program")
    operation = qsharp.compile(program)
    qir = str(operation)
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert '"required_num_qubits"="1" "required_num_results"="1"' in qir


def test_compile_qir_str_from_python_callable_with_single_arg() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval(
        "operation Program(nQubits : Int) : Result[] { use qs = Qubit[nQubits]; MResetEachZ(qs) }"
    )
    operation = qsharp.compile(qdk.code.Program, 5)
    qir = str(operation)
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert (
        "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @5, i64 0, i64 0))"
        in qir
    )
    assert '"required_num_qubits"="5" "required_num_results"="5"' in qir


def test_compile_qir_str_from_qsharp_callable_with_single_arg() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval(
        "operation Program(nQubits : Int) : Result[] { use qs = Qubit[nQubits]; MResetEachZ(qs) }"
    )
    program = qsharp.eval("Program")
    operation = qsharp.compile(program, 5)
    qir = str(operation)
    assert "define i64 @ENTRYPOINT__main()" in qir
    assert (
        "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @5, i64 0, i64 0))"
        in qir
    )
    assert '"required_num_qubits"="5" "required_num_results"="5"' in qir


def test_compile_qir_str_from_python_callable_with_array_arg() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval(
        "operation Program(nQubits : Int[]) : Result[] { use qs = Qubit[nQubits[1]]; MResetEachZ(qs) }"
    )
    operation = qsharp.compile(qdk.code.Program, [5, 3])
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
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval(
        "operation Program(nQubits : Int[]) : Result[] { use qs = Qubit[nQubits[1]]; MResetEachZ(qs) }"
    )
    program = qsharp.eval("Program")
    operation = qsharp.compile(program, [5, 3])
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
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval(
        "operation Program(nQubits : Int, nResults : Int) : Result[] { use qs = Qubit[nQubits]; MResetEachZ(qs)[...nResults-1] }"
    )
    operation = qsharp.compile(qdk.code.Program, 5, 3)
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
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval(
        "operation Program(nQubits : Int, nResults : Int) : Result[] { use qs = Qubit[nQubits]; MResetEachZ(qs)[...nResults-1] }"
    )
    program = qsharp.eval("Program")
    operation = qsharp.compile(program, 5, 3)
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
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval(
        "operation Program(nQubits : Int, nResults : Int) : Result[] { use qs = Qubit[nQubits]; MResetEachZ(qs)[...nResults-1] }"
    )
    args = (5, 3)
    operation = qsharp.compile(qdk.code.Program, args)
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
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval(
        "operation Program(nQubits : Int, nResults : Int) : Result[] { use qs = Qubit[nQubits]; MResetEachZ(qs)[...nResults-1] }"
    )
    args = (5, 3)
    program = qsharp.eval("Program")
    operation = qsharp.compile(program, args)
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
    config = qsharp.init(target_name="ionq.simulator")
    assert config._config["targetProfile"] == "base"
    config = qsharp.init(target_name="rigetti.sim.qvm")
    assert config._config["targetProfile"] == "base"
    config = qsharp.init(target_name="quantinuum.sim")
    assert config._config["targetProfile"] == "adaptive_ri"
    config = qsharp.init(target_name="Quantinuum")
    assert config._config["targetProfile"] == "adaptive_ri"
    config = qsharp.init(target_name="IonQ")
    assert config._config["targetProfile"] == "base"


def test_run_with_result(capsys) -> None:
    qsharp.init()
    qsharp.eval('operation Foo() : Result { Message("Hello, world!"); Zero }')
    results = qsharp.run("Foo()", 3)
    assert results == [qsharp.Result.Zero, qsharp.Result.Zero, qsharp.Result.Zero]
    stdout = capsys.readouterr().out
    assert stdout == "Hello, world!\nHello, world!\nHello, world!\n"


def test_run_with_result_from_python_callable(capsys) -> None:
    qsharp.init()
    qsharp.eval(
        'operation Foo() : Result { Message("Hello, world!"); use q = Qubit(); M(q) }'
    )
    results = qsharp.run(qdk.code.Foo, 3)
    assert results == [qsharp.Result.Zero, qsharp.Result.Zero, qsharp.Result.Zero]
    stdout = capsys.readouterr().out
    assert stdout == "Hello, world!\nHello, world!\nHello, world!\n"


def test_run_with_result_from_qsharp_callable(capsys) -> None:
    qsharp.init()
    qsharp.eval(
        'operation Foo() : Result { Message("Hello, world!"); use q = Qubit(); M(q) }'
    )
    foo = qsharp.eval("Foo")
    results = qsharp.run(foo, 3)
    assert results == [qsharp.Result.Zero, qsharp.Result.Zero, qsharp.Result.Zero]
    stdout = capsys.readouterr().out
    assert stdout == "Hello, world!\nHello, world!\nHello, world!\n"


def test_run_with_result_from_python_callable_while_global_qubits_allocated(
    capsys,
) -> None:
    qsharp.init()
    qsharp.eval("use q = Qubit();")
    qsharp.eval(
        'operation Foo() : Result { Message("Hello, world!"); use q = Qubit(); M(q) }'
    )
    results = qsharp.run(qdk.code.Foo, 3)
    assert results == [qsharp.Result.Zero, qsharp.Result.Zero, qsharp.Result.Zero]
    stdout = capsys.readouterr().out
    assert stdout == "Hello, world!\nHello, world!\nHello, world!\n"


def test_run_with_result_from_qsharp_callable_while_global_qubits_allocated(
    capsys,
) -> None:
    qsharp.init()
    qsharp.eval("use q = Qubit();")
    qsharp.eval(
        'operation Foo() : Result { Message("Hello, world!"); use q = Qubit(); M(q) }'
    )
    foo = qsharp.eval("Foo")
    results = qsharp.run(foo, 3)
    assert results == [qsharp.Result.Zero, qsharp.Result.Zero, qsharp.Result.Zero]
    stdout = capsys.readouterr().out
    assert stdout == "Hello, world!\nHello, world!\nHello, world!\n"


def test_run_with_result_callback(capsys) -> None:
    def on_result(result):
        nonlocal called
        called = True
        assert result["result"] == qsharp.Result.Zero
        assert str(result["events"]) == "[Hello, world!]"

    called = False
    qsharp.init()
    qsharp.eval('operation Foo() : Result { Message("Hello, world!"); Zero }')
    results = qsharp.run("Foo()", 3, on_result=on_result, save_events=True)
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
        assert result["result"] == [qsharp.Result.Zero, qsharp.Result.Zero]
        assert str(result["events"]) == "[Hello, world!]"

    called = False
    qsharp.init()
    qsharp.eval(
        'operation Foo(nResults : Int) : Result[] { Message("Hello, world!"); Repeated(Zero, nResults) }'
    )
    results = qsharp.run(qdk.code.Foo, 3, 2, on_result=on_result, save_events=True)
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
        assert result["result"] == [qsharp.Result.Zero, qsharp.Result.Zero]
        assert str(result["events"]) == "[Hello, world!]"

    called = False
    qsharp.init()
    qsharp.eval(
        'operation Foo(nResults : Int) : Result[] { Message("Hello, world!"); Repeated(Zero, nResults) }'
    )
    foo = qsharp.eval("Foo")
    results = qsharp.run(foo, 3, 2, on_result=on_result, save_events=True)
    assert (
        str(results)
        == "[{'result': [Zero, Zero], 'events': [Hello, world!], 'messages': ['Hello, world!'], 'matrices': [], 'dumps': []}, {'result': [Zero, Zero], 'events': [Hello, world!], 'messages': ['Hello, world!'], 'matrices': [], 'dumps': []}, {'result': [Zero, Zero], 'events': [Hello, world!], 'messages': ['Hello, world!'], 'matrices': [], 'dumps': []}]"
    )
    stdout = capsys.readouterr().out
    assert stdout == ""
    assert called


def test_run_with_invalid_shots_produces_error() -> None:
    qsharp.init()
    qsharp.eval('operation Foo() : Result { Message("Hello, world!"); Zero }')
    try:
        qsharp.run("Foo()", -1)
    except ValueError as e:
        assert str(e) == "The number of shots must be greater than 0."
    else:
        assert False

    try:
        qsharp.run("Foo()", 0)
    except ValueError as e:
        assert str(e) == "The number of shots must be greater than 0."
    else:
        assert False


def test_run_with_complex_udt() -> None:
    qsharp.init()
    val = qsharp.run(
        """
        {
            new Std.Math.Complex { Real = 2., Imag = 3. }
        }
        """,
        2,
    )[0]
    assert val == 2 + 3j


def test_identity_returning_complex_udt() -> None:
    qsharp.init()
    qsharp.eval("function Identity(a : Std.Math.Complex) : Std.Math.Complex { a }")
    assert qdk.code.Identity(2 + 3j) == 2 + 3j


def test_run_with_udt() -> None:
    qsharp.init()
    val = qsharp.run(
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
    qsharp.init()
    qsharp.eval("function Four() : Int { 4 }")
    assert qdk.code.Four() == 4, "callable should be available"
    qsharp.eval("function Add(a : Int, b : Int) : Int { a + b }")
    assert qdk.code.Four() == 4, "first callable should still be available"
    assert qdk.code.Add(2, 3) == 5, "second callable should be available"
    # After init, the callables should be cleared and no longer available
    qsharp.init()
    with pytest.raises(AttributeError):
        qdk.code.Four()


def test_callable_exposed_into_env_complex_types() -> None:
    qsharp.eval(
        "function Complicated(a : Int, b : (Double, BigInt)) : ((Double, BigInt), Int) { (b, a) }"
    )
    assert qdk.code.Complicated(2, (3.0, 4000000000000000000000)) == (
        (3.0, 4000000000000000000000),
        2,
    ), "callables that take complex types should marshall them correctly"


def test_callable_exposed_into_env_with_array() -> None:
    qsharp.init()
    qsharp.eval("function Smallest(a : Int[]) : Int { Std.Math.Min(a)}")
    assert (
        qdk.code.Smallest([1, 2, 3, 0, 4, 5]) == 0
    ), "callable that takes array should work"


def test_callable_with_int_exposed_into_env_fails_incorrect_types() -> None:
    qsharp.init()
    qsharp.eval("function Identity(a : Int) : Int { a }")
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
    qsharp.init()
    qsharp.eval("function Identity(a : Double) : Double { a }")
    assert qdk.code.Identity(4.0) == 4.0
    assert qdk.code.Identity(4) == 4.0
    with pytest.raises(TypeError):
        qdk.code.Identity("4")
    with pytest.raises(TypeError):
        qdk.code.Identity([4])


def test_callable_with_bigint_exposed_into_env_fails_incorrect_types() -> None:
    qsharp.init()
    qsharp.eval("function Identity(a : BigInt) : BigInt { a }")
    assert qdk.code.Identity(4000000000000000000000) == 4000000000000000000000
    with pytest.raises(TypeError):
        qdk.code.Identity("4")
    with pytest.raises(TypeError):
        qdk.code.Identity(4.0)


def test_callable_with_string_exposed_into_env_fails_incorrect_types() -> None:
    qsharp.init()
    qsharp.eval("function Identity(a : String) : String { a }")
    assert qdk.code.Identity("4") == "4"
    with pytest.raises(TypeError):
        qdk.code.Identity(4)
    with pytest.raises(TypeError):
        qdk.code.Identity(4.0)
    with pytest.raises(TypeError):
        qdk.code.Identity([4])


def test_callable_with_bool_exposed_into_env_fails_incorrect_types() -> None:
    qsharp.init()
    qsharp.eval("function Identity(a : Bool) : Bool { a }")
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
    qsharp.init()
    qsharp.eval("function Identity(a : Int[]) : Int[] { a }")
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
    qsharp.init()
    qsharp.eval("function Identity(a : (Int, Double)) : (Int, Double) { a }")
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
    qsharp.init()
    # callables should be created with their namespaces
    qsharp.eval("namespace Test { function Four() : Int { 4 } }")
    qsharp.eval("function Identity(a : Int) : Int { a }")
    # should be able to import callables from env and namespace submodule
    from qdk.code import Identity
    from qdk.code.Test import Four

    assert Identity(4) == 4
    assert Four() == 4
    qsharp.init()
    # namespaces should be removed
    with pytest.raises(AttributeError):
        qdk.code.Test
    with pytest.raises(AttributeError):
        qdk.code.Identity()
    # imported callables should fail gracefully
    with pytest.raises(qsharp.QSharpError):
        Four()


def test_callables_with_unsupported_types_raise_errors_on_call() -> None:
    qsharp.init()
    qsharp.eval("function Unsupported(a : Int, q : Qubit) : Unit { }")
    with pytest.raises(qsharp.QSharpError, match="unsupported input type: `Qubit`"):
        qdk.code.Unsupported()


def test_callables_with_unsupported_types_in_tuples_raise_errors_on_call() -> None:
    qsharp.init()
    qsharp.eval("function Unsupported(q : (Int, Qubit)[]) : Unit { }")
    with pytest.raises(qsharp.QSharpError, match="unsupported input type: `Qubit`"):
        qdk.code.Unsupported()


def test_callables_with_unsupported_return_types_raise_errors_on_call() -> None:
    qsharp.init()
    qsharp.eval('function Unsupported() : Qubit { fail "won\'t be called" }')
    with pytest.raises(qsharp.QSharpError, match="unsupported output type: `Qubit`"):
        qdk.code.Unsupported()


def test_callable_with_unsupported_udt_type_raises_error_on_call() -> None:
    qsharp.init()
    qsharp.eval("""
        newtype Data = (Int, Double);
        function Unsupported(a : Data) : Unit { }
        """)
    with pytest.raises(
        qsharp.QSharpError, match='unsupported input type: `UDT<"Data":'
    ):
        qdk.code.Unsupported()


def test_callable_with_unsupported_udt_return_type_raises_error_on_call() -> None:
    qsharp.init()
    qsharp.eval("""
        newtype Data = (Int, Double);
        function Unsupported() : Data { fail "won\'t be called" }
        """)
    with pytest.raises(
        qsharp.QSharpError, match='unsupported output type: `UDT<"Data":'
    ):
        qdk.code.Unsupported()


def test_callable_with_functor_raises_error_on_call() -> None:
    qsharp.init()
    callable = qsharp.eval("""
        operation Unsupported() : Unit is Adj + Ctl { }
        Adjoint Unsupported
        """)
    with pytest.raises(
        qsharp.QSharpError,
        match="callable must have no applied functors",
    ):
        qsharp.run(callable, shots=1)
    callable = qsharp.eval("Controlled Unsupported")
    with pytest.raises(
        qsharp.QSharpError,
        match="callable must have no applied functors",
    ):
        qsharp.run(callable, shots=1)


def test_callable_with_generic_type_raises_error_on_call() -> None:
    qsharp.init()
    qsharp.eval("""
        function Unsupported<'T>(a : 'T) : Unit { }
        """)
    with pytest.raises(qsharp.QSharpError, match="unsupported input type"):
        qdk.code.Unsupported()


def test_callable_with_function_argument_with_generic_type_raises_error_on_call() -> (
    None
):
    qsharp.init()
    qsharp.eval("""
        function Unsupported<'T>(a : 'T -> 'T) : Unit { }
        """)
    with pytest.raises(qsharp.QSharpError, match="unsupported input type"):
        qdk.code.Unsupported()


def test_callable_with_generic_function_as_input_raises_error_on_call() -> None:
    qsharp.init()
    qsharp.eval("""
        function Identity<'T>(x : 'T) : 'T { x }
        function IntIdentity(x : Int, id : (Int -> Int)) : Int { id(x) }
        """)
    with pytest.raises(qsharp.QSharpError, match="unsupported input type"):
        qdk.code.IntIdentity(42, qdk.code.Identity)


def test_returning_unsupported_udt_from_eval_raises_error_on_call() -> None:
    qsharp.init()
    with pytest.raises(
        TypeError, match="structs with anonymous fields are not supported: Data"
    ):
        qsharp.eval("""
            {
                newtype Data = (Int, Double);
                Data(2, 3.0)
            }
            """)


def test_struct_call_constructor_exposed_into_env() -> None:
    qsharp.init()
    qsharp.eval("struct CustomUDT { a : Int }")
    val = qdk.code.CustomUDT(2)
    assert val.a == 2


def test_udts_are_accepted_as_input() -> None:
    qsharp.init()
    qsharp.eval("""
        struct Data { a : Int, b : Int }
        function SwapData(data : Data) : Data {
            new Data { a = data.b, b = data.a }
        }
        """)
    # Dict
    val = qdk.code.SwapData({"a": 2, "b": 3})
    assert val.a == 3 and val.b == 2

    # qsharp.code class
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
    qsharp.init()
    qsharp.eval("a -> a + 1")
    assert not hasattr(qdk.code, ".lambda")
    qsharp.eval("q => I(q)")
    assert not hasattr(qdk.code, ".lambda")


def test_function_defined_before_namespace_keeps_both_accessible() -> None:
    qsharp.init()
    qsharp.eval("function Four() : Int { 4 }")
    qsharp.eval("namespace Four { function Two() : Int { 42 } }")
    assert qdk.code.Four() == 4
    assert qdk.code.Four.Two() == 42
    from qdk.code import Four
    from qdk.code.Four import Two

    assert Four() == 4
    assert Two() == 42


def test_namespace_defined_before_function_keeps_both_accessible() -> None:
    qsharp.init()
    qsharp.eval("namespace Four { function Two() : Int { 42 } }")
    qsharp.eval("function Four() : Int { 4 }")
    assert qdk.code.Four() == 4
    assert qdk.code.Four.Two() == 42
    from qdk.code import Four
    from qdk.code.Four import Two

    assert Four() == 4
    assert Two() == 42


def test_callables_can_be_shadowed() -> None:
    qsharp.init()
    qsharp.eval("function Foo() : Int { 1 }")
    assert qdk.code.Foo() == 1
    qsharp.eval("function Foo() : Int { 2 }")
    assert qdk.code.Foo() == 2


def test_qsharp_callables_are_not_shadowed() -> None:
    qsharp.init()
    qsharp.eval("function Foo() : Int { 1 }")
    foo_orig = qsharp.eval("Foo")
    assert qdk.code.Foo() == 1
    qsharp.eval("function Foo() : Int { 2 }")
    foo_new = qsharp.eval("Foo")
    assert qdk.code.Foo() == 2
    assert qsharp.run(foo_orig, 1)[0] == 1
    assert qsharp.run(foo_new, 1)[0] == 2


def test_circuit_from_python_callable() -> None:
    qsharp.init()
    qsharp.eval("""
    operation Foo() : Unit {
        use q1 = Qubit();
        use q2 = Qubit();
        X(q1);
    }
    """)
    circuit = qsharp.circuit(qdk.code.Foo)
    assert str(circuit) == dedent("""\
        q_0    ── X ──
        q_1    ───────
        """)


def test_circuit_from_qsharp_callable() -> None:
    qsharp.init()
    qsharp.eval("""
    operation Foo() : Unit {
        use q1 = Qubit();
        use q2 = Qubit();
        X(q1);
    }
    """)
    foo = qsharp.eval("Foo")
    circuit = qsharp.circuit(foo)
    assert str(circuit) == dedent("""\
        q_0    ── X ──
        q_1    ───────
        """)


def test_circuit_with_generation_method() -> None:
    qsharp.init()
    qsharp.eval("""
    operation Foo() : Unit {
        use q1 = Qubit();
        use q2 = Qubit();
        X(q1);
        Reset(q1);
    }
    """)
    circuit = qsharp.circuit(
        qdk.code.Foo, generation_method=qsharp.CircuitGenerationMethod.Simulate
    )
    assert str(circuit) == dedent("""\
        q_0    ── X ──── |0〉 ──
        q_1    ────────────────
        """)


def test_circuit_with_static_generation_method() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qsharp.eval("""
    operation Foo() : Result {
        use q = Qubit();
        H(q);
        let r = M(q);
        if r == One { X(q); } else { Z(q); }
        Reset(q);
        r
    }
    """)
    circuit = qsharp.circuit(
        "Foo()", generation_method=qsharp.CircuitGenerationMethod.Static
    )
    assert str(circuit) == dedent("""\
        q_0    ── H ──── M ──── X ──── Z ──── |0〉 ──
                         ╘═════ ● ════ ○ ═══════════
        """)


def test_circuit_from_qsharp_callable_static() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive_RIF)
    qsharp.eval("""
    operation Foo() : Unit {
        use q = Qubit();
        H(q);
        let r = M(q);
        if r == One { X(q); }
        Reset(q);
    }
    """)
    circuit = qsharp.circuit(
        qdk.code.Foo, generation_method=qsharp.CircuitGenerationMethod.Static
    )
    assert str(circuit) == dedent("""\
        q_0    ── H ──── M ──── X ──── |0〉 ──
                         ╘═════ ● ═══════════
        """)


def test_circuit_from_python_callable_with_args() -> None:
    qsharp.init()
    qsharp.eval("""
    operation Foo(nQubits : Int) : Unit {
        use qs = Qubit[nQubits];
        ApplyToEach(X, qs);
    }
    """)
    circuit = qsharp.circuit(qdk.code.Foo, 2)
    assert str(circuit) == dedent("""\
        q_0    ── X ──
        q_1    ── X ──
        """)


def test_circuit_from_qsharp_callable_with_args() -> None:
    qsharp.init()
    qsharp.eval("""
    operation Foo(nQubits : Int) : Unit {
        use qs = Qubit[nQubits];
        ApplyToEach(X, qs);
    }
    """)
    foo = qsharp.eval("Foo")
    circuit = qsharp.circuit(foo, 2)
    assert str(circuit) == dedent("""\
        q_0    ── X ──
        q_1    ── X ──
        """)


def test_circuit_with_measure_from_callable() -> None:
    qsharp.init()
    qsharp.eval("operation Foo() : Result { use q = Qubit(); H(q); return M(q) }")
    circuit = qsharp.circuit(qdk.code.Foo)
    assert str(circuit) == dedent("""\
        q_0    ── H ──── M ──
                         ╘═══
        """)


def test_swap_label_circuit_from_callable() -> None:
    qsharp.init()
    qsharp.eval(
        "operation Foo() : Unit { use q1 = Qubit(); use q2 = Qubit(); X(q1); Relabel([q1, q2], [q2, q1]); X(q2); }"
    )
    circuit = qsharp.circuit(qdk.code.Foo)
    assert str(circuit) == dedent("""\
        q_0    ── X ──── X ──
        q_1    ──────────────
        """)


def test_qdk_config() -> None:
    qsharp.init(qdk_config={"int_config": 123})
    assert qsharp.eval("""Std.Core.ConfigValue("int_config", 0)""") == 123


def test_qsharp_input_arg_cannot_be_loss() -> None:
    qsharp.init()
    qsharp.eval("""
    operation Foo(r : Result) : Result {
        r
    }
    """)
    with pytest.raises(TypeError):
        qsharp.compile(qdk.code.Foo, qdk.Result.Loss)
