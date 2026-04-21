# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pathlib import Path
from collections import Counter
from typing import Sequence, cast
import pyqir
import pytest
import math

import qsharp
from qsharp import openqasm, QSharpError
from qsharp._simulation import run_qir_clifford, NoiseConfig
from qsharp._device._atom import NeutralAtomDevice
from qsharp._device._atom._decomp import DecomposeRzAnglesToCliffordGates
from qsharp._device._atom._validate import ValidateNoConditionalBranches
from qsharp import TargetProfile, Result

current_file_path = Path(__file__)
# Get the directory of the current file
current_dir = current_file_path.parent

# Tests for the Q# noisy simulator.


def transform_to_clifford(input) -> str:
    native_qir = NeutralAtomDevice().compile(input)
    module = pyqir.Module.from_ir(pyqir.Context(), str(native_qir))
    ValidateNoConditionalBranches().run(module)
    DecomposeRzAnglesToCliffordGates().run(module)
    return str(module)


def read_file(file_name: str) -> str:
    return Path(file_name).read_text(encoding="utf-8")


def read_file_relative(file_name: str) -> str:
    return Path(current_dir / file_name).read_text(encoding="utf-8")


def result_array_to_string(results: Sequence[Result]) -> str:
    chars = []
    for value in results:
        if value == Result.Zero:
            chars.append("0")
        elif value == Result.One:
            chars.append("1")
        else:
            chars.append("-")
    return "".join(chars)


def test_smoke():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    input = qsharp.compile(
        "IsingModel2DEvolution(5, 5, PI() / 2.0, PI() / 2.0, 5.0, 5)"
    )
    input = transform_to_clifford(input)
    output = run_qir_clifford(input, 10, NoiseConfig())
    print(output)


def test_1224_clifford_ising():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    input = qsharp.compile(
        "IsingModel2DEvolution(20, 50, PI() / 2.0, PI() / 2.0, 5.0, 5)"
    )
    qir = transform_to_clifford(input)

    output = run_qir_clifford(qir, 1, NoiseConfig())

    print(output)


def test_million():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordCalls.qs"))

    ir = qsharp.compile("Main()")
    output = run_qir_clifford(str(ir), 1, NoiseConfig())
    print(output)


def test_s_noise_inherits_from_rz():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval("operation Main() : Result { use q = Qubit(); S(q); MResetZ(q) }")
    ir = qsharp.compile("Main()")
    noise = NoiseConfig()
    noise.rz.x = 1.0
    output = run_qir_clifford(str(ir), 1, noise)
    assert output == [Result.One]


def test_z_noise_inherits_from_rz():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval("operation Main() : Result { use q = Qubit(); Z(q); MResetZ(q) }")
    ir = qsharp.compile("Main()")
    noise = NoiseConfig()
    noise.rz.x = 1.0
    output = run_qir_clifford(str(ir), 1, noise)
    assert output == [Result.One]


def test_s_adj_noise_inherits_from_rz():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(
        "operation Main() : Result { use q = Qubit(); Adjoint S(q); MResetZ(q) }"
    )
    ir = qsharp.compile("Main()")
    noise = NoiseConfig()
    noise.rz.x = 1.0
    output = run_qir_clifford(str(ir), 1, noise)
    assert output == [Result.One]


def test_program_with_branching_fails():
    qsharp.init(target_profile=TargetProfile.Adaptive_RI)
    qsharp.eval(
        """
        operation Main() : Result {
            use q = Qubit();
            H(q);
            if (MResetZ(q) == One) {
                X(q);
            }
            return MResetZ(q);
        }
        """
    )
    ir = qsharp.compile("Main()")
    try:
        run_qir_clifford(str(ir), 1, NoiseConfig())
        assert False, "Expected ValueError for branching control flow"
    except ValueError as e:
        assert (
            "simulation of programs with branching control flow is not supported"
            in str(e)
        )


def test_program_with_unconditional_branching_succeeds():
    qir = """
%Result = type opaque
%Qubit = type opaque

@empty_tag = internal constant [1 x i8] c"\\00"
@0 = internal constant [6 x i8] c"0_a0r\\00"
@1 = internal constant [6 x i8] c"1_a1r\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  br label %block_1
block_1:
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
  br label %block_2
block_2:
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  br label %block_3
block_3:
  call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

declare void @__quantum__rt__array_record_output(i64, i8*)

declare void @__quantum__rt__result_record_output(%Result*, i8*)

declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="2" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
"""

    output = run_qir_clifford(qir, 1, NoiseConfig())
    assert output == [[Result.Zero, Result.Zero]] or output == [
        [Result.One, Result.One]
    ]


def test_cy_direct_qir():
    qir = """
%Result = type opaque
%Qubit = type opaque

@empty_tag = internal constant [1 x i8] c"\\00"
@0 = internal constant [6 x i8] c"0_a0r\\00"
@1 = internal constant [6 x i8] c"1_a1r\\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
    call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
    call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
    call void @__quantum__qis__s__body(%Qubit* inttoptr (i64 1 to %Qubit*))
    call void @__quantum__qis__cy__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
    call void @__quantum__qis__s__adj(%Qubit* inttoptr (i64 1 to %Qubit*))
    call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__s__body(%Qubit*)

declare void @__quantum__qis__s__adj(%Qubit*)

declare void @__quantum__qis__cy__body(%Qubit*, %Qubit*)

declare void @__quantum__rt__array_record_output(i64, i8*)

declare void @__quantum__rt__result_record_output(%Result*, i8*)

declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="2" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
"""

    # Do not go through Neutral Atom device compilation since we want to test CY.
    output = run_qir_clifford(qir, 50, NoiseConfig())
    # This test should deterministically produce Zero.
    # If CZ or CX is executed instead of CY, then some measurements will produce One.
    assert all(shot[1] == Result.Zero for shot in output)


def test_clifford_run_no_noise():
    """Simple test that Clifford simulator works without noise."""
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    output = qsharp.run(
        "IsingModel2DEvolution(4, 4, PI() / 2.0, PI() / 2.0, 10.0, 10)",
        1,
        type="clifford",
    )
    print(output)
    # Expecting deterministic output, no randomization seed needed.
    assert output == [[Result.Zero] * 16], "Expected result of 0s with pi/2 angles."

    # Same execution should work with the operation itself.
    output = qsharp.run(
        qsharp.code.IsingModel2DEvolution,
        1,
        4,
        4,
        math.pi / 2,
        math.pi / 2,
        10.0,
        10,
        type="clifford",
    )
    print(output)
    assert output == [[Result.Zero] * 16], "Expected result of 0s with pi/2 angles."


def test_clifford_run_bitflip_noise():
    """Bitflip noise for Clifford simulator."""
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    p_noise = 0.005
    noise = NoiseConfig()
    noise.rx.set_bitflip(p_noise)
    noise.rzz.set_pauli_noise("XX", p_noise)
    noise.mresetz.set_bitflip(p_noise)

    output = qsharp.run(
        "IsingModel2DEvolution(4, 4, PI() / 2.0, PI() / 2.0, 10.0, 10)",
        shots=1,
        noise=noise,
        seed=17,
        type="clifford",
    )
    result = [result_array_to_string(cast(Sequence[Result], x)) for x in output]
    print(result)
    # Reasonable results obtained from manual run
    assert result == ["0000001100000000"]

    # Same execution should work with the operation itself.
    output = qsharp.run(
        qsharp.code.IsingModel2DEvolution,
        1,
        4,
        4,
        math.pi / 2,
        math.pi / 2,
        10.0,
        10,
        noise=noise,
        seed=17,
        type="clifford",
    )
    result = [result_array_to_string(cast(Sequence[Result], x)) for x in output]
    print(result)
    assert result == ["0000001100000000"]


def test_clifford_run_mixed_noise():
    qsharp.init(target_profile=TargetProfile.Base)
    qsharp.eval(read_file_relative("CliffordIsing.qs"))

    noise = NoiseConfig()
    noise.rz.set_bitflip(0.008)
    noise.rz.loss = 0.005
    noise.rzz.set_depolarizing(0.008)
    noise.rzz.loss = 0.005

    output = qsharp.run(
        "IsingModel2DEvolution(4, 4, PI() / 2.0, PI() / 2.0, 4.0, 4)",
        shots=1,
        noise=noise,
        seed=234,
        type="clifford",
    )
    result = [result_array_to_string(cast(Sequence[Result], x)) for x in output]
    print(result)
    # Reasonable results obtained from manual run
    assert result == ["-000-01000100010"]


def test_clifford_run_isolated_loss():
    qsharp.init(target_profile=TargetProfile.Base)
    program = """
import Std.Math.PI;
operation Main() : Result[] {
    use qs = Qubit[3];
    X(qs[0]);
    X(qs[1]);
    CNOT(qs[0], qs[1]);
    // When loss is configured for X gate, qubit 2 should be unaffected.
    Rx(PI() / 2.0, qs[2]);
    Rx(PI() / 2.0, qs[2]);
    MeasureEachZ(qs)
}
    """
    qsharp.eval(program)

    noise = NoiseConfig()
    noise.x.loss = 0.1

    output = qsharp.run("Main()", shots=1000, noise=noise, type="clifford")
    result = [result_array_to_string(cast(Sequence[Result], x)) for x in output]
    histogram = Counter(result)
    total = sum(histogram.values())
    allowed_percent = {
        "101": 0.81,
        "1-1": 0.09,
        "-11": 0.09,
        "--1": 0.01,
    }
    tolerance = 0.2 * total
    for bitstring, actual_count in histogram.items():
        assert (
            bitstring in allowed_percent
        ), f"Unexpected measurement string: '{bitstring}'."
        expected_count = allowed_percent[bitstring] * total
        assert abs(actual_count - expected_count) <= tolerance, (
            f"Count for {bitstring} outside 20% tolerance. "
            f"Actual={actual_count}, Expected≈{expected_count:.0f}, Shots={total}."
        )
    # We don't check for missing strings, as low-probability strings may not appear in finite shots.


def test_clifford_run_isolated_loss_and_noise():
    qsharp.init(target_profile=TargetProfile.Base)
    program = """
import Std.Math.PI;
operation Main() : Result[] {
    use qs = Qubit[5];
    for _ in 1..100 {
        X(qs[0]);
        X(qs[1]);
        CNOT(qs[0], qs[1]);
    }
    Rx(PI() / 2.0, qs[4]);
    Rx(PI() / 2.0, qs[4]);
    MeasureEachZ(qs)
}
    """
    qsharp.eval(program)

    noise = NoiseConfig()
    noise.x.set_bitflip(0.001)
    noise.x.loss = 0.001

    output = qsharp.run("Main()", shots=1000, noise=noise, type="clifford")
    result = [result_array_to_string(cast(Sequence[Result], x)) for x in output]
    histogram = Counter(result)
    total = sum(histogram.values())
    assert total > 0, "No measurement results recorded."
    for bitstring in histogram:
        assert bitstring.endswith("001"), f"Unexpected suffix in '{bitstring}'."
    probability_00001 = histogram.get("00001", 0) / total
    assert 0.5 < probability_00001 < 0.8, (
        f"Probability of 00001 outside expected range. "
        f"Actual={probability_00001:.2%}, Shots={total}."
    )


def build_x_chain_qasm(n_instances: int, n_x: int) -> str:
    # Construct multiple instances of x gate chains
    prefix = f"""
        OPENQASM 3.0;
        include "stdgates.inc";
        bit[{n_instances}] c;
        qubit[{n_instances}] q;
    """

    infix = """
        x q;
    """

    suffix = """
        c = measure q;
    """

    src_parallel = prefix + infix * n_x + suffix

    return src_parallel


def build_cy_noise_qasm(n_cy: int) -> str:
    src = """
        OPENQASM 3.0;
        include "stdgates.inc";
        bit[2] c;
        qubit[2] q;
        x q[0];
        h q[1];
        """
    src += "cy q[0], q[1];\n" * n_cy
    src += """
        h q[1];
        c = measure q;
        """

    return src


@pytest.mark.parametrize(
    "p_noise, n_x, n_instances, n_shots, max_percent",
    [
        (0.001, 200, 6, 500, 5.0),
        (0.01, 200, 6, 500, 5.0),
        (0.001, 50, 12, 200, 5.0),
    ],
)
def test_clifford_run_x_chain(
    p_noise: float, n_x: int, n_instances: int, n_shots: int, max_percent: float
):
    """
    Simulate multi-instance X-chain with bitflip noise many times
    Compare result frequencies with analytically computed probabilities
    """
    # Use the Clifford simulator with noise
    qsharp.init()
    noise = NoiseConfig()
    noise.x.set_bitflip(p_noise)

    qasm = build_x_chain_qasm(n_instances, n_x)
    output = openqasm.run(qasm, shots=n_shots, noise=noise, seed=42, type="clifford")
    histogram = [0 for _ in range(n_instances + 1)]
    for shot in output:
        shot_results = cast(Sequence[Result], shot)
        count_1 = shot_results.count(Result.One)
        histogram[count_1] += 1

    # Probability of obtaining 0 and 1 at the end of the X chain.
    p_0 = ((2.0 * p_noise - 1.0) ** n_x + 1.0) / 2.0
    p_1 = 1.0 - p_0

    # Number of results with k ones that should be there.
    p_N = [
        p_0 ** ((n_instances - k)) * (p_1**k) * math.comb(n_instances, k) * n_shots
        for k in range(n_instances + 1)
    ]

    # Error % for deviation from analytical value
    error_percent = [abs(a - b) * 100.0 / n_shots for (a, b) in zip(histogram, p_N)]
    print(", ".join(f"{a} (Δ≈{b:.1f}%)" for (a, b) in zip(histogram, error_percent)))

    # We tolerate configured percentage error.
    assert all(
        err < max_percent for err in error_percent
    ), f"Error percent too high: {error_percent}"


def test_clifford_run_cy_noise_distribution():
    """
    Apply CY with per-gate Z noise and validate the expected odd-parity flip rate.
    """
    n_cy = 10
    p_z = 0.01
    n_shots = 1000
    expected_p1 = (1.0 - (1.0 - 2.0 * p_z) ** n_cy) / 2.0

    qsharp.init()
    noise = NoiseConfig()
    noise.cy.set_pauli_noise("IZ", p_z)

    qasm = build_cy_noise_qasm(n_cy)
    output = openqasm.run(qasm, shots=n_shots, noise=noise, seed=77, type="clifford")

    count_target_one = 0
    for shot in output:
        shot_results = cast(Sequence[Result], shot)
        if shot_results[1] == Result.One:
            count_target_one += 1

    actual_p1 = count_target_one / n_shots
    tolerance = 0.05
    print(
        f"CY noise rate outside tolerance. Expected≈{expected_p1:.3f}, "
        f"actual={actual_p1:.3f}, tol={tolerance:.3f}"
    )
    assert abs(actual_p1 - expected_p1) <= tolerance, "CY noise rate outside tolerance."


def test_clifford_run_with_t_fails():
    qsharp.init()
    qsharp.eval(
        """
        operation Main() : Result {
            use q = Qubit();
            T(q);
            return MResetZ(q);
        }
        """
    )
    try:
        qsharp.run("Main()", shots=1, type="clifford")
        assert False, "Expected QSharpError for non-Clifford gate"
    except QSharpError as e:
        assert "T gate is not supported in Clifford simulation" in str(e)


def test_clifford_run_with_adjoint_t_fails():
    qsharp.init()
    qsharp.eval(
        """
        operation Main() : Result {
            use q = Qubit();
            Adjoint T(q);
            return MResetZ(q);
        }
        """
    )
    try:
        qsharp.run("Main()", shots=1, type="clifford")
        assert False, "Expected QSharpError for non-Clifford gate"
    except QSharpError as e:
        assert "adjoint T gate is not supported in Clifford simulation" in str(e)


def test_clifford_run_with_non_clifford_rotation_fails():
    qsharp.init()
    qsharp.eval(
        """
        operation Main() : Result {
            use q = Qubit();
            Rx(1.0, q);
            return MResetZ(q);
        }
        """
    )
    try:
        qsharp.run("Main()", shots=1, type="clifford")
        assert False, "Expected QSharpError for non-Clifford gate"
    except QSharpError as e:
        assert "angle must be a multiple of PI/2 in Clifford simulation" in str(e)


def test_clifford_run_with_too_many_qubits_fails():
    qsharp.init()
    qsharp.eval(
        """
        operation Main() : Unit {
            use qs = Qubit[10];
        }
        """
    )
    try:
        qsharp.run("Main()", shots=1, type="clifford", num_qubits=5)
        assert False, "Expected QSharpError for too many qubits"
    except QSharpError as e:
        assert "qubit limit exceeded" in str(e)
