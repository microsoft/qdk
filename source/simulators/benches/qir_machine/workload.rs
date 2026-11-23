// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The bytecode the measurements are written in: a program of blocks
//! ([`program`]), and its instructions.

use qdk_simulators::bytecode::{AdaptiveProgram, Block, Instruction, Op};

use crate::encoding::{
    FLAG_AUX1_IMM, FLAG_AUX2_IMM, FLAG_DST_IMM, FLAG_SRC0_IMM, FLAG_SRC1_IMM, ICMP_SLT, OP_ADD,
    OP_BRANCH, OP_ICMP, OP_JUMP, OP_MEASURE, OP_MOV, OP_RECORD_OUTPUT, OP_RET,
};

/// A program of `blocks`, the first its entry block, on 3 qubits, 1 result
/// and 4 registers, with an operation for each operation id below 32 at
/// the index of its id.
pub(crate) fn program(blocks: &[&[Instruction<u64>]]) -> AdaptiveProgram<u64> {
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
        num_qubits: 3,
        num_results: 1,
        num_registers: 4,
        entry_block: 0,
        instructions,
        block_table,
        function_table: vec![Default::default()],
        phi_entries: vec![Default::default()],
        switch_cases: vec![Default::default()],
        call_args: vec![0],
        constant_data: vec![0],
        quantum_ops: (0..32)
            .map(|op_id| Op::from_tuple((op_id, 0, 0, 0, 0)))
            .collect(),
    }
}

/// A copy of `bytecode`, for a measurement that builds a program of it
/// (`QirProgram::new` takes its bytecode) and runs `run_shot` on it too.
pub(crate) fn copy_of(bytecode: &AdaptiveProgram<u64>) -> AdaptiveProgram<u64> {
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

/// Whether `bytecode` records any output; when it does not, a shot's
/// records are every result.
pub(crate) fn records_output(bytecode: &AdaptiveProgram<u64>) -> bool {
    bytecode
        .instructions
        .iter()
        .any(|instruction| instruction.primary_opcode() == OP_RECORD_OUTPUT)
}

/// An instruction of `opcode` with `flags`, its fields zero.
pub(crate) fn op(opcode: u8, flags: u64) -> Instruction<u64> {
    Instruction {
        opcode: u64::from(opcode) | flags,
        ..Instruction::default()
    }
}

/// The program's return.
pub(crate) fn ret() -> Instruction<u64> {
    op(OP_RET, FLAG_DST_IMM)
}

/// A jump to `block`.
pub(crate) fn jump(block: u64) -> Instruction<u64> {
    Instruction {
        dst: block,
        ..op(OP_JUMP, 0)
    }
}

/// Register `dst` set to the constant `value`.
pub(crate) fn set(dst: u64, value: u64) -> Instruction<u64> {
    Instruction {
        dst,
        src0: value,
        ..op(OP_MOV, FLAG_SRC0_IMM)
    }
}

/// A measurement in the Z basis (`OPID_MZ`) of the constant `qubit` into
/// the constant `result`.
pub(crate) fn measure_z(qubit: u64, result: u64) -> Instruction<u64> {
    Instruction {
        aux0: crate::encoding::OPID_MZ,
        aux1: qubit,
        aux2: result,
        ..op(OP_MEASURE, FLAG_AUX1_IMM | FLAG_AUX2_IMM)
    }
}

/// The kind of output a record of an integer is (`aux1` of
/// `OP_RECORD_OUTPUT`).
const RECORD_INT: u64 = 4;

/// A record of register `register` as an integer.
pub(crate) fn record_int(register: u64) -> Instruction<u64> {
    Instruction {
        src0: register,
        aux1: RECORD_INT,
        ..op(OP_RECORD_OUTPUT, 0)
    }
}

/// The blocks of a counting loop, from block 1: register 0 counts up by
/// one each turn, after `body`, and block 1 runs again while it is below
/// `turns` (register 1 holds the comparison); then block 2 records the
/// counter and the register `recorded` and returns. Block 0, the entry
/// block, sets the counter to 0 and jumps to block 1.
///
/// Recording the counter makes the runtimes' records agree only if each
/// ran every turn.
pub(crate) fn counting_loop(
    body: &[Instruction<u64>],
    turns: u64,
    recorded: Option<u64>,
) -> [Vec<Instruction<u64>>; 3] {
    let entry = vec![set(0, 0), jump(1)];
    let mut turn = body.to_vec();
    turn.extend([
        Instruction {
            dst: 0,
            src0: 0,
            src1: 1,
            ..op(OP_ADD, FLAG_SRC1_IMM)
        },
        Instruction {
            opcode: u64::from(OP_ICMP) | (u64::from(ICMP_SLT) << 8) | FLAG_SRC1_IMM,
            dst: 1,
            src0: 0,
            src1: turns,
            ..Instruction::default()
        },
        Instruction {
            src0: 1,
            aux0: 1,
            aux1: 2,
            ..op(OP_BRANCH, 0)
        },
    ]);
    let mut exit = vec![record_int(0)];
    exit.extend(recorded.map(record_int));
    exit.push(ret());
    [entry, turn, exit]
}
