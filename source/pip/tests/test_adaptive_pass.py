# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Unit tests for AdaptiveProfilePass.

Tests verify the Python QIR-to-bytecode translation pass by feeding
LLVM IR strings through the pass and checking the resulting
instruction dict encoding.
"""

from dataclasses import astuple, asdict
import pyqir
import pytest

from qsharp._adaptive_pass import AdaptiveProfilePass, AdaptiveProgram
from qsharp._adaptive_bytecode import *


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _run_pass(ir: str, name: str = "test.ll") -> AdaptiveProgram:
    """Parse an LLVM IR string and run through AdaptiveProfilePass."""
    mod = pyqir.Module.from_ir(pyqir.Context(), ir, name)
    return AdaptiveProfilePass().run(mod)


def _primary(opcode_word: int) -> int:
    """Extract primary opcode from opcode word."""
    return opcode_word & 0xFF


def _sub(opcode_word: int) -> int:
    """Extract sub-opcode from opcode word."""
    return (opcode_word >> 8) & 0xFF


def _flags(opcode_word: int) -> int:
    """Extract flags from opcode word."""
    return opcode_word & 0xFFFF0000


# ---------------------------------------------------------------------------
# Test: Simple linear (H, CNOT, MResetZ on static qubits, no branching)
# ---------------------------------------------------------------------------

LINEAR_QIR = """\
%Result = type opaque
%Qubit = type opaque

define void @ENTRYPOINT__main() #0 {
entry:
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  call void @__quantum__rt__tuple_record_output(i64 1, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)
declare void @__quantum__rt__tuple_record_output(i64, i8*)
declare void @__quantum__rt__result_record_output(%Result*, i8*)

attributes #0 = { "entry_point" "required_num_qubits"="2" "required_num_results"="1" }
"""


def test_linear_structure():
    """Linear QIR with no branching produces correct blocks and ops."""
    r = _run_pass(LINEAR_QIR)
    assert r.num_qubits == 2
    assert r.num_results == 1
    assert r.entry_block == 0
    assert len(r.blocks) == 1
    assert len(r.phi_entries) == 0
    assert len(r.functions) == 0
    assert len(r.switch_cases) == 0


def test_linear_quantum_ops():
    """Linear QIR emits the correct quantum op IDs."""
    r = _run_pass(LINEAR_QIR)
    op_ids = [q.op_id for q in r.quantum_ops]
    assert 5 in op_ids, "Missing H gate (OpID=5)"
    assert 15 in op_ids, "Missing CNOT gate (OpID=15)"
    assert 22 in op_ids, "Missing MResetZ gate (OpID=22)"


def test_linear_instruction_opcodes():
    """Linear QIR instructions have expected primary opcodes."""
    r = _run_pass(LINEAR_QIR)
    primaries = [_primary(inst.opcode) for inst in r.instructions]
    assert OP_QUANTUM_GATE in primaries, "Missing OP_QUANTUM_GATE"
    assert OP_MEASURE in primaries, "Missing OP_MEASURE"
    assert OP_RECORD_OUTPUT in primaries, "Missing OP_RECORD_OUTPUT"
    # Should have a RET at the end
    assert _primary(r.instructions[-1].opcode) == OP_RET


def test_linear_block_offset_consistency():
    """Instruction offsets and counts in blocks must cover all instructions."""
    r = _run_pass(LINEAR_QIR)
    total = 0
    for b in r.blocks:
        offset, count = b.instr_offset, b.instr_count
        assert offset == total
        total += count
    assert total == len(r.instructions)


def test_linear_static_qubit_sentinel():
    """Static qubit IDs use DYN_QUBIT_SENTINEL in aux1/aux2 (no dynamic override)."""
    r = _run_pass(LINEAR_QIR)
    for inst in r.instructions:
        if _primary(inst.opcode) == OP_QUANTUM_GATE:
            # aux1 and aux2 should be DYN_QUBIT_SENTINEL for static qubits
            assert inst.opcode & FLAG_AUX1_IMM, "Expected FLAG_AUX1_IMM in opcode"
            assert inst.opcode & FLAG_AUX2_IMM, "Expected FLAG_AUX2_IMM in opcode"
            return
    pytest.fail("No OP_QUANTUM_GATE found")


# ---------------------------------------------------------------------------
# Test: Measure-and-correct (conditional branch on measurement result)
# ---------------------------------------------------------------------------

MEASURE_CORRECT_QIR = """\
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

attributes #0 = { "entry_point" "required_num_qubits"="1" "required_num_results"="1" }
"""


def test_measure_correct_structure():
    """Measure-and-correct has 3 blocks (entry, then, end)."""
    r = _run_pass(MEASURE_CORRECT_QIR)
    assert r.num_qubits == 1
    assert r.num_results == 1
    assert r.entry_block == 0
    assert len(r.blocks) == 3
    assert len(r.phi_entries) == 0


def test_measure_correct_branch():
    """Entry block ends with a conditional branch."""
    r = _run_pass(MEASURE_CORRECT_QIR)
    entry_block = r.blocks[0]
    offset, count = entry_block.instr_offset, entry_block.instr_count
    last_instr = r.instructions[offset + count - 1]
    assert _primary(last_instr.opcode) == OP_BRANCH


def test_measure_correct_read_result():
    """Read result instruction is emitted."""
    r = _run_pass(MEASURE_CORRECT_QIR)
    primaries = [_primary(inst.opcode) for inst in r.instructions]
    assert OP_READ_RESULT in primaries


def test_measure_correct_quantum_ops():
    """H, MResetZ, and X gates are emitted."""
    r = _run_pass(MEASURE_CORRECT_QIR)
    op_ids = [q.op_id for q in r.quantum_ops]
    assert 5 in op_ids, "Missing H"
    assert 22 in op_ids, "Missing MResetZ"
    assert 2 in op_ids, "Missing X (OpID=2)"


def test_measure_correct_read_result_bool_type():
    """read_result destination register has REG_TYPE_BOOL."""
    r = _run_pass(MEASURE_CORRECT_QIR)
    for inst in r.instructions:
        if _primary(inst.opcode) == OP_READ_RESULT:
            dst_reg = inst.dst
            assert r.register_types[dst_reg] == REG_TYPE_BOOL, (
                f"read_result dst reg type is {r.register_types[dst_reg]}, "
                f"expected REG_TYPE_BOOL={REG_TYPE_BOOL}"
            )
            return
    pytest.fail("No OP_READ_RESULT found")


def test_measure_correct_unconditional_jump():
    """'then' block ends with OP_JUMP (unconditional branch to 'end')."""
    r = _run_pass(MEASURE_CORRECT_QIR)
    then_block = r.blocks[1]  # block 1 = then
    offset, count = then_block.instr_offset, then_block.instr_count
    last_instr = r.instructions[offset + count - 1]
    assert (
        _primary(last_instr.opcode) == OP_JUMP
    ), f"Expected OP_JUMP at end of 'then' block, got {_primary(last_instr.opcode):#x}"


# ---------------------------------------------------------------------------
# Test: Loop with phi node
# ---------------------------------------------------------------------------

LOOP_PHI_QIR = """\
%Result = type opaque
%Qubit = type opaque

define void @ENTRYPOINT__main() #0 {
entry:
  br label %loop

loop:
  %i = phi i64 [ 0, %entry ], [ %next, %loop ]
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  %next = add i64 %i, 1
  %cond = icmp ult i64 %next, 4
  br i1 %cond, label %loop, label %exit

exit:
  call void @__quantum__rt__tuple_record_output(i64 0, i8* null)
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "required_num_qubits"="1" "required_num_results"="0" }
"""


def test_loop_phi_structure():
    """Loop with phi produces 3 blocks (entry, loop, exit)."""
    r = _run_pass(LOOP_PHI_QIR)
    assert len(r.blocks) == 3
    assert r.entry_block == 0


def test_loop_phi_entries():
    """Phi node generates 2 phi_entries (from entry and back-edge)."""
    r = _run_pass(LOOP_PHI_QIR)
    assert len(r.phi_entries) == 2
    # Each phi entry is (block_id, val_reg)
    block_ids = {pe.block_id for pe in r.phi_entries}
    # Should come from entry (block 0) and loop (block 1, back-edge)
    assert 0 in block_ids, "Missing phi entry from entry block"
    assert 1 in block_ids, "Missing phi entry from loop back-edge"


def test_loop_phi_register_types():
    """Phi destination register should have i64 type tag."""
    r = _run_pass(LOOP_PHI_QIR)
    # Find the PHI instruction
    for inst in r.instructions:
        if _primary(inst.opcode) == OP_PHI:
            dst_reg = inst.dst
            assert r.register_types[dst_reg] == REG_TYPE_I64
            return
    pytest.fail("No PHI instruction found")


def test_loop_phi_forward_ref_reuse():
    """Forward-referenced phi incoming value shares register with its definition."""
    r = _run_pass(LOOP_PHI_QIR)
    # The phi incoming from the loop back-edge references %next (add result)
    # Find the phi entry from the loop block (block 1)
    back_edge_entry = next(pe for pe in r.phi_entries if pe.block_id == 1)
    next_reg = back_edge_entry.val_reg

    # Find the ADD instruction (for %next = add %i, 1)
    for inst in r.instructions:
        if _primary(inst.opcode) == OP_ADD:
            add_dst = inst.dst
            assert (
                add_dst == next_reg
            ), f"Forward ref register {next_reg} != ADD dst {add_dst}"
            return
    pytest.fail("No ADD instruction found")


def test_loop_icmp_and_branch():
    """Loop body ends with icmp + conditional branch."""
    r = _run_pass(LOOP_PHI_QIR)
    loop_block = r.blocks[1]  # block 1 = loop
    offset, count = loop_block.instr_offset, loop_block.instr_count
    instrs = r.instructions[offset : offset + count]
    primaries = [_primary(inst.opcode) for inst in instrs]
    assert OP_ICMP in primaries, "Missing ICMP in loop block"
    assert primaries[-1] == OP_BRANCH, "Loop block should end with BRANCH"


def test_loop_icmp_ult_sub_opcode():
    """The icmp ult instruction encodes ICMP_ULT in the sub-opcode field."""
    r = _run_pass(LOOP_PHI_QIR)
    for inst in r.instructions:
        if _primary(inst.opcode) == OP_ICMP:
            assert (
                _sub(inst.opcode) == ICMP_ULT
            ), f"Expected ICMP_ULT={ICMP_ULT} sub-opcode, got {_sub(inst.opcode)}"
            return
    pytest.fail("No OP_ICMP found")


def test_loop_entry_unconditional_jump():
    """Entry block ends with OP_JUMP (unconditional branch to loop header)."""
    r = _run_pass(LOOP_PHI_QIR)
    entry_block = r.blocks[0]  # block 0 = entry
    offset, count = entry_block.instr_offset, entry_block.instr_count
    last_instr = r.instructions[offset + count - 1]
    assert (
        _primary(last_instr.opcode) == OP_JUMP
    ), f"Expected OP_JUMP at end of entry block, got {_primary(last_instr.opcode):#x}"


# ---------------------------------------------------------------------------
# Test: Classical boolean (AND of two read_results, conditional branch)
# ---------------------------------------------------------------------------

CLASSICAL_BOOLEAN_QIR = """\
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
  br i1 %both, label %then, label %end

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

attributes #0 = { "entry_point" "required_num_qubits"="2" "required_num_results"="2" }
"""


def test_classical_boolean_and():
    """AND of two read_results emits an OP_AND instruction."""
    r = _run_pass(CLASSICAL_BOOLEAN_QIR)
    primaries = [_primary(inst.opcode) for inst in r.instructions]
    assert OP_AND in primaries, "Missing OP_AND for boolean AND"


def test_classical_boolean_structure():
    """Classical boolean program has 3 blocks and correct qubits/results."""
    r = _run_pass(CLASSICAL_BOOLEAN_QIR)
    assert r.num_qubits == 2
    assert r.num_results == 2
    assert len(r.blocks) == 3


def test_classical_boolean_read_results():
    """Two read_result instructions are emitted."""
    r = _run_pass(CLASSICAL_BOOLEAN_QIR)
    read_count = sum(
        1 for inst in r.instructions if _primary(inst.opcode) == OP_READ_RESULT
    )
    assert read_count == 2


# ---------------------------------------------------------------------------
# Test: Select instruction
# ---------------------------------------------------------------------------

SELECT_QIR = """\
%Result = type opaque
%Qubit = type opaque

define void @ENTRYPOINT__main() #0 {
entry:
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
  %r = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 0 to %Result*))
  %val = select i1 %r, i32 42, i32 7
  call void @__quantum__rt__int_record_output(i64 0, i8* null)
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)
declare i1 @__quantum__qis__read_result__body(%Result*)
declare void @__quantum__rt__int_record_output(i64, i8*)

attributes #0 = { "entry_point" "required_num_qubits"="1" "required_num_results"="1" }
"""


def test_select_instruction():
    """Select instruction emits OP_SELECT."""
    r = _run_pass(SELECT_QIR)
    primaries = [_primary(inst.opcode) for inst in r.instructions]
    assert OP_SELECT in primaries, "Missing OP_SELECT"


def test_select_i32_type():
    """Select with i32 result type assigns REG_TYPE_I32."""
    r = _run_pass(SELECT_QIR)
    for inst in r.instructions:
        if _primary(inst.opcode) == OP_SELECT:
            dst_reg = inst.dst
            assert r.register_types[dst_reg] == REG_TYPE_I32, (
                f"select dst type is {r.register_types[dst_reg]}, "
                f"expected REG_TYPE_I32={REG_TYPE_I32}"
            )
            return
    pytest.fail("No OP_SELECT found")


def test_select_const_operands():
    """Select true/false values (i32 42 and 7)."""
    r = _run_pass(SELECT_QIR)
    for inst in r.instructions:
        if _primary(inst.opcode) == OP_SELECT:
            assert inst.opcode & FLAG_AUX0_IMM, "aux0 should be an immediate"
            assert inst.opcode & FLAG_AUX1_IMM, "aux1 should be an immediate"
            assert inst.aux0 == 42, "aux0 operand should be const 42"
            assert inst.aux1 == 7, "aux1 operand should be const 7"
            return
    pytest.fail("No OP_SELECT found")


# ---------------------------------------------------------------------------
# Test: Reset gate
# ---------------------------------------------------------------------------

RESET_QIR = """\
%Result = type opaque
%Qubit = type opaque

define void @ENTRYPOINT__main() #0 {
entry:
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__rt__tuple_record_output(i64 0, i8* null)
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__reset__body(%Qubit*)
declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "required_num_qubits"="1" "required_num_results"="0" }
"""


def test_reset_instruction():
    """Reset gate emits OP_RESET."""
    r = _run_pass(RESET_QIR)
    quantum_instrs = [
        inst for inst in r.instructions if _primary(inst.opcode) == OP_RESET
    ]
    assert len(quantum_instrs) == 1
    reset = quantum_instrs[0]
    assert reset.opcode & FLAG_AUX1_IMM, "Qubit argument should be static"
    assert reset.aux1 == 0


# ---------------------------------------------------------------------------
# Test: Dynamic qubit (inttoptr with non-constant → OP_MOV, no sentinel)
# ---------------------------------------------------------------------------

BELL_LOOP_QIR = """\
%Result = type opaque
%Qubit = type opaque

define i64 @ENTRYPOINT__main() #0 {
block_0:
  br label %loop_cond
loop_cond:                                        ; preds = %loop_body, %block_0
  %i = phi i64 [ 0, %block_0 ], [ %i_next, %loop_body ]
  %cond = icmp ult i64 %i, 8
  br i1 %cond, label %loop_body, label %loop_cond2
loop_body:                                        ; preds = %loop_cond
  %q0 = inttoptr i64 %i to %Qubit*
  %i1 = add i64 %i, 1
  %q1 = inttoptr i64 %i1 to %Qubit*
  call void @__quantum__qis__h__body(%Qubit* %q0)
  call void @__quantum__qis__cx__body(%Qubit* %q0, %Qubit* %q1)
  %i_next = add i64 %i, 2
  br label %loop_cond
loop_cond2:                                       ; preds = %loop_cond
  %i3 = phi i64 [ 0, %loop_cond ], [ %i_next2, %loop_body2 ]
  %cond2 = icmp ult i64 %i3, 16
  br i1 %cond2, label %loop_body2, label %end
loop_body2:                                       ; preds = %loop_cond2
  %q2 = inttoptr i64 %i3 to %Qubit*
  %r = inttoptr i64 %i3 to %Result*
  call void @__quantum__qis__mresetz__body(%Qubit* %q2, %Result* %r)
  %i_next2 = add i64 %i3, 1
  br label %loop_cond2
end:                                              ; preds = %loop_cond2
  call void @__quantum__rt__array_record_output(i64 8, i8* null)
  call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* null)
  call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 3 to %Result*), i8* null)
  call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 5 to %Result*), i8* null)
  call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 6 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 7 to %Result*), i8* null)
  call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 8 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 9 to %Result*), i8* null)
  call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 10 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 11 to %Result*), i8* null)
  call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 12 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 13 to %Result*), i8* null)
  call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 14 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 15 to %Result*), i8* null)
  ret i64 0
}

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

declare i1 @__quantum__rt__read_loss(%Result*)

declare i1 @__quantum__qis__read_result__body(%Result*)

declare void @__quantum__qis__z__body(%Qubit*)

declare void @__quantum__rt__array_record_output(i64, i8*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__result_record_output(%Result*, i8*)

declare void @__quantum__rt__bool_record_output(i1, i8*)
declare void @__quantum__rt__int_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="16" "required_num_results"="16" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
"""


def test_bell_loop_dynamic_qubit():
    """Dynamic qubits from inttoptr use register index (not DYN_QUBIT_SENTINEL) in aux1."""
    r = _run_pass(BELL_LOOP_QIR)
    # bell_loop has dynamic qubits via inttoptr of loop variable
    # At least one OP_QUANTUM_GATE should have aux1 != DYN_QUBIT_SENTINEL
    has_dynamic = False
    for inst in r.instructions:
        if _primary(inst.opcode) == OP_QUANTUM_GATE and not (
            OP_QUANTUM_GATE & FLAG_AUX1_IMM
        ):
            has_dynamic = True
            # The dynamic reg must be a valid register index
            assert (
                0 <= inst.aux1 < r.num_registers
            ), f"Invalid dynamic qubit register: {inst.aux1}"
    assert has_dynamic, "Expected at least one dynamic qubit gate in bell_loop"


def test_bell_loop_inttoptr_emits_mov():
    """inttoptr instructions in bell_loop emit OP_MOV (aliasing int to ptr register)."""
    r = _run_pass(BELL_LOOP_QIR)
    primaries = [_primary(inst.opcode) for inst in r.instructions]
    assert OP_MOV in primaries, "Missing OP_MOV for inttoptr"


# ---------------------------------------------------------------------------
# Test: Bell loop with IR-defined function calls
# ---------------------------------------------------------------------------

BELL_LOOP_FUNCS_QIR = """\
%Result = type opaque
%Qubit = type opaque

define i64 @ENTRYPOINT__main() #0 {
block_0:
  br label %loop_cond
loop_cond:                                        ; preds = %loop_body, %block_0
  %i = phi i64 [ 0, %block_0 ], [ %i_next, %loop_body ]
  %cond = icmp ult i64 %i, 8
  br i1 %cond, label %loop_body, label %loop_cond2
loop_body:                                        ; preds = %loop_cond
  %q0 = inttoptr i64 %i to %Qubit*
  %i1 = add i64 %i, 1
  %q1 = inttoptr i64 %i1 to %Qubit*
  call void @make_bell(%Qubit* %q0, %Qubit* %q1)
  %i_next = add i64 %i, 2
  br label %loop_cond
loop_cond2:                                       ; preds = %loop_cond
  %i3 = phi i64 [ 0, %loop_cond ], [ %i_next2, %loop_body2 ]
  %cond2 = icmp ult i64 %i3, 16
  br i1 %cond2, label %loop_body2, label %end
loop_body2:                                       ; preds = %loop_cond2
  %q2 = inttoptr i64 %i3 to %Qubit*
  %r = inttoptr i64 %i3 to %Result*
  call void @__quantum__qis__mresetz__body(%Qubit* %q2, %Result* %r)
  %i_next2 = add i64 %i3, 1
  br label %loop_cond2
end:                                              ; preds = %loop_cond2
  call void @__quantum__rt__array_record_output(i64 8, i8* null)
  call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* null)
  call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 3 to %Result*), i8* null)
  call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 4 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 5 to %Result*), i8* null)
  call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 6 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 7 to %Result*), i8* null)
  call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 8 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 9 to %Result*), i8* null)
  call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 10 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 11 to %Result*), i8* null)
  call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 12 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 13 to %Result*), i8* null)
  call void @__quantum__rt__tuple_record_output(i64 2, i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 14 to %Result*), i8* null)
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 15 to %Result*), i8* null)
  ret i64 0
}

define void @make_bell(%Qubit* %q0, %Qubit* %q1) {
  call void @__quantum__qis__h__body(%Qubit* %q0)
  call void @__quantum__qis__cx__body(%Qubit* %q0, %Qubit* %q1)
  ret void
}

declare void @__quantum__qis__x__body(%Qubit*)

declare void @__quantum__qis__h__body(%Qubit*)

declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

declare i1 @__quantum__rt__read_loss(%Result*)

declare i1 @__quantum__qis__read_result__body(%Result*)

declare void @__quantum__qis__z__body(%Qubit*)

declare void @__quantum__rt__array_record_output(i64, i8*)

declare void @__quantum__rt__tuple_record_output(i64, i8*)

declare void @__quantum__rt__result_record_output(%Result*, i8*)

declare void @__quantum__rt__bool_record_output(i1, i8*)
declare void @__quantum__rt__int_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="16" "required_num_results"="16" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
"""


def test_bell_loop_funcs_structure():
    """Bell loop with make_bell function: 7 blocks (6 main + 1 make_bell), 1 function."""
    r = _run_pass(BELL_LOOP_FUNCS_QIR)
    assert r.num_qubits == 16
    assert r.num_results == 16
    assert r.entry_block == 0
    assert len(r.blocks) == 7  # 6 in main + 1 in make_bell
    assert len(r.functions) == 1  # make_bell


def test_bell_loop_funcs_function_entry():
    """The make_bell function table entry has correct param count and entry block."""
    r = _run_pass(BELL_LOOP_FUNCS_QIR)
    func = r.functions[0]  # (entry_block, num_params, param_base, reserved)
    entry_block, num_params, param_base = astuple(func)
    assert num_params == 2, "make_bell takes 2 params (%Qubit*, %Qubit*)"
    # entry_block should be a valid block ID
    valid_block_ids = {b.block_id for b in r.blocks}
    assert (
        entry_block in valid_block_ids
    ), f"Function entry block {entry_block} not found"


def test_bell_loop_funcs_call_instruction():
    """An OP_CALL instruction is emitted for the make_bell call."""
    r = _run_pass(BELL_LOOP_FUNCS_QIR)
    call_instrs = [inst for inst in r.instructions if _primary(inst.opcode) == OP_CALL]
    assert len(call_instrs) >= 1, "Expected at least one OP_CALL for make_bell"
    # The call should reference function 0 (make_bell) via aux0
    call = call_instrs[0]
    assert call.aux0 == 0, f"OP_CALL aux0 (func_id) should be 0, got {call.aux0}"
    # aux1 = num_args (2 qubit pointers)
    assert call.aux1 == 2, f"OP_CALL aux1 (num_args) should be 2, got {call.aux1}"


def test_bell_loop_funcs_call_args():
    """call_args contains resolved registers for the make_bell call arguments."""
    r = _run_pass(BELL_LOOP_FUNCS_QIR)
    assert (
        len(r.call_args) >= 2
    ), "Expected at least 2 call args for make_bell(%q0, %q1)"
    call_instrs = [inst for inst in r.instructions if _primary(inst.opcode) == OP_CALL]
    call = call_instrs[0]
    num_args = call.aux1  # aux1 = num_args
    arg_offset = call.aux2  # aux2 = arg_offset into call_args
    args = r.call_args[arg_offset : arg_offset + num_args]
    assert len(args) == 2, f"Expected 2 call args, got {len(args)}"
    # Both args should be valid register indices
    for a in args:
        assert 0 <= a < r.num_registers, f"Invalid call arg register: {a}"


def test_bell_loop_funcs_call_return():
    """The make_bell function body ends with OP_CALL_RETURN."""
    r = _run_pass(BELL_LOOP_FUNCS_QIR)
    # Find the make_bell function's entry block
    func_entry_block = r.functions[0].func_entry_block
    # Find the block tuple for that block
    func_block = next(b for b in r.blocks if b.block_id == func_entry_block)
    offset, count = func_block.instr_offset, func_block.instr_count
    last_instr = r.instructions[offset + count - 1]
    assert (
        _primary(last_instr.opcode) == OP_CALL_RETURN
    ), f"make_bell should end with OP_CALL_RETURN, got {_primary(last_instr.opcode):#x}"


def test_bell_loop_funcs_quantum_ops():
    """H and CX are emitted from make_bell; MResetZ from main."""
    r = _run_pass(BELL_LOOP_FUNCS_QIR)
    op_ids = [q.op_id for q in r.quantum_ops]
    assert 5 in op_ids, "Missing H gate"
    assert 15 in op_ids, "Missing CNOT gate"
    assert 22 in op_ids, "Missing MResetZ gate"


def test_bell_loop_funcs_param_registers():
    """make_bell params get allocated registers with PTR type tag."""
    r = _run_pass(BELL_LOOP_FUNCS_QIR)
    func = r.functions[0]
    _, num_params, param_base = astuple(func)
    for i in range(num_params):
        reg = param_base + i
        assert (
            r.register_types[reg] == REG_TYPE_PTR
        ), f"Param register {reg} type is {r.register_types[reg]}, expected REG_TYPE_PTR"


# ---------------------------------------------------------------------------
# Test: Bell loop from file (integration test with real QIR)
# ---------------------------------------------------------------------------


def test_bell_loop_structure():
    """Bell loop has 6 blocks, 16 qubits, 16 results, 2 phi nodes."""
    r = _run_pass(BELL_LOOP_QIR)
    assert r.num_qubits == 16
    assert r.num_results == 16
    assert r.entry_block == 0
    assert len(r.blocks) == 6
    assert len(r.phi_entries) == 4  # 2 phi nodes * 2 incoming each


def test_bell_loop_phi_block_refs():
    """Phi entries reference valid block IDs within the program."""
    r = _run_pass(BELL_LOOP_QIR)
    valid_block_ids = {b.block_id for b in r.blocks}
    for pe in r.phi_entries:
        block_id, val_reg = pe.block_id, pe.val_reg
        assert block_id in valid_block_ids, f"Invalid phi block_id: {block_id}"
        assert 0 <= val_reg < r.num_registers, f"Invalid phi val_reg: {val_reg}"


def test_bell_loop_phi_i64_types():
    """Both phi destination registers have i64 type tag."""
    r = _run_pass(BELL_LOOP_QIR)
    phi_dst_regs = []
    for inst in r.instructions:
        if _primary(inst.opcode) == OP_PHI:
            phi_dst_regs.append(inst.dst)
    assert len(phi_dst_regs) == 2
    for reg in phi_dst_regs:
        assert (
            r.register_types[reg] == REG_TYPE_I64
        ), f"Phi dst reg {reg} type is {r.register_types[reg]}, expected REG_TYPE_I64={REG_TYPE_I64}"


def test_bell_loop_quantum_ops():
    """Bell loop emits H, CNOT, and MResetZ quantum operations."""
    r = _run_pass(BELL_LOOP_QIR)
    op_ids = [q.op_id for q in r.quantum_ops]
    assert 5 in op_ids, "Missing H gate"
    assert 15 in op_ids, "Missing CNOT gate"
    assert 22 in op_ids, "Missing MResetZ gate"


def test_bell_loop_forward_ref_consistency():
    """Forward-referenced phi values share registers with their definitions."""
    r = _run_pass(BELL_LOOP_QIR)
    # Find all ADD instructions and their dst registers
    add_dsts = set()
    for inst in r.instructions:
        if _primary(inst.opcode) == OP_ADD:
            add_dsts.add(inst.dst)

    # The phi entries from back-edge blocks should reference ADD dst registers
    # Block 2 = loop_body (back-edge for phi %i), block 4 = loop_body2 (back-edge for phi %i3)
    back_edge_regs = set()
    for pe in r.phi_entries:
        if pe.block_id in (2, 4):  # back-edge blocks
            back_edge_regs.add(pe.val_reg)

    assert back_edge_regs.issubset(
        add_dsts
    ), f"Phi back-edge registers {back_edge_regs} not in ADD dsts {add_dsts}"


def test_bell_loop_block_offset_consistency():
    """Block instruction offsets are contiguous and cover all instructions."""
    r = _run_pass(BELL_LOOP_QIR)
    total = 0
    for b in r.blocks:
        offset, count = b.instr_offset, b.instr_count
        assert offset == total, f"Block offset {offset} != expected {total}"
        total += count
    assert total == len(r.instructions)


# ---------------------------------------------------------------------------
# Test: Output dict schema completeness
# ---------------------------------------------------------------------------


def test_output_schema_keys():
    """Output dict contains all expected keys."""
    r = _run_pass(LINEAR_QIR)
    expected_keys = {
        "num_qubits",
        "num_results",
        "num_registers",
        "entry_block",
        "blocks",
        "instructions",
        "quantum_ops",
        "functions",
        "phi_entries",
        "switch_cases",
        "call_args",
        "labels",
        "register_types",
    }
    assert set(r.as_dict().keys()) == expected_keys


def test_instruction_tuple_length():
    """All instructions are 8-tuples."""
    r = _run_pass(LINEAR_QIR)
    for i, inst in enumerate(r.instructions):
        assert (
            len(astuple(inst)) == 8
        ), f"Instruction {i} has {len(astuple(inst))} fields, expected 8"


def test_quantum_op_tuple_length():
    """All quantum ops are 5-tuples."""
    r = _run_pass(LINEAR_QIR)
    for i, qop in enumerate(r.quantum_ops):
        assert (
            len(astuple(qop)) == 5
        ), f"Quantum op {i} has {len(astuple(qop))} fields, expected 5"
