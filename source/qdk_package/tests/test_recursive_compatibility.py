# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import json
from pathlib import Path
import subprocess
import sys

import pytest

from qdk import qsharp
from qdk.simulation import run_qir

PROFILES = [
    qsharp.TargetProfile.Adaptive_RI,
    qsharp.TargetProfile.Adaptive_RIF,
    qsharp.TargetProfile.Adaptive,
]
RUNTIME_PAYLOAD_CASES = [
    pytest.param("", 2, qsharp.Result.Zero, id="zero-even-depth"),
    pytest.param("", 3, qsharp.Result.Zero, id="zero-odd-depth"),
    pytest.param("X(input);", 2, qsharp.Result.Zero, id="one-even-depth"),
    pytest.param("X(input);", 3, qsharp.Result.One, id="one-odd-depth"),
]


def check_result(
    source: str,
    profile: qsharp.TargetProfile,
    expected: qsharp.Result | int,
    *,
    entry_expr: str = "Main()",
    shots: int = 4,
) -> None:
    expected_value: str | int
    if isinstance(expected, qsharp.Result):
        expected_value = "One" if expected == qsharp.Result.One else "Zero"
    else:
        expected_value = expected
    _run_probe_subprocess(
        [str(Path(__file__).resolve())],
        input_data=json.dumps(
            {
                "source": source,
                "profile": str(profile),
                "expected": expected_value,
                "entry_expr": entry_expr,
                "shots": shots,
            }
        ),
    )


def _run_probe_subprocess(
    arguments: list[str], *, input_data: str = "", timeout: float = 120
) -> None:
    try:
        result = subprocess.run(
            [sys.executable, *arguments],
            input=input_data,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        raise AssertionError(
            f"recursive probe timed out after {timeout} seconds\n"
            f"stdout: {error.stdout!r}\nstderr: {error.stderr!r}"
        ) from error
    if result.returncode != 0:
        raise AssertionError(
            f"recursive probe failed with exit code {result.returncode}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )


def _check_result_in_process(
    source: str,
    profile: qsharp.TargetProfile,
    expected: qsharp.Result | int,
    *,
    entry_expr: str,
    shots: int,
) -> None:
    print("Interpreting source", flush=True)
    qsharp.init(target_profile=profile)
    qsharp.eval(source)
    interpreted = qsharp.eval(entry_expr)
    if interpreted != expected:
        raise AssertionError(
            f"source result mismatch: expected {expected!r}, got {interpreted!r}"
        )
    print("Compiling QIR", flush=True)
    qir = str(qsharp.compile(entry_expr))
    print("Executing QIR", flush=True)
    compiled = run_qir(qir, shots=shots, type="cpu")
    if compiled != [expected] * shots:
        raise AssertionError(
            f"compiled result mismatch: expected {[expected] * shots!r}, got {compiled!r}"
        )


def test_probe_subprocess_preserves_input_and_success() -> None:
    _run_probe_subprocess(
        [
            "-c",
            "import sys\n"
            "if sys.stdin.read() != 'probe input': raise AssertionError('input changed')",
        ],
        input_data="probe input",
    )


def test_probe_subprocess_preserves_assertion_diagnostics() -> None:
    with pytest.raises(AssertionError, match="probe assertion marker"):
        _run_probe_subprocess(["-c", "raise AssertionError('probe assertion marker')"])


def test_probe_subprocess_reports_abrupt_exit() -> None:
    with pytest.raises(AssertionError, match="exit code 23"):
        _run_probe_subprocess(["-c", "import os; os._exit(23)"])


def test_probe_subprocess_kills_and_reaps_timed_out_child(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    killed: list[subprocess.Popen[str]] = []
    original_kill = subprocess.Popen.kill

    def record_kill(process: subprocess.Popen[str]) -> None:
        killed.append(process)
        original_kill(process)

    monkeypatch.setattr(subprocess.Popen, "kill", record_kill)
    with pytest.raises(AssertionError, match="timed out") as error:
        _run_probe_subprocess(
            [
                "-c",
                "import time; print('probe started', flush=True); time.sleep(60)",
            ],
            timeout=5,
        )
    assert "probe started" in str(error.value)
    assert len(killed) == 1
    assert killed[0].returncode is not None


@pytest.mark.parametrize("optimization", ["0", "1"])
def test_probe_subprocess_reports_source_result_mismatch(
    optimization: str, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("PYTHONOPTIMIZE", optimization)
    with pytest.raises(AssertionError, match="source result mismatch"):
        check_result("operation Main() : Int { 1 }", qsharp.TargetProfile.Adaptive, 2)


def test_probe_subprocess_preserves_compiler_diagnostic() -> None:
    with pytest.raises(AssertionError, match="Qdk.Qsc.PartialEval.OutputResultLiteral"):
        check_result(
            "operation Main() : Result { Zero }",
            qsharp.TargetProfile.Adaptive,
            qsharp.Result.Zero,
        )


@pytest.mark.parametrize("profile", PROFILES, ids=str)
@pytest.mark.parametrize("preparation,depth,expected", RUNTIME_PAYLOAD_CASES)
def test_static_recursion_bound_preserves_measured_result(
    profile: qsharp.TargetProfile,
    preparation: str,
    depth: int,
    expected: qsharp.Result,
) -> None:
    source = f"""
        operation Rec(remaining : Int, payload : Result, flag : Qubit, target : Qubit) : Unit {{
            if remaining > 0 {{
                if MResetZ(flag) == Zero {{
                    if payload == One {{ X(target); }}
                    Rec(remaining - 1, payload, flag, target);
                }}
            }}
        }}
        operation Main() : Result {{
            use (input, flag, target) = (Qubit(), Qubit(), Qubit());
            {preparation}
            let measured = MResetZ(input);
            Rec({depth}, measured, flag, target);
            MResetZ(target)
        }}
    """
    check_result(source, profile, expected)


@pytest.mark.parametrize("profile", PROFILES, ids=str)
@pytest.mark.parametrize("preparation,depth,expected", RUNTIME_PAYLOAD_CASES)
def test_static_recursion_bound_preserves_measured_boolean(
    profile: qsharp.TargetProfile,
    preparation: str,
    depth: int,
    expected: qsharp.Result,
) -> None:
    source = f"""
        operation Rec(remaining : Int, payload : Bool, flag : Qubit, target : Qubit) : Unit {{
            if remaining > 0 {{
                if MResetZ(flag) == Zero {{
                    if payload {{ X(target); }}
                    Rec(remaining - 1, payload, flag, target);
                }}
            }}
        }}
        operation Main() : Result {{
            use (input, flag, target) = (Qubit(), Qubit(), Qubit());
            {preparation}
            let measured = MResetZ(input);
            Rec({depth}, measured == One, flag, target);
            MResetZ(target)
        }}
    """
    check_result(source, profile, expected)


@pytest.mark.parametrize("profile", PROFILES, ids=str)
@pytest.mark.parametrize("preparation,depth,expected", RUNTIME_PAYLOAD_CASES)
def test_static_recursion_bound_preserves_measured_integer(
    profile: qsharp.TargetProfile,
    preparation: str,
    depth: int,
    expected: qsharp.Result,
) -> None:
    source = f"""
        operation Rec(remaining : Int, payload : Int, flag : Qubit, target : Qubit) : Unit {{
            if remaining > 0 {{
                if MResetZ(flag) == Zero {{
                    if payload == 1 {{ X(target); }}
                    Rec(remaining - 1, payload, flag, target);
                }}
            }}
        }}
        operation Main() : Result {{
            use (input, flag, target) = (Qubit(), Qubit(), Qubit());
            {preparation}
            let measured = MResetZ(input);
            Rec({depth}, if measured == One {{ 1 }} else {{ 0 }}, flag, target);
            MResetZ(target)
        }}
    """
    check_result(source, profile, expected)


@pytest.mark.parametrize("profile", PROFILES, ids=str)
@pytest.mark.parametrize(
    "depth,expected", [(2, qsharp.Result.Zero), (3, qsharp.Result.One)]
)
def test_static_bound_in_tuple_preserves_runtime_result(
    profile: qsharp.TargetProfile, depth: int, expected: qsharp.Result
) -> None:
    source = f"""
        operation Rec(state : (Int, Result), flag : Qubit, target : Qubit) : Unit {{
            let (remaining, payload) = state;
            if remaining > 0 {{
                if MResetZ(flag) == Zero {{
                    if payload == One {{ X(target); }}
                    Rec((remaining - 1, payload), flag, target);
                }}
            }}
        }}
        operation Main() : Result {{
            use (input, flag, target) = (Qubit(), Qubit(), Qubit());
            X(input);
            Rec(({depth}, MResetZ(input)), flag, target);
            MResetZ(target)
        }}
    """
    check_result(source, profile, expected)


@pytest.mark.parametrize("profile", PROFILES, ids=str)
def test_static_bound_in_array_element_preserves_results_after_child_returns(
    profile: qsharp.TargetProfile,
) -> None:
    source = """
        operation Rec(state : (Int, Result)[], flag : Qubit, targets : Qubit[]) : Unit {
            let (remaining, first) = state[0];
            if remaining > 0 {
                if MResetZ(flag) == Zero {
                    Rec([(remaining - 1, first), state[1], state[2]], flag, targets);
                    let (_, payload) = state[remaining - 1];
                    if payload == One { X(targets[remaining - 1]); }
                }
            }
        }
        operation Main() : Int {
            use (input, flag) = (Qubit(), Qubit());
            use targets = Qubit[3];
            X(input);
            let one = MResetZ(input);
            let zero = MResetZ(input);
            Rec([(3, one), (0, one), (0, zero)], flag, targets);
            let first = if MResetZ(targets[0]) == One { 1 } else { 0 };
            let second = if MResetZ(targets[1]) == One { 2 } else { 0 };
            let third = if MResetZ(targets[2]) == One { 4 } else { 0 };
            first + second + third
        }
    """
    check_result(source, profile, 3)


@pytest.mark.parametrize("profile", PROFILES, ids=str)
def test_shrinking_result_array_preserves_values_after_child_returns(
    profile: qsharp.TargetProfile,
) -> None:
    source = """
        operation Rec(values : Result[], flag : Qubit, targets : Qubit[]) : Unit {
            let count = Length(values);
            if count > 0 {
                if MResetZ(flag) == Zero {
                    Rec(values[1...], flag, targets);
                    if values[0] == One { X(targets[count - 1]); }
                }
            }
        }
        operation Main() : Int {
            use (input, flag) = (Qubit(), Qubit());
            use targets = Qubit[3];
            X(input);
            let one = MResetZ(input);
            let zero = MResetZ(input);
            Rec([one, one, zero], flag, targets);
            let first = if MResetZ(targets[0]) == One { 1 } else { 0 };
            let second = if MResetZ(targets[1]) == One { 2 } else { 0 };
            let third = if MResetZ(targets[2]) == One { 4 } else { 0 };
            first + second + third
        }
    """
    check_result(source, profile, 6)


@pytest.mark.parametrize("profile", PROFILES, ids=str)
@pytest.mark.parametrize(
    "depth,expected", [(2, qsharp.Result.Zero), (3, qsharp.Result.One)]
)
def test_mutual_recursion_preserves_static_tuple_bound_and_runtime_result(
    profile: qsharp.TargetProfile, depth: int, expected: qsharp.Result
) -> None:
    source = f"""
        operation Rec(state : (Int, Result), flag : Qubit, target : Qubit) : Unit {{
            let (remaining, payload) = state;
            if remaining > 0 {{
                if MResetZ(flag) == Zero {{
                    if payload == One {{ X(target); }}
                    Other((remaining - 1, payload), flag, target);
                }}
            }}
        }}
        operation Other(state : (Int, Result), flag : Qubit, target : Qubit) : Unit {{
            Rec(state, flag, target);
        }}
        operation Main() : Result {{
            use (input, flag, target) = (Qubit(), Qubit(), Qubit());
            X(input);
            Rec(({depth}, MResetZ(input)), flag, target);
            MResetZ(target)
        }}
    """
    check_result(source, profile, expected)


@pytest.mark.parametrize("profile", PROFILES, ids=str)
@pytest.mark.parametrize(
    "control_preparation,depth,expected",
    [
        pytest.param("", 2, qsharp.Result.Zero, id="inactive-control-even-depth"),
        pytest.param("", 3, qsharp.Result.Zero, id="inactive-control-odd-depth"),
        pytest.param(
            "X(control);", 2, qsharp.Result.Zero, id="active-control-even-depth"
        ),
        pytest.param(
            "X(control);", 3, qsharp.Result.One, id="active-control-odd-depth"
        ),
    ],
)
def test_static_bound_preserves_runtime_result_in_controlled_adjoint_recursion(
    profile: qsharp.TargetProfile,
    control_preparation: str,
    depth: int,
    expected: qsharp.Result,
) -> None:
    source = (
        """
        operation Rec(remaining : Int, payload : Result, qs : Qubit[]) : Unit is Adj + Ctl {
            body (...) {}
            adjoint (...) {}
            controlled (cs, ...) {}
            controlled adjoint (cs, ...) {
                if remaining > 0 {
                    if MResetZ(qs[0]) == Zero {
                        if payload == One { Controlled X(cs, qs[1]); }
                        Controlled Controlled Adjoint Rec([], (cs, (remaining - 1, payload, qs)));
                    }
                }
            }
        }
    """
        + f"""
        operation Main() : Result {{
            use (input, flag, target, control) = (Qubit(), Qubit(), Qubit(), Qubit());
            X(input);
            {control_preparation}
            Controlled Adjoint Rec([control], ({depth}, MResetZ(input), [flag, target]));
            Reset(control);
            MResetZ(target)
        }}
    """
    )
    check_result(source, profile, expected)


@pytest.mark.parametrize("profile", PROFILES, ids=str)
@pytest.mark.parametrize(
    "inherited_preparation,added_preparation,expected",
    [
        pytest.param(
            "X(inherited);", "X(added);", qsharp.Result.Zero, id="both-controls-active"
        ),
        pytest.param(
            "X(inherited);", "", qsharp.Result.One, id="added-control-inactive"
        ),
        pytest.param(
            "", "X(added);", qsharp.Result.Zero, id="inherited-control-inactive"
        ),
    ],
)
def test_static_bound_preserves_runtime_result_when_recursive_descent_adds_control(
    profile: qsharp.TargetProfile,
    inherited_preparation: str,
    added_preparation: str,
    expected: qsharp.Result,
) -> None:
    source = f"""
        operation Rec(
            remaining : Int, payload : Result, qs : Qubit[]
        ) : Unit is Adj + Ctl {{
            body (...) {{}}
            adjoint (...) {{}}
            controlled (cs, ...) {{}}
            controlled adjoint (cs, ...) {{
                if remaining > 0 {{
                    if MResetZ(qs[0]) == Zero {{
                        if payload == One {{ Controlled X(cs, qs[1]); }}
                        if remaining > 1 {{
                            // The new control is no longer a data qubit.
                            Controlled Controlled Adjoint Rec(
                                [qs[2]],
                                (cs, (remaining - 1, payload, [qs[0], qs[1]]))
                            );
                        }}
                    }}
                }}
            }}
        }}
        operation Main() : Result {{
            use (input, flag, target) = (Qubit(), Qubit(), Qubit());
            use (inherited, added) = (Qubit(), Qubit());
            X(input);
            {inherited_preparation}
            {added_preparation}
            Controlled Adjoint Rec(
                [inherited], (2, MResetZ(input), [flag, target, added])
            );
            Reset(inherited);
            Reset(added);
            MResetZ(target)
        }}
    """
    check_result(source, profile, expected)


@pytest.mark.parametrize("profile", PROFILES, ids=str)
@pytest.mark.parametrize(
    "target_preparation",
    [
        pytest.param("", id="measurement-zero"),
        pytest.param("X(target);", id="measurement-one"),
    ],
)
def test_adjoint_specialization_selected_under_runtime_control_preserves_result(
    profile: qsharp.TargetProfile, target_preparation: str
) -> None:
    source = f"""
        operation Act(target : Qubit) : Unit is Adj {{
            body (...) {{ if M(target) == One {{ Adjoint Act(target); }} }}
            adjoint (...) {{ X(target); }}
        }}
        operation Main() : Result {{
            use target = Qubit();
            {target_preparation}
            Act(target);
            MResetZ(target)
        }}
    """
    check_result(source, profile, qsharp.Result.Zero)


@pytest.mark.parametrize("depth,expected", [(1, 0), (3, 1)])
def test_named_measurement_is_read_before_runtime_recursive_call(
    depth: int, expected: int
) -> None:
    source = f"""
        operation Retry(budget : Int, q : Qubit, target : Qubit) : Unit {{
            if budget > 0 {{
                let measured = MResetZ(q);
                if measured == Zero {{
                    X(q);
                    Retry(budget - 1, q, target);
                }} else {{ X(target); }}
            }}
        }}
        operation Main() : Int {{
            use (flag, q, target) = (Qubit(), Qubit(), Qubit());
            X(flag);
            let depth = if MResetZ(flag) == One {{ {depth} }} else {{ 0 }};
            Retry(depth, q, target);
            Reset(q);
            if MResetZ(target) == One {{ 1 }} else {{ 0 }}
        }}
    """
    check_result(source, qsharp.TargetProfile.Adaptive, expected)


@pytest.mark.parametrize("depth,expected", [(1, 1), (3, 0)])
def test_boolean_helper_consumes_named_result_before_recursive_reentry(
    depth: int, expected: int
) -> None:
    source = f"""
        operation Read(q : Qubit) : Bool {{
            let measured = MResetZ(q);
            measured == One
        }}
        operation Rec(n : Int, q : Qubit, target : Qubit) : Unit {{
            if n % 2 == 1 {{ X(q); }}
            let saved = Read(q);
            if n > 0 {{ Rec(n - 1, q, target); }}
            if saved {{ X(target); }}
        }}
        operation Main() : Int {{
            use (flag, q, target) = (Qubit(), Qubit(), Qubit());
            X(flag);
            let depth = if MResetZ(flag) == One {{ {depth} }} else {{ 0 }};
            Rec(depth, q, target);
            if MResetZ(target) == One {{ 1 }} else {{ 0 }}
        }}
    """
    check_result(source, qsharp.TargetProfile.Adaptive, expected)


@pytest.mark.parametrize("depth,expected", [(1, 1), (3, 0)])
def test_result_as_bool_consumes_result_before_runtime_recursive_call(
    depth: int, expected: int
) -> None:
    source = f"""
        operation Rec(n : Int, q : Qubit, target : Qubit) : Unit {{
            if n % 2 == 1 {{ X(q); }}
            let measured = MResetZ(q);
            let saved = Std.Convert.ResultAsBool(measured);
            if n > 0 {{ Rec(n - 1, q, target); }}
            if saved {{ X(target); }}
        }}
        operation Main() : Int {{
            use (flag, q, target) = (Qubit(), Qubit(), Qubit());
            X(flag);
            let depth = if MResetZ(flag) == One {{ {depth} }} else {{ 0 }};
            Rec(depth, q, target);
            if MResetZ(target) == One {{ 1 }} else {{ 0 }}
        }}
    """
    check_result(source, qsharp.TargetProfile.Adaptive, expected)


@pytest.mark.parametrize("depth,expected", [(1, 1), (3, 0)])
def test_branch_selected_result_is_consumed_before_recursive_reentry(
    depth: int, expected: int
) -> None:
    source = f"""
        operation Rec(n : Int, q : Qubit, target : Qubit) : Unit {{
            let measured = if n % 2 == 0 {{
                X(q);
                MResetZ(q)
            }} else {{ MResetZ(q) }};
            let alias = measured;
            let saved = alias == One;
            if n > 0 {{ Rec(n - 1, q, target); }}
            if saved {{ X(target); }}
        }}
        operation Main() : Int {{
            use (flag, q, target) = (Qubit(), Qubit(), Qubit());
            X(flag);
            let depth = if MResetZ(flag) == One {{ {depth} }} else {{ 0 }};
            Rec(depth, q, target);
            if MResetZ(target) == One {{ 1 }} else {{ 0 }}
        }}
    """
    check_result(source, qsharp.TargetProfile.Adaptive, expected)


@pytest.mark.parametrize("depth,expected", [(1, 1), (3, 0)])
def test_loop_local_named_results_are_consumed_before_next_iteration(
    depth: int, expected: int
) -> None:
    source = f"""
        operation Rec(n : Int, q : Qubit, target : Qubit) : Unit {{
            mutable count = n;
            while count > 0 {{
                if count % 2 == 1 {{ X(q); }}
                let measured = MResetZ(q);
                if measured == One {{ X(target); }}
                count -= 1;
            }}
            if n > 0 {{ Rec(n - 1, q, target); }}
        }}
        operation Main() : Int {{
            use (flag, q, target) = (Qubit(), Qubit(), Qubit());
            X(flag);
            let depth = if MResetZ(flag) == One {{ {depth} }} else {{ 0 }};
            Rec(depth, q, target);
            if MResetZ(target) == One {{ 1 }} else {{ 0 }}
        }}
    """
    check_result(source, qsharp.TargetProfile.Adaptive, expected)


@pytest.mark.parametrize("depth,expected", [(0, 0), (2, 1)])
def test_classical_tuple_swap_preserves_runtime_measuring_recursion(
    depth: int, expected: int
) -> None:
    source = f"""
        operation Rec(n : Int, q : Qubit, target : Qubit) : Unit {{
            let bit = MResetZ(q) == Zero;
            mutable (a, b) = (n, 0);
            (a, b) = (b, a);
            if bit and b == 1 {{ X(target); }}
            if n > 0 {{ Rec(n - 1, q, target); }}
        }}
        operation Main() : Int {{
            use (flag, q, target) = (Qubit(), Qubit(), Qubit());
            X(flag);
            let depth = if MResetZ(flag) == One {{ {depth} }} else {{ 0 }};
            Rec(depth, q, target);
            if MResetZ(target) == One {{ 1 }} else {{ 0 }}
        }}
    """
    check_result(source, qsharp.TargetProfile.Adaptive, expected)


@pytest.mark.parametrize("depth,expected", [(0, 0), (2, 1)])
def test_nested_tuple_swap_with_discard_in_recursive_helper(
    depth: int, expected: int
) -> None:
    source = f"""
        operation SwapAndFlip(n : Int, bit : Bool, target : Qubit) : Unit {{
            mutable (a, b) = (n, 0);
            (_, (a, b)) = (9, (b, a));
            if bit and b == 1 {{ X(target); }}
        }}
        operation Rec(n : Int, q : Qubit, target : Qubit) : Unit {{
            let bit = MResetZ(q) == Zero;
            SwapAndFlip(n, bit, target);
            if n > 0 {{ Rec(n - 1, q, target); }}
        }}
        operation Main() : Int {{
            use (flag, q, target) = (Qubit(), Qubit(), Qubit());
            X(flag);
            let depth = if MResetZ(flag) == One {{ {depth} }} else {{ 0 }};
            Rec(depth, q, target);
            if MResetZ(target) == One {{ 1 }} else {{ 0 }}
        }}
    """
    check_result(source, qsharp.TargetProfile.Adaptive, expected)


@pytest.mark.parametrize("depth,expected", [(0, 0), (2, 0), (3, 1)])
def test_empty_local_qubit_array_preserves_runtime_recursion(
    depth: int, expected: int
) -> None:
    source = f"""
        operation Rec(n : Int, flag : Qubit, target : Qubit) : Unit {{
            if n > 0 {{
                {{ use empty = Qubit[0]; }}
                X(target);
                if MResetZ(flag) == Zero {{ Rec(n - 1, flag, target); }}
            }}
        }}
        operation Main() : Int {{
            use (flag, target) = (Qubit(), Qubit());
            let depth = if MResetZ(flag) == Zero {{ {depth} }} else {{ 0 }};
            Rec(depth, flag, target);
            if MResetZ(target) == One {{ 1 }} else {{ 0 }}
        }}
    """
    check_result(source, qsharp.TargetProfile.Adaptive, expected)


@pytest.mark.parametrize("depth,expected", [(1, 1), (2, 0)])
def test_live_empty_qubit_array_preserves_runtime_recursion(
    depth: int, expected: int
) -> None:
    source = f"""
        operation Rec(n : Int, target : Qubit) : Unit {{
            use empty = Qubit[0];
            if n > 0 {{
                X(target);
                Rec(n - 1, target);
            }}
            if Length(empty) != 0 {{ X(target); }}
        }}
        operation Main() : Int {{
            use (flag, target) = (Qubit(), Qubit());
            Rec(if MResetZ(flag) == Zero {{ {depth} }} else {{ 0 }}, target);
            if MResetZ(target) == One {{ 1 }} else {{ 0 }}
        }}
    """
    check_result(source, qsharp.TargetProfile.Adaptive, expected)


@pytest.mark.parametrize("depth,expected", [(0, 1), (1, 0), (2, 1)])
def test_live_empty_qubit_array_preserves_loop_recursion(
    depth: int, expected: int
) -> None:
    source = f"""
        operation Rec(n : Int, target : Qubit) : Unit {{
            use empty = Qubit[0];
            mutable count = n;
            while count > 0 {{
                Rec(count - 1, target);
                count = 0;
            }}
            if Length(empty) == 0 {{ X(target); }}
        }}
        operation Main() : Int {{
            use (flag, target) = (Qubit(), Qubit());
            Rec(if MResetZ(flag) == Zero {{ {depth} }} else {{ 0 }}, target);
            if MResetZ(target) == One {{ 1 }} else {{ 0 }}
        }}
    """
    check_result(source, qsharp.TargetProfile.Adaptive, expected)


@pytest.mark.parametrize("depth,expected", [(1, 1), (2, 0)])
def test_conditional_local_qubit_is_released_before_recursive_call(
    depth: int, expected: int
) -> None:
    source = f"""
        operation Rec(n : Int, target : Qubit) : Unit {{
            if n > 0 {{
                {{
                    use local = Qubit();
                    X(local);
                    CNOT(local, target);
                    Reset(local);
                }}
                Rec(n - 1, target);
            }}
        }}
        operation Main() : Int {{
            use (flag, target) = (Qubit(), Qubit());
            X(flag);
            let depth = if MResetZ(flag) == One {{ {depth} }} else {{ 0 }};
            Rec(depth, target);
            if MResetZ(target) == One {{ 1 }} else {{ 0 }}
        }}
    """
    check_result(source, qsharp.TargetProfile.Adaptive, expected)


@pytest.mark.parametrize("depth,expected", [(2, 1), (3, 0)])
def test_loop_local_qubits_preserve_outer_qubit_across_iterations(
    depth: int, expected: int
) -> None:
    source = f"""
        operation Rec(n : Int, target : Qubit) : Unit {{
            {{
                use outer = Qubit();
                X(outer);
                mutable count = n;
                while count > 0 {{
                    use inner = Qubit();
                    CNOT(outer, inner);
                    CNOT(inner, target);
                    CNOT(outer, inner);
                    count -= 1;
                }}
                X(outer);
            }}
            if n > 0 {{ Rec(n - 1, target); }}
        }}
        operation Main() : Int {{
            use (flag, target) = (Qubit(), Qubit());
            X(flag);
            let depth = if MResetZ(flag) == One {{ {depth} }} else {{ 0 }};
            Rec(depth, target);
            if MResetZ(target) == One {{ 1 }} else {{ 0 }}
        }}
    """
    check_result(source, qsharp.TargetProfile.Adaptive, expected)


@pytest.mark.parametrize("depth,expected", [(2, 0), (3, 1)])
def test_reset_local_qubit_can_remain_in_scope_across_recursion(
    depth: int, expected: int
) -> None:
    source = f"""
        operation Rec(n : Int, target : Qubit) : Unit {{
            if n > 0 {{
                use local = Qubit();
                X(local);
                CNOT(local, target);
                Reset(local);
                Rec(n - 1, target);
            }}
        }}
        operation Main() : Int {{
            use (flag, target) = (Qubit(), Qubit());
            X(flag);
            Rec(if MResetZ(flag) == One {{ {depth} }} else {{ 0 }}, target);
            if MResetZ(target) == One {{ 1 }} else {{ 0 }}
        }}
    """
    check_result(source, qsharp.TargetProfile.Adaptive, expected)


@pytest.mark.parametrize("depth,expected", [(2, 1), (3, 0)])
def test_measure_reset_local_preserves_consumed_boolean_across_recursion(
    depth: int, expected: int
) -> None:
    source = f"""
        operation Rec(n : Int, target : Qubit) : Unit {{
            if n > 0 {{
                use local = Qubit();
                if n % 2 == 1 {{ X(local); }}
                let observed = MResetZ(local) == One;
                Rec(n - 1, target);
                if observed {{ X(target); }}
            }}
        }}
        operation Main() : Int {{
            use (flag, target) = (Qubit(), Qubit());
            X(flag);
            Rec(if MResetZ(flag) == One {{ {depth} }} else {{ 0 }}, target);
            if MResetZ(target) == One {{ 1 }} else {{ 0 }}
        }}
    """
    check_result(source, qsharp.TargetProfile.Adaptive, expected)


@pytest.mark.parametrize("depth,expected", [(2, 0), (3, 1)])
def test_local_qubit_reset_on_both_branches_can_remain_in_scope(
    depth: int, expected: int
) -> None:
    source = f"""
        operation Rec(n : Int, target : Qubit) : Unit {{
            if n > 0 {{
                use local = Qubit();
                X(local);
                CNOT(local, target);
                if n % 2 == 0 {{ Reset(local); }}
                else {{ let ignored = MResetZ(local); }}
                Rec(n - 1, target);
            }}
        }}
        operation Main() : Int {{
            use (flag, target) = (Qubit(), Qubit());
            X(flag);
            Rec(if MResetZ(flag) == One {{ {depth} }} else {{ 0 }}, target);
            if MResetZ(target) == One {{ 1 }} else {{ 0 }}
        }}
    """
    check_result(source, qsharp.TargetProfile.Adaptive, expected)


@pytest.mark.parametrize("depth,expected", [(2, 0), (3, 1)])
def test_mutually_recursive_reset_locals_keep_separate_reservations(
    depth: int, expected: int
) -> None:
    source = f"""
        operation First(n : Int, target : Qubit) : Unit {{
            if n > 0 {{
                use local = Qubit();
                X(local);
                CNOT(local, target);
                Reset(local);
                Second(n - 1, target);
            }}
        }}
        operation Second(n : Int, target : Qubit) : Unit {{
            if n > 0 {{
                use local = Qubit();
                X(local);
                CNOT(local, target);
                Reset(local);
                First(n - 1, target);
            }}
        }}
        operation Main() : Int {{
            use (flag, target) = (Qubit(), Qubit());
            X(flag);
            First(if MResetZ(flag) == One {{ {depth} }} else {{ 0 }}, target);
            if MResetZ(target) == One {{ 1 }} else {{ 0 }}
        }}
    """
    check_result(source, qsharp.TargetProfile.Adaptive, expected)


@pytest.mark.parametrize("depth,expected", [(1, 1), (2, 1), (3, 0)])
def test_inlined_allocation_extends_only_its_own_qubit_reservation(
    depth: int, expected: int
) -> None:
    source = f"""
        operation Helper(targets : Qubit[]) : Unit {{
            use local = Qubit();
            X(local);
            CNOT(local, targets[0]);
            Reset(local);
        }}
        operation Other(n : Int, target : Qubit) : Unit {{
            use local = Qubit();
            X(local);
            CNOT(local, target);
            Reset(local);
            if n > 0 {{ Other(n - 1, target); }}
        }}
        operation Rec(n : Int, target : Qubit) : Unit {{
            if n > 0 {{
                use local = Qubit();
                Reset(local);
                Other(n, target);
                Helper([target]);
                Rec(n - 1, target);
            }}
        }}
        operation Main() : Int {{
            use (flag, target) = (Qubit(), Qubit());
            X(flag);
            Rec(if MResetZ(flag) == One {{ {depth} }} else {{ 0 }}, target);
            if MResetZ(target) == One {{ 1 }} else {{ 0 }}
        }}
    """
    check_result(source, qsharp.TargetProfile.Adaptive, expected)


@pytest.mark.parametrize(
    "budget,preparation,expected",
    [
        pytest.param(0, "", 0, id="zero-budget-zero-state"),
        pytest.param(0, "X(state);", 2, id="zero-budget-one-state"),
        pytest.param(1, "", 3, id="first-attempt-success"),
        pytest.param(1, "X(state);", 0, id="budget-exhausted"),
        pytest.param(2, "X(state);", 3, id="last-attempt-success"),
        pytest.param(40, "", 3, id="large-budget-first-attempt-success"),
        pytest.param(40, "X(state);", 3, id="large-budget-second-attempt-success"),
    ],
)
def test_allocating_retry_preserves_success_and_known_budget_exhaustion(
    budget: int, preparation: str, expected: int
) -> None:
    source = f"""
        operation Retry(budget : Int, state : Qubit, flag : Qubit, output : Qubit) : Unit {{
            if budget > 0 {{
                {{
                    use local = Qubit();
                    X(local); CNOT(local, state); X(local);
                }}
                CNOT(state, flag);
                if MResetZ(flag) == Zero {{ Retry(budget - 1, state, flag, output); }}
                else {{ X(output); }}
            }}
        }}
        operation Main() : Int {{
            use (state, flag, output) = (Qubit(), Qubit(), Qubit());
            {preparation}
            Retry({budget}, state, flag, output);
            let success = if MResetZ(output) == One {{ 1 }} else {{ 0 }};
            let finalState = if MResetZ(state) == One {{ 2 }} else {{ 0 }};
            success + finalState
        }}
    """
    check_result(source, qsharp.TargetProfile.Adaptive, expected)


if __name__ == "__main__":
    case = json.load(sys.stdin)
    expected = case["expected"]
    if isinstance(expected, str):
        expected = {"Zero": qsharp.Result.Zero, "One": qsharp.Result.One}[expected]
    _check_result_in_process(
        case["source"],
        getattr(qsharp.TargetProfile, case["profile"]),
        expected,
        entry_expr=case["entry_expr"],
        shots=case["shots"],
    )
