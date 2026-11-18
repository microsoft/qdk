// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests' programs, written in the Adaptive Profile bytecode block by
//! block, and the writer they use. Each instruction names its qubits and
//! results by constants, as the Python pass writes most of them.

use crate::bytecode::{AdaptiveProgram, Block, Instruction, Op};
use crate::qir::QirProgram;

use crate::bytecode::encoding::{
    FLAG_AUX0_IMM, FLAG_AUX1_IMM, FLAG_AUX2_IMM, FLAG_DST_IMM, FLAG_SRC0_IMM, OP_BRANCH, OP_JUMP,
    OP_MEASURE, OP_MOV, OP_PEEK_LOSS, OP_QUANTUM_GATE, OP_READ_RESULT, OP_READOUT_NOISE,
    OP_RECORD_OUTPUT, OP_RESET, OP_RET, OPID_CORRELATED_NOISE, OPID_H, OPID_MZ, OPID_RESETZ,
    OPID_X,
};

/// A program of `blocks`, the first its entry block, on `qubits` qubits,
/// with as many results and registers, built.
pub(super) fn program(qubits: u32, blocks: &[&[Instruction<u64>]]) -> QirProgram {
    program_with(qubits, qubits.max(1), &[], blocks)
}

/// The bytecode of a program of `blocks`, the first its entry block, on
/// `qubits` qubits, with as many results and registers.
pub(super) fn bytecode(qubits: u32, blocks: &[&[Instruction<u64>]]) -> AdaptiveProgram<u64> {
    bytecode_with(qubits, qubits.max(1), &[], blocks)
}

/// A program of `blocks`, the first its entry block, on `qubits` qubits,
/// with `results` results, as many registers as either, and the call
/// arguments `call_args` (registers), built.
pub(super) fn program_with(
    qubits: u32,
    results: u32,
    call_args: &[u64],
    blocks: &[&[Instruction<u64>]],
) -> QirProgram {
    QirProgram::new(bytecode_with(qubits, results, call_args, blocks))
        .expect("the test's program has no fault")
}

/// The bytecode of [`program_with`]'s program.
fn bytecode_with(
    qubits: u32,
    results: u32,
    call_args: &[u64],
    blocks: &[&[Instruction<u64>]],
) -> AdaptiveProgram<u64> {
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
        num_qubits: qubits,
        num_results: results,
        num_registers: qubits.max(results).max(1),
        entry_block: 0,
        instructions,
        block_table,
        function_table: Vec::new(),
        phi_entries: Vec::new(),
        switch_cases: Vec::new(),
        call_args: call_args.to_vec(),
        constant_data: Vec::new(),
        // One operation per id, at the index of its id.
        quantum_ops: (0..=OPID_CORRELATED_NOISE)
            .map(|op_id| Op::from_tuple((op_id, 0, 0, 0, 0)))
            .collect(),
    }
}

/// An instruction of `opcode` whose fields are zero and, but for those
/// `flags` marks constant, registers.
fn op(opcode: u8, flags: u64) -> Instruction<u64> {
    Instruction {
        opcode: u64::from(opcode) | flags,
        ..Instruction::default()
    }
}

/// The gate of operation id `operation` on qubit `q1` (and `q2`, for a
/// two-qubit gate), with the angle `angle` (read by a rotation only).
pub(super) fn gate(operation: u64, q1: u64, q2: u64, angle: f64) -> Instruction<u64> {
    Instruction {
        src0: angle.to_bits(),
        aux0: operation,
        aux1: q1,
        aux2: q2,
        ..op(
            OP_QUANTUM_GATE,
            FLAG_SRC0_IMM | FLAG_AUX1_IMM | FLAG_AUX2_IMM,
        )
    }
}

/// A one-qubit gate of operation id `operation`, with no angle.
pub(super) fn gate1(operation: u64, qubit: u64) -> Instruction<u64> {
    gate(operation, qubit, 0, 0.0)
}

/// A two-qubit gate of operation id `operation`, with no angle.
pub(super) fn gate2(operation: u64, q1: u64, q2: u64) -> Instruction<u64> {
    gate(operation, q1, q2, 0.0)
}

/// The two-qubit gate of operation id `operation` (no angle) on the qubit
/// held in register `register` and the constant `q2`.
pub(super) fn gate2_from_register(operation: u64, register: u64, q2: u64) -> Instruction<u64> {
    Instruction {
        aux0: operation,
        aux1: register,
        aux2: q2,
        ..op(OP_QUANTUM_GATE, FLAG_SRC0_IMM | FLAG_AUX2_IMM)
    }
}

/// The measurement of operation id `operation` (`mz` or `mresetz`) of
/// `qubit` into `result`.
pub(super) fn measure(operation: u64, qubit: u64, result: u64) -> Instruction<u64> {
    Instruction {
        aux0: operation,
        aux1: qubit,
        aux2: result,
        ..op(OP_MEASURE, FLAG_AUX1_IMM | FLAG_AUX2_IMM)
    }
}

/// `mz` of `qubit` into the result of the same number.
pub(super) fn mz(qubit: u64) -> Instruction<u64> {
    measure(OPID_MZ, qubit, qubit)
}

/// The reset of `qubit`.
pub(super) fn reset(qubit: u64) -> Instruction<u64> {
    Instruction {
        aux0: OPID_RESETZ,
        aux1: qubit,
        ..op(OP_RESET, FLAG_AUX1_IMM)
    }
}

/// Readout noise on `result`, which reads each value as the other with
/// probability `flip`.
pub(super) fn readout_noise(flip: f64, result: u64) -> Instruction<u64> {
    Instruction {
        aux0: flip.to_bits(),
        aux1: flip.to_bits(),
        aux2: result,
        ..op(
            OP_READOUT_NOISE,
            FLAG_AUX0_IMM | FLAG_AUX1_IMM | FLAG_AUX2_IMM,
        )
    }
}

/// Correlated noise from noise table 0 on the qubits held in the registers
/// of the `count` call arguments from `offset`.
pub(super) fn correlated_noise(count: u64, offset: u64) -> Instruction<u64> {
    Instruction {
        aux0: OPID_CORRELATED_NOISE,
        aux1: count,
        aux2: offset,
        ..op(OP_QUANTUM_GATE, FLAG_AUX1_IMM | FLAG_AUX2_IMM)
    }
}

/// A loss check of `qubit` into `result`.
pub(super) fn peek_loss(qubit: u64, result: u64) -> Instruction<u64> {
    Instruction {
        aux0: qubit,
        aux1: result,
        ..op(OP_PEEK_LOSS, FLAG_AUX0_IMM | FLAG_AUX1_IMM)
    }
}

/// Register `register` set to `value`.
pub(super) fn set(register: u64, value: u64) -> Instruction<u64> {
    Instruction {
        dst: register,
        src0: value,
        ..op(OP_MOV, FLAG_SRC0_IMM)
    }
}

/// `result` read into register `register`.
pub(super) fn read_result(register: u64, result: u64) -> Instruction<u64> {
    Instruction {
        dst: register,
        src0: result,
        ..op(OP_READ_RESULT, FLAG_SRC0_IMM)
    }
}

/// A branch to `taken` when register `condition` holds 1, to `other` when
/// it holds 0; a trap when it holds anything else.
pub(super) fn branch(condition: u64, taken: u64, other: u64) -> Instruction<u64> {
    Instruction {
        src0: condition,
        aux0: taken,
        aux1: other,
        ..op(OP_BRANCH, 0)
    }
}

/// A jump to `block`.
pub(super) fn jump(block: u64) -> Instruction<u64> {
    Instruction {
        dst: block,
        ..op(OP_JUMP, 0)
    }
}

/// The output of `result` recorded.
pub(super) fn record_result(result: u64) -> Instruction<u64> {
    Instruction {
        src0: result,
        ..op(OP_RECORD_OUTPUT, FLAG_SRC0_IMM)
    }
}

/// The program's return, with exit code 0.
pub(super) fn ret() -> Instruction<u64> {
    op(OP_RET, FLAG_DST_IMM)
}

/// A coin (`h` and `mz` of qubit 0, into result 0) whose value is copied
/// by a branch: on 1, an `x` flips qubit 1, which is then read into its
/// result. Every shot records the coin twice; the shots that read 0 and
/// those that read 1 run different second batches.
pub(super) fn coin_copied_by_a_branch() -> QirProgram {
    let (flip, join) = (1, 2);
    program(
        2,
        &[
            &[
                gate1(OPID_H, 0),
                mz(0),
                read_result(0, 0),
                branch(0, flip, join),
            ],
            &[gate1(OPID_X, 1), jump(join)],
            &[mz(1), record_result(0), record_result(1), ret()],
        ],
    )
}

/// `coins` coins: `h` and `mz` of each qubit, into its result, all in one
/// batch. Every shot records its coins' values.
pub(super) fn coins(coins: u32) -> QirProgram {
    let mut block: Vec<_> = (0..u64::from(coins))
        .flat_map(|qubit| [gate1(OPID_H, qubit), mz(qubit)])
        .collect();
    block.push(ret());
    program(coins, &[&block])
}
