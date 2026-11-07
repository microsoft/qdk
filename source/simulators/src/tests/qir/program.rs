// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a [`QirProgram`] tells about its qubits and its instructions: each
//! instruction counted once, whether a shot runs it many times or never.
//! What building a program refuses is tested in `program/check.rs`.

mod check;
mod continuation;

use super::{SIZE, branch, correlated_noise, gate, jump, measure, op, program, ret};
use crate::bytecode::encoding::{
    FLAG_AUX0_IMM, FLAG_AUX1_IMM, FLAG_AUX2_IMM, FLAG_SRC0_IMM, OP_MEASURE, OP_PEEK_LOSS,
    OP_QUANTUM_GATE, OP_READ_RESULT, OPID_CX, OPID_H, OPID_MRESETZ, OPID_MZ, OPID_RZZ,
};
use crate::bytecode::{AdaptiveProgram, Instruction, Op};
use crate::qir::{MeasuredQubit, QirProgram, TwoQubitGate};

/// Builds `bytecode`, which has no fault.
fn built(bytecode: &AdaptiveProgram<u64>) -> QirProgram {
    QirProgram::new(bytecode).expect("the program has no fault")
}

/// The mnemonics of the QIR instructions `bytecode` uses, in the machine's
/// order.
fn mnemonics(bytecode: &AdaptiveProgram<u64>) -> Vec<&'static str> {
    built(bytecode)
        .instructions()
        .iter()
        .map(|instruction| instruction.mnemonic())
        .collect()
}

/// An instruction of `opcode` reading the constant result 0 into
/// register 0.
fn reads(opcode: u8) -> Instruction<u64> {
    op(opcode, FLAG_SRC0_IMM)
}

/// The loss check of the constant qubit 0 into the constant result 0.
fn peek_loss() -> Instruction<u64> {
    op(OP_PEEK_LOSS, FLAG_AUX0_IMM | FLAG_AUX1_IMM)
}

/// A Bell pair, measured.
#[test]
fn a_program_has_its_bytecode_s_qubits() {
    let bytecode = program(&[&[
        gate(OPID_H, 0, 0),
        gate(OPID_CX, 0, 1),
        measure(OPID_MZ, 0, 0),
        measure(OPID_MZ, 1, 1),
        ret(),
    ]]);
    assert_eq!(built(&bytecode).qubits(), SIZE);
}

/// A program measuring, then running a two-qubit gate, correlated noise
/// and a Hadamard, the Hadamard in a block no shot reaches, beside a loss
/// check, and a read result that is no QIR instruction.
#[test]
fn a_program_lists_the_instructions_it_uses_in_the_machine_s_order() {
    let bytecode = program(&[
        &[
            measure(OPID_MZ, 0, 0),
            gate(OPID_CX, 0, 1),
            correlated_noise(0, 0),
            peek_loss(),
            reads(OP_READ_RESULT),
            branch(0, 2, 2),
        ],
        &[gate(OPID_H, 0, 0), ret()],
        &[ret()],
    ]);
    assert_eq!(
        mnemonics(&bytecode),
        ["h", "cx", "mz", "correlated_noise", "peek_loss"]
    );
}

/// A program laid out as the compiler lays it out: its operation table
/// holds an entry for each quantum instruction, in the order it emits
/// them, here a `cx` then an `h`; so the instructions name their operations
/// by their places in the table (0 and 1), not by their ids.
#[test]
fn a_quantum_instruction_s_operation_is_the_one_at_its_place_in_the_table() {
    let (cx, h) = (0, 1);
    let mut bytecode = program(&[&[gate(cx, 0, 1), gate(h, 0, 0), ret()]]);
    bytecode.quantum_ops = vec![
        Op::from_tuple((OPID_CX, 0, 0, 0, 0)),
        Op::from_tuple((OPID_H, 0, 0, 0, 0)),
    ];
    assert_eq!(mnemonics(&bytecode), ["h", "cx"]);
}

/// Two programs running the same instructions in other orders and numbers
/// list the same instructions.
#[test]
fn programs_using_the_same_instructions_list_them_alike() {
    let first = program(&[&[
        gate(OPID_H, 0, 0),
        gate(OPID_CX, 0, 1),
        measure(OPID_MZ, 0, 0),
        ret(),
    ]]);
    let second = program(&[&[
        measure(OPID_MZ, 1, 1),
        gate(OPID_CX, 1, 0),
        gate(OPID_H, 1, 1),
        gate(OPID_H, 0, 0),
        ret(),
    ]]);
    assert_eq!(built(&first).instructions(), built(&second).instructions());
}

/// A program whose first block, in the block table, is laid out after the
/// entry block: its gates are listed in the table's order.
#[test]
fn two_qubit_gates_are_listed_in_the_order_of_the_blocks() {
    let mut bytecode = program(&[
        &[gate(OPID_CX, 0, 1), jump(0)],
        &[gate(OPID_CX, 2, 3), ret()],
    ]);
    bytecode.block_table.swap(0, 1);
    bytecode.entry_block = 1;
    assert_eq!(
        built(&bytecode).two_qubit_gates(),
        [
            TwoQubitGate::Known { q1: 2, q2: 3 },
            TwoQubitGate::Known { q1: 0, q2: 1 },
        ]
    );
}

/// A program running a `cx` in a loop and an `rzz` in a block no shot
/// reaches, beside one-qubit gates, measurements and correlated noise on
/// two qubits: each two-qubit gate is listed once, with its qubits in its
/// operand order.
#[test]
fn two_qubit_gates_are_listed_once_each_in_operand_order() {
    let mut bytecode = program(&[
        &[gate(OPID_H, 0, 0), correlated_noise(2, 0), jump(1)],
        &[
            gate(OPID_CX, 2, 1),
            measure(OPID_MZ, 1, 0),
            reads(OP_READ_RESULT),
            branch(0, 1, 3),
        ],
        &[gate(OPID_RZZ, 3, 0), ret()],
        &[ret()],
    ]);
    bytecode.call_args = vec![0, 1];
    assert_eq!(
        built(&bytecode).two_qubit_gates(),
        [
            TwoQubitGate::Known { q1: 2, q2: 1 },
            TwoQubitGate::Known { q1: 3, q2: 0 },
        ]
    );
}

/// Two-qubit gates whose first or second qubit is held in a register.
#[test]
fn a_two_qubit_gate_on_a_register_s_qubit_is_known_only_in_a_shot() {
    let bytecode = program(&[&[
        Instruction {
            aux0: OPID_CX,
            aux2: 1,
            ..op(OP_QUANTUM_GATE, FLAG_SRC0_IMM | FLAG_AUX2_IMM)
        },
        Instruction {
            aux0: OPID_CX,
            aux1: 1,
            ..op(OP_QUANTUM_GATE, FLAG_SRC0_IMM | FLAG_AUX1_IMM)
        },
        ret(),
    ]]);
    assert_eq!(
        built(&bytecode).two_qubit_gates(),
        [TwoQubitGate::FromRegisters, TwoQubitGate::FromRegisters]
    );
}

/// A program measuring with `mz` and `mresetz`, beside a loss check and
/// gates: each measurement is listed, with its qubit and result.
#[test]
fn measurements_are_listed_with_their_qubits_and_results() {
    let bytecode = program(&[&[
        gate(OPID_H, 0, 0),
        measure(OPID_MZ, 0, 2),
        peek_loss(),
        gate(OPID_CX, 0, 1),
        measure(OPID_MRESETZ, 1, 3),
        ret(),
    ]]);
    assert_eq!(
        built(&bytecode).measured_qubits(),
        [
            MeasuredQubit::Known {
                qubit: 0,
                result: 2
            },
            MeasuredQubit::Known {
                qubit: 1,
                result: 3
            },
        ]
    );
}

/// A program whose blocks are laid out in one order, listed in the block
/// table in another, and measure qubits and results in a third: its
/// measurements are listed in the table's order.
#[test]
fn measurements_are_listed_in_the_order_of_the_blocks() {
    let mut bytecode = program(&[
        &[measure(OPID_MZ, 2, 1), ret()],
        &[measure(OPID_MZ, 0, 2), ret()],
        &[measure(OPID_MZ, 1, 0), ret()],
    ]);
    bytecode.block_table.swap(0, 2);
    bytecode.entry_block = 2;
    assert_eq!(
        built(&bytecode).measured_qubits(),
        [
            MeasuredQubit::Known {
                qubit: 1,
                result: 0
            },
            MeasuredQubit::Known {
                qubit: 0,
                result: 2
            },
            MeasuredQubit::Known {
                qubit: 2,
                result: 1
            },
        ]
    );
}

/// Measurements whose qubit or result is held in a register.
#[test]
fn a_measurement_of_a_register_s_qubit_or_into_a_register_s_result_is_known_only_in_a_shot() {
    let bytecode = program(&[&[
        Instruction {
            aux0: OPID_MZ,
            ..op(OP_MEASURE, FLAG_AUX2_IMM)
        },
        Instruction {
            aux0: OPID_MZ,
            ..op(OP_MEASURE, FLAG_AUX1_IMM)
        },
        ret(),
    ]]);
    assert_eq!(
        built(&bytecode).measured_qubits(),
        [MeasuredQubit::FromRegisters, MeasuredQubit::FromRegisters]
    );
}
