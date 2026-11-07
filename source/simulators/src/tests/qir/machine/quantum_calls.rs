// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The quantum calls a context makes: each call's operands, read from
//! constants or registers, correlated noise, and the traps on a qubit past
//! the program's.

use super::super::{
    SIZE, correlated_noise, gate, measure, op, program, reset, ret, set, with_noise_table,
};
use super::{batches, built, trap};
use crate::bytecode::encoding::{
    FLAG_AUX0_IMM, FLAG_AUX1_IMM, FLAG_AUX2_IMM, FLAG_SRC0_IMM, OP_MEASURE, OP_PEEK_LOSS,
    OP_QUANTUM_GATE, OPID_CORRELATED_NOISE, OPID_CX, OPID_CY, OPID_CZ, OPID_H, OPID_MOVE,
    OPID_MRESETZ, OPID_MZ, OPID_RX, OPID_RXX, OPID_RY, OPID_RYY, OPID_RZ, OPID_RZZ, OPID_S,
    OPID_S_ADJ, OPID_SWAP, OPID_SX, OPID_SX_ADJ, OPID_T, OPID_T_ADJ, OPID_X, OPID_Y, OPID_Z,
};
use crate::bytecode::{Instruction, Op};
use crate::qir::{QirCall, QirMachine, QirStop, QirTrap};

/// Calls of one, two and three operands (`x`, `rx`, `cx`, `rzz`, `mz`),
/// their qubits, angle and result constants, and read from registers.
#[test]
fn a_quantum_call_has_its_operands_values_constant_or_from_registers() {
    let angle = 0.25f64.to_bits();
    let constant = |op_id| Instruction {
        src0: angle,
        aux0: op_id,
        aux1: 1,
        aux2: 2,
        ..op(
            OP_QUANTUM_GATE,
            FLAG_SRC0_IMM | FLAG_AUX1_IMM | FLAG_AUX2_IMM,
        )
    };
    let from_registers = |op_id| Instruction {
        src0: 2,
        aux0: op_id,
        aux1: 0,
        aux2: 1,
        ..op(OP_QUANTUM_GATE, 0)
    };
    let gates = [OPID_X, OPID_RX, OPID_CX, OPID_RZZ];
    let mut constants: Vec<_> = gates.map(constant).into();
    constants.extend([measure(OPID_MZ, 1, 2), ret()]);
    let mut registers = vec![set(0, 1), set(1, 2), set(2, angle), set(3, 2)];
    registers.extend(gates.map(from_registers));
    registers.extend([
        Instruction {
            aux0: OPID_MZ,
            aux1: 0,
            aux2: 3,
            ..op(OP_MEASURE, 0)
        },
        ret(),
    ]);
    let expected = [
        "x(qubit 1), rx(qubit 1, angle 0.25), cx(qubit 1, qubit 2), \
         rzz(qubit 1, qubit 2, angle 0.25), mz(qubit 1, result 2)",
    ];
    assert_eq!(batches(&program(&[&constants])), expected, "constants");
    assert_eq!(batches(&program(&[&registers])), expected, "registers");
}

/// Every gate, called on qubits 3 then 1 at an angle of 0.5,
/// and `cx` on qubit 2 twice: a gate's quantum call has as many qubits as
/// the gate
/// acts on, in the program's order and not checked distinct, and an angle
/// only for a rotation.
#[test]
fn a_gate_call_has_its_gate_s_qubits_in_order_and_an_angle_only_for_a_rotation() {
    let called = |op_id, q1, q2| Instruction {
        src0: 0.5f64.to_bits(),
        aux0: op_id,
        aux1: q1,
        aux2: q2,
        ..op(
            OP_QUANTUM_GATE,
            FLAG_SRC0_IMM | FLAG_AUX1_IMM | FLAG_AUX2_IMM,
        )
    };
    let gates = [
        OPID_X,
        OPID_Y,
        OPID_Z,
        OPID_H,
        OPID_S,
        OPID_S_ADJ,
        OPID_T,
        OPID_T_ADJ,
        OPID_SX,
        OPID_SX_ADJ,
        OPID_RX,
        OPID_RY,
        OPID_RZ,
        OPID_CX,
        OPID_CZ,
        OPID_RXX,
        OPID_RYY,
        OPID_RZZ,
        OPID_SWAP,
        OPID_CY,
    ];
    let mut block: Vec<_> = gates.iter().map(|&op_id| called(op_id, 3, 1)).collect();
    block.extend([called(OPID_CX, 2, 2), ret()]);
    let expected = [
        "x(qubit 3)",
        "y(qubit 3)",
        "z(qubit 3)",
        "h(qubit 3)",
        "s(qubit 3)",
        "s_adj(qubit 3)",
        "t(qubit 3)",
        "t_adj(qubit 3)",
        "sx(qubit 3)",
        "sx_adj(qubit 3)",
        "rx(qubit 3, angle 0.5)",
        "ry(qubit 3, angle 0.5)",
        "rz(qubit 3, angle 0.5)",
        "cx(qubit 3, qubit 1)",
        "cz(qubit 3, qubit 1)",
        "rxx(qubit 3, qubit 1, angle 0.5)",
        "ryy(qubit 3, qubit 1, angle 0.5)",
        "rzz(qubit 3, qubit 1, angle 0.5)",
        "swap(qubit 3, qubit 1)",
        "cy(qubit 3, qubit 1)",
        "cx(qubit 2, qubit 2)",
    ];
    assert_eq!(batches(&program(&[&block])), [expected.join(", ")]);
}

/// `move`, `mresetz`, `reset` and a loss check, each on qubit 2, the
/// measurement into result 1 and the check into result 3: each quantum
/// call has the operands its instruction takes, in their order.
#[test]
fn a_quantum_call_that_is_no_gate_has_its_instruction_s_operands() {
    let bytecode = program(&[&[
        gate(OPID_MOVE, 2, 0),
        measure(OPID_MRESETZ, 2, 1),
        reset(2),
        Instruction {
            aux0: 2,
            aux1: 3,
            ..op(OP_PEEK_LOSS, FLAG_AUX0_IMM | FLAG_AUX1_IMM)
        },
        ret(),
    ]]);
    assert_eq!(
        batches(&bytecode),
        [
            "move(qubit 2), mresetz(qubit 2, result 1), reset(qubit 2), \
          peek_loss(qubit 2, result 3)"
        ]
    );
}

/// Correlated noise from the operation table's noise table, with its count
/// and offset read from registers 0 and 1, set to `count` and `offset`
/// first.
fn noise_from_registers(count: u64, offset: u64) -> [Instruction<u64>; 3] {
    let mut noise = correlated_noise(0, 1);
    noise.opcode &= !(FLAG_AUX1_IMM | FLAG_AUX2_IMM);
    [set(0, count), set(1, offset), noise]
}

/// Correlated noise from noise table 5 on the qubits 1, 2 and 0 that
/// registers 4, 2 and 3 hold, named by the `call_args` table from 1, then on
/// none; its count and offset constants, and read from registers.
#[test]
fn correlated_noise_is_a_quantum_call_on_its_qubits_from_its_noise_table() {
    let targets = [set(4, 1), set(2, 2), set(3, 0)];
    let mut constants = targets.to_vec();
    constants.extend([correlated_noise(3, 1), correlated_noise(0, 4), ret()]);
    let mut registers = targets.to_vec();
    registers.extend(noise_from_registers(3, 1));
    registers.extend(noise_from_registers(0, 4));
    registers.push(ret());
    let expected = ["correlated_noise(qubits [1, 2, 0], table 5), \
         correlated_noise(qubits [], table 5)"];
    for (name, instructions) in [("constants", constants), ("registers", registers)] {
        let mut bytecode = program(&[&instructions]);
        with_noise_table(&mut bytecode, 5);
        bytecode.num_registers = 5;
        bytecode.call_args = vec![0, 4, 2, 3];
        assert_eq!(batches(&bytecode), expected, "{name}");
    }
}

/// Correlated noise on 4, 5 and 6 of a program's 6 qubits, named by
/// entries of the `call_args` table that run from qubit 5 down: as many as
/// a quantum call holds in place, and more, which it allocates.
#[test]
fn correlated_noise_keeps_its_qubits_however_many_they_are() {
    for count in 4..=6 {
        let mut instructions: Vec<_> = (0..6).map(|qubit| set(qubit, qubit)).collect();
        instructions.extend([correlated_noise(count as u64, 0), ret()]);
        let mut bytecode = program(&[&instructions]);
        with_noise_table(&mut bytecode, 5);
        (bytecode.num_qubits, bytecode.num_registers) = (6, 6);
        bytecode.call_args = (0..6).rev().collect();
        let qubits: Vec<String> = (0..6).rev().take(count).map(|q| q.to_string()).collect();
        let expected = format!("correlated_noise(qubits [{}], table 5)", qubits.join(", "));
        assert_eq!(batches(&bytecode), [expected], "on {count}");
    }
}

/// Correlated noise on 2^16 + 1 qubits, each named by an entry of the
/// `call_args` table naming register 0, which holds qubit 2. A call keeps
/// every qubit, with no limit; no test can show that, so this one shows
/// it at least past a count of 8 or 16 bits.
#[test]
fn correlated_noise_keeps_a_count_of_qubits_past_16_bits() {
    let count = (1 << 16) + 1;
    let mut bytecode = program(&[&[set(0, 2), correlated_noise(count, 0), ret()]]);
    bytecode.call_args = vec![0; usize::try_from(count).expect("a count of qubits")];
    let program = built(&bytecode);
    let machine = QirMachine::new(&program);
    let mut context = machine.start_shot();
    let stop = machine.resume(&mut context, &[]).expect("a context starts");
    let QirStop::Batch([QirCall::CorrelatedNoise { qubits, .. }]) = stop else {
        panic!("the context stops at the noise: {stop:?}");
    };
    // COMPATIBILITY NOTE: `run_shot` (bytecode/runtime.rs) reads the count
    // whole, with no limit but the `call_args` table; we keep it so that
    // the machine runs every such quantum call `run_shot` runs.
    assert_eq!(qubits.len(), (1 << 16) + 1);
    assert!(qubits.iter().all(|&qubit| qubit == 2), "{qubits:?}");
}

/// Correlated noise whose operation names noise table
/// 2^32 + `0x8765_4321`, past the 32 bits of a simulator's table ids, and
/// whose low half is past 16 bits and past an `i32`.
#[test]
fn correlated_noise_s_noise_table_id_is_cut_to_its_low_32_bits() {
    let mut bytecode = program(&[&[set(0, 1), correlated_noise(1, 0), ret()]]);
    with_noise_table(&mut bytecode, (1 << 32) + 0x8765_4321);
    bytecode.call_args = vec![0];
    // DISCUSS COMPATIBILITY: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `op.q1 as u32`), but it is odd: an id past 32
    // bits names another table rather than being refused, so we keep it
    // here meanwhile but we expect discussion/feedback as part of the
    // review. We think this is OK because no program has such an id: the
    // Python pass takes the ids from the noise configuration, whose ids
    // are 32 bits, so we recommend keeping the cut (question Q-CN1).
    assert_eq!(
        batches(&bytecode),
        ["correlated_noise(qubits [1], table 2271560481)"]
    );
}

/// A program laid out as the compiler lays it out: its operation table
/// holds an entry for each quantum instruction, in the order it emits
/// them, here an `h`, a `cx`, correlated noise from noise table 9 on
/// qubits 2 and 3, whose entry holds the table, then an `mz`; so the noise
/// names its operation by its place in the table (2), not by its id (131).
#[test]
fn correlated_noise_draws_from_the_noise_table_of_the_operation_at_its_place() {
    let (h, cx, noise, mz) = (0, 1, 2, 3);
    let mut bytecode = program(&[&[
        gate(h, 0, 0),
        gate(cx, 0, 1),
        set(0, 2),
        set(1, 3),
        Instruction {
            aux0: noise,
            ..correlated_noise(2, 0)
        },
        measure(mz, 2, 0),
        ret(),
    ]]);
    bytecode.call_args = vec![0, 1];
    bytecode.quantum_ops = vec![
        Op::from_tuple((OPID_H, 0, 0, 0, 0)),
        Op::from_tuple((OPID_CX, 0, 0, 0, 0)),
        Op::from_tuple((OPID_CORRELATED_NOISE, 9, 2, 0, 0)),
        Op::from_tuple((OPID_MZ, 0, 0, 0, 0)),
    ];
    // COMPATIBILITY NOTE: `run_shot` (bytecode/runtime.rs) reads the
    // operation at the instruction's place in the table, and its noise
    // table from that operation's `q1`; we keep it, as the compiler emits
    // that layout.
    assert_eq!(
        batches(&bytecode),
        [
            "h(qubit 0), cx(qubit 0, qubit 1), correlated_noise(qubits [2, 3], table 9), \
          mz(qubit 2, result 0)"
        ]
    );
}

/// A gate, and correlated noise as the middle of three qubits, on the
/// qubit just past the program's, held in a register, and on the qubit
/// 2^32 + 1, which is not qubit 1 cut to 32 bits.
#[test]
fn a_qubit_past_the_program_s_is_a_trap() {
    let gate_on_register = Instruction {
        aux0: OPID_X,
        aux1: 0,
        ..op(OP_QUANTUM_GATE, FLAG_SRC0_IMM)
    };
    let noise = correlated_noise(3, 0);
    for (instruction, qubit) in [
        (gate_on_register, SIZE.into()),
        (gate_on_register, (1 << 32) + 1),
        (noise, SIZE.into()),
        (noise, (1 << 32) + 1),
    ] {
        let mut bytecode = program(&[&[set(0, qubit), instruction, ret()]]);
        bytecode.call_args = vec![1, 0, 2];
        assert_eq!(
            trap(&bytecode),
            QirTrap::NoSuchQubit {
                instruction: 1,
                qubit,
                qubits: SIZE
            },
            "qubit {qubit}"
        );
    }
}

/// Correlated noise whose count and offset, read from registers, reach past the
/// two entries of the program's `call_args` table: by one, from an offset at
/// the end on no qubit, by an offset or a count so large that their sum
/// overflows, and by an offset or a count of 2^32 + 1, which is no 1. One
/// ending at their end is not. And correlated noise whose count only, or offset
/// only, is read from a register, reaching past them by one.
#[test]
fn correlated_noise_s_qubits_past_the_call_args_table_are_a_trap() {
    let past_32_bits = (1 << 32) + 1;
    for (count, offset) in [
        (3, 0),
        (2, 1),
        (0, 3),
        (1, u64::MAX),
        (u64::MAX, 1),
        (1, past_32_bits),
        (past_32_bits, 0),
    ] {
        let mut bytecode =
            program(&[&[&noise_from_registers(count, offset)[..], &[ret()]].concat()]);
        bytecode.call_args = vec![2, 3];
        assert_eq!(
            trap(&bytecode),
            QirTrap::PastCallArgs {
                instruction: 2,
                offset,
                count,
                len: 2
            },
            "{count} from {offset}"
        );
    }
    let mut at_the_end = program(&[&[&noise_from_registers(0, 2)[..], &[ret()]].concat()]);
    at_the_end.call_args = vec![2, 3];
    assert_eq!(
        batches(&at_the_end),
        ["correlated_noise(qubits [], table 0)"]
    );
    let mut count_from_a_register = correlated_noise(0, 1);
    count_from_a_register.opcode &= !FLAG_AUX1_IMM;
    let mut offset_from_a_register = correlated_noise(2, 0);
    offset_from_a_register.opcode &= !FLAG_AUX2_IMM;
    for (name, noise, register_0) in [
        ("the count", count_from_a_register, 2),
        ("the offset", offset_from_a_register, 1),
    ] {
        let mut bytecode = program(&[&[set(0, register_0), noise, ret()]]);
        bytecode.call_args = vec![2, 3];
        assert_eq!(
            trap(&bytecode),
            QirTrap::PastCallArgs {
                instruction: 1,
                offset: 1,
                count: 2,
                len: 2
            },
            "{name} from a register"
        );
    }
}

/// Correlated noise, with its count read from a register, whose middle
/// entry of three in the `call_args` table names the register just past
/// the program's, or the register 2^32 + 1, which is no register 1: the
/// constants the machine keeps after the program's registers hold no
/// qubit of it.
#[test]
fn a_correlated_noise_qubit_register_past_the_program_s_is_a_trap() {
    for register in [SIZE.into(), (1 << 32) + 1] {
        let mut bytecode = program(&[&[&noise_from_registers(3, 0)[..], &[ret()]].concat()]);
        bytecode.call_args = vec![1, register, 2];
        assert_eq!(
            trap(&bytecode),
            QirTrap::NoSuchRegister {
                instruction: 2,
                register,
                registers: SIZE
            },
            "register {register}"
        );
    }
}
