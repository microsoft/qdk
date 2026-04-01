# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""End-to-end tests for the adaptive CPU bytecode interpreter pipeline.

Tests run Adaptive Profile QIR through the full pipeline:
Python AdaptiveProfilePass → Rust receiver → CPU interpreter → results.

This is a CPU counterpart to ``test_adaptive_gpu_quantum_ops.py``.

For smaller tests covering the full Adaptive Profile instruction set,
see ``test_adaptive_cpu_bytecode.py``.
"""

from collections import Counter

import pytest

from qsharp._simulation import run_qir, Result

SIM_TYPES = ["cpu", "clifford"]


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def map_result_list_to_str(results):
    results_str = ""
    for r in results:
        match r:
            case Result.Zero:
                results_str += "0"
            case Result.One:
                results_str += "1"
            case Result.Loss:
                results_str += "L"
    return results_str


def _run(qir: str, shots: int, seed: int = 42, sim_type: str = "cpu"):
    """Run *qir* on the given simulator and return shot results as a list of strings."""
    results = run_qir(qir, shots, seed=seed, type=sim_type)
    return [map_result_list_to_str(r) for r in results]


# ---------------------------------------------------------------------------
# QIR source
# ---------------------------------------------------------------------------

# Example 1: Measure-and-correct (H → MResetZ → read_result → branch → X)
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

# Example 2: Loop with phi node — GHZ state preparation
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

# Example 5: Teleport chain
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

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="4" }
attributes #1 = { "irreversible" }
"""


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_measure_and_correct_histogram(sim_type):
    """Example 1: H → MResetZ → read_result → conditional X.

    Run 10000 shots and verify ~50/50 split of "0" and "1" outcomes.
    """
    results = _run(MEASURE_AND_CORRECT_QIR, shots=10000, seed=42, sim_type=sim_type)
    assert len(results) == 10000

    counts = Counter(results)
    count_0 = counts.get("0", 0)
    count_1 = counts.get("1", 0)

    assert count_0 > 4000, f"Expected ~5000 '0' results, got {count_0}"
    assert count_1 > 4000, f"Expected ~5000 '1' results, got {count_1}"
    assert count_0 + count_1 == 10000, "All shots should produce a result"


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_conditional_loop_all_results_are_one(sim_type):
    """Example 3: The loop exits only when measurement yields 1.

    Every shot's recorded result should be "1".
    """
    shots = 5000
    results = _run(CONDITIONAL_LOOP_QIR, shots=shots, seed=99, sim_type=sim_type)
    assert len(results) == shots

    counts = Counter(results)
    assert (
        counts.get("1", 0) == shots
    ), f"Expected all {shots} shots to produce '1', got counts: {counts}"


# ---------------------------------------------------------------------------
# Tests — Example 2: Loop with phi (GHZ state)
# ---------------------------------------------------------------------------


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_loop_with_phi_ghz_histogram(sim_type):
    """Example 2: H → loop CNOT(q0, q_i) for i=1..4 → measure all.

    Creates (|00000⟩ + |11111⟩)/√2. All 5 measurements must agree.
    """
    results = _run(LOOP_WITH_PHI_QIR, shots=10000, seed=42, sim_type=sim_type)
    assert len(results) == 10000

    counts = Counter(results)
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


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_boolean_computation_histogram(sim_type):
    """Example 4: H(q0), H(q1) → MResetZ both → AND results → conditional X.

    r2=1 only when both r0=1 AND r1=1 (~25% of shots).
    """
    results = _run(BOOLEAN_COMPUTATION_QIR, shots=10000, seed=42, sim_type=sim_type)
    assert len(results) == 10000

    counts = Counter(results)
    count_0 = counts.get("0", 0)
    count_1 = counts.get("1", 0)

    assert 1500 < count_1 < 3500, f"Expected ~2500 '1' results (~25%), got {count_1}"
    assert 6500 < count_0 < 8500, f"Expected ~7500 '0' results (~75%), got {count_0}"
    assert count_0 + count_1 == 10000, "All shots should produce a result"


# ---------------------------------------------------------------------------
# Tests — Example 5: Teleport chain
# ---------------------------------------------------------------------------


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_teleport_chain_histogram(sim_type):
    """Example 5: Teleport chain with 2 Bell pairs and measure-and-correct.

    Final measurements of q0 and q4 should be correlated:
    both "0" or both "1", near 50/50.
    """
    results = _run(TELEPORT_CHAIN_QIR, shots=10000, seed=42, sim_type=sim_type)
    assert len(results) == 10000

    counts = Counter(results)
    assert set(counts.keys()) <= {
        "00",
        "11",
    }, f"Unexpected outcomes in teleport chain: {counts}"

    count_00 = counts.get("00", 0)
    count_11 = counts.get("11", 0)

    assert count_00 > 4000, f"Expected ~5000 '00' results, got {count_00}"
    assert count_11 > 4000, f"Expected ~5000 '11' results, got {count_11}"
    assert count_00 + count_11 == 10000, "All shots should produce a result"
