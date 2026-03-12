# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Per-opcode tests for the adaptive GPU bytecode interpreter.

Each test targets one (or a small family of) bytecode instruction(s) by
supplying hand-written Adaptive Profile QIR that exercises the instruction
and encodes the expected result into a measurement outcome.

Tests are ordered to match the opcode definitions in ``_adaptive_opcodes.py``
so that coverage can be verified by reading both files side by side.

Requires QDK_GPU_TESTS env var and a GPU adapter.
"""

import os
import sys
from collections import Counter
import pytest

# Skip the whole module when GPU tests aren't requested.
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

from qsharp._simulation import GpuSimulator

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

# Deterministic programs need a single shot but we run multiple shots
# to verify that multiple shots yield the same result.
SHOTS = 100


def _run(qir: str, shots: int = SHOTS, seed: int = 42):
    """Run *qir* on the GPU and return the shot_results list."""
    sim = GpuSimulator()
    sim.set_program(qir)
    results = sim.run_shots(shots, seed=seed)
    return results["shot_results"]


def check_result(
    qir_fragment: str,
    expected: str,
    *,
    extra_decls: str = "",
    num_qubits: int = 1,
    num_results: int = 1,
    record=None,
):
    """Assert every shot produces *expected*."""
    qir = format_qir(
        qir_fragment,
        extra_decls=extra_decls,
        num_qubits=num_qubits,
        num_results=num_results,
        record=record,
    )
    results = _run(qir, SHOTS)
    counts = Counter(results)
    assert counts == {
        expected: SHOTS
    }, f"Expected all {SHOTS} shots to be '{expected}', got {counts}"


def check_arith_result(qir_fragment: str, expected: str):
    body = build_arith_body(qir_fragment)
    check_result(body, expected)


_DECLS = """\
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare void @__quantum__qis__reset__body(%Qubit*)
declare void @__quantum__qis__cnot__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__z__body(%Qubit*)
declare void @__quantum__qis__s__body(%Qubit*)
declare void @__quantum__qis__t__body(%Qubit*)
declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__rz__body(double, %Qubit*)
declare i1 @__quantum__qis__read_result__body(%Result*)
declare void @__quantum__rt__tuple_record_output(i64, i8*)
declare void @__quantum__rt__result_record_output(%Result*, i8*)
declare void @__quantum__rt__initialize(i8*)
"""


def format_qir(
    body: str,
    *,
    extra_decls: str = "",
    num_qubits: int = 1,
    num_results: int = 1,
    record=None,
):
    if record is None:
        record = range(num_results)
    output_recording = (
        f"  call void @__quantum__rt__tuple_record_output(i64 {len(record)}, i8* null)"
    )
    for result_id in record:
        output_recording += f"\n  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 {result_id} to %Result*), i8* null)"

    return f"""\
%Result = type opaque
%Qubit = type opaque

define i64 @ENTRYPOINT__main() #0 {{
{body}
{output_recording}
  ret i64 0
}}

{_DECLS}
{extra_decls}
attributes #0 = {{ "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="{num_qubits}" "required_num_results"="{num_results}" }}
attributes #1 = {{ "irreversible" }}
"""


def build_arith_body(
    arith_fragment: str,
):
    """Builds the body for a QIR module that does classical work and
    then conditionally applies X to qubit 0 before measuring into result 0.

    *arith_fragment* should produce ``%flag`` (i1) which, when true, causes X.
    The measurement of qubit 0 into result 0 is the observable.
    """
    return f"""\
entry:
{arith_fragment}
  br i1 %flag, label %then, label %end
then:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %end
end:
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""


# #########################################################################
#  Control Flow
# #########################################################################


# =========================================================================
# OP_NOP — no-op
# =========================================================================

# NOP is not directly emittable from QIR, but we confirm basic program works.
# Covered implicitly by every test above. A separate "smoke" is still nice:
NOP_SMOKE_QIR = """
entry:
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_nop_smoke():
    """Minimal program: just measure |0⟩ → always 0."""
    check_result(NOP_SMOKE_QIR, "0")


# =========================================================================
# OP_RET — return / program termination
# =========================================================================

# Every test already exercises RET implicitly. This tests an explicit early ret.
RET_QIR = """
entry:
  ret i64 0
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_ret():
    check_result(RET_QIR, "0")


# =========================================================================
# OP_JUMP — unconditional jump
# =========================================================================

JUMP_QIR = """
entry:
  br label %target
  ret i64 0  ; early return - unreachable
target:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_jump():
    """Unconditional jump lands at target block, X applied → measure 1."""
    check_result(JUMP_QIR, "1")


# =========================================================================
# OP_BRANCH — conditional branch
# =========================================================================

BRANCH_TRUE_QIR = """
entry:
  %c = icmp eq i64 1, 1
  br i1 %c, label %yes, label %no
  ret i64 0  ; early return - unreachable
yes:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %measure
no:
  br label %measure
measure:
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""

BRANCH_FALSE_QIR = """
entry:
  %c = icmp eq i64 1, 2
  br i1 %c, label %yes, label %no
  ret i64 0  ; early return - unreachable
yes:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %measure
no:
  br label %measure
measure:
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_branch_true():
    check_result(BRANCH_TRUE_QIR, "1")


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_branch_false():
    check_result(BRANCH_FALSE_QIR, "0")


# =========================================================================
# OP_SWITCH — switch dispatch
# =========================================================================

SWITCH_CASE1_QIR = """
entry:
  %val = add i64 0, 1
  switch i64 %val, label %default [
    i64 0, label %case0
    i64 1, label %case1
    i64 2, label %case2
  ]
case0:
  br label %measure
case1:
  ; This is the expected path for val==1
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %measure
case2:
  br label %measure
default:
  br label %measure
measure:
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""

SWITCH_DEFAULT_QIR = """
entry:
  %val = add i64 0, 99
  switch i64 %val, label %default [
    i64 0, label %case0
    i64 1, label %case1
  ]
case0:
  br label %measure
case1:
  br label %measure
default:
  ; val=99 takes default path → X applied
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %measure
measure:
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_switch_case():
    check_result(SWITCH_CASE1_QIR, "1")


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_switch_default():
    check_result(SWITCH_DEFAULT_QIR, "1")


# =========================================================================
# OP_CALL / OP_CALL_RETURN — function calls
# =========================================================================

CALL_QIR = """
entry:
  call void @apply_x(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""

CALL_QIR_FN = """
define void @apply_x(%Qubit* %q) {
entry:
  call void @__quantum__qis__x__body(%Qubit* %q)
  ret void
}
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_call_and_return():
    """Call a helper function that applies X, then measure."""
    check_result(CALL_QIR, "1", extra_decls=CALL_QIR_FN)


# #########################################################################
#  Quantum
# #########################################################################


# =========================================================================
# OP_QUANTUM_GATE — single and two-qubit gates
# =========================================================================

# X gate: |0⟩ → |1⟩
GATE_X_QIR = """
entry:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""

# CNOT gate: |10⟩ → |11⟩
GATE_CNOT_QIR = """
entry:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__cnot__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_gate_x():
    check_result(GATE_X_QIR, "1")


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_gate_cnot():
    check_result(GATE_CNOT_QIR, "1", num_qubits=2)


# =========================================================================
# OP_MEASURE — measurement (also see OP_READ_RESULT below)
# =========================================================================

# OP_MEASURE is exercised in nearly every test via mresetz. This test
# explicitly uses mz (non-resetting measurement) + separate reset.
MZ_THEN_RESET_QIR = """
entry:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  ; After mz, qubit should still be |1⟩
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  ; After reset, qubit should be |0⟩
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_mz_then_reset():
    "X → MZ → MZ → reset should give 110."
    check_result(MZ_THEN_RESET_QIR, "110", num_results=3)


# =========================================================================
# OP_RESET — qubit reset
# =========================================================================

RESET_QIR = """
entry:
  ; Put qubit 0 in |1⟩
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  ; Reset it back to |0⟩
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  ; Measure — should be 0
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_reset():
    """X → reset → measure should give 0."""
    check_result(RESET_QIR, "0")


# =========================================================================
# OP_READ_RESULT + OP_MEASURE — read measurement results
# =========================================================================

READ_RESULT_QIR = """
entry:
  ; Prepare |1⟩ on qubit 0 via X
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  ; Measure qubit 0 → should always be 1
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  ; Read back the result
  %r = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
  ; If result was 1, apply X again so qubit is back in |1⟩ for second measurement
  br i1 %r, label %then, label %end

then:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %end

end:
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_read_result():
    """X → MResetZ → read_result → if 1: X again → MResetZ.
    First result is always 1, read_result sees it, applies X, second result is also 1.
    """
    check_result(READ_RESULT_QIR, "11", num_results=2)


# =========================================================================
# OP_RECORD_OUTPUT — output recording
# =========================================================================

# Explicitly test recording two results in order.
RECORD_OUTPUT_QIR = """
entry:
  ; q0 = |1⟩, q1 = |0⟩
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_record_output_ordering():
    """Two results recorded: result0=1, result1=0 → '10'."""
    check_result(RECORD_OUTPUT_QIR, "10", num_qubits=2, num_results=2)


# #########################################################################
#  Integer Arithmetic
# #########################################################################

INT_ARITH_PARAMS = [
    # Int
    ("add", 3, 4, 7),
    ("sub", 10, 3, 7),
    ("sub", 3, 10, -7),
    ("mul", 6, 7, 42),
    ("udiv", 42, 7, 6),
    ("sdiv", -42, 7, -6),
    ("urem", 10, 3, 1),
    ("srem", -10, 3, -1),
    # Bitwise
    ("and", 255, 15, 15),
    ("or", 240, 15, 255),
    ("xor", 255, 15, 240),
    ("shl", 1, 3, 8),
    ("lshr", 32, 2, 8),
    ("ashr", -16, 2, -4),
]


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
@pytest.mark.parametrize(
    "bin_op,lhs,rhs,expected",
    INT_ARITH_PARAMS,
)
def test_int_arith_imm_imm(bin_op, lhs, rhs, expected):
    check_arith_result(
        f"""
        %a = {bin_op} i64 {lhs}, {rhs}
        %flag = icmp eq i64 %a, {expected}""",
        "1",
    )


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
@pytest.mark.parametrize(
    "bin_op,lhs,rhs,expected",
    INT_ARITH_PARAMS,
)
def test_int_arith_imm_reg(bin_op, lhs, rhs, expected):
    check_arith_result(
        f"""
        %rhs = add i64 {rhs}, 0
        %a = {bin_op} i64 {lhs}, %rhs
        %flag = icmp eq i64 %a, {expected}""",
        "1",
    )


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
@pytest.mark.parametrize(
    "bin_op,lhs,rhs,expected",
    INT_ARITH_PARAMS,
)
def test_int_arith_reg_imm(bin_op, lhs, rhs, expected):
    check_arith_result(
        f"""
        %lhs = add i64 {lhs}, 0
        %a = {bin_op} i64 %lhs, {rhs}
        %flag = icmp eq i64 %a, {expected}""",
        "1",
    )


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
@pytest.mark.parametrize(
    "bin_op,lhs,rhs,expected",
    INT_ARITH_PARAMS,
)
def test_int_arith_reg_reg(bin_op, lhs, rhs, expected):
    check_arith_result(
        f"""
        %lhs = add i64 {lhs}, 0
        %rhs = add i64 {rhs}, 0
        %a = {bin_op} i64 %lhs, %rhs
        %flag = icmp eq i64 %a, {expected}""",
        "1",
    )


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
@pytest.mark.parametrize(
    "bin_op,lhs,rhs,expected",
    INT_ARITH_PARAMS,
)
def test_int_arith_negative_test(bin_op, lhs, rhs, expected):
    """Checks that the tests fail if the result is different from the expected value."""
    # Override the expected value.
    expected = 12345
    check_arith_result(
        f"""
        %a = {bin_op} i64 {lhs}, {rhs}
        %flag = icmp eq i64 %a, {expected}""",
        "0",
    )


# #########################################################################
#  Comparison  (OP_ICMP, OP_FCMP)
# #########################################################################


# =========================================================================
# OP_ICMP — integer comparison (all condition codes)
# =========================================================================


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
@pytest.mark.parametrize(
    "pred,lhs,rhs,expected",
    [
        ("eq", 2, 2, "1"),
        ("eq", 2, 3, "0"),
        ("ne", 2, 3, "1"),
        ("ne", 2, 2, "0"),
        ("slt", 2, 3, "1"),
        ("slt", 2, 2, "0"),
        ("sle", 2, 2, "1"),
        ("sle", 3, 2, "0"),
        ("sgt", 3, 2, "1"),
        ("sgt", 2, 3, "0"),
        ("sge", 3, 3, "1"),
        ("sge", 2, 3, "0"),
        ("ult", 2, 3, "1"),
        ("ult", 3, 2, "0"),
        ("ule", 3, 3, "1"),
        ("ule", 3, 2, "0"),
        ("ugt", 3, 2, "1"),
        ("ugt", 2, 3, "0"),
        ("uge", 3, 3, "1"),
        ("uge", 2, 3, "0"),
    ],
)
def test_icmp(pred, lhs, rhs, expected):
    check_arith_result(
        f"%flag = icmp {pred} i64 {lhs}, {rhs}",
        expected,
    )


# =========================================================================
# OP_ICMP — signed vs unsigned edge case (negative as unsigned)
# =========================================================================

ICMP_SIGNED_VS_UNSIGNED_QIR = """
  ; -1 in two's complement is 0xFFFFFFFFFFFFFFFF, which is the max u64
  ; signed: -1 < 0 → true
  %neg1 = sub i64 0, 1
  %flag = icmp slt i64 %neg1, 0
"""

ICMP_UNSIGNED_WRAP_QIR = """
  ; unsigned: -1 wraps to max u64, so -1 > 0 → true (unsigned)
  %neg1 = sub i64 0, 1
  %flag = icmp ugt i64 %neg1, 0
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_icmp_signed_negative():
    check_arith_result(ICMP_SIGNED_VS_UNSIGNED_QIR, "1")


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_icmp_unsigned_wrap():
    check_arith_result(ICMP_UNSIGNED_WRAP_QIR, "1")


# =========================================================================
# OP_FCMP — float comparison
# =========================================================================


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
@pytest.mark.parametrize(
    "pred,lhs,rhs,expected",
    [
        ("oeq", "3.0", "3.0", "1"),
        ("oeq", "3.0", "4.0", "0"),
        ("one", "3.0", "4.0", "1"),
        ("one", "3.0", "3.0", "0"),
        ("olt", "2.0", "3.0", "1"),
        ("olt", "3.0", "2.0", "0"),
        ("ole", "3.0", "3.0", "1"),
        ("ole", "4.0", "3.0", "0"),
        ("ogt", "4.0", "3.0", "1"),
        ("ogt", "3.0", "4.0", "0"),
        ("oge", "3.0", "3.0", "1"),
        ("oge", "2.0", "3.0", "0"),
    ],
)
def test_fcmp(pred, lhs, rhs, expected):
    check_arith_result(
        f"%flag = fcmp {pred} double {lhs}, {rhs}",
        expected,
    )


# #########################################################################
#  Float Arithmetic  (OP_FADD → OP_FDIV)
# #########################################################################

FLOAT_ARITH_PARAMS = [
    ("fadd", 1.5, 2.5, 4.0),
    ("fsub", 10.0, 3.0, 7.0),
    ("fsub", 3.0, 10.0, -7.0),
    ("fmul", 6.0, 7.0, 42.0),
    ("fdiv", 8.0, 2.0, 4.0),
]


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
@pytest.mark.parametrize(
    "bin_op,lhs,rhs,expected",
    FLOAT_ARITH_PARAMS,
)
def test_float_arith_imm_imm(bin_op, lhs, rhs, expected):
    check_arith_result(
        f"""
        %a = {bin_op} double {lhs}, {rhs}
        %flag = fcmp oeq double %a, {expected}""",
        "1",
    )


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
@pytest.mark.parametrize(
    "bin_op,lhs,rhs,expected",
    FLOAT_ARITH_PARAMS,
)
def test_float_arith_imm_reg(bin_op, lhs, rhs, expected):
    check_arith_result(
        f"""
        %rhs = fadd double {rhs}, 0.0
        %a = {bin_op} double {lhs}, %rhs
        %flag = fcmp oeq double %a, {expected}""",
        "1",
    )


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
@pytest.mark.parametrize(
    "bin_op,lhs,rhs,expected",
    FLOAT_ARITH_PARAMS,
)
def test_float_arith_reg_imm(bin_op, lhs, rhs, expected):
    check_arith_result(
        f"""
        %lhs = fadd double {lhs}, 0.0
        %a = {bin_op} double %lhs, {rhs}
        %flag = fcmp oeq double %a, {expected}""",
        "1",
    )


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
@pytest.mark.parametrize(
    "bin_op,lhs,rhs,expected",
    FLOAT_ARITH_PARAMS,
)
def test_float_arith_reg_reg(bin_op, lhs, rhs, expected):
    check_arith_result(
        f"""
        %lhs = fadd double {lhs}, 0.0
        %rhs = fadd double {rhs}, 0.0
        %a = {bin_op} double %lhs, %rhs
        %flag = fcmp oeq double %a, {expected}""",
        "1",
    )


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
@pytest.mark.parametrize(
    "bin_op,lhs,rhs,expected",
    FLOAT_ARITH_PARAMS,
)
def test_float_arith_negative_test(bin_op, lhs, rhs, expected):
    """Checks that the tests fail if the result is different from the expected value."""
    # Override the expected value.
    expected = 12345.0
    check_arith_result(
        f"""
        %a = {bin_op} double {lhs}, {rhs}
        %flag = fcmp oeq double %a, {expected}""",
        "0",
    )


# #########################################################################
#  Type Conversion  (OP_ZEXT → OP_SITOFP)
# #########################################################################


# =========================================================================
# OP_ZEXT — zero extension
# =========================================================================

ZEXT_QIR = """
  ; zext i1 true to i64 → 1, check 1 == 1 → true
  %z = zext i1 true to i64
  %flag = icmp eq i64 %z, 1
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_zext():
    check_arith_result(ZEXT_QIR, "1")


# =========================================================================
# OP_SEXT — sign extension
# =========================================================================

SEXT_QIR = """
  ; sext i1 true to i64 → -1 (all ones), check -1 < 0 → true
  %s = sext i1 true to i64
  %flag = icmp slt i64 %s, 0
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_sext():
    check_arith_result(SEXT_QIR, "1")


# =========================================================================
# OP_TRUNC — truncation
# =========================================================================

TRUNC_QIR = """
  ; trunc i64 257 to i32 → 257 (fits), check 257 == 257 → true
  %t = trunc i64 257 to i32
  %z = zext i32 %t to i64
  %flag = icmp eq i64 %z, 257
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_trunc():
    check_arith_result(TRUNC_QIR, "1")


# =========================================================================
# OP_FPEXT / OP_FPTRUNC — float extension/truncation
# (identity on GPU since everything is f32)
# =========================================================================

FPEXT_QIR = """
  ; fpext float 3.0 to double, then check == 3
  %f32 = fadd float 1.0, 2.0
  %f64 = fpext float %f32 to double
  %i = fptosi double %f64 to i64
  %flag = icmp eq i64 %i, 3
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_fpext():
    check_arith_result(FPEXT_QIR, "1")


# =========================================================================
# OP_INTTOPTR / OP_MOV — dynamic qubit addressing
# =========================================================================

# inttoptr is used when qubit IDs come from computations rather than literals.
# Compute qubit index from arithmetic, then apply X via inttoptr.
INTTOPTR_QIR = """
entry:
  ; Compute qubit ID 0 from arithmetic
  %q_id = sub i64 1, 1
  %q = inttoptr i64 %q_id to %Qubit*
  call void @__quantum__qis__x__body(%Qubit* %q)
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_inttoptr_dynamic_qubit():
    check_result(INTTOPTR_QIR, "1")


# =========================================================================
# OP_FPTOSI — float to signed int
# =========================================================================

FPTOSI_QIR = """
  ; fptosi -3.7 → -3 (truncation toward zero), check -3 < 0 → true
  %neg = fsub double 0.0, 3.7
  %i = fptosi double %neg to i64
  %flag = icmp slt i64 %i, 0
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_fptosi():
    check_arith_result(FPTOSI_QIR, "1")


# =========================================================================
# OP_SITOFP — signed int to float
# =========================================================================

SITOFP_QIR = """
  ; sitofp -5 → -5.0, then -5.0 < 0.0 → true
  %neg5 = sub i64 0, 5
  %f = sitofp i64 %neg5 to double
  %zero = sitofp i64 0 to double
  %flag = fcmp olt double %f, %zero
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_sitofp():
    check_arith_result(SITOFP_QIR, "1")


# #########################################################################
#  SSA / Data Movement  (OP_PHI → OP_CONST)
# #########################################################################


# =========================================================================
# OP_PHI — phi node
# =========================================================================

# Classic loop counter: phi selects 0 from entry, incremented value from loop.
# Loops 5 times, then checks counter == 5 → X → measure 1.
PHI_LOOP_QIR = """
entry:
  br label %loop

loop:
  %i = phi i64 [ 0, %entry ], [ %next, %loop ]
  %next = add i64 %i, 1
  %cond = icmp slt i64 %next, 5
  br i1 %cond, label %loop, label %done

done:
  ; %next should be 5 here
  %flag = icmp eq i64 %next, 5
  br i1 %flag, label %apply_x, label %measure

apply_x:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %measure

measure:
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_phi_loop_counter():
    check_result(PHI_LOOP_QIR, "1")


# =========================================================================
# OP_SELECT
# =========================================================================

SELECT_TRUE_QIR = """
  ; select i1 true, i64 1, i64 0 → 1, then icmp eq 1, 1 → true
  %s = select i1 true, i64 1, i64 0
  %flag = icmp eq i64 %s, 1
"""

SELECT_FALSE_QIR = """
  ; select i1 false, i64 1, i64 0 → 0, then icmp eq 0, 0 → true
  %s = select i1 false, i64 1, i64 0
  %flag = icmp eq i64 %s, 0
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_select_true():
    check_arith_result(SELECT_TRUE_QIR, "1")


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_select_false():
    check_arith_result(SELECT_FALSE_QIR, "1")


# =========================================================================
# OP_CONST — constant materialization
# =========================================================================

# Constants are exercised in nearly every test (immediates in icmp, add, etc.)
# This explicitly tests a large constant going through the pipeline.
CONST_QIR = """
  ; Use a specific constant 12345, check add identity
  %a = add i64 12345, 0
  %flag = icmp eq i64 %a, 12345
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_const():
    check_arith_result(CONST_QIR, "1")


# #########################################################################
#  Boolean (i1) variants of bitwise ops
# #########################################################################


# =========================================================================
# OP_AND with i1 (boolean AND) — used in classical boolean logic
# =========================================================================

AND_I1_QIR = """
entry:
  ; Prepare both qubits in |1⟩ deterministically
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  %r0 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
  %r1 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  %both = and i1 %r0, %r1
  ; both should be true (1 AND 1 = 1), apply X → measure 1
  br i1 %both, label %then, label %measure

then:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %measure

measure:
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_and_i1_boolean():
    """Deterministic boolean AND: both qubits |1⟩ → and i1 true, true → X → 1."""
    check_result(AND_I1_QIR, "1", num_qubits=2, num_results=3, record=[2])


# =========================================================================
# OP_OR with i1 (boolean OR)
# =========================================================================

OR_I1_QIR = """
entry:
  ; q0 = |1⟩, q1 = |0⟩
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  %r0 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
  %r1 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  %either = or i1 %r0, %r1
  ; true OR false = true → X → measure 1
  br i1 %either, label %then, label %measure
then:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %measure
measure:
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_or_i1_boolean():
    """Deterministic boolean OR: q0=1, q1=0 → or i1 true, false → true → X → 1."""
    check_result(OR_I1_QIR, "1", num_qubits=2, num_results=3, record=[2])


# =========================================================================
# OP_XOR with i1 (boolean XOR / NOT)
# =========================================================================

XOR_NOT_QIR = """
entry:
  ; q0 = |0⟩ → measure 0
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  %r0 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
  ; XOR with true is NOT: false XOR true = true
  %not_r0 = xor i1 %r0, true
  br i1 %not_r0, label %then, label %measure

then:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %measure

measure:
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_xor_i1_not():
    """XOR i1 used as NOT: measure 0 → XOR true → true → X → 1."""
    check_result(XOR_NOT_QIR, "1", num_qubits=1, num_results=2, record=[1])


# #########################################################################
#  Compound / Integration Tests
# #########################################################################


# =========================================================================
# Chained arithmetic — complex expression
# =========================================================================

CHAINED_ARITH_QIR = """
  ; (3 + 4) * 2 - 1 = 13, check 13 == 13 → true
  %a = add i64 3, 4
  %b = mul i64 %a, 2
  %c = sub i64 %b, 1
  %flag = icmp eq i64 %c, 13
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_chained_arithmetic():
    check_arith_result(CHAINED_ARITH_QIR, "1")


# =========================================================================
# OP_PHI with multiple predecessors (diamond CFG)
# =========================================================================

PHI_DIAMOND_QIR = """
entry:
  %c = icmp eq i64 1, 1
  br i1 %c, label %left, label %right
left:
  br label %merge
right:
  br label %merge
merge:
  ; From left: 42, from right: 0. Since condition is true, we go left → 42.
  %v = phi i64 [ 42, %left ], [ 0, %right ]
  %flag = icmp eq i64 %v, 42
  br i1 %flag, label %apply_x, label %measure
apply_x:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %measure
measure:
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_phi_diamond():
    """Diamond CFG with phi: true branch → phi resolves to 42 → X → 1."""
    check_result(PHI_DIAMOND_QIR, "1")


# =========================================================================
# OP_SELECT with computed condition
# =========================================================================

SELECT_COMPUTED_QIR = """
  ; 5 > 3 is true → select returns 10, check 10 == 10 → true
  %cmp = icmp sgt i64 5, 3
  %s = select i1 %cmp, i64 10, i64 20
  %flag = icmp eq i64 %s, 10
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_select_computed():
    check_arith_result(SELECT_COMPUTED_QIR, "1")


# =========================================================================
# Nested loop — OP_PHI + OP_BRANCH + OP_ADD + OP_ICMP combined
# =========================================================================

# Sum 1+2+3+4+5 = 15 using a loop, then check sum == 15
NESTED_LOOP_SUM_QIR = """
entry:
  br label %loop
loop:
  %i = phi i64 [ 1, %entry ], [ %next_i, %loop ]
  %sum = phi i64 [ 0, %entry ], [ %next_sum, %loop ]
  %next_sum = add i64 %sum, %i
  %next_i = add i64 %i, 1
  %cond = icmp sle i64 %next_i, 5
  br i1 %cond, label %loop, label %done
done:
  ; %next_sum should be 15
  %flag = icmp eq i64 %next_sum, 15
  br i1 %flag, label %apply_x, label %measure
apply_x:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %measure
measure:
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_nested_loop_sum():
    """Sum 1..5 using phi loop, check total == 15."""
    check_result(NESTED_LOOP_SUM_QIR, "1")


# =========================================================================
# OP_QUANTUM_GATE — dynamic qubit addressing in a loop (GHZ-like)
# =========================================================================

DYNAMIC_QUBIT_LOOP_QIR = """
entry:
  ; Create |+⟩ on q0
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %loop
loop:
  %i = phi i64 [ 1, %entry ], [ %next_i, %loop ]
  %qi = inttoptr i64 %i to %Qubit*
  call void @__quantum__qis__cnot__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* %qi)
  %next_i = add i64 %i, 1
  %cond = icmp sle i64 %next_i, 2
  br i1 %cond, label %loop, label %measure
measure:
  ; Measure all 3 qubits — GHZ state means all agree
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_dynamic_qubit_loop():
    """3-qubit GHZ via dynamic qubit loop — only '000' and '111' should appear."""
    qir = format_qir(DYNAMIC_QUBIT_LOOP_QIR, num_qubits=3, num_results=3)
    results = _run(qir, shots=5000, seed=42)
    counts = Counter(results)
    assert set(counts.keys()) <= {"000", "111"}, f"Unexpected GHZ outcomes: {counts}"
    assert counts.get("000", 0) > 1500
    assert counts.get("111", 0) > 1500


# =========================================================================
# OP_SHL + OP_OR combined — bit packing
# =========================================================================

BIT_PACK_QIR = """
  ; Pack bits: (1 << 2) | 1 = 5, check 5 == 5 → true
  %shifted = shl i64 1, 2
  %packed = or i64 %shifted, 1
  %flag = icmp eq i64 %packed, 5
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_bit_packing():
    check_arith_result(BIT_PACK_QIR, "1")


# =========================================================================
# Combined test: all shift and bitwise ops in sequence
# =========================================================================

SHIFT_BITWISE_CHAIN_QIR = """
  ; Start with 0b1010 = 10
  ; SHL by 1 → 0b10100 = 20
  ; OR with 0b00011 = 3 → 0b10111 = 23
  ; AND with 0b11110 = 30 → 0b10110 = 22
  ; XOR with 0b00010 = 2 → 0b10100 = 20
  ; LSHR by 2 → 0b00101 = 5
  %step1 = shl i64 10, 1
  %step2 = or i64 %step1, 3
  %step3 = and i64 %step2, 30
  %step4 = xor i64 %step3, 2
  %step5 = lshr i64 %step4, 2
  %flag = icmp eq i64 %step5, 5
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_shift_bitwise_chain():
    check_arith_result(SHIFT_BITWISE_CHAIN_QIR, "1")


# =========================================================================
# OP_SWITCH with computed value from arithmetic
# =========================================================================

SWITCH_ARITH_QIR = """
entry:
  ; Compute 2 * 3 - 4 = 2
  %a = mul i64 2, 3
  %val = sub i64 %a, 4
  switch i64 %val, label %default [
    i64 0, label %case0
    i64 1, label %case1
    i64 2, label %case2
    i64 3, label %case3
  ]
case0:
  br label %measure
case1:
  br label %measure
case2:
  ; Expected path
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %measure
case3:
  br label %measure
default:
  br label %measure
measure:
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_switch_from_arithmetic():
    """Switch on computed value 2*3-4=2 → case2 → X → 1."""
    check_result(SWITCH_ARITH_QIR, "1")


# =========================================================================
# Float: sitofp → fadd → fptosi round-trip
# =========================================================================

FLOAT_ROUNDTRIP_QIR = """
  ; sitofp 7 → 7.0, fadd 7.0 + 3.0 → 10.0, fptosi → 10, check == 10
  %f = sitofp i64 7 to double
  %three = fadd double 0.0, 3.0
  %sum = fadd double %f, %three
  %i = fptosi double %sum to i64
  %flag = icmp eq i64 %i, 10
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_float_roundtrip():
    check_arith_result(FLOAT_ROUNDTRIP_QIR, "1")


# =========================================================================
# OP_CALL with return value
# =========================================================================

CALL_WITH_RETVAL_QIR = """
entry:
  %result = call i64 @add_numbers(i64 3, i64 4)
  %flag = icmp eq i64 %result, 7
  br i1 %flag, label %then, label %measure
then:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %measure
measure:
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""

CALL_WITH_RETVAL_QIR_FN = """
define i64 @add_numbers(i64 %a, i64 %b) {
entry:
  %sum = add i64 %a, %b
  ret i64 %sum
}
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_call_with_return_value():
    """Call a function returning i64, use result in comparison."""
    check_result(CALL_WITH_RETVAL_QIR, "1", extra_decls=CALL_WITH_RETVAL_QIR_FN)


# =========================================================================
# OP_MUL + OP_UDIV + OP_UREM combined
# =========================================================================

MUL_DIV_REM_QIR = """
  ; 17 / 5 = 3 (udiv), 17 % 5 = 2 (urem), 3 * 5 + 2 = 17
  %q = udiv i64 17, 5
  %r = urem i64 17, 5
  %product = mul i64 %q, 5
  %reconstructed = add i64 %product, %r
  %flag = icmp eq i64 %reconstructed, 17
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_mul_div_rem_identity():
    """Division identity: (a/b)*b + (a%b) == a."""
    check_arith_result(MUL_DIV_REM_QIR, "1")


# =========================================================================
# OP_MEASURE with mid-circuit branch (measure-and-correct pattern)
# =========================================================================

MEASURE_BRANCH_QIR = """
entry:
  ; Deterministically put qubit in |1⟩
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  ; Measure (should be 1) and reset to |0⟩
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  %r = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
  ; Since r=1, branch to 'correct' which applies X to restore |1⟩
  br i1 %r, label %correct, label %measure

correct:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  br label %measure

measure:
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_measure_and_branch():
    """Deterministic measure-and-correct: X→MResetZ→read_result→X→MResetZ → always 1."""
    check_result(MEASURE_BRANCH_QIR, "1", num_results=2, record=[1])


# =========================================================================
# OP_ADD with register-register (no immediates)
# =========================================================================

ADD_REG_REG_QIR = """
  ; Use computed values in registers, not just immediates
  %a = add i64 2, 1
  %b = add i64 3, 1
  %c = add i64 %a, %b
  ; 3 + 4 = 7
  %flag = icmp eq i64 %c, 7
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_add_register_register():
    check_arith_result(ADD_REG_REG_QIR, "1")


# =========================================================================
# Error code check — all tests should produce clean shots
# =========================================================================


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_no_error_codes():
    """Verify that a representative program produces zero error codes."""
    qir = format_qir(build_arith_body(ADD_REG_REG_QIR))
    sim = GpuSimulator()
    sim.set_program(qir)
    results = sim.run_shots(100, seed=42)
    codes = results["shot_result_codes"]
    assert all(
        c == 0 for c in codes
    ), f"Non-zero error codes: {[c for c in codes if c != 0]}"


# =========================================================================
# Error code check — can return error codes
# =========================================================================

ERROR_CODE_QIR = """
entry:
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  ret i64 1
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_error_codes():
    """Verify that a representative program produces zero error codes."""
    qir = format_qir(ERROR_CODE_QIR)
    sim = GpuSimulator()
    sim.set_program(qir)
    results = sim.run_shots(100, seed=42)
    codes = results["shot_result_codes"]
    assert all(
        c == 1 for c in codes
    ), f"All error codes should be 1: {[c for c in codes if c != 1]}"


# #########################################################################
#  Regression tests — exercising specific edge-cases that previously failed
# #########################################################################


# =========================================================================
# SREM with negative dividend  (GPU signed-modulo edge case)
# =========================================================================

SREM_NEG_DIVIDEND_QIR = """
  ; -7 % 2 = -1, verify result < 0
  %neg7 = sub i64 0, 7
  %a = srem i64 %neg7, 2
  %flag = icmp slt i64 %a, 0
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_srem_negative_dividend():
    """srem must preserve the sign of the dividend on GPU."""
    check_arith_result(SREM_NEG_DIVIDEND_QIR, "1")


SREM_NEG_BOTH_QIR = """
  ; -10 % -3 = -1  (sign follows dividend)
  %neg10 = sub i64 0, 10
  %neg3 = sub i64 0, 3
  %a = srem i64 %neg10, %neg3
  %neg1 = sub i64 0, 1
  %flag = icmp eq i64 %a, %neg1
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_srem_negative_both():
    """srem with both operands negative."""
    check_arith_result(SREM_NEG_BOTH_QIR, "1")


# =========================================================================
# SEXT from i1  (sign-extension must convert 1 → -1)
# =========================================================================

SEXT_I1_FALSE_QIR = """
  ; sext i1 false to i64 → 0, check 0 == 0 → true
  %s = sext i1 false to i64
  %flag = icmp eq i64 %s, 0
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_sext_i1_false():
    """sext of false (i1 0) must be 0."""
    check_arith_result(SEXT_I1_FALSE_QIR, "1")


SEXT_I1_RUNTIME_QIR = """
  ; compute i1 true at runtime, sext → -1, check < 0
  %one = add i64 1, 0
  %b = icmp eq i64 %one, 1
  %s = sext i1 %b to i64
  %flag = icmp slt i64 %s, 0
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_sext_i1_runtime():
    """sext of a runtime i1 true value must also sign-extend to -1."""
    check_arith_result(SEXT_I1_RUNTIME_QIR, "1")


# =========================================================================
# Call to IR-defined function with inttoptr constant argument
# =========================================================================

CALL_INTTOPTR_ARG_QIR = """
entry:
  call void @apply_h_then_z_then_h(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""

CALL_INTTOPTR_ARG_QIR_FN = """
define void @apply_h_then_z_then_h(%Qubit* %q) {
entry:
  call void @__quantum__qis__h__body(%Qubit* %q)
  call void @__quantum__qis__z__body(%Qubit* %q)
  call void @__quantum__qis__h__body(%Qubit* %q)
  ret void
}
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_call_inttoptr_arg():
    """Call a helper with an inttoptr constant expression argument."""
    check_result(CALL_INTTOPTR_ARG_QIR, "1", extra_decls=CALL_INTTOPTR_ARG_QIR_FN)


# =========================================================================
# SITOFP with negative value  (signed int → float)
# =========================================================================

SITOFP_NEG_QIR = """
  ; sitofp -3 → -3.0, then -3.0 < 0.0 → true
  %neg3 = sub i64 0, 3
  %f = sitofp i64 %neg3 to double
  %zero = sitofp i64 0 to double
  %flag = fcmp olt double %f, %zero
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_sitofp_negative():
    """sitofp must correctly convert a negative integer."""
    check_arith_result(SITOFP_NEG_QIR, "1")


# =========================================================================
# Call stack overflow guard
# =========================================================================

RECURSIVE_OVERFLOW_BODY = """
entry:
  call void @recursive_fn(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""

RECURSIVE_OVERFLOW_FN = """
define void @recursive_fn(%Qubit* %q) {
entry:
  call void @recursive_fn(%Qubit* %q)
  ret void
}
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_call_stack_overflow_guard():
    """Verify GPU interpreter handles call stack overflow gracefully."""
    qir = format_qir(RECURSIVE_OVERFLOW_BODY, extra_decls=RECURSIVE_OVERFLOW_FN)
    sim = GpuSimulator()
    sim.set_program(qir)
    results = sim.run_shots(10, seed=42)
    # Every shot should return error code 3 (ERR_CALL_STACK_OVERFLOW)
    codes = results["shot_result_codes"]
    assert all(
        c == 3 for c in codes
    ), f"Expected all error codes to be 3 (stack overflow), got {codes}"
