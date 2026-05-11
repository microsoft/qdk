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

from qdk._adaptive_pass import AdaptiveProfilePass, AdaptiveProgram, Bytecode
from qdk._adaptive_bytecode import *

_HAS_GLOBAL_VARIABLES = hasattr(
    pyqir.Module(pyqir.Context(), "probe"), "global_variables"
)
_skip_no_globals = pytest.mark.skipif(
    not _HAS_GLOBAL_VARIABLES,
    reason="pyqir Module lacks global_variables support",
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _run_pass(ir: str, name: str = "test.ll") -> AdaptiveProgram:
    """Parse an LLVM IR string and run through AdaptiveProfilePass."""
    mod = pyqir.Module.from_ir(pyqir.Context(), ir, name)
    return AdaptiveProfilePass(Bytecode.Bit32).run(mod)


def _primary(opcode_word: int) -> int:
    """Extract primary opcode from opcode word."""
    return opcode_word & 0xFF


def _sub(opcode_word: int) -> int:
    """Extract sub-opcode from opcode word."""
    return (opcode_word >> 8) & 0xFF


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

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="1" }
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

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
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

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
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

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
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
        "constant_data",
        "memory_size",
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


# ---------------------------------------------------------------------------
# Test: Barrier instruction
# ---------------------------------------------------------------------------


BARRIER_QIR = r"""
%Result = type opaque
%Qubit = type opaque

@0 = internal constant [4 x i8] c"0_t\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__qis__barrier__body()
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)
declare void @__quantum__qis__barrier__body()
declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
attributes #1 = { "irreversible" }

!llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
!5 = !{i32 5, !"float_computations", !{!"double"}}

"""


def test_pass_on_qir_with_barrier_instruction_succeeds():
    _run_pass(BARRIER_QIR)


# ---------------------------------------------------------------------------
# Test: Arrays
# ---------------------------------------------------------------------------


ADAPTIVE_RIFLA_QIR = r"""
%Result = type opaque
%Qubit = type opaque

@0 = internal constant [4 x i8] c"0_t\00"

define i64 @ENTRYPOINT__main() #0 {
block_0:
  call void @__quantum__rt__initialize(i8* null)
  call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
  ret i64 0
}

declare void @__quantum__rt__initialize(i8*)
declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

!0 = !{i32 1, !"qir_major_version", i32 1}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
!5 = !{i32 5, !"float_computations", !{!"double"}}
!6 = !{i32 7, !"backwards_branching", i2 3}
!7 = !{i32 1, !"arrays", i1 true}
"""


def test_arrays_flag_accepted():
    """Modules with 'arrays' flag are accepted (no longer raise ValueError)."""
    r = _run_pass(ADAPTIVE_RIFLA_QIR)
    assert r.num_qubits == 0
    assert r.num_results == 0


# ---------------------------------------------------------------------------
# Test: Memory operations (alloca, load, store, GEP)
# ---------------------------------------------------------------------------

ALLOCA_QIR = """\
%Result = type opaque
%Qubit = type opaque

define void @ENTRYPOINT__main() #0 {
entry:
  %ptr = alloca i64, align 8
  store i64 42, i64* %ptr, align 8
  %val = load i64, i64* %ptr, align 8
  call void @__quantum__rt__tuple_record_output(i64 0, i8* null)
  ret void
}

declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
"""


def test_alloca_instruction_emitted():
    """ALLOCA instruction emits OP_ALLOCA in the instruction stream."""
    r = _run_pass(ALLOCA_QIR)
    primaries = [_primary(inst.opcode) for inst in r.instructions]
    assert OP_ALLOCA in primaries, "Missing OP_ALLOCA"


def test_load_instruction_emitted():
    """LOAD instruction emits OP_LOAD in the instruction stream."""
    r = _run_pass(ALLOCA_QIR)
    primaries = [_primary(inst.opcode) for inst in r.instructions]
    assert OP_LOAD in primaries, "Missing OP_LOAD"


def test_store_instruction_emitted():
    """STORE instruction emits OP_STORE in the instruction stream."""
    r = _run_pass(ALLOCA_QIR)
    primaries = [_primary(inst.opcode) for inst in r.instructions]
    assert OP_STORE in primaries, "Missing OP_STORE"


# ---------------------------------------------------------------------------
# Test: Static constant arrays (constant_data population)
# ---------------------------------------------------------------------------

STATIC_ARRAY_QIR = """\
%Result = type opaque
%Qubit = type opaque

@array0 = internal constant [3 x i64] [i64 10, i64 20, i64 30]

define void @ENTRYPOINT__main() #0 {
entry:
  %ptr = getelementptr inbounds [3 x i64], [3 x i64]* @array0, i64 0, i64 1
  %val = load i64, i64* %ptr, align 8
  call void @__quantum__rt__tuple_record_output(i64 0, i8* null)
  ret void
}

declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0}
!0 = !{i32 1, !"arrays", i1 true}
"""


def test_static_array_constant_data():
    """Global constant array populates constant_data with correct values."""
    r = _run_pass(STATIC_ARRAY_QIR)
    assert len(r.constant_data) == 3
    assert r.constant_data == [10, 20, 30]


def test_gep_instruction_emitted():
    """GEP instruction emits OP_GEP in the instruction stream."""
    r = _run_pass(STATIC_ARRAY_QIR)
    primaries = [_primary(inst.opcode) for inst in r.instructions]
    assert OP_GEP in primaries, "Missing OP_GEP"


# ---------------------------------------------------------------------------
# Test: Aggregate alloca rejected
# ---------------------------------------------------------------------------

ARRAY_ALLOCA_QIR = """\
define void @ENTRYPOINT__main() #0 {
entry:
  %big = alloca [300 x i64]
  ret void
}

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0}
!0 = !{i32 1, !"arrays", i1 true}
"""

STRUCT_ALLOCA_QIR = """\
define void @ENTRYPOINT__main() #0 {
entry:
  %s = alloca { i64, i64 }
  ret void
}

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
"""


def test_array_alloca_rejected():
    """Alloca of an array type is rejected to prevent silent undersizing."""
    with pytest.raises(NotImplementedError, match="Aggregate stack allocations"):
        _run_pass(ARRAY_ALLOCA_QIR)


def test_struct_alloca_rejected():
    """Alloca of a struct type is rejected to prevent silent undersizing."""
    with pytest.raises(NotImplementedError, match="Aggregate stack allocations"):
        _run_pass(STRUCT_ALLOCA_QIR)


# ---------------------------------------------------------------------------
# Test: Multiple constant arrays (offset correctness)
# ---------------------------------------------------------------------------

MULTI_ARRAY_QIR = """\
%Result = type opaque
%Qubit = type opaque

@a = internal constant [2 x i64] [i64 1, i64 2]
@b = internal constant [3 x i64] [i64 3, i64 4, i64 5]

define void @ENTRYPOINT__main() #0 {
entry:
  %ptr_a = getelementptr inbounds [2 x i64], [2 x i64]* @a, i64 0, i64 0
  %ptr_b = getelementptr inbounds [3 x i64], [3 x i64]* @b, i64 0, i64 0
  %va = load i64, i64* %ptr_a, align 8
  %vb = load i64, i64* %ptr_b, align 8
  call void @__quantum__rt__tuple_record_output(i64 0, i8* null)
  ret void
}

declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }

!llvm.module.flags = !{!0}
!0 = !{i32 1, !"arrays", i1 true}
"""


def test_multiple_constant_arrays():
    """Multiple global arrays produce concatenated constant_data with correct offsets."""
    r = _run_pass(MULTI_ARRAY_QIR)
    assert len(r.constant_data) == 5
    assert r.constant_data == [1, 2, 3, 4, 5]


ARRAY_OF_POINTERS_QIR = r"""
@0 = internal constant [4 x i8] c"0_a\00"
@1 = internal constant [6 x i8] c"1_a0r\00"
@2 = internal constant [6 x i8] c"2_a1r\00"
@array0 = internal constant [2 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr)]

define i64 @ENTRYPOINT__main() #0 {
block_0:
  %var_1 = alloca i64
  call void @__quantum__rt__initialize(ptr null)
  store i64 0, ptr %var_1
  br label %block_1
block_1:
  %var_7 = load i64, ptr %var_1
  %var_2 = icmp slt i64 %var_7, 2
  br i1 %var_2, label %block_2, label %block_3
block_2:
  %var_8 = load i64, ptr %var_1
  %var_3 = getelementptr ptr, ptr @array0, i64 %var_8
  %var_9 = load ptr, ptr %var_3
  call void @__quantum__qis__reset__body(ptr %var_9)
  %var_5 = add i64 %var_8, 1
  store i64 %var_5, ptr %var_1
  br label %block_1
block_3:
  call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
  call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
  call void @__quantum__rt__array_record_output(i64 2, ptr @0)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
  ret i64 0
}

declare void @__quantum__rt__initialize(ptr)
declare void @__quantum__qis__reset__body(ptr) #1
declare void @__quantum__qis__h__body(ptr)
declare void @__quantum__qis__m__body(ptr, ptr) #1
declare void @__quantum__rt__array_record_output(i64, ptr)
declare void @__quantum__rt__result_record_output(ptr, ptr)

attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
attributes #1 = { "irreversible" }

; module flags

!llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

!0 = !{i32 1, !"qir_major_version", i32 2}
!1 = !{i32 7, !"qir_minor_version", i32 1}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
!5 = !{i32 5, !"float_computations", !{!"double"}}
!6 = !{i32 7, !"backwards_branching", i2 3}
!7 = !{i32 1, !"arrays", i1 true}
"""


def test_array_of_pointers():
    """Pointer array ([N x ptr]) elements are resolved to constant_data."""
    r = _run_pass(ARRAY_OF_POINTERS_QIR)
    # @array0 = [2 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr)]
    assert r.constant_data == [0, 1]


# ---------------------------------------------------------------------------
# Test: i32 constant array
# ---------------------------------------------------------------------------

I32_ARRAY_QIR = """\
%Result = type opaque
%Qubit = type opaque

@ints = internal constant [3 x i32] [i32 10, i32 20, i32 30]

define void @ENTRYPOINT__main() #0 {
entry:
  %ptr = getelementptr inbounds [3 x i32], [3 x i32]* @ints, i64 0, i64 0
  %val = load i32, i32* %ptr, align 4
  call void @__quantum__rt__tuple_record_output(i64 0, i8* null)
  ret void
}

declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
"""


def test_i32_array_constant_data():
    """i32 constant array populates constant_data with correct values."""
    r = _run_pass(I32_ARRAY_QIR)
    assert r.constant_data == [10, 20, 30]


# ---------------------------------------------------------------------------
# Test: double (f64) constant array
# ---------------------------------------------------------------------------

DOUBLE_ARRAY_QIR = """\
%Result = type opaque
%Qubit = type opaque

@angles = internal constant [2 x double] [double 1.5, double 2.75]

define void @ENTRYPOINT__main() #0 {
entry:
  %ptr = getelementptr inbounds [2 x double], [2 x double]* @angles, i64 0, i64 0
  %val = load double, double* %ptr, align 8
  call void @__quantum__rt__tuple_record_output(i64 0, i8* null)
  ret void
}

declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
"""


def test_double_array_constant_data():
    """double constant array stores IEEE-754 f32 bit patterns in 32-bit mode."""
    r = _run_pass(DOUBLE_ARRAY_QIR)
    assert len(r.constant_data) == 2
    # 1.5 as f32 bits = 0x3fc00000 = 1069547520
    # 2.75 as f32 bits = 0x40300000 = 1076887552
    assert r.constant_data == [1069547520, 1076887552]


# ---------------------------------------------------------------------------
# Test: nested array (2D matrix) [2 x [3 x i64]]
# ---------------------------------------------------------------------------

NESTED_ARRAY_QIR = """\
%Result = type opaque
%Qubit = type opaque

@matrix = internal constant [2 x [3 x i64]] [
  [3 x i64] [i64 1, i64 2, i64 3],
  [3 x i64] [i64 4, i64 5, i64 6]
]

define void @ENTRYPOINT__main() #0 {
entry:
  %ptr = getelementptr inbounds [2 x [3 x i64]], [2 x [3 x i64]]* @matrix, i64 0, i64 0, i64 0
  %val = load i64, i64* %ptr, align 8
  call void @__quantum__rt__tuple_record_output(i64 0, i8* null)
  ret void
}

declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
"""


def test_nested_array_constant_data():
    """Nested array [2 x [3 x i64]] flattens to row-major constant_data."""
    r = _run_pass(NESTED_ARRAY_QIR)
    assert r.constant_data == [1, 2, 3, 4, 5, 6]


# ---------------------------------------------------------------------------
# Test: array of pointers to row arrays (GlobalVariable references)
# ---------------------------------------------------------------------------

PTR_TO_ROWS_QIR = """\
%Result = type opaque
%Qubit = type opaque

@row0 = internal constant [3 x i64] [i64 10, i64 20, i64 30]
@row1 = internal constant [3 x i64] [i64 40, i64 50, i64 60]
@matrix = internal constant [2 x ptr] [ptr @row0, ptr @row1]

define void @ENTRYPOINT__main() #0 {
entry:
  %row_ptr = getelementptr inbounds [2 x ptr], ptr @matrix, i64 0, i64 0
  %row = load ptr, ptr %row_ptr, align 8
  %elem_ptr = getelementptr inbounds [3 x i64], ptr %row, i64 0, i64 1
  %val = load i64, ptr %elem_ptr, align 8
  call void @__quantum__rt__tuple_record_output(i64 0, i8* null)
  ret void
}

declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
"""


def test_ptr_to_rows_constant_data():
    """Array of pointers to row arrays encodes rows then pointer table."""
    r = _run_pass(PTR_TO_ROWS_QIR)
    # @row0 at offset 0: [10, 20, 30]
    # @row1 at offset 3: [40, 50, 60]
    # @matrix at offset 6: [0, 3] (addresses of @row0 and @row1)
    assert r.constant_data == [10, 20, 30, 40, 50, 60, 0, 3]


def test_ptr_to_rows_memory_size():
    """Memory size accounts for all constant data from pointer-to-row arrays."""
    r = _run_pass(PTR_TO_ROWS_QIR)
    assert r.memory_size == 8  # 3 + 3 + 2


# ---------------------------------------------------------------------------
# Test: byte-string globals are skipped (used as output labels)
# ---------------------------------------------------------------------------

BYTE_STRING_GLOBALS_QIR = """\
%Result = type opaque
%Qubit = type opaque

@label = internal constant [4 x i8] c"0_r\\00"
@data = internal constant [2 x i64] [i64 42, i64 99]

define void @ENTRYPOINT__main() #0 {
entry:
  %ptr = getelementptr inbounds [2 x i64], [2 x i64]* @data, i64 0, i64 0
  %val = load i64, i64* %ptr, align 8
  call void @__quantum__rt__tuple_record_output(i64 0, i8* null)
  ret void
}

declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
"""


def test_byte_string_globals_skipped():
    """[N x i8] byte-string globals are skipped; only data arrays contribute."""
    r = _run_pass(BYTE_STRING_GLOBALS_QIR)
    assert r.constant_data == [42, 99]


# ---------------------------------------------------------------------------
# Test: mutable global array still works
# ---------------------------------------------------------------------------

MUTABLE_GLOBAL_QIR = """\
%Result = type opaque
%Qubit = type opaque

@buf = global [3 x i64] [i64 100, i64 200, i64 300]

define void @ENTRYPOINT__main() #0 {
entry:
  %ptr = getelementptr inbounds [3 x i64], [3 x i64]* @buf, i64 0, i64 0
  %val = load i64, i64* %ptr, align 8
  call void @__quantum__rt__tuple_record_output(i64 0, i8* null)
  ret void
}

declare void @__quantum__rt__tuple_record_output(i64, i8*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
"""


def test_mutable_global_array():
    """Mutable (non-constant) global arrays are still encoded in constant_data."""
    r = _run_pass(MUTABLE_GLOBAL_QIR)
    assert r.constant_data == [100, 200, 300]
