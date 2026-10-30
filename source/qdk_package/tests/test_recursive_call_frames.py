# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import os
from typing import Literal

import pyqir
import pytest
from qdk import qsharp
from qdk.simulation import run_qir

Backend = Literal["cpu", "stabilizer", "clifford", "gpu", "qodec"]


@pytest.fixture(params=["cpu", "stabilizer", "clifford", "gpu", "qodec"])
def backend(request: pytest.FixtureRequest) -> Backend:
    name = request.param
    if name == "gpu":
        if not os.environ.get("QDK_GPU_TESTS"):
            pytest.skip("GPU tests require QDK_GPU_TESTS")
        from qdk._native import try_create_gpu_adapter

        try:
            try_create_gpu_adapter()
        except OSError as error:
            pytest.skip(str(error))
    if name == "qodec":
        pytest.importorskip("qodec")
    return name


def run_one_shot(qir: str, backend: Backend) -> qsharp.Result | int | float:
    if backend != "qodec":
        return run_qir(qir, shots=1, type=backend)[0]
    from qdk._adaptive_pass import AdaptiveProfilePass, Bytecode
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime
    from qdk.simulation._qodec.execution_pipeline import ExecutionPipeline
    from qdk.simulation._qodec.quantum_backend import full_state_backend

    program = AdaptiveProfilePass(Bytecode.Bit64).run(
        pyqir.Module.from_ir(pyqir.Context(), qir)
    )
    pipeline = ExecutionPipeline(AdaptiveRuntime(), [], full_state_backend(None, 42))
    (value,) = pipeline.run(program)
    return value


def classical_qir(body: str, functions: str, output_type: str = "i64") -> str:
    # The GPU needs a nonempty quantum-op pool even for classical-only execution.
    record = "int" if output_type == "i64" else "double"
    return f"""
        @constants = constant [2 x i64] [i64 7, i64 11]
        define i64 @ENTRYPOINT__main() #0 {{
        entry:
            {body}
            call void @__quantum__rt__{record}_record_output({output_type} %answer, ptr null)
            ret i64 0
        }}
        {functions}
        define void @unused_quantum_op() {{
            call void @__quantum__qis__x__body(ptr null)
            ret void
        }}
        declare void @__quantum__qis__x__body(ptr)
        declare void @__quantum__rt__{record}_record_output({output_type}, ptr)
        attributes #0 = {{ "entry_point" "qir_profiles"="adaptive_profile"
            "required_num_qubits"="1" "required_num_results"="1" }}
    """


def test_recursive_calls_preserve_parent_registers_and_return_values(
    backend: Backend,
) -> None:
    qir = classical_qir(
        "%answer = call i64 @sum(i64 3)",
        """
        define i64 @sum(i64 %n) {
        entry:
            %done = icmp eq i64 %n, 0
            br i1 %done, label %base, label %recurse
        base:
            ret i64 0
        recurse:
            %next = sub i64 %n, 1
            %child = call i64 @sum(i64 %next)
            %result = add i64 %child, %n
            ret i64 %result
        }
        """,
    )
    assert run_one_shot(qir, backend) == 6


def test_recursive_argument_swap_reads_all_arguments_before_binding(
    backend: Backend,
) -> None:
    qir = classical_qir(
        "%answer = call i64 @swap(i64 1, i64 2, i64 1)",
        """
        define i64 @swap(i64 %a, i64 %b, i64 %n) {
        entry:
            %done = icmp eq i64 %n, 0
            br i1 %done, label %base, label %recurse
        base:
            %tens = mul i64 %a, 10
            %result = add i64 %tens, %b
            ret i64 %result
        recurse:
            %next = sub i64 %n, 1
            %child = call i64 @swap(i64 %b, i64 %a, i64 %next)
            ret i64 %child
        }
        """,
    )
    assert run_one_shot(qir, backend) == 21


def test_recursive_allocations_are_private_but_pointer_writes_are_shared(
    backend: Backend,
) -> None:
    qir = classical_qir(
        """
        %counter = alloca i64
        store i64 0, ptr %counter
        %first = call i64 @sum(i64 3, ptr %counter)
        %second = call i64 @sum(i64 3, ptr %counter)
        %count = load i64, ptr %counter
        %total = add i64 %first, %second
        %scaled = mul i64 %total, 10
        %with_count = add i64 %scaled, %count
        %global = getelementptr [2 x i64], ptr @constants, i64 0, i64 1
        %constant = load i64, ptr %global
        %answer = add i64 %with_count, %constant
        """,
        """
        define i64 @sum(i64 %n, ptr %counter) {
        entry:
            %local = alloca i64
            store i64 %n, ptr %local
            %count = load i64, ptr %counter
            %incremented = add i64 %count, 1
            store i64 %incremented, ptr %counter
            %done = icmp eq i64 %n, 0
            br i1 %done, label %base, label %recurse
        base:
            ret i64 0
        recurse:
            %next = sub i64 %n, 1
            %child = call i64 @sum(i64 %next, ptr %counter)
            %saved = load i64, ptr %local
            %result = add i64 %child, %saved
            ret i64 %result
        }
        """,
    )
    assert run_one_shot(qir, backend) == 139


def test_mutual_recursion_preserves_each_callers_value(backend: Backend) -> None:
    qir = classical_qir(
        "%answer = call i64 @a(i64 3)",
        """
        define i64 @a(i64 %n) {
        entry:
            %done = icmp eq i64 %n, 0
            br i1 %done, label %base, label %recurse
        base:
            ret i64 0
        recurse:
            %next = sub i64 %n, 1
            %child = call i64 @b(i64 %next)
            %result = add i64 %child, %n
            ret i64 %result
        }
        define i64 @b(i64 %n) {
        entry:
            %child = call i64 @a(i64 %n)
            ret i64 %child
        }
        """,
    )
    assert run_one_shot(qir, backend) == 6


def test_recursive_float_return_is_read_before_restoring_caller(
    backend: Backend,
) -> None:
    qir = classical_qir(
        "%answer = call double @sum(i64 3, double 1.25)",
        """
        define double @sum(i64 %n, double %increment) {
        entry:
            %done = icmp eq i64 %n, 0
            br i1 %done, label %base, label %recurse
        base:
            ret double 1.5
        recurse:
            %next = sub i64 %n, 1
            %child = call double @sum(i64 %next, double %increment)
            %result = fadd double %child, %increment
            ret double %result
        }
        """,
        "double",
    )
    assert run_one_shot(qir, backend) == 5.25


def test_repeated_calls_reclaim_local_memory_and_preserve_loop_state(
    backend: Backend,
) -> None:
    qir = classical_qir(
        """
        br label %loop
        loop:
            %n = phi i64 [0, %entry], [%next, %loop]
            %total = phi i64 [0, %entry], [%sum, %loop]
            %value = call i64 @local(i64 %n)
            %sum = add i64 %total, %value
            %next = add i64 %n, 1
            %again = icmp slt i64 %next, 20
            br i1 %again, label %loop, label %end
        end:
            %answer = add i64 %sum, 0
        """,
        """
        define i64 @local(i64 %n) {
        entry:
            %buffer = alloca [128 x i64]
            store i64 %n, ptr %buffer
            %value = load i64, ptr %buffer
            ret i64 %value
        }
        """,
    )
    assert run_one_shot(qir, backend) == 190


def test_loop_allocations_at_one_site_have_distinct_addresses(backend: Backend) -> None:
    qir = classical_qir(
        """
        br label %loop
        loop:
            %n = phi i64 [0, %entry], [1, %again]
            %previous = phi ptr [null, %entry], [%local, %again]
            %local = alloca i64
            store i64 %n, ptr %local
            %first = icmp eq i64 %n, 0
            br i1 %first, label %again, label %end
        again:
            br label %loop
        end:
            %answer = load i64, ptr %previous
        """,
        "",
    )
    assert run_one_shot(qir, backend) == 0


@pytest.mark.parametrize("backend", ["gpu"], indirect=True)
@pytest.mark.parametrize("depth,expected_code", [(7, 0), (8, 3)])
def test_gpu_recursive_depth_limit_remains_explicit(
    backend: Backend, depth: int, expected_code: int
) -> None:
    from qdk.simulation._simulation import GpuSimulator

    qir = classical_qir(
        f"%answer = call i64 @recurse(i64 {depth})",
        """
        define i64 @recurse(i64 %n) {
        entry:
            %done = icmp eq i64 %n, 0
            br i1 %done, label %base, label %recurse
        base:
            ret i64 42
        recurse:
            %next = sub i64 %n, 1
            %child = call i64 @recurse(i64 %next)
            ret i64 %child
        }
        """,
    )
    simulator = GpuSimulator()
    simulator.set_program(qir)
    results = simulator.run_shots(2)
    assert results["shot_result_codes"] == [expected_code] * 2
    if expected_code == 0:
        assert results["shot_results"] == [42, 42]


@pytest.mark.parametrize("backend", ["gpu"], indirect=True)
@pytest.mark.parametrize("words_per_call,expected_code", [(128, 0), (129, 6)])
def test_gpu_recursive_allocation_limit_excludes_saved_registers(
    backend: Backend, words_per_call: int, expected_code: int
) -> None:
    """Saved registers must neither consume nor enlarge the 1024-word budget."""
    from qdk.simulation._simulation import GpuSimulator

    # Depths 7 through 0 keep eight allocations live at once.
    qir = classical_qir(
        "%answer = call i64 @sum(i64 7)",
        f"""
        define i64 @sum(i64 %n) {{
        entry:
            %local = alloca [{words_per_call} x i64]
            %last = getelementptr [{words_per_call} x i64], ptr %local,
                i64 0, i64 {words_per_call - 1}
            store i64 %n, ptr %last
            %done = icmp eq i64 %n, 0
            br i1 %done, label %base, label %recurse
        base:
            ret i64 0
        recurse:
            %next = sub i64 %n, 1
            %child = call i64 @sum(i64 %next)
            %saved = load i64, ptr %last
            %result = add i64 %child, %saved
            ret i64 %result
        }}
        """,
    )
    simulator = GpuSimulator()
    simulator.set_program(qir)
    results = simulator.run_shots(2)
    assert results["shot_result_codes"] == [expected_code] * 2
    if expected_code == 0:
        assert results["shot_results"] == [28, 28]


def test_compiled_recursion_preserves_different_saved_measurements(
    backend: Backend,
) -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive)
    qsharp.eval("""
        operation Rec(depth : Int, target : Qubit) : Unit {
            let saved = {
                use local = Qubit();
                if depth % 2 == 1 { X(local); }
                MResetZ(local) == One
            };
            if depth > 0 { Rec(depth - 1, target); }
            if saved { X(target); }
        }
        operation Main() : Result {
            use (flag, target) = (Qubit(), Qubit()); X(flag);
            let depth = if MResetZ(flag) == One { 1 } else { 0 };
            Rec(depth, target);
            MResetZ(target)
        }
    """)
    assert qsharp.eval("Main()") == qsharp.Result.One
    assert run_one_shot(str(qsharp.compile("Main()")), backend) == qsharp.Result.One


def compile_retry_to_depth(depth: int) -> str:
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive)
    qsharp.eval(f"""
        operation Rec(n : Int, flag : Qubit, target : Qubit) : Unit {{
            if n > 0 {{
                {{
                    use local = Qubit();
                    X(local); CNOT(local, target); X(local);
                }}
                if MResetZ(flag) == Zero {{ Rec(n - 1, flag, target); }}
            }}
        }}
        operation Main() : Int {{
            use (flag, target) = (Qubit(), Qubit());
            Rec({depth}, flag, target);
            if MResetZ(target) == One {{ 1 }} else {{ 0 }}
        }}
    """)
    return str(qsharp.compile("Main()"))


@pytest.mark.parametrize("backend", ["gpu"], indirect=True)
@pytest.mark.parametrize("simulator_type", ["gpu", None])
@pytest.mark.parametrize("depth,expected", [(7, 1), (8, 0)])
def test_compiled_recursion_within_gpu_call_depth_returns_correct_results(
    backend: Backend,
    simulator_type: Literal["gpu"] | None,
    depth: int,
    expected: int,
) -> None:
    qir = compile_retry_to_depth(depth)
    assert qsharp.eval("Main()") == expected
    assert run_one_shot(qir, backend) == expected
    assert run_qir(qir, shots=2, type=simulator_type) == [expected] * 2


@pytest.mark.parametrize("backend", ["gpu"], indirect=True)
@pytest.mark.parametrize("simulator_type", ["gpu", None])
@pytest.mark.parametrize("depth,expected", [(9, 1), (10, 0)])
def test_compiled_recursion_exceeding_gpu_call_depth_raises(
    backend: Backend,
    simulator_type: Literal["gpu"] | None,
    depth: int,
    expected: int,
) -> None:
    from qdk.simulation._simulation import GpuSimulator

    qir = compile_retry_to_depth(depth)
    assert qsharp.eval("Main()") == expected
    assert run_one_shot(qir, "cpu") == expected
    assert backend == "gpu"
    simulator = GpuSimulator()
    simulator.set_program(qir)
    raw = simulator.run_shots(2)
    assert raw["shot_result_codes"] == [3, 3]
    assert raw["shot_results"] == [None, None]
    with pytest.raises(
        RuntimeError, match=r"GPU shot 0 failed: call stack overflow \(error code 3\)"
    ):
        run_qir(qir, shots=2, type=simulator_type)


@pytest.mark.parametrize("backend", ["gpu"], indirect=True)
@pytest.mark.parametrize("simulator_type", ["gpu", None])
def test_gpu_allocation_failure_raises_in_public_api(
    backend: Backend, simulator_type: Literal["gpu"] | None
) -> None:
    assert backend == "gpu"
    qir = classical_qir(
        "%buffer = alloca [1025 x i64]\n%answer = add i64 0, 42",
        "",
    )
    with pytest.raises(
        RuntimeError, match=r"allocation out of bounds \(error code 6\)"
    ):
        run_qir(qir, shots=2, type=simulator_type)


@pytest.mark.parametrize("backend", ["gpu"], indirect=True)
@pytest.mark.parametrize("simulator_type", ["gpu", None])
def test_gpu_mixed_shot_failures_raise_without_partial_results(
    backend: Backend, simulator_type: Literal["gpu"] | None
) -> None:
    from qdk.simulation._simulation import GpuSimulator

    assert backend == "gpu"
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive)
    qsharp.eval("""
        operation Rec(n : Int, target : Qubit) : Unit {
            if n > 0 { Rec(n - 1, target); }
            X(target);
        }
        operation Main() : Int {
            use (flag, target) = (Qubit(), Qubit());
            H(flag);
            if MResetZ(flag) == One { Rec(9, target); }
            else { X(target); }
            if MResetZ(target) == One { 1 } else { 0 }
        }
    """)
    qir = str(qsharp.compile("Main()"))
    simulator = GpuSimulator()
    simulator.set_program(qir)
    codes = simulator.run_shots(16, seed=42)["shot_result_codes"]
    assert set(codes) == {0, 3}
    assert codes[0] == 0
    first_failure = codes.index(3)
    with pytest.raises(
        RuntimeError,
        match=rf"GPU shot {first_failure} failed: call stack overflow \(error code 3\)",
    ):
        run_qir(qir, shots=16, seed=42, type=simulator_type)


@pytest.mark.parametrize("backend", ["gpu"], indirect=True)
@pytest.mark.parametrize(
    "preparation,expected", [("", qsharp.Result.Zero), ("X(q);", qsharp.Result.One)]
)
def test_base_gpu_execution_preserves_successful_results(
    backend: Backend, preparation: str, expected: qsharp.Result
) -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval(f"""
        operation Main() : Result {{
            use q = Qubit();
            {preparation}
            MResetZ(q)
        }}
    """)
    assert qsharp.eval("Main()") == expected
    qir = str(qsharp.compile("Main()"))
    assert run_one_shot(qir, backend) == expected
    assert run_qir(qir, shots=2, type="gpu") == [expected] * 2
