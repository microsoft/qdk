// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests' programs, written in the Adaptive Profile bytecode block by
//! block, and the writer they use: a mid-circuit measurement and a branch
//! on it ([`coin_then_branch`]), and its variants, each with the paths a
//! rule of the grouping resumer needs.

use qdk_simulators::bytecode::{AdaptiveProgram, Block, Instruction, Op};
use qdk_simulators::qir::QirProgram;

use crate::encoding::{
    FLAG_AUX0_IMM, FLAG_AUX1_IMM, FLAG_AUX2_IMM, FLAG_DST_IMM, FLAG_SRC0_IMM, OP_BRANCH, OP_JUMP,
    OP_MEASURE, OP_MOV, OP_QUANTUM_GATE, OP_READ_RESULT, OP_READOUT_NOISE, OP_RECORD_OUTPUT,
    OP_RESET, OP_RET, OPID_CORRELATED_NOISE, OPID_H, OPID_MZ, OPID_RESETZ, OPID_X,
};

/// The operation ids [`program`] gives an operation for: those of the
/// bytecode, correlated noise (131) the last.
const OPERATIONS: u64 = OPID_CORRELATED_NOISE + 1;

/// A program of `blocks`, laid out in their order, the first its entry
/// block; with `size` qubits, results and registers; and one quantum
/// operation for each operation id, at the index of its id, so that an
/// instruction names an operation by its id.
pub fn program(size: u32, blocks: &[&[Instruction<u64>]]) -> AdaptiveProgram<u64> {
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
        num_qubits: size,
        num_results: size,
        num_registers: size,
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

/// `bytecode`, a program of [`program`], built.
pub fn built(bytecode: &AdaptiveProgram<u64>) -> QirProgram {
    QirProgram::new(bytecode).expect("the test's program has no fault")
}

/// An instruction of `opcode` whose fields are all zero and, but for those
/// `flags` marks constant, registers.
pub fn op(opcode: u8, flags: u64) -> Instruction<u64> {
    Instruction {
        opcode: u64::from(opcode) | flags,
        ..Instruction::default()
    }
}

/// The gate of operation id `operation` on the constant qubits `q1` and
/// `q2` (`q2` unused by a one-qubit gate), with the angle 0.
pub fn gate(operation: u64, q1: u64, q2: u64) -> Instruction<u64> {
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

/// The measurement of operation id `operation` (`mz` or `mresetz`) of the
/// constant `qubit` into the constant `result`.
pub fn measure(operation: u64, qubit: u64, result: u64) -> Instruction<u64> {
    Instruction {
        aux0: operation,
        aux1: qubit,
        aux2: result,
        ..op(OP_MEASURE, FLAG_AUX1_IMM | FLAG_AUX2_IMM)
    }
}

/// The reset of the constant `qubit`.
pub fn reset(qubit: u64) -> Instruction<u64> {
    Instruction {
        aux0: OPID_RESETZ,
        aux1: qubit,
        ..op(OP_RESET, FLAG_AUX1_IMM)
    }
}

/// Correlated noise (in [`program`]'s operation table, from noise table 0)
/// on the qubits held in the registers of the constant `count` call
/// arguments from the constant `offset`.
pub fn correlated_noise(count: u64, offset: u64) -> Instruction<u64> {
    Instruction {
        aux0: OPID_CORRELATED_NOISE,
        aux1: count,
        aux2: offset,
        ..op(OP_QUANTUM_GATE, FLAG_AUX1_IMM | FLAG_AUX2_IMM)
    }
}

/// `bytecode`, a program of [`program`], with its correlated noise drawn
/// from noise table `table`.
pub fn with_noise_table(bytecode: &mut AdaptiveProgram<u64>, table: u64) {
    bytecode.quantum_ops[usize::try_from(OPID_CORRELATED_NOISE).expect("an index")].q1 = table;
}

/// Readout noise on the constant `result`, which reads each value as the
/// other with probability `flip`, as the Stim compiler writes it after a
/// noisy measurement.
pub fn readout_noise(flip: f64, result: u64) -> Instruction<u64> {
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

/// Register `register` set to the constant `value`.
pub fn set(register: u64, value: u64) -> Instruction<u64> {
    Instruction {
        dst: register,
        src0: value,
        ..op(OP_MOV, FLAG_SRC0_IMM)
    }
}

/// The constant `result` read into register `register`.
pub fn read_result(register: u64, result: u64) -> Instruction<u64> {
    Instruction {
        dst: register,
        src0: result,
        ..op(OP_READ_RESULT, FLAG_SRC0_IMM)
    }
}

/// A branch to `taken` when register `condition` holds 1, else to `other`.
pub fn branch(condition: u64, taken: u64, other: u64) -> Instruction<u64> {
    Instruction {
        src0: condition,
        aux0: taken,
        aux1: other,
        ..op(OP_BRANCH, 0)
    }
}

/// A jump to `block`.
pub fn jump(block: u64) -> Instruction<u64> {
    Instruction {
        dst: block,
        ..op(OP_JUMP, 0)
    }
}

/// The output of the constant `result` recorded.
pub fn record_result(result: u64) -> Instruction<u64> {
    Instruction {
        src0: result,
        ..op(OP_RECORD_OUTPUT, FLAG_SRC0_IMM)
    }
}

/// The program's return, with exit code 0.
pub fn ret() -> Instruction<u64> {
    op(OP_RET, FLAG_DST_IMM)
}

/// Tosses a coin (`h` and `mz` of qubit 0, into result 0) and branches on
/// it: on 1, it flips qubit 1. It then reads qubit 1 into result 1, a copy
/// of the coin, and tosses a second coin (qubit 2, into result 2); it
/// records the three results.
///
/// Each shot runs three batches, as each stops where it reads a result
/// its batch writes: the first coin; the copy, with an `x` before it on the
/// shots that read 1; the second coin. The shots that read 0 and those that
/// read 1 run different second batches, and the same third one.
pub fn coin_then_branch() -> QirProgram {
    let (coin, copy, second) = (0, 1, 2);
    let (read_coin, read_copy) = (0, 1);
    // Block 0, the entry, branches to `flip` or `join`.
    let (flip, join) = (1, 2);
    built(&program(
        3,
        &[
            &[
                gate(OPID_H, coin, 0),
                measure(OPID_MZ, coin, coin),
                read_result(read_coin, coin),
                branch(read_coin, flip, join),
            ],
            &[gate(OPID_X, copy, 0), jump(join)],
            &[
                measure(OPID_MZ, copy, copy),
                read_result(read_copy, copy),
                gate(OPID_H, second, 0),
                measure(OPID_MZ, second, second),
                record_result(coin),
                record_result(copy),
                record_result(second),
                ret(),
            ],
        ],
    ))
}

/// Tosses a coin (qubit 0, into result 0) and branches on it: on 1, it
/// records the coin and ends; on 0, it tosses a second coin (qubit 1, into
/// result 1) and records both. The shots that read 1 end after one batch,
/// the others after two.
pub fn coin_then_end_or_toss() -> QirProgram {
    let (end, toss) = (1, 2);
    built(&program(
        2,
        &[
            &[
                gate(OPID_H, 0, 0),
                measure(OPID_MZ, 0, 0),
                read_result(0, 0),
                branch(0, end, toss),
            ],
            &[record_result(0), ret()],
            &[
                gate(OPID_H, 1, 0),
                measure(OPID_MZ, 1, 1),
                record_result(0),
                record_result(1),
                ret(),
            ],
        ],
    ))
}

/// Tosses a coin (qubit 0, into result 0) and branches on it: on 1, it
/// reads qubit 1 into result 1; on 0, qubit 2. It records both results.
/// The second batches of the two paths have one quantum call each, of the same
/// instruction into the same result, on other qubits.
pub fn coin_then_read_one_of_two_qubits() -> QirProgram {
    let (one, zero, join) = (1, 2, 3);
    built(&program(
        3,
        &[
            &[
                gate(OPID_H, 0, 0),
                measure(OPID_MZ, 0, 0),
                read_result(0, 0),
                branch(0, one, zero),
            ],
            &[measure(OPID_MZ, 1, 1), jump(join)],
            &[measure(OPID_MZ, 2, 1), jump(join)],
            &[record_result(0), record_result(1), ret()],
        ],
    ))
}

/// Tosses a coin (qubit 0, into result 0) and branches on it: on 1, it
/// tosses a second coin (qubit 1) into result 1; on 0, the same coin into
/// result 2. It records the first coin and the second. The second batches
/// of the two paths start with the same quantum call, the `h` of qubit 1, and
/// differ only in the result of the `mz` after it.
pub fn coin_then_toss_into_one_of_two_results() -> QirProgram {
    let (one, zero) = (1, 2);
    built(&program(
        3,
        &[
            &[
                gate(OPID_H, 0, 0),
                measure(OPID_MZ, 0, 0),
                read_result(0, 0),
                branch(0, one, zero),
            ],
            &[
                gate(OPID_H, 1, 0),
                measure(OPID_MZ, 1, 1),
                record_result(0),
                record_result(1),
                ret(),
            ],
            &[
                gate(OPID_H, 1, 0),
                measure(OPID_MZ, 1, 2),
                record_result(0),
                record_result(2),
                ret(),
            ],
        ],
    ))
}

/// Tosses a coin (qubit 0, into result 0) and branches on it: on 1, it
/// tosses a second coin (qubit 1, into result 1) and applies an `h` to
/// qubit 2; on 0, it tosses the second coin alone. It reads the second
/// coin and records both. The second batch of the shots that read 0 is a
/// strict prefix of that of the shots that read 1.
pub fn coin_then_a_batch_or_its_prefix() -> QirProgram {
    let (one, zero, join) = (1, 2, 3);
    built(&program(
        3,
        &[
            &[
                gate(OPID_H, 0, 0),
                measure(OPID_MZ, 0, 0),
                read_result(0, 0),
                branch(0, one, zero),
            ],
            &[
                gate(OPID_H, 1, 0),
                measure(OPID_MZ, 1, 1),
                gate(OPID_H, 2, 0),
                read_result(1, 1),
                jump(join),
            ],
            &[
                gate(OPID_H, 1, 0),
                measure(OPID_MZ, 1, 1),
                read_result(1, 1),
                jump(join),
            ],
            &[record_result(0), record_result(1), ret()],
        ],
    ))
}
