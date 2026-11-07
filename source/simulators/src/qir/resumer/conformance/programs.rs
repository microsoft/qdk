// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The QIR programs the conformance kit runs on resumers, whose records
//! any correct quantum state gives, so that a resumer is checked on them
//! whatever its quantum state:
//! fixed records (`x` then `mz`), records equal within a shot, traps, and
//! random records (`h` then `mz`).

use crate::bytecode::encoding::{
    FLAG_AUX1_IMM, FLAG_AUX2_IMM, FLAG_DST_IMM, FLAG_SRC0_IMM, OP_BRANCH, OP_JUMP, OP_MEASURE,
    OP_MOV, OP_QUANTUM_GATE, OP_READ_RESULT, OP_RECORD_OUTPUT, OP_RESET, OP_RET, OP_UDIV, OPID_H,
    OPID_MZ, OPID_RESETZ, OPID_X,
};
use crate::bytecode::{AdaptiveProgram, Block, Instruction, Op};
use crate::qir::{QirProgram, QirTrap};

/// The operation table of every program, each instruction naming its
/// operation by its index here.
const OPERATIONS: [u64; 4] = [OPID_H, OPID_X, OPID_MZ, OPID_RESETZ];
const H: u64 = 0;
const X: u64 = 1;
const MZ: u64 = 2;
const RESETZ: u64 = 3;

/// The program of `blocks`, the first its entry block, with `qubits`
/// qubits, `results` results and 3 registers.
fn program(qubits: u32, results: u32, blocks: &[&[Instruction<u64>]]) -> QirProgram {
    let mut instructions = Vec::new();
    let mut block_table = Vec::new();
    for block in blocks {
        block_table.push(Block {
            instr_offset: instructions.len() as u64,
            instr_count: block.len() as u64,
        });
        instructions.extend_from_slice(block);
    }
    QirProgram::new(&AdaptiveProgram {
        num_qubits: qubits,
        num_results: results,
        num_registers: 3,
        entry_block: 0,
        instructions,
        block_table,
        function_table: Vec::new(),
        phi_entries: Vec::new(),
        switch_cases: Vec::new(),
        call_args: Vec::new(),
        constant_data: Vec::new(),
        quantum_ops: OPERATIONS
            .iter()
            .map(|&op_id| Op::from_tuple((op_id, 0, 0, 0, 0)))
            .collect(),
    })
    .expect("the kit's programs build")
}

/// An instruction of `opcode`, its fields marked constant by `flags`, with
/// the fields `[dst, src0, src1, aux0, aux1, aux2]`.
fn op(opcode: u8, flags: u64, fields: [u64; 6]) -> Instruction<u64> {
    let [dst, src0, src1, aux0, aux1, aux2] = fields;
    Instruction {
        opcode: u64::from(opcode) | flags,
        dst,
        src0,
        src1,
        aux0,
        aux1,
        aux2,
        ..Instruction::default()
    }
}

/// The one-qubit gate of `operation` (an index of [`OPERATIONS`]) on
/// `qubit`.
fn gate(operation: u64, qubit: u64) -> Instruction<u64> {
    op(
        OP_QUANTUM_GATE,
        FLAG_SRC0_IMM | FLAG_AUX1_IMM | FLAG_AUX2_IMM,
        [0, 0, 0, operation, qubit, 0],
    )
}

fn h(qubit: u64) -> Instruction<u64> {
    gate(H, qubit)
}

fn x(qubit: u64) -> Instruction<u64> {
    gate(X, qubit)
}

/// `mz` of `qubit` into `result`.
fn mz(qubit: u64, result: u64) -> Instruction<u64> {
    op(
        OP_MEASURE,
        FLAG_AUX1_IMM | FLAG_AUX2_IMM,
        [0, 0, 0, MZ, qubit, result],
    )
}

fn reset(qubit: u64) -> Instruction<u64> {
    op(OP_RESET, FLAG_AUX1_IMM, [0, 0, 0, RESETZ, qubit, 0])
}

/// `result` read into `register`.
fn read(register: u64, result: u64) -> Instruction<u64> {
    op(
        OP_READ_RESULT,
        FLAG_SRC0_IMM,
        [register, result, 0, 0, 0, 0],
    )
}

/// A branch to block `taken` when `register` holds 1, else to `other`.
fn branch(register: u64, taken: u64, other: u64) -> Instruction<u64> {
    op(OP_BRANCH, 0, [0, register, 0, taken, other, 0])
}

fn jump(block: u64) -> Instruction<u64> {
    op(OP_JUMP, 0, [block, 0, 0, 0, 0, 0])
}

/// `register` set to `value`.
fn set(register: u64, value: u64) -> Instruction<u64> {
    op(OP_MOV, FLAG_SRC0_IMM, [register, value, 0, 0, 0, 0])
}

/// `register` set to 1 divided by the register `divisor`.
fn one_over(register: u64, divisor: u64) -> Instruction<u64> {
    op(OP_UDIV, FLAG_SRC0_IMM, [register, 1, divisor, 0, 0, 0])
}

/// The output of `result` recorded.
fn record_result(result: u64) -> Instruction<u64> {
    op(OP_RECORD_OUTPUT, FLAG_SRC0_IMM, [0, result, 0, 0, 0, 0])
}

/// `register` recorded as an `Int` (the record kind 4).
fn record_int(register: u64) -> Instruction<u64> {
    op(OP_RECORD_OUTPUT, 0, [0, register, 0, 0, 4, 0])
}

/// The program's return, with exit code 0.
fn ret() -> Instruction<u64> {
    op(OP_RET, FLAG_DST_IMM, [0; 6])
}

/// One batch, `x`, `mz`, `reset`, `mz` on one qubit: the records `One`,
/// `Zero` when the batch runs in order, `Zero`, `Zero` in reverse.
pub(super) fn in_order() -> QirProgram {
    program(1, 2, &[&[x(0), mz(0, 0), reset(0), mz(0, 1), ret()]])
}

/// Two batches whose outcomes go to results other than their calls'
/// places: `x` and `mz` into result 3, then `mz` of qubit 0 into result 1
/// and of qubit 1 into result 0. Records results 0, 1 and 3: `Zero`,
/// `One`, `One`.
pub(super) fn results_out_of_place() -> QirProgram {
    program(
        2,
        4,
        &[&[
            x(0),
            mz(0, 3),
            read(0, 3),
            mz(0, 1),
            mz(1, 0),
            record_result(0),
            record_result(1),
            record_result(3),
            ret(),
        ]],
    )
}

/// A random outcome (`h`, `mz` into result 0), measured again (into
/// result 1) and copied through a branch (`x` of the other qubit when it
/// is `One`, measured into result 2): three equal records in every shot.
/// A shot given another shot's first outcome records it in result 0 and
/// in result 2, and its own in result 1.
pub(super) fn consistent() -> QirProgram {
    program(
        2,
        3,
        &[
            &[h(0), mz(0, 0), read(0, 0), branch(0, 1, 2)],
            &[x(1), jump(2)],
            &[mz(0, 1), mz(1, 2), ret()],
        ],
    )
}

/// `x` then `mz` of one qubit, which is left flipped: the record `One` on
/// a fresh state, `Zero` on the state a run of it left.
pub(super) fn flips_a_qubit() -> QirProgram {
    program(1, 1, &[&[x(0), mz(0, 0), ret()]])
}

/// `h` then `mz` on each of 4 qubits: 4 random records.
pub(super) fn random() -> QirProgram {
    program(
        4,
        4,
        &[&[
            h(0),
            h(1),
            h(2),
            h(3),
            mz(0, 0),
            mz(1, 1),
            mz(2, 2),
            mz(3, 3),
            ret(),
        ]],
    )
}

/// A division by zero before any quantum call, and its trap.
pub(super) fn traps_at_once() -> (QirProgram, QirTrap) {
    (
        program(0, 0, &[&[set(0, 0), one_over(1, 0), ret()]]),
        division_by_zero(1),
    )
}

/// A division by the outcome of a batch, `mz` of a fresh qubit, and its
/// trap.
pub(super) fn traps_after_a_batch() -> (QirProgram, QirTrap) {
    (
        program(1, 1, &[&[mz(0, 0), read(0, 0), one_over(1, 0), ret()]]),
        division_by_zero(2),
    )
}

/// A division by the outcome of the second of two batches, each `mz` of
/// a fresh qubit, and its trap.
pub(super) fn traps_after_two_batches() -> (QirProgram, QirTrap) {
    let program = program(
        1,
        2,
        &[&[
            mz(0, 0),
            read(0, 0),
            mz(0, 1),
            read(1, 1),
            one_over(2, 1),
            ret(),
        ]],
    );
    (program, division_by_zero(4))
}

/// The trap of a division by zero at the instruction at `instruction`.
fn division_by_zero(instruction: usize) -> QirTrap {
    QirTrap::DivisionByZero { instruction }
}

/// A random outcome (`h`, `mz`), recorded when it is `Zero`; when it is
/// `One`, a division by zero follows.
pub(super) fn traps_on_one() -> QirProgram {
    program(
        1,
        1,
        &[
            &[h(0), mz(0, 0), read(0, 0), branch(0, 1, 2)],
            &[set(1, 0), one_over(2, 1), ret()],
            &[ret()],
        ],
    )
}

/// A random outcome (`h`, `mz`): when it is `One`, a division by zero
/// follows at once; when `Zero`, a division by the outcome of a second
/// batch, `mz` of a fresh qubit. Every shot traps, in its first round or
/// its second; and the trap of the second.
pub(super) fn traps_sooner_or_later() -> (QirProgram, QirTrap) {
    (
        program(
            2,
            2,
            &[
                &[h(0), mz(0, 0), read(0, 0), branch(0, 1, 2)],
                &[set(1, 0), one_over(2, 1), ret()],
                &[mz(1, 1), read(1, 1), one_over(2, 1), ret()],
            ],
        ),
        division_by_zero(9),
    )
}

/// No quantum call: records the `Int` 42.
pub(super) fn no_quantum_call() -> QirProgram {
    program(0, 0, &[&[set(0, 42), record_int(0), ret()]])
}

/// A random outcome (`h`, `mz`): when it is `Zero`, the shot records it
/// and ends; when `One`, it measures `x` of another qubit and records
/// both, `One` and `One`, after a second batch.
pub(super) fn ends_or_continues() -> QirProgram {
    program(
        2,
        2,
        &[
            &[h(0), mz(0, 0), read(0, 0), branch(0, 1, 2)],
            &[x(1), mz(1, 1), record_result(0), record_result(1), ret()],
            &[record_result(0), ret()],
        ],
    )
}
