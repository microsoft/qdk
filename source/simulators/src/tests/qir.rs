// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Bytecode programs for the tests of [`crate::qir`], written block by
//! block.

mod gate;
mod machine;
mod operator_matrix;
mod program;
mod resumer;

use crate::bytecode::encoding::{
    FLAG_AUX1_IMM, FLAG_AUX2_IMM, FLAG_DST_IMM, FLAG_SRC0_IMM, FLAG_SRC1_IMM, OP_BRANCH, OP_CALL,
    OP_CALL_RETURN, OP_JUMP, OP_MEASURE, OP_MOV, OP_PHI, OP_QUANTUM_GATE, OP_RECORD_OUTPUT,
    OP_RESET, OP_RET, OP_SWITCH, OP_WRITE_RESULT, OPID_CORRELATED_NOISE, OPID_RESETZ,
};
use crate::bytecode::{AdaptiveProgram, Block, Instruction, Op};
use crate::qir::{QirError, QirFault, QirFaultSite, QirProgram};

/// The qubits, results and registers of the programs [`program`] builds.
pub(super) const SIZE: u32 = 4;

/// The operation ids [`program`] gives an operation for: those of the
/// bytecode, correlated noise (131) the last.
const OPERATIONS: u64 = 132;

/// A program of `blocks`, laid out in their order, the first its entry
/// block; with [`SIZE`] qubits, results and registers; and one quantum
/// operation for each operation id, at the index of its id, so that an
/// instruction names an operation by its id.
pub(super) fn program(blocks: &[&[Instruction<u64>]]) -> AdaptiveProgram<u64> {
    let mut instructions = Vec::new();
    let mut block_table = Vec::new();
    for block in blocks {
        block_table.push(Block {
            instr_offset: instructions.len() as u64,
            instr_count: block.len() as u64,
        });
        instructions.extend_from_slice(block);
    }
    AdaptiveProgram {
        num_qubits: SIZE,
        num_results: SIZE,
        num_registers: SIZE,
        entry_block: 0,
        instructions,
        block_table,
        function_table: Vec::new(),
        phi_entries: Vec::new(),
        switch_cases: Vec::new(),
        call_args: Vec::new(),
        constant_data: Vec::new(),
        quantum_ops: (0..OPERATIONS)
            .map(|op_id| Op::from_tuple((op_id, 0, 0, 0, 0)))
            .collect(),
    }
}

/// An instruction of `opcode` whose fields are all zero and, but for those
/// `flags` marks constant, registers.
pub(super) fn op(opcode: u8, flags: u64) -> Instruction<u64> {
    Instruction {
        opcode: u64::from(opcode) | flags,
        ..Instruction::default()
    }
}

/// The program's return, with exit code 0.
pub(super) fn ret() -> Instruction<u64> {
    op(OP_RET, FLAG_DST_IMM)
}

/// A jump to `block`.
fn jump(block: u64) -> Instruction<u64> {
    Instruction {
        dst: block,
        ..op(OP_JUMP, 0)
    }
}

/// A branch to `taken` when register `condition` holds 1, else to
/// `other`.
pub(super) fn branch(condition: u64, taken: u64, other: u64) -> Instruction<u64> {
    Instruction {
        src0: condition,
        aux0: taken,
        aux1: other,
        ..op(OP_BRANCH, 0)
    }
}

/// The gate of the operation at `operation` in the operation table (in
/// [`program`]'s, the operation of that id) on the constant qubits `q1` and
/// `q2` (`q2` unused by a one-qubit gate), with the angle 0.
fn gate(operation: u64, q1: u64, q2: u64) -> Instruction<u64> {
    Instruction {
        aux0: operation,
        aux1: q1,
        aux2: q2,
        ..op(
            OP_QUANTUM_GATE,
            FLAG_SRC0_IMM | FLAG_AUX1_IMM | FLAG_AUX2_IMM,
        )
    }
}

/// Correlated noise (in [`program`]'s operation table, from noise table 0)
/// on the qubits held in the registers of the constant `count` call
/// arguments from the constant `offset`.
fn correlated_noise(count: u64, offset: u64) -> Instruction<u64> {
    Instruction {
        aux0: OPID_CORRELATED_NOISE,
        aux1: count,
        aux2: offset,
        ..op(OP_QUANTUM_GATE, FLAG_AUX1_IMM | FLAG_AUX2_IMM)
    }
}

/// `bytecode`, a program of [`program`], with its correlated noise drawn
/// from noise table `table`.
fn with_noise_table(bytecode: &mut AdaptiveProgram<u64>, table: u64) {
    bytecode.quantum_ops[usize::try_from(OPID_CORRELATED_NOISE).expect("an index")].q1 = table;
}

/// The measurement of operation id `op_id` (`mz` or `mresetz`) of the
/// constant `qubit` into the constant `result`.
fn measure(op_id: u64, qubit: u64, result: u64) -> Instruction<u64> {
    Instruction {
        aux0: op_id,
        aux1: qubit,
        aux2: result,
        ..op(OP_MEASURE, FLAG_AUX1_IMM | FLAG_AUX2_IMM)
    }
}

/// A switch on register `value`, to `default` unless one of `count` cases
/// from `offset` matches.
fn switch(value: u64, default: u64, offset: u64, count: u64) -> Instruction<u64> {
    Instruction {
        src0: value,
        aux0: default,
        aux1: offset,
        aux2: count,
        ..op(OP_SWITCH, 0)
    }
}

/// The reset of the constant `qubit`.
fn reset(qubit: u64) -> Instruction<u64> {
    Instruction {
        aux0: OPID_RESETZ,
        aux1: qubit,
        ..op(OP_RESET, FLAG_AUX1_IMM)
    }
}

/// A call of `function` with `count` arguments from `offset` of the call
/// arguments, its value kept in register `kept` (or dropped:
/// `VOID_RETURN`).
fn call(function: u64, count: u64, offset: u64, kept: u64) -> Instruction<u64> {
    Instruction {
        dst: kept,
        aux0: function,
        aux1: count,
        aux2: offset,
        ..op(OP_CALL, 0)
    }
}

/// A return from a function, with the value of register `value`.
fn call_return(value: u64) -> Instruction<u64> {
    Instruction {
        src0: value,
        ..op(OP_CALL_RETURN, 0)
    }
}

/// An instruction of `opcode` reading `src0` and `src1` (constants when
/// `flags` says so) and writing register `dst`.
pub(super) fn binary(opcode: u8, flags: u64, dst: u64, src0: u64, src1: u64) -> Instruction<u64> {
    Instruction {
        dst,
        src0,
        src1,
        ..op(opcode, flags)
    }
}

/// A comparison of `opcode` (`ICMP` or `FCMP`) by `predicate`.
fn compare(opcode: u8, predicate: u8) -> Instruction<u64> {
    Instruction {
        opcode: u64::from(opcode) | (u64::from(predicate) << 8),
        ..Instruction::default()
    }
}

/// Register `dst` set to the constant `value`.
fn set(dst: u64, value: u64) -> Instruction<u64> {
    binary(OP_MOV, FLAG_SRC0_IMM, dst, value, 0)
}

/// An instruction of `opcode` (`READ_RESULT`, `READ_LOSS`) reading the
/// constant `result` into register `dst`.
fn read(opcode: u8, dst: u64, result: u64) -> Instruction<u64> {
    binary(opcode, FLAG_SRC0_IMM, dst, result, 0)
}

/// The constant `value` written to the constant `result`.
fn write(value: u64, result: u64) -> Instruction<u64> {
    binary(
        OP_WRITE_RESULT,
        FLAG_SRC0_IMM | FLAG_SRC1_IMM,
        0,
        value,
        result,
    )
}

/// The output of the constant `result` recorded.
fn record_result(result: u64) -> Instruction<u64> {
    binary(OP_RECORD_OUTPUT, FLAG_SRC0_IMM, 0, result, 0)
}

/// Register `value` recorded as a record of `kind`: 3 a `Bool`, 4 an `Int`.
fn record(kind: u64, value: u64) -> Instruction<u64> {
    Instruction {
        src0: value,
        aux1: kind,
        ..op(OP_RECORD_OUTPUT, 0)
    }
}

/// Register `value` recorded as a `Bool`.
fn record_bool(value: u64) -> Instruction<u64> {
    record(3, value)
}

/// Register `value` recorded as an `Int`.
fn record_int(value: u64) -> Instruction<u64> {
    record(4, value)
}

/// A `PHI` into register `dst` of `count` entries from `offset`.
fn phi(dst: u64, offset: u64, count: u64) -> Instruction<u64> {
    Instruction {
        dst,
        aux0: offset,
        aux1: count,
        ..op(OP_PHI, 0)
    }
}

/// A copy of `bytecode`, for a test that builds a program of it
/// (`QirProgram::new` takes its bytecode) and reads it after.
pub(super) fn copy_of(bytecode: &AdaptiveProgram<u64>) -> AdaptiveProgram<u64> {
    AdaptiveProgram {
        num_qubits: bytecode.num_qubits,
        num_results: bytecode.num_results,
        num_registers: bytecode.num_registers,
        entry_block: bytecode.entry_block,
        instructions: bytecode.instructions.clone(),
        block_table: bytecode.block_table.clone(),
        function_table: bytecode.function_table.clone(),
        phi_entries: bytecode.phi_entries.clone(),
        switch_cases: bytecode.switch_cases.clone(),
        call_args: bytecode.call_args.clone(),
        constant_data: bytecode.constant_data.clone(),
        quantum_ops: bytecode
            .quantum_ops
            .iter()
            .map(|op| Op {
                op_id: op.op_id,
                q1: op.q1,
                q2: op.q2,
                q3: op.q3,
                angle: op.angle,
            })
            .collect(),
    }
}

/// The faults building `bytecode` reports.
fn faults(bytecode: &AdaptiveProgram<u64>) -> Vec<QirFault> {
    QirProgram::new(copy_of(bytecode))
        .expect_err("the program has a fault")
        .0
}

/// The fault `error` at the instruction at `index`.
fn at_instruction(index: usize, error: QirError) -> QirFault {
    QirFault {
        at: QirFaultSite::Instruction(index),
        error,
    }
}
