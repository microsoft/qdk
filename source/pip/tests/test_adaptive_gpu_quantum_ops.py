# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""End-to-end tests for the adaptive GPU bytecode interpreter pipeline.

Tests run Adaptive Profile QIR through the full pipeline:
Python AdaptiveProfilePass → Rust receiver → GPU interpreter → results.

Requires QDK_GPU_TESTS env var and a GPU adapter.

For smaller tests covering the full Adaptive Profile instruction set,
see `test_adaptive_gpu_bytecode.py`.
"""

import os
import sys
from collections import Counter

import pytest

# Skip all tests in this module if QDK_GPU_TESTS is not set
if not os.environ.get("QDK_GPU_TESTS"):
    pytest.skip("Skipping GPU tests (QDK_GPU_TESTS not set)", allow_module_level=True)

SKIP_REASON = "GPU is not available"
GPU_AVAILABLE = False

try:
    from qsharp._native import try_create_gpu_adapter

    gpu_info = try_create_gpu_adapter()
    print(f"*** USING GPU: {gpu_info}", file=sys.stderr)
    GPU_AVAILABLE = True
except OSError as e:
    SKIP_REASON = str(e)

from qsharp._simulation import GpuSimulator, Result


def map_result_list_to_str(results):
    s = ""
    if isinstance(results, (list, tuple)):
        for r in results:
            s += map_result_list_to_str(r)
    else:
        match results:
            case Result.Zero:
                s += "0"
            case Result.One:
                s += "1"
            case Result.Loss:
                s += "L"
    return s


# Acquiring the GPU resources takes time, so we acquire them once and use them
# for all the tests. This is fine since pytest runs tests sequencially.
sim = GpuSimulator()


def run_shots(qir: str, shots: int = 10_000, seed: int = 42):
    """Run *qir* on the GPU and return the shot_results list."""
    global sim
    sim.set_program(qir)
    return sim.run_shots(shots, seed=seed)


# ---------------------------------------------------------------------------
# QIR source
# ---------------------------------------------------------------------------

# Example 1: Measure-and-correct (H → MResetZ → read_result → branch → X)
# After H and MResetZ, qubit 0 collapses to |0⟩ or |1⟩ with equal probability.
# If measured 1, X is applied to flip it back to |0⟩.
# The result register records the measurement outcome before correction.
# Expected histogram: ~50% "0" and ~50% "1" on result 0.
MEASURE_AND_CORRECT_QIR = """\
%Result = type opaque
%Qubit = type opaque

define void @ENTRYPOINT__main() #0 {
entry:
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  %r = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
  br i1 %r, label %then, label %end

then:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %end

end:
  call void @__quantum__rt__tuple_record_output(i64 1, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)
declare i1 @__quantum__qis__read_result__body(%Result*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__rt__tuple_record_output(i64, i8*)
declare void @__quantum__rt__result_record_output(%Result*, i8*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
"""

# Example 3: Conditionally terminating loop
# Repeatedly applies H → Mz → read_result until result is 1.
# Each iteration has 50% chance of exiting. Loop iteration count
# follows a geometric distribution with p=0.5.
# Result register 0 always records 1 (the exit condition).
CONDITIONAL_LOOP_QIR = """\
%Result = type opaque
%Qubit = type opaque

define void @ENTRYPOINT__main() #0 {
entry:
  br label %loop

loop:
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  %r = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
  br i1 %r, label %done, label %loop

done:
  call void @__quantum__rt__tuple_record_output(i64 1, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)
declare i1 @__quantum__qis__read_result__body(%Result*)
declare void @__quantum__rt__tuple_record_output(i64, i8*)
declare void @__quantum__rt__result_record_output(%Result*, i8*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
"""


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_measure_and_correct_histogram():
    """Example 1: H → MResetZ → read_result → conditional X.

    Run 10000 shots and verify ~50/50 split of "0" and "1" outcomes.
    The measurement result records whether H collapsed to |1⟩ (then X corrects).
    """
    results = run_shots(MEASURE_AND_CORRECT_QIR)
    shot_results = results["shot_results"]
    assert len(shot_results) == 10000
    assert all(
        code == 0 for code in results["shot_result_codes"]
    ), f"Some shots had non-zero error codes: {[c for c in results['shot_result_codes'] if c != 0]}"

    counts = Counter(map_result_list_to_str(r) for r in shot_results)
    # Each shot produces a single-bit result string: "0" or "1"
    count_0 = counts.get("0", 0)
    count_1 = counts.get("1", 0)

    # Verify ~50/50 within 10% tolerance (very generous for 10000 shots)
    assert count_0 > 4000, f"Expected ~5000 '0' results, got {count_0}"
    assert count_1 > 4000, f"Expected ~5000 '1' results, got {count_1}"
    assert count_0 + count_1 == 10000, "All shots should produce a result"


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_conditional_loop_all_results_are_one():
    """Example 3: The loop exits only when measurement yields 1.

    Every shot's recorded result should be "1" since the loop continues
    until that outcome.
    """
    shots = 5000
    results = run_shots(CONDITIONAL_LOOP_QIR, shots=shots)
    shot_results = results["shot_results"]
    assert len(shot_results) == shots
    assert all(
        code == 0 for code in results["shot_result_codes"]
    ), f"Some shots had non-zero error codes: {[c for c in results['shot_result_codes'] if c != 0]}"

    counts = Counter(map_result_list_to_str(r) for r in shot_results)
    # Every shot should exit with result "1"
    assert (
        counts.get("1", 0) == shots
    ), f"Expected all {shots} shots to produce '1', got counts: {counts}"


# Example 2: Loop with phi node — GHZ state preparation
# Applies H to qubit 0, then loops from i=1 to 4,
# applying CNOT(q0, q_i) in each iteration using a phi node
# to track the loop counter. After the loop, all 5 qubits
# are measured. This creates a GHZ-like state (|00000⟩ + |11111⟩)/√2,
# so all 5 measurements must agree.
LOOP_WITH_PHI_QIR = """\
%Result = type opaque
%Qubit = type opaque

define void @ENTRYPOINT__main() #0 {
entry:
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %loop

loop:
  %i = phi i64 [ 1, %entry ], [ %next_i, %loop ]
  %qi = inttoptr i64 %i to %Qubit*
  call void @__quantum__qis__cnot__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* %qi)
  %next_i = add i64 %i, 1
  %cond = icmp sle i64 %next_i, 4
  br i1 %cond, label %loop, label %measure

measure:
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
  call void @__quantum__rt__tuple_record_output(i64 5, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 3 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* null)
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__cnot__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)
declare void @__quantum__rt__tuple_record_output(i64, i8*)
declare void @__quantum__rt__result_record_output(%Result*, i8*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="5" }
"""

# Example 4: Classical boolean computation
# Applies H to qubits 0 and 1, measures both, then computes
# the AND of the two results. If both are 1, applies X to qubit 0
# (which was reset to |0⟩ by MResetZ). Final measurement of qubit 0
# records whether both original measurements were 1.
# Expected: ~25% "1" (both measured 1) and ~75% "0".
BOOLEAN_COMPUTATION_QIR = """\
%Result = type opaque
%Qubit = type opaque

define void @ENTRYPOINT__main() #0 {
entry:
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  %r0 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
  %r1 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  %both = and i1 %r0, %r1
  br i1 %both, label %then, label %else

then:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %end

else:
  br label %end

end:
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  call void @__quantum__rt__tuple_record_output(i64 1, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* null)
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)
declare i1 @__quantum__qis__read_result__body(%Result*)
declare void @__quantum__rt__tuple_record_output(i64, i8*)
declare void @__quantum__rt__result_record_output(%Result*, i8*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="3" }
"""


# ---------------------------------------------------------------------------
# Tests — Example 2: Loop with phi (GHZ state)
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_loop_with_phi_ghz_histogram():
    """Example 2: H → loop CNOT(q0, q_i) for i=1..4 → measure all.

    Creates (|00000⟩ + |11111⟩)/√2. All 5 measurements must agree.
    Run 10000 shots and verify only "00000" and "11111" appear near 50/50.
    """
    results = run_shots(LOOP_WITH_PHI_QIR)
    shot_results = results["shot_results"]
    assert len(shot_results) == 10000
    assert all(
        code == 0 for code in results["shot_result_codes"]
    ), f"Some shots had non-zero error codes: {[c for c in results['shot_result_codes'] if c != 0]}"

    counts = Counter(map_result_list_to_str(r) for r in shot_results)
    # Only "00000" and "11111" should appear
    assert set(counts.keys()) <= {
        "00000",
        "11111",
    }, f"Unexpected outcomes in GHZ state: {counts}"

    count_00000 = counts.get("00000", 0)
    count_11111 = counts.get("11111", 0)

    assert count_00000 > 4000, f"Expected ~5000 '00000' results, got {count_00000}"
    assert count_11111 > 4000, f"Expected ~5000 '11111' results, got {count_11111}"
    assert count_00000 + count_11111 == 10000, "All shots should produce a result"


# ---------------------------------------------------------------------------
# Tests — Example 4: Boolean computation (AND gate)
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_boolean_computation_histogram():
    """Example 4: H(q0), H(q1) → MResetZ both → AND results → conditional X.

    r2=1 only when both r0=1 AND r1=1 (~25% of shots).
    Run 10000 shots and verify ~25% "1" and ~75% "0".
    """
    results = run_shots(BOOLEAN_COMPUTATION_QIR)
    shot_results = results["shot_results"]
    assert len(shot_results) == 10000
    assert all(
        code == 0 for code in results["shot_result_codes"]
    ), f"Some shots had non-zero error codes: {[c for c in results['shot_result_codes'] if c != 0]}"

    counts = Counter(map_result_list_to_str(r) for r in shot_results)
    count_0 = counts.get("0", 0)
    count_1 = counts.get("1", 0)

    assert 1500 < count_1 < 3500, f"Expected ~2500 '1' results (~25%), got {count_1}"
    assert 6500 < count_0 < 8500, f"Expected ~7500 '0' results (~75%), got {count_0}"
    assert count_0 + count_1 == 10000, "All shots should produce a result"


# ---------------------------------------------------------------------------
# QIR fixture — Example 5: Teleport chain
# ---------------------------------------------------------------------------

# Example 5: Teleport chain
# Creates two Bell pairs: (q0,q1) and (q2,q4).
# Teleports q1's state to q4 via measure-and-correct on the q1-q2 channel,
# with separate mz and reset operations. After teleportation, q0 and q4
# are entangled. The final measurements of q0 and q4 (results 4 and 5,
# labeled "0_t0" and "0_t1") should be correlated: either both "0" or
# both "1", with ~50/50 distribution.
TELEPORT_CHAIN_QIR = """\
%Result = type opaque
%Qubit = type opaque

@0 = internal constant [5 x i8] c"0_t0\\00"
@1 = internal constant [5 x i8] c"0_t1\\00"

define void @TeleportChain() #0 {
entry:
  call void @__quantum__rt__initialize(i8* null)
  br label %body
body:
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__cnot__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__cnot__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
  call void @__quantum__qis__cnot__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  %0 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
  br i1 %0, label %then__1, label %continue__1
then__1:
  call void @__quantum__qis__z__body(%Qubit* inttoptr (i64 4 to %Qubit*))
  br label %continue__1
continue__1:
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  %1 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  br i1 %1, label %then__2, label %continue__2
then__2:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 4 to %Qubit*))
  br label %continue__2
continue__2:
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 4 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 4 to %Qubit*))
  br label %exit
exit:
  call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([5 x i8], [5 x i8]* @0, i32 0, i32 0))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 3 to %Result*), i8* getelementptr inbounds ([5 x i8], [5 x i8]* @1, i32 0, i32 0))
  ret void
}

declare void @__quantum__qis__cnot__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__z__body(%Qubit*)
declare void @__quantum__qis__reset__body(%Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare void @__quantum__rt__initialize(i8*)
declare i1 @__quantum__qis__read_result__body(%Result*)
declare void @__quantum__rt__result_record_output(%Result*, i8*)
declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="4" }
attributes #1 = { "irreversible" }
"""


# ---------------------------------------------------------------------------
# Tests — Example 5: Teleport chain
# ---------------------------------------------------------------------------


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_teleport_chain_histogram():
    """Example 5: Teleport chain with 2 Bell pairs and measure-and-correct.

    Creates Bell pairs (q0,q1) and (q2,q4), then teleports q1's state to q4
    via the q1→q2 channel. After teleportation, q0 and q4 are entangled.
    Final measurements of q0 and q4 (results 2 and 3, labeled "0_t0" and
    "0_t1") should be correlated: both "0" or both "1", near 50/50.
    """
    results = run_shots(TELEPORT_CHAIN_QIR)
    shot_results = results["shot_results"]
    assert len(shot_results) == 10000
    assert all(
        code == 0 for code in results["shot_result_codes"]
    ), f"Some shots had non-zero error codes: {[c for c in results['shot_result_codes'] if c != 0]}"

    counts = Counter(map_result_list_to_str(r) for r in shot_results)
    # Only "00" and "11" should appear (results 4 and 5 are correlated)
    assert set(counts.keys()) <= {
        "00",
        "11",
    }, f"Unexpected outcomes in teleport chain: {counts}"

    count_00 = counts.get("00", 0)
    count_11 = counts.get("11", 0)

    assert count_00 > 4000, f"Expected ~5000 '00' results, got {count_00}"
    assert count_11 > 4000, f"Expected ~5000 '11' results, got {count_11}"
    assert count_00 + count_11 == 10000, "All shots should produce a result"


DYNAMIC_ROTATION_ANGLE_QIR = r"""
%Result = type opaque
%Qubit = type opaque

@0 = internal constant [4 x i8] c"0_r\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  %var_1 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
  %var_2 = icmp eq i1 %var_1, false
  br i1 %var_2, label %block_1, label %block_2
block_1:
  br label %block_3
block_2:
  br label %block_3
block_3:
  %var_3 = phi double [0.5, %block_1], [1.0, %block_2]
  call void @__quantum__qis__rx__body(double %var_3, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)
declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__rt__read_result(%Result*)
declare void @__quantum__qis__rx__body(double, %Qubit*)
declare void @__quantum__rt__result_record_output(%Result*, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
attributes #1 = { "irreversible" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
!5 = !{i32 5, !"float_computations", !{!"double"}}
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_dynamic_rotation_angle():
    results = run_shots(DYNAMIC_ROTATION_ANGLE_QIR)
    shot_results = results["shot_results"]
    assert len(shot_results) == 10_000
    assert all(
        code == 0 for code in results["shot_result_codes"]
    ), f"Some shots had non-zero error codes: {[c for c in results['shot_result_codes'] if c != 0]}"

    counts = Counter(map_result_list_to_str(r) for r in shot_results)
    count_0 = counts.get("0", 0)
    count_1 = counts.get("1", 0)

    assert count_1 > 1400, f"Expected ~15% '1' results, got {count_1}"
    assert count_0 > 8400, f"Expected ~85% '0' results, got {count_0}"
    assert count_0 + count_1 == 10_000, "All shots should produce a result"
