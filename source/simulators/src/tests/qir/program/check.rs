// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Building a [`QirProgram`] refuses the content on which a shot would
//! stop that the bytecode shows, and blocks that a shot could leave other
//! than through their terminator; it reports every fault.

use super::super::{
    SIZE, at_instruction, binary, branch, call, call_return, compare, copy_of, correlated_noise,
    faults, gate, jump, measure, op, phi, program, reset, ret, switch,
};
use crate::bytecode::encoding::{
    FCMP_OEQ, FLAG_AUX0_IMM, FLAG_AUX1_IMM, FLAG_AUX2_IMM, FLAG_SRC0_IMM, FLAG_SRC1_IMM, ICMP_EQ,
    OP_ADD, OP_ALLOCA, OP_AND, OP_ASHR, OP_CONST, OP_FADD, OP_FCMP, OP_FDIV, OP_FMUL, OP_FPEXT,
    OP_FPTOSI, OP_FPTOUI, OP_FPTRUNC, OP_FREM, OP_FSUB, OP_GEP, OP_ICMP, OP_INTTOPTR, OP_LOAD,
    OP_LSHR, OP_MOV, OP_MUL, OP_OR, OP_PEEK_LOSS, OP_READ_LOSS, OP_READ_RESULT, OP_READOUT_NOISE,
    OP_RECORD_OUTPUT, OP_RET, OP_SDIV, OP_SELECT, OP_SEXT, OP_SHL, OP_SITOFP, OP_SREM, OP_STORE,
    OP_SUB, OP_TRUNC, OP_UDIV, OP_UITOFP, OP_UREM, OP_WRITE_RESULT, OP_XOR, OP_ZEXT,
    OPID_CORRELATED_NOISE, OPID_CX, OPID_MZ, OPID_X, VOID_RETURN,
};
use crate::bytecode::{
    AdaptiveProgram, Block, Function, Instruction, Op, PhiNodeEntry, SwitchCase,
};
use crate::qir::{QirError, QirFault, QirFaultSite, QirFaults, QirProgram, SideTable};

/// Builds `bytecode`, which has no fault.
fn builds(bytecode: &AdaptiveProgram<u64>) {
    QirProgram::new(copy_of(bytecode)).expect("the program has no fault");
}

fn at(site: QirFaultSite, error: QirError) -> QirFault {
    QirFault { at: site, error }
}

/// A program whose entry block is past its block table.
#[test]
fn the_entry_block_must_exist() {
    let mut bytecode = program(&[&[ret()]]);
    bytecode.entry_block = 1;
    assert_eq!(
        faults(&bytecode),
        [at(
            QirFaultSite::Program,
            QirError::NoSuchBlock {
                block: 1,
                blocks: 1
            }
        )]
    );
}

/// A function table whose second function starts at a block that does not
/// exist.
#[test]
fn a_function_s_entry_block_must_exist() {
    let mut bytecode = program(&[&[ret()], &[call_return(0)]]);
    bytecode.function_table = vec![
        Function::from_tuple((1, 0, 0)),
        Function::from_tuple((2, 0, 0)),
    ];
    assert_eq!(
        faults(&bytecode),
        [at(
            QirFaultSite::Function(1),
            QirError::NoSuchBlock {
                block: 2,
                blocks: 2
            }
        )]
    );
}

/// A block table with blocks past the instructions, one by an offset so
/// large that its end overflows: each is refused, and nothing panics.
#[test]
fn a_block_must_lie_within_the_instructions() {
    let mut bytecode = program(&[&[ret()]]);
    bytecode.block_table.push(Block::from_tuple((1, 1)));
    bytecode.block_table.push(Block::from_tuple((u64::MAX, 2)));
    assert_eq!(
        faults(&bytecode),
        [
            at(
                QirFaultSite::Block(1),
                QirError::BlockPastInstructions {
                    offset: 1,
                    count: 1,
                    instructions: 1
                }
            ),
            at(
                QirFaultSite::Block(2),
                QirError::BlockPastInstructions {
                    offset: u64::MAX,
                    count: 2,
                    instructions: 1
                }
            ),
        ]
    );
}

/// A block table with a block of no instruction, after the last one.
#[test]
fn a_block_must_hold_an_instruction() {
    let mut bytecode = program(&[&[ret()]]);
    bytecode.block_table.push(Block::from_tuple((1, 0)));
    assert_eq!(
        faults(&bytecode),
        [at(QirFaultSite::Block(1), QirError::EmptyBlock)]
    );
}

/// A third block that holds the last instruction of the first and the
/// second's: it is reported once, naming the first.
#[test]
fn blocks_must_not_share_instructions() {
    let mut bytecode = program(&[&[gate(OPID_X, 0, 0), ret()], &[ret()]]);
    bytecode.block_table.push(Block::from_tuple((1, 2)));
    assert_eq!(
        faults(&bytecode),
        [at(
            QirFaultSite::Block(2),
            QirError::BlocksOverlap { earlier: 0 }
        )]
    );
}

/// An instruction after the last block.
#[test]
fn every_instruction_must_belong_to_a_block() {
    let mut bytecode = program(&[&[ret()]]);
    bytecode.instructions.push(ret());
    assert_eq!(
        faults(&bytecode),
        [at_instruction(1, QirError::OutsideBlocks)]
    );
}

/// A block whose last instruction is a gate, so that a shot would go on
/// into whatever instruction follows.
#[test]
fn a_block_must_end_with_a_terminator() {
    let bytecode = program(&[&[gate(OPID_X, 0, 0)]]);
    assert_eq!(
        faults(&bytecode),
        [at(QirFaultSite::Block(0), QirError::NoTerminator)]
    );
}

/// Blocks ending with each terminator: a branch, a jump, a switch, a
/// return, and a function's return.
#[test]
fn a_block_may_end_with_any_terminator() {
    let mut bytecode = program(&[
        &[branch(0, 1, 2)],
        &[jump(3)],
        &[switch(0, 3, 0, 0)],
        &[call(0, 0, 0, VOID_RETURN), ret()],
        &[call_return(0)],
    ]);
    bytecode.function_table = vec![Function::from_tuple((4, 0, 0))];
    builds(&bytecode);
}

/// A return followed by another in its block, so that the second never
/// runs.
#[test]
fn a_terminator_must_end_its_block() {
    let bytecode = program(&[&[ret(), ret()]]);
    assert_eq!(
        faults(&bytecode),
        [at_instruction(0, QirError::TerminatorInsideBlock)]
    );
}

/// Returns from a function in blocks reached from the entry block through
/// a branch, a switch's default and case, and a jump, so with no
/// function call to return from.
#[test]
fn a_return_from_a_function_must_be_reached_through_a_function_call() {
    let mut bytecode = program(&[
        &[branch(0, 1, 2)],
        &[call_return(0)],
        &[switch(0, 3, 0, 1)],
        &[call_return(0)],
        &[jump(5)],
        &[call_return(0)],
    ]);
    bytecode.switch_cases = vec![SwitchCase::from_tuple((0, 4))];
    assert_eq!(
        faults(&bytecode),
        [
            at_instruction(1, QirError::ReturnOutsideFunction),
            at_instruction(3, QirError::ReturnOutsideFunction),
            at_instruction(5, QirError::ReturnOutsideFunction),
        ]
    );
}

/// An entry block calling two functions and ending the program, and ends
/// of the program in blocks reached from the first function's entry block,
/// through a branch, a switch's default and case, and a jump, and in the
/// second function's entry block: each but the entry block's would end the
/// program inside a function call.
#[test]
fn the_program_must_not_end_inside_a_function() {
    // Not `run_shot`'s behaviour: it (bytecode/runtime.rs, `OP_RET`)
    // ended the program at any depth of function calls, dropping the rest
    // of every caller. Building refuses it instead, because the adaptive pass
    // (`_adaptive_pass.py`, `_emit_ret`) emits `RET` only in the entry
    // point, and a function's return as `CALL_RETURN`; building refuses
    // the converse too (`ReturnOutsideFunction`).
    let mut bytecode = program(&[
        &[
            call(0, 0, 0, VOID_RETURN),
            call(1, 0, 0, VOID_RETURN),
            ret(),
        ],
        &[branch(0, 2, 3)],
        &[ret()],
        &[switch(0, 4, 0, 1)],
        &[ret()],
        &[jump(6)],
        &[ret()],
        &[ret()],
    ]);
    bytecode.function_table = vec![
        Function::from_tuple((1, 0, 0)),
        Function::from_tuple((7, 0, 0)),
    ];
    bytecode.switch_cases = vec![SwitchCase::from_tuple((0, 5))];
    assert_eq!(
        faults(&bytecode),
        [
            at_instruction(4, QirError::ProgramEndInsideFunction),
            at_instruction(6, QirError::ProgramEndInsideFunction),
            at_instruction(8, QirError::ProgramEndInsideFunction),
            at_instruction(9, QirError::ProgramEndInsideFunction),
        ]
    );
}

/// A program with no function as the Python bindings give it: they fill
/// its empty function table with a default entry, a function at block 0,
/// the entry block, which no `CALL` names, so never runs. And a function
/// at block 1 that no `CALL` names. The program's end is accepted in
/// both.
#[test]
fn the_program_may_end_in_a_function_no_function_call_names() {
    let mut padded = program(&[&[ret()]]);
    padded.function_table = vec![Function::default()];
    builds(&padded);
    let mut uncalled = program(&[&[ret()], &[ret()]]);
    uncalled.function_table = vec![Function::from_tuple((1, 0, 0))];
    builds(&uncalled);
}

/// A function at block 1 that no `CALL` names, returning with
/// `CALL_RETURN`: no run reaches its return, so it is accepted.
#[test]
fn a_function_no_function_call_names_may_return() {
    let mut bytecode = program(&[&[ret()], &[call_return(0)]]);
    bytecode.function_table = vec![Function::from_tuple((1, 0, 0))];
    builds(&bytecode);
}

/// An entry block calling a function that calls another, whose entry
/// block ends the program: a shot reaches that end two function calls
/// deep.
#[test]
fn a_function_called_only_by_a_function_must_not_end_the_program() {
    let mut bytecode = program(&[
        &[call(0, 0, 0, VOID_RETURN), ret()],
        &[call(1, 0, 0, VOID_RETURN), call_return(0)],
        &[ret()],
    ]);
    bytecode.function_table = vec![
        Function::from_tuple((1, 0, 0)),
        Function::from_tuple((2, 0, 0)),
    ];
    assert_eq!(
        faults(&bytecode),
        [at_instruction(4, QirError::ProgramEndInsideFunction)]
    );
}

/// An entry block that calls a function whose entry block is the
/// program's own, then ends the program: a shot reaches that end inside
/// the function call.
#[test]
fn the_entry_block_must_not_end_the_program_when_a_function_call_names_a_function_there() {
    let mut bytecode = program(&[&[call(0, 0, 0, VOID_RETURN), ret()]]);
    bytecode.function_table = vec![Function::from_tuple((0, 0, 0))];
    assert_eq!(
        faults(&bytecode),
        [at_instruction(1, QirError::ProgramEndInsideFunction)]
    );
}

/// A block that ends the program, which the entry block jumps to after a
/// function call, and the called function jumps to as well: a shot can reach it
/// inside the function call, so it is refused, though the entry block reaches
/// it too.
#[test]
fn a_block_reached_from_the_entry_and_from_a_function_must_not_end_the_program() {
    let mut bytecode = program(&[&[call(0, 0, 0, VOID_RETURN), jump(2)], &[jump(2)], &[ret()]]);
    bytecode.function_table = vec![Function::from_tuple((1, 0, 0))];
    assert_eq!(
        faults(&bytecode),
        [at_instruction(3, QirError::ProgramEndInsideFunction)]
    );
}

/// A block that returns from a function, which the entry block jumps to after a
/// function call, and the called function jumps to as well: a shot can reach it
/// with no function call to return from, so it is refused, though the function
/// reaches it too.
#[test]
fn a_block_reached_from_the_entry_and_from_a_function_must_not_return_from_a_function() {
    let mut bytecode = program(&[
        &[call(0, 0, 0, VOID_RETURN), jump(2)],
        &[jump(2)],
        &[call_return(0)],
    ]);
    bytecode.function_table = vec![Function::from_tuple((1, 0, 0))];
    assert_eq!(
        faults(&bytecode),
        [at_instruction(3, QirError::ReturnOutsideFunction)]
    );
}

/// A branch, a jump and a switch (its default and a case) to blocks that
/// do not exist.
#[test]
fn a_target_block_must_exist() {
    let mut bytecode = program(&[&[branch(0, 9, 8)], &[jump(7)], &[switch(0, 6, 0, 1)]]);
    bytecode.switch_cases = vec![SwitchCase::from_tuple((0, 5))];
    let no_such_block = |block| QirError::NoSuchBlock { block, blocks: 3 };
    assert_eq!(
        faults(&bytecode),
        [
            at_instruction(0, no_such_block(9)),
            at_instruction(0, no_such_block(8)),
            at_instruction(1, no_such_block(7)),
            at_instruction(2, no_such_block(6)),
            at_instruction(2, no_such_block(5)),
        ]
    );
}

/// A switch on register 0 with cases for 5, 7 and 9, two of them to the
/// same block, builds; with 5 and 7 each listed again, it is refused once
/// for each, though they go where their first cases go.
#[test]
fn a_switch_must_not_repeat_a_case_value() {
    let with_cases = |cases: &[(u64, u64)]| {
        let mut bytecode = program(&[
            &[switch(0, 3, 0, cases.len() as u64)],
            &[ret()],
            &[ret()],
            &[ret()],
        ]);
        bytecode.switch_cases = cases.iter().copied().map(SwitchCase::from_tuple).collect();
        bytecode
    };
    builds(&with_cases(&[(5, 1), (7, 2), (9, 1)]));
    // COMPATIBILITY NOTE: `run_shot` (bytecode/runtime.rs, `OP_SWITCH`),
    // as the GPU shader and `_qodec` do, takes the first case for a
    // value; building refuses, since no valid QIR gives two (see
    // `QirError::DuplicateSwitchCase`).
    assert_eq!(
        faults(&with_cases(&[
            (5, 1),
            (7, 2),
            (7, 2),
            (9, 1),
            (5, 1),
            (5, 2)
        ])),
        [
            at_instruction(0, QirError::DuplicateSwitchCase { value: 7 }),
            at_instruction(0, QirError::DuplicateSwitchCase { value: 5 }),
        ]
    );
}

/// Opcodes the bytecode does not define, between and past those it does.
#[test]
fn an_opcode_must_be_defined() {
    let bytecode = program(&[&[op(0x01, 0), op(0xFF, 0), ret()]]);
    assert_eq!(
        faults(&bytecode),
        [
            at_instruction(0, QirError::UnknownOpcode { opcode: 0x01 }),
            at_instruction(1, QirError::UnknownOpcode { opcode: 0xFF }),
        ]
    );
}

/// Integer comparisons by the first and last predicates of `ICMP` and the
/// one past; float comparisons by the first and last of LLVM's 16 `FCMP`
/// predicates and the one past.
#[test]
fn a_comparison_predicate_must_be_defined() {
    let bytecode = program(&[&[
        compare(OP_ICMP, 0),
        compare(OP_ICMP, 9),
        compare(OP_ICMP, 10),
        compare(OP_FCMP, 0),
        compare(OP_FCMP, 15),
        compare(OP_FCMP, 16),
        ret(),
    ]]);
    assert_eq!(
        faults(&bytecode),
        [
            at_instruction(2, QirError::UnknownPredicate { predicate: 10 }),
            at_instruction(5, QirError::UnknownPredicate { predicate: 16 }),
        ]
    );
}

/// Additions reading and writing the last register, reading the one past
/// as a constant, and reading register 2^32 + 1, which is register 1 cut
/// to 32 bits: only the last is refused.
#[test]
fn a_register_must_exist() {
    let bytecode = program(&[&[
        binary(OP_ADD, 0, 3, 3, 3),
        binary(OP_ADD, FLAG_SRC1_IMM, 0, 3, 4),
        binary(OP_ADD, 0, 0, 3, (1 << 32) + 1),
        ret(),
    ]]);
    assert_eq!(
        faults(&bytecode),
        [at_instruction(
            2,
            QirError::NoSuchRegister {
                register: (1 << 32) + 1,
                registers: 4
            }
        )]
    );
}

/// The fields of an instruction that hold a register or a constant.
const FIELDS: [&str; 6] = ["dst", "src0", "src1", "aux0", "aux1", "aux2"];

/// The field `name` (one of [`FIELDS`]) of `instruction`.
fn field<'a>(instruction: &'a mut Instruction<u64>, name: &str) -> &'a mut u64 {
    match name {
        "dst" => &mut instruction.dst,
        "src0" => &mut instruction.src0,
        "src1" => &mut instruction.src1,
        "aux0" => &mut instruction.aux0,
        "aux1" => &mut instruction.aux1,
        "aux2" => &mut instruction.aux2,
        _ => unreachable!("an instruction has no field {name}"),
    }
}

/// Each instruction that computes or moves a classical value, with no
/// field marked constant and every field register 0; and the fields the
/// machine reads or writes as registers (as `run_shot` does), the others being
/// unused or constants whatever the flags (a `CONST`'s value, a `SEXT`'s
/// width).
fn classical_instructions() -> Vec<(Instruction<u64>, &'static [&'static str])> {
    const COMPUTES_ONE: &[&str] = &["dst", "src0"];
    const COMPUTES_TWO: &[&str] = &["dst", "src0", "src1"];
    let mut instructions: Vec<(Instruction<u64>, &[&str])> = [
        OP_ADD, OP_SUB, OP_MUL, OP_UDIV, OP_SDIV, OP_UREM, OP_SREM, OP_AND, OP_OR, OP_XOR, OP_SHL,
        OP_LSHR, OP_ASHR, OP_FADD, OP_FSUB, OP_FMUL, OP_FDIV, OP_FREM,
    ]
    .into_iter()
    .map(|opcode| (op(opcode, 0), COMPUTES_TWO))
    .collect();
    instructions.extend([
        (compare(OP_ICMP, ICMP_EQ), COMPUTES_TWO),
        (compare(OP_FCMP, FCMP_OEQ), COMPUTES_TWO),
    ]);
    instructions.extend(
        [
            OP_ZEXT,
            OP_SEXT,
            OP_TRUNC,
            OP_FPEXT,
            OP_FPTRUNC,
            OP_INTTOPTR,
            OP_FPTOSI,
            OP_SITOFP,
            OP_FPTOUI,
            OP_UITOFP,
            OP_MOV,
            OP_LOAD,
            OP_ALLOCA,
            OP_READ_RESULT,
            OP_READ_LOSS,
        ]
        .into_iter()
        .map(|opcode| (op(opcode, 0), COMPUTES_ONE)),
    );
    instructions.extend([
        (op(OP_CONST, 0), &["dst"][..]),
        (op(OP_SELECT, 0), &["dst", "src0", "aux0", "aux1"]),
        (op(OP_GEP, 0), &["dst", "src0", "src1", "aux0"]),
        (op(OP_STORE, 0), &["src0", "src1"]),
        (op(OP_WRITE_RESULT, 0), &["src0", "src1"]),
    ]);
    instructions
}

/// A program computing with every classical instruction, each naming in
/// one of the fields the machine reads or writes as a register the one
/// past the
/// last: each such field is refused, once.
#[test]
fn each_register_a_classical_instruction_uses_must_exist() {
    let mut instructions = Vec::new();
    for (instruction, registers) in classical_instructions() {
        for name in registers {
            let mut past_last = instruction;
            *field(&mut past_last, name) = u64::from(SIZE);
            instructions.push(past_last);
        }
    }
    let count = instructions.len();
    instructions.push(ret());
    let no_such_register = QirError::NoSuchRegister {
        register: u64::from(SIZE),
        registers: SIZE,
    };
    assert_eq!(
        faults(&program(&[&instructions])),
        (0..count)
            .map(|index| at_instruction(index, no_such_register.clone()))
            .collect::<Vec<_>>()
    );
}

/// A program computing with every classical instruction, each holding the
/// register past the last in every field the machine does not read as a
/// register
/// (a compiler may leave anything there): it builds.
#[test]
fn a_field_the_machine_does_not_read_as_a_register_is_not_checked() {
    let mut instructions: Vec<_> = classical_instructions()
        .into_iter()
        .map(|(mut instruction, registers)| {
            for name in FIELDS.iter().filter(|name| !registers.contains(name)) {
                *field(&mut instruction, name) = u64::from(SIZE);
            }
            instruction
        })
        .collect();
    instructions.push(ret());
    builds(&program(&[&instructions]));
}

/// A branch and a switch on a register past the last, and a return of an
/// exit code held in one.
#[test]
fn a_terminator_s_value_must_be_a_register() {
    let bytecode = program(&[
        &[branch(4, 1, 1)],
        &[switch(4, 2, 0, 0)],
        &[Instruction {
            dst: 4,
            ..op(OP_RET, 0)
        }],
    ]);
    let no_such_register = QirError::NoSuchRegister {
        register: 4,
        registers: 4,
    };
    assert_eq!(
        faults(&bytecode),
        (0..3)
            .map(|index| at_instruction(index, no_such_register.clone()))
            .collect::<Vec<_>>()
    );
}

/// A program with no register, as one with no classical computation has:
/// a branch and a switch on a constant, and the constant exit code every
/// entry point without a value returns. It builds.
#[test]
fn a_terminator_s_constant_value_is_not_a_register() {
    let mut on_constant = branch(1, 1, 1);
    on_constant.opcode |= FLAG_SRC0_IMM;
    let mut switch_on_constant = switch(1, 2, 0, 0);
    switch_on_constant.opcode |= FLAG_SRC0_IMM;
    let mut bytecode = program(&[&[on_constant], &[switch_on_constant], &[ret()]]);
    bytecode.num_registers = 0;
    builds(&bytecode);
}

/// One-qubit gates whose second qubit and angle fields hold no register,
/// which the machine reads whatever the gate; as constants, any value builds.
#[test]
fn a_gate_reads_its_angle_and_both_qubit_fields() {
    let mut second_qubit = gate(OPID_X, 0, 4);
    second_qubit.opcode &= !FLAG_AUX2_IMM;
    let mut angle = gate(OPID_X, 0, 0);
    angle.opcode &= !FLAG_SRC0_IMM;
    angle.src0 = 4;
    let bytecode = program(&[&[second_qubit, angle, gate(OPID_X, 0, 9), ret()]]);
    let no_such_register = QirError::NoSuchRegister {
        register: 4,
        registers: 4,
    };
    assert_eq!(
        faults(&bytecode),
        [
            at_instruction(0, no_such_register.clone()),
            at_instruction(1, no_such_register),
        ]
    );
}

/// Side-table ranges past the end of their tables, among them correlated
/// noise's qubits on one qubit and on none, ranges ending at their table's
/// end, and an empty switch range starting past its table's end.
#[test]
fn a_side_table_range_must_lie_within_it() {
    let mut bytecode = program(&[
        &[jump(1)],
        &[
            phi(0, 0, 1),
            phi(0, 1, 1),
            call(0, 1, 1, VOID_RETURN),
            correlated_noise(1, 0),
            correlated_noise(0, 1),
            correlated_noise(1, 1),
            correlated_noise(0, 2),
            switch(0, 3, 0, 1),
        ],
        &[call_return(0)],
        &[ret()],
        &[switch(0, 3, 5, 0)],
    ]);
    bytecode.phi_entries = vec![PhiNodeEntry::from_tuple((0, 0))];
    bytecode.call_args = vec![0];
    bytecode.function_table = vec![Function::from_tuple((2, 0, 0))];
    assert_eq!(
        faults(&bytecode),
        [
            at_instruction(
                2,
                QirError::PastSideTable {
                    table: SideTable::PhiEntries,
                    offset: 1,
                    count: 1,
                    len: 1
                }
            ),
            at_instruction(
                3,
                QirError::PastSideTable {
                    table: SideTable::CallArgs,
                    offset: 1,
                    count: 1,
                    len: 1
                }
            ),
            at_instruction(
                6,
                QirError::PastSideTable {
                    table: SideTable::CallArgs,
                    offset: 1,
                    count: 1,
                    len: 1
                }
            ),
            at_instruction(
                7,
                QirError::PastSideTable {
                    table: SideTable::CallArgs,
                    offset: 2,
                    count: 0,
                    len: 1
                }
            ),
            at_instruction(
                8,
                QirError::PastSideTable {
                    table: SideTable::SwitchCases,
                    offset: 0,
                    count: 1,
                    len: 0
                }
            ),
            // DISCUSS COMPATIBILITY: `run_shot` (bytecode/runtime.rs,
            // `OP_SWITCH`, `OP_CALL`, `OP_PHI`) accepts an empty range at
            // any offset, as it reads only `count` entries, but no
            // producer emits one past its table (the adaptive pass takes
            // each offset from its table's length so far), so we refuse
            // it here, and every range then obeys one rule: it starts and
            // ends within its table. We expect discussion/feedback as
            // part of the review. We think this is OK because it refuses
            // no program a producer emits, so we recommend keeping the
            // refusal.
            at_instruction(
                11,
                QirError::PastSideTable {
                    table: SideTable::SwitchCases,
                    offset: 5,
                    count: 0,
                    len: 0
                }
            ),
        ]
    );
}

/// A phi whose value comes from a register past the last, and a function
/// call and correlated noise whose entry in the `call_args` table does.
#[test]
fn phi_values_and_call_args_entries_must_be_registers() {
    let mut bytecode = program(&[
        &[jump(1)],
        &[
            phi(0, 0, 1),
            call(0, 1, 0, VOID_RETURN),
            correlated_noise(1, 0),
            ret(),
        ],
        &[call_return(0)],
    ]);
    bytecode.phi_entries = vec![PhiNodeEntry::from_tuple((0, 4))];
    bytecode.call_args = vec![4];
    bytecode.function_table = vec![Function::from_tuple((2, 1, 0))];
    let no_such_register = QirError::NoSuchRegister {
        register: 4,
        registers: 4,
    };
    assert_eq!(
        faults(&bytecode),
        [
            at_instruction(1, no_such_register.clone()),
            at_instruction(2, no_such_register.clone()),
            at_instruction(3, no_such_register),
        ]
    );
}

/// Correlated noise whose count, and whose offset, is held in a register
/// past the last, the other a constant within the `call_args` table: each
/// register is a fault; and two with an angle field naming no register,
/// which the machine does not read for it: one naming its operation by the
/// operation's id (131), one by another place in the table (0), as the
/// compiler does.
#[test]
fn correlated_noise_reads_its_count_and_offset_but_no_angle() {
    let mut count = correlated_noise(4, 0);
    count.opcode &= !FLAG_AUX1_IMM;
    let mut offset = correlated_noise(1, 4);
    offset.opcode &= !FLAG_AUX2_IMM;
    let mut angle = correlated_noise(0, 0);
    angle.src0 = 4;
    let angle_elsewhere = Instruction { aux0: 0, ..angle };
    let mut bytecode = program(&[&[count, offset, angle, angle_elsewhere, ret()]]);
    bytecode.call_args = vec![0];
    bytecode.quantum_ops[0] = Op::from_tuple((OPID_CORRELATED_NOISE, 0, 0, 0, 0));
    let no_such_register = QirError::NoSuchRegister {
        register: 4,
        registers: 4,
    };
    // COMPATIBILITY NOTE: `run_shot` (bytecode/runtime.rs) reads only the
    // count and offset fields of correlated noise, not the angle field the
    // other gates read; we keep that encoding, so the angle field is no
    // register to check.
    assert_eq!(
        faults(&bytecode),
        [
            at_instruction(0, no_such_register.clone()),
            at_instruction(1, no_such_register),
        ]
    );
}

/// Correlated noise whose count and offset registers could name qubits
/// past the `call_args` table, which is empty, and correlated noise whose
/// count only, or offset only, comes from a register, the other a
/// constant 1 past it: the range is checked when the machine reads it.
#[test]
fn correlated_noise_s_qubits_from_registers_are_not_checked() {
    let mut from_registers = correlated_noise(0, 1);
    from_registers.opcode &= !(FLAG_AUX1_IMM | FLAG_AUX2_IMM);
    let mut count_from_a_register = correlated_noise(0, 1);
    count_from_a_register.opcode &= !FLAG_AUX1_IMM;
    let mut offset_from_a_register = correlated_noise(1, 0);
    offset_from_a_register.opcode &= !FLAG_AUX2_IMM;
    for (name, noise) in [
        ("both", from_registers),
        ("the count", count_from_a_register),
        ("the offset", offset_from_a_register),
    ] {
        let bytecode = program(&[&[noise, ret()]]);
        let built = QirProgram::new(bytecode).map(|_| ());
        assert_eq!(
            built.map_err(|faults| faults.to_string()),
            Ok(()),
            "{name} from a register"
        );
    }
}

/// Phis of a value held in register 0, written into the last register and
/// into the one past.
#[test]
fn a_phi_s_destination_must_be_a_register() {
    let mut bytecode = program(&[&[jump(1)], &[phi(3, 0, 1), phi(4, 0, 1), ret()]]);
    bytecode.phi_entries = vec![PhiNodeEntry::from_tuple((0, 0))];
    assert_eq!(
        faults(&bytecode),
        [at_instruction(
            2,
            QirError::NoSuchRegister {
                register: 4,
                registers: 4
            }
        )]
    );
}

/// A block 3 reached by a jump from block 1, by the default and both
/// cases of a switch in block 2, and by its own branch (a loop), holding
/// a `PHI` of entries for some of these blocks, and one for block 0, which
/// does not go to it; the `PHI` starts its block, or follows a `MOV` of a
/// constant, as the adaptive pass puts before a `PHI` of a constant value.
/// No edge goes to block 2, so no run takes its edges.
#[test]
fn a_phi_must_have_an_entry_for_each_predecessor() {
    let constant_move = binary(OP_MOV, FLAG_SRC0_IMM, 2, 7, 0);
    for leading in [&[][..], &[constant_move]] {
        let program_with = |entries: &[u64]| {
            let block: Vec<_> = leading
                .iter()
                .copied()
                .chain([phi(1, 0, entries.len() as u64), branch(0, 3, 4)])
                .collect();
            let mut bytecode = program(&[
                &[jump(1)],
                &[jump(3)],
                &[switch(0, 3, 0, 2)],
                &block,
                &[ret()],
            ]);
            bytecode.phi_entries = entries
                .iter()
                .map(|block| PhiNodeEntry::from_tuple((*block, 0)))
                .collect();
            bytecode.switch_cases = vec![
                SwitchCase::from_tuple((1, 3)),
                SwitchCase::from_tuple((2, 3)),
            ];
            bytecode
        };
        builds(&program_with(&[0, 3, 2, 1]));
        let phi_at = 3 + leading.len();
        let missing = |predecessor| {
            at_instruction(
                phi_at,
                QirError::MissingPhiEntry {
                    block: 3,
                    predecessor,
                },
            )
        };
        // COMPATIBILITY NOTE: `run_shot` (bytecode/runtime.rs, `OP_PHI`),
        // as the GPU shader and `_qodec` do, writes nothing on an edge a
        // `PHI` has no entry for, so its register keeps its earlier value,
        // silently; building refuses, since no valid QIR gives such a
        // `PHI`: LLVM requires an entry for each predecessor (see
        // `QirError::MissingPhiEntry`).
        assert_eq!(faults(&program_with(&[1, 3])), [missing(2)], "{phi_at}");
        assert_eq!(faults(&program_with(&[1, 2])), [missing(3)], "{phi_at}");
        assert_eq!(
            faults(&program_with(&[0])),
            [missing(1), missing(2), missing(3)],
            "{phi_at}"
        );
    }
}

/// A program whose entry block calls a function at block 2 and jumps to
/// block 1, which starts with a `PHI` and is also the entry block of a
/// function no `CALL` names; then a `PHI` added at the start of the
/// program's entry block, or after its `CALL`, or at the start of the
/// called function's. And a program of one block holding a `PHI` whose
/// entries lie past their table, with a function at that block, as the
/// loader gives a program with no function. And an entry block starting
/// with a `PHI` of no entry for the block that jumps back to it: refused
/// for each, and for the jump.
#[test]
fn a_phi_must_not_be_in_an_entry_block() {
    let program_with = |phi_in_main: Option<usize>, phi_in_function: bool| {
        let mut main = vec![call(0, 0, 0, VOID_RETURN), jump(1)];
        if let Some(at) = phi_in_main {
            main.insert(at, phi(1, 0, 1));
        }
        let function: Vec<_> = phi_in_function
            .then(|| phi(1, 0, 1))
            .into_iter()
            .chain([call_return(0)])
            .collect();
        let mut bytecode = program(&[&main, &[phi(1, 0, 1), ret()], &function]);
        bytecode.phi_entries = vec![PhiNodeEntry::from_tuple((0, 0))];
        bytecode.function_table = vec![
            Function::from_tuple((2, 0, 0)),
            Function::from_tuple((1, 0, 0)),
        ];
        bytecode
    };
    builds(&program_with(None, false));
    // COMPATIBILITY NOTE: `run_shot` (bytecode/runtime.rs, `OP_CALL`
    // keeps the caller's previous block), as the GPU shader and `_qodec`
    // do, runs such a `PHI`: it takes the entry for the caller's last
    // edge (at the program's entry, for block 0), or writes nothing with
    // none; building refuses, since no valid QIR gives such a `PHI`:
    // LLVM forbids a `phi` in a function's entry block (see
    // `QirError::PhiInEntryBlock`).
    assert_eq!(
        faults(&program_with(Some(0), false)),
        [at_instruction(0, QirError::PhiInEntryBlock { block: 0 })]
    );
    assert_eq!(
        faults(&program_with(Some(1), false)),
        [
            at_instruction(1, QirError::PhiAfterInstruction { instruction: 0 }),
            at_instruction(1, QirError::PhiInEntryBlock { block: 0 }),
        ]
    );
    assert_eq!(
        faults(&program_with(None, true)),
        [at_instruction(4, QirError::PhiInEntryBlock { block: 2 })]
    );
    let mut padded = program(&[&[phi(1, 0, 1), ret()]]);
    padded.function_table = vec![Function::from_tuple((0, 0, 0))];
    assert_eq!(
        faults(&padded),
        [
            at_instruction(
                0,
                QirError::PastSideTable {
                    table: SideTable::PhiEntries,
                    offset: 0,
                    count: 1,
                    len: 0
                }
            ),
            at_instruction(0, QirError::PhiInEntryBlock { block: 0 }),
        ]
    );
    let mut looping = program(&[&[phi(1, 0, 1), jump(1)], &[jump(0)]]);
    looping.phi_entries = vec![PhiNodeEntry::from_tuple((0, 0))];
    assert_eq!(
        faults(&looping),
        [
            at_instruction(0, QirError::PhiInEntryBlock { block: 0 }),
            at_instruction(
                0,
                QirError::MissingPhiEntry {
                    block: 0,
                    predecessor: 1
                }
            ),
            at_instruction(2, QirError::EdgeToEntryBlock { block: 0 }),
        ]
    );
}

/// A program whose entry block adds and jumps to block 1, which holds
/// some instructions, then a `PHI` of an entry for block 0, then an
/// addition and its end; block 2 is a function that a `CALL` in block 1
/// may name. Before the `PHI` the adaptive pass puts the `MOV`s of
/// immediates that load a `PHI`'s constant values, interleaved with other
/// `PHI`s: accepted; any other instruction there (a `CALL`, a `MOV` of a
/// register, an addition, a `READ_RESULT` of a constant result, a `CONST`
/// of an immediate) is refused, the fault naming the last such
/// instruction before the `PHI`, once per `PHI`.
#[test]
fn a_phi_must_follow_only_phis_and_moves_of_immediates() {
    let constant_move = binary(OP_MOV, FLAG_SRC0_IMM, 2, 7, 0);
    let register_move = binary(OP_MOV, 0, 2, 3, 0);
    let addition = binary(OP_ADD, 0, 2, 3, 1);
    let constant_read = binary(OP_READ_RESULT, FLAG_SRC0_IMM, 2, 0, 0);
    let constant_const = binary(OP_CONST, FLAG_SRC0_IMM, 2, 7, 0);
    let a_call = call(0, 0, 0, VOID_RETURN);
    let other_phi = phi(3, 0, 1);
    let program_with = |leading: &[Instruction<u64>]| {
        let block: Vec<_> = leading
            .iter()
            .copied()
            .chain([phi(1, 0, 1), addition, ret()])
            .collect();
        let mut bytecode = program(&[&[addition, jump(1)], &block, &[call_return(0)]]);
        bytecode.phi_entries = vec![PhiNodeEntry::from_tuple((0, 0))];
        bytecode.function_table = vec![Function::from_tuple((2, 0, 0))];
        bytecode
    };
    builds(&program_with(&[]));
    // DISCUSS COMPATIBILITY: this behavior was present in the adaptive
    // pass, which loads each constant value of a `phi` into a fresh
    // register with a `MOV` of an immediate in the `PHI`'s own block, as
    // the `PHI` entries hold registers; but the QIR's `phi`s come first
    // in their block, as LLVM requires, so we accept such a `MOV` here
    // meanwhile but we expect discussion/feedback as part of the review.
    // We think this is OK because the `MOV` leaves the last edge as it is,
    // so the `PHI` takes the same entry, so we recommend keeping this
    // acceptance until the pass puts these `MOV`s in the predecessor
    // blocks, then refusing a `PHI` after any instruction but a `PHI`, as
    // LLVM does.
    builds(&program_with(&[constant_move]));
    builds(&program_with(&[
        constant_move,
        other_phi,
        constant_move,
        constant_move,
    ]));
    let after = |phi_at: usize, instruction: usize| {
        at_instruction(phi_at, QirError::PhiAfterInstruction { instruction })
    };
    // COMPATIBILITY NOTE: `run_shot` (bytecode/runtime.rs, `OP_PHI`), as
    // the GPU shader and `_qodec` do, runs a `PHI` anywhere in its block,
    // taking the entry for the last edge into it (a `CALL`'s return
    // restores it); building refuses it, since no valid QIR gives it: LLVM
    // puts `phi`s first in their block (see
    // `QirError::PhiAfterInstruction`).
    assert_eq!(faults(&program_with(&[a_call])), [after(3, 2)]);
    assert_eq!(faults(&program_with(&[register_move])), [after(3, 2)]);
    assert_eq!(faults(&program_with(&[addition])), [after(3, 2)]);
    assert_eq!(faults(&program_with(&[constant_read])), [after(3, 2)]);
    assert_eq!(faults(&program_with(&[constant_const])), [after(3, 2)]);
    assert_eq!(
        faults(&program_with(&[
            addition,
            constant_move,
            a_call,
            constant_move
        ])),
        [after(6, 4)]
    );
    assert_eq!(
        faults(&program_with(&[a_call, other_phi, constant_move])),
        [after(3, 2), after(5, 2)]
    );
}

/// A program whose entry block calls a function at block 1, which jumps to
/// block 2, which goes on to block 3, which returns; block 4 is the entry
/// block of a function no `CALL` names, which goes to block 3. Then block
/// 2 ends with a jump, a branch, or a switch's default or case to block 3
/// or 4, or back to block 1, the called function's entry block, or a
/// branch to block 1 and then to block 0, the program's (one fault for
/// each, in the branch's order; the `RET` of block 0 is then reached
/// inside the function). And a program whose block 1 branches back to its
/// entry block.
#[test]
fn an_edge_must_not_go_to_an_entry_block() {
    let program_with = |terminator| {
        let mut bytecode = program(&[
            &[call(0, 0, 0, VOID_RETURN), ret()],
            &[jump(2)],
            &[terminator],
            &[call_return(0)],
            &[jump(3)],
        ]);
        bytecode.function_table = vec![
            Function::from_tuple((1, 0, 0)),
            Function::from_tuple((4, 0, 0)),
        ];
        bytecode.switch_cases = vec![
            SwitchCase::from_tuple((1, 1)),
            SwitchCase::from_tuple((2, 3)),
        ];
        bytecode
    };
    for terminator in [jump(3), jump(4), branch(0, 3, 4), switch(0, 4, 1, 1)] {
        builds(&program_with(terminator));
    }
    // COMPATIBILITY NOTE: `run_shot` (bytecode/runtime.rs, `OP_JUMP`,
    // `OP_BRANCH`, `OP_SWITCH`), as the GPU shader and `_qodec` do, takes
    // an edge into an entry block as any other; building refuses, since
    // no valid QIR gives one: LLVM forbids any edge into a function's
    // entry block (see `QirError::EdgeToEntryBlock`).
    let into_function = [at_instruction(3, QirError::EdgeToEntryBlock { block: 1 })];
    for terminator in [
        jump(1),
        branch(0, 1, 3),
        branch(0, 3, 1),
        branch(0, 1, 1),
        switch(0, 1, 0, 0),
        switch(0, 3, 0, 1),
    ] {
        assert_eq!(
            faults(&program_with(terminator)),
            into_function,
            "{terminator:?}"
        );
    }
    let back_to_main = program(&[&[jump(1)], &[branch(0, 2, 0)], &[ret()]]);
    assert_eq!(
        faults(&program_with(branch(0, 1, 0))),
        [
            at_instruction(1, QirError::ProgramEndInsideFunction),
            at_instruction(3, QirError::EdgeToEntryBlock { block: 1 }),
            at_instruction(3, QirError::EdgeToEntryBlock { block: 0 }),
        ]
    );
    assert_eq!(
        faults(&back_to_main),
        [at_instruction(1, QirError::EdgeToEntryBlock { block: 0 })]
    );
}

/// A function call naming the function past the last.
#[test]
fn a_called_function_must_exist() {
    let mut bytecode = program(&[&[call(1, 0, 0, VOID_RETURN), ret()], &[call_return(0)]]);
    bytecode.function_table = vec![Function::from_tuple((1, 0, 0))];
    assert_eq!(
        faults(&bytecode),
        [at_instruction(
            0,
            QirError::NoSuchFunction {
                function: 1,
                functions: 1
            }
        )]
    );
}

/// A function whose parameters start at register 2: called with two
/// arguments, they fill the last register; with three, the one past.
/// Another whose parameters start at the last `u64`: two arguments would
/// run past it, not wrap round to register 0.
#[test]
fn a_function_call_s_arguments_must_fit_its_function_s_registers() {
    let mut bytecode = program(&[
        &[
            call(0, 2, 0, VOID_RETURN),
            call(0, 3, 0, VOID_RETURN),
            call(1, 2, 0, VOID_RETURN),
            ret(),
        ],
        &[call_return(0)],
    ]);
    bytecode.call_args = vec![0, 0, 0];
    bytecode.function_table = vec![
        Function::from_tuple((1, 3, 2)),
        Function::from_tuple((1, 2, u64::MAX)),
    ];
    assert_eq!(
        faults(&bytecode),
        [
            at_instruction(
                1,
                QirError::NoSuchRegister {
                    register: 4,
                    registers: 4
                }
            ),
            at_instruction(
                2,
                QirError::NoSuchRegister {
                    register: u64::MAX,
                    registers: 4
                }
            )
        ]
    );
}

/// Calls keeping their function's value in the last register and in the
/// one past, and one dropping it.
#[test]
fn a_function_call_keeps_its_return_value_in_a_register() {
    let mut bytecode = program(&[
        &[
            call(0, 0, 0, 3),
            call(0, 0, 0, 4),
            call(0, 0, 0, VOID_RETURN),
            ret(),
        ],
        &[call_return(0)],
    ]);
    bytecode.function_table = vec![Function::from_tuple((1, 0, 0))];
    assert_eq!(
        faults(&bytecode),
        [at_instruction(
            1,
            QirError::NoSuchRegister {
                register: 4,
                registers: 4
            }
        )]
    );
}

/// A function returning the value of a register past the last (past `u32`
/// too, where the function call drops it): it builds while every
/// function call drops the value, which a shot then never reads, and is
/// refused once a function call keeps it.
#[test]
fn a_returned_value_is_read_only_when_a_function_call_keeps_it() {
    let mut dropped = program(&[
        &[call(0, 0, 0, VOID_RETURN), ret()],
        &[call_return(1 << 32)],
    ]);
    dropped.function_table = vec![Function::from_tuple((1, 0, 0))];
    builds(&dropped);
    let mut kept = program(&[&[call(0, 0, 0, 0), ret()], &[call_return(4)]]);
    kept.function_table = vec![Function::from_tuple((1, 0, 0))];
    assert_eq!(
        faults(&kept),
        [at_instruction(
            2,
            QirError::NoSuchRegister {
                register: 4,
                registers: 4
            }
        )]
    );
}

/// Two functions returning the value of a register past the last, one
/// whose value a function call keeps, the other whose value its only
/// function call drops: both are refused, though a shot never reads the
/// dropped one.
#[test]
fn once_any_function_call_keeps_a_value_every_returned_value_is_checked() {
    let mut bytecode = program(&[
        &[call(0, 0, 0, 0), call(1, 0, 0, VOID_RETURN), ret()],
        &[call_return(4)],
        &[call_return(4)],
    ]);
    bytecode.function_table = vec![
        Function::from_tuple((1, 0, 0)),
        Function::from_tuple((2, 0, 0)),
    ];
    let no_such_register = QirError::NoSuchRegister {
        register: 4,
        registers: 4,
    };
    assert_eq!(
        faults(&bytecode),
        [
            at_instruction(3, no_such_register.clone()),
            at_instruction(4, no_such_register),
        ]
    );
}

/// A gate, a measurement and a reset naming the operation past the last.
#[test]
fn a_quantum_operation_must_exist() {
    let bytecode = program(&[&[
        gate(132, 0, 0),
        measure(132, 0, 0),
        Instruction {
            aux0: 132,
            ..reset(0)
        },
        ret(),
    ]]);
    let no_such_operation = QirError::NoSuchOperation {
        operation: 132,
        operations: 132,
    };
    assert_eq!(
        faults(&bytecode),
        [
            at_instruction(0, no_such_operation.clone()),
            at_instruction(1, no_such_operation.clone()),
            at_instruction(2, no_such_operation),
        ]
    );
}

/// Operations their opcode does not run: `ccx` (20), which the Python
/// passes decompose first, and `mz` as gates; `x` as a measurement and as
/// a reset. The `ccx` names qubits past the last, unchecked, as what an
/// operand means depends on the operation.
#[test]
fn an_operation_must_be_one_its_opcode_runs() {
    let bytecode = program(&[&[
        gate(20, 9, 9),
        gate(OPID_MZ, 0, 0),
        measure(OPID_X, 0, 0),
        Instruction {
            aux0: OPID_X,
            ..reset(0)
        },
        ret(),
    ]]);
    assert_eq!(
        faults(&bytecode),
        [
            at_instruction(0, QirError::UnsupportedOperation { op_id: 20 }),
            at_instruction(1, QirError::UnsupportedOperation { op_id: OPID_MZ }),
            at_instruction(2, QirError::UnsupportedOperation { op_id: OPID_X }),
            at_instruction(3, QirError::UnsupportedOperation { op_id: OPID_X }),
        ]
    );
}

/// A gate of operation 0, the identity, then a reset, operation 1, which
/// builds.
#[test]
fn the_identity_gate_is_refused() {
    let bytecode = program(&[&[gate(0, 0, 0), reset(0), ret()]]);
    // COMPATIBILITY NOTE: `run_shot` (bytecode/runtime.rs,
    // `OP_QUANTUM_GATE`) panics on operation 0, hence the machine refuses
    // it too, reported at build (see qir/quantum_instruction_encoding.rs,
    // its header).
    assert_eq!(
        faults(&bytecode),
        [at_instruction(
            0,
            QirError::UnsupportedOperation { op_id: 0 }
        )]
    );
}

/// Gates, a measurement, a reset and a loss check on the last qubit and on
/// the one past, as a first or second qubit, and a gate on qubit 2^32 + 1,
/// which is qubit 1 cut to 32 bits.
#[test]
fn a_constant_qubit_must_exist() {
    let peek_loss = |qubit| Instruction {
        aux0: qubit,
        ..op(OP_PEEK_LOSS, FLAG_AUX0_IMM | FLAG_AUX1_IMM)
    };
    let bytecode = program(&[&[
        gate(OPID_CX, 3, 2),
        gate(OPID_X, 4, 0),
        gate(OPID_CX, 0, 4),
        measure(OPID_MZ, 4, 0),
        reset(4),
        peek_loss(4),
        gate(OPID_X, (1 << 32) + 1, 0),
        ret(),
    ]]);
    let no_such_qubit = |qubit| QirError::NoSuchQubit { qubit, qubits: 4 };
    let mut expected = (1..=5)
        .map(|index| at_instruction(index, no_such_qubit(4)))
        .collect::<Vec<_>>();
    expected.push(at_instruction(6, no_such_qubit((1 << 32) + 1)));
    assert_eq!(faults(&bytecode), expected);
}

/// A measurement, a loss check, readout noise and a result written, on the
/// last result and on the one past, and a measurement into result
/// 2^32 + 1, which is result 1 cut to 32 bits.
#[test]
fn a_constant_result_written_must_exist() {
    let peek_loss = |result| Instruction {
        aux1: result,
        ..op(OP_PEEK_LOSS, FLAG_AUX0_IMM | FLAG_AUX1_IMM)
    };
    let readout_noise = |result| Instruction {
        aux2: result,
        ..op(
            OP_READOUT_NOISE,
            FLAG_AUX0_IMM | FLAG_AUX1_IMM | FLAG_AUX2_IMM,
        )
    };
    let write_result =
        |result| binary(OP_WRITE_RESULT, FLAG_SRC0_IMM | FLAG_SRC1_IMM, 0, 1, result);
    let bytecode = program(&[&[
        measure(OPID_MZ, 0, 3),
        peek_loss(3),
        readout_noise(3),
        write_result(3),
        measure(OPID_MZ, 0, 4),
        peek_loss(4),
        readout_noise(4),
        write_result(4),
        measure(OPID_MZ, 0, (1 << 32) + 1),
        ret(),
    ]]);
    let no_such_result = |result| QirError::NoSuchResult { result, results: 4 };
    let mut expected = (4..8)
        .map(|index| at_instruction(index, no_such_result(4)))
        .collect::<Vec<_>>();
    expected.push(at_instruction(8, no_such_result((1 << 32) + 1)));
    assert_eq!(faults(&bytecode), expected);
}

/// A result read, its loss read and its output recorded, on the last
/// result, on the one past and on result 2^32 + 1, which is result 1 cut
/// to 32 bits; and the constant 9 recorded as an integer and a double,
/// which is no result.
#[test]
fn a_constant_result_read_must_exist() {
    let reads = |opcode, result| binary(opcode, FLAG_SRC0_IMM, 0, result, 0);
    let value = |kind| Instruction {
        src0: 9,
        aux1: kind,
        ..op(OP_RECORD_OUTPUT, FLAG_SRC0_IMM)
    };
    let mut instructions = Vec::new();
    for result in [3, 4, (1 << 32) + 1] {
        for opcode in [OP_READ_RESULT, OP_READ_LOSS, OP_RECORD_OUTPUT] {
            instructions.push(reads(opcode, result));
        }
    }
    instructions.extend([value(4), value(5), ret()]);
    let bytecode = program(&[&instructions]);
    let no_such_result = |result| QirError::NoSuchResult { result, results: 4 };
    // COMPATIBILITY NOTE: `run_shot` (bytecode/runtime.rs,
    // `OP_READ_RESULT`, `OP_READ_LOSS`, `OP_RECORD_OUTPUT`) reads `Zero`
    // here; building refuses, because the read can never see a written
    // value: it is a producer's bug that would give plausible output
    // (see `QirMachine`'s doc).
    assert_eq!(
        faults(&bytecode),
        (3..6)
            .map(|index| at_instruction(index, no_such_result(4)))
            .chain((6..9).map(|index| at_instruction(index, no_such_result((1 << 32) + 1))))
            .collect::<Vec<_>>()
    );
}

/// Each division and remainder by the constant zero; by one, by 2^32
/// (whose low 32 bits are 0) and by a register, which build.
#[test]
fn a_division_by_the_constant_zero_is_refused() {
    let by = |opcode, divisor| binary(opcode, FLAG_SRC1_IMM, 0, 0, divisor);
    let bytecode = program(&[&[
        by(OP_UDIV, 0),
        by(OP_SDIV, 0),
        by(OP_UREM, 0),
        by(OP_SREM, 0),
        by(OP_UDIV, 1),
        by(OP_SDIV, 1 << 32),
        binary(OP_UDIV, 0, 0, 0, 0),
        ret(),
    ]]);
    assert_eq!(
        faults(&bytecode),
        (0..4)
            .map(|index| at_instruction(index, QirError::DivisionByZero))
            .collect::<Vec<_>>()
    );
}

/// The constants 0, 1, 2 and 2^32 (whose low 32 bits are 0) each read as
/// a bit, in a block of their own: written to a result, recorded as a
/// bool, and as a select's condition and a branch's.
#[test]
fn a_constant_read_as_a_bit_must_be_0_or_1() {
    let values = [0, 1, 2, 1 << 32];
    let blocks: Vec<Vec<_>> = (0..)
        .zip(values)
        .map(|(block, value)| {
            let mut on_value = branch(value, block + 1, block + 1);
            on_value.opcode |= FLAG_SRC0_IMM;
            vec![
                binary(OP_WRITE_RESULT, FLAG_SRC0_IMM | FLAG_SRC1_IMM, 0, value, 0),
                Instruction {
                    src0: value,
                    aux1: 3,
                    ..op(OP_RECORD_OUTPUT, FLAG_SRC0_IMM)
                },
                Instruction {
                    src0: value,
                    ..op(OP_SELECT, FLAG_SRC0_IMM)
                },
                on_value,
            ]
        })
        .chain([vec![ret()]])
        .collect();
    let blocks: Vec<&[Instruction<u64>]> = blocks.iter().map(Vec::as_slice).collect();
    // COMPATIBILITY NOTE: `run_shot` (bytecode/runtime.rs) reads such a
    // constant as true, but panics on it in a `WRITE_RESULT`; the machine
    // traps on a register holding one, and building refuses the constant,
    // which the bytecode shows, as it refuses the other content on which a
    // shot would stop.
    assert_eq!(
        faults(&program(&blocks)),
        (8..16)
            .map(|index| {
                let value = values[index / 4];
                at_instruction(index, QirError::NotABit { value })
            })
            .collect::<Vec<_>>()
    );
}

/// Outputs of each kind recorded from a register past the last: a result,
/// an array, a tuple, a bool, an integer and a double. The array and the
/// tuple only mark the output's shape, and read nothing.
#[test]
fn an_output_marker_reads_no_register() {
    let record = |kind| Instruction {
        src0: 4,
        aux1: kind,
        ..op(OP_RECORD_OUTPUT, 0)
    };
    let bytecode = program(&[&[
        record(0),
        record(1),
        record(2),
        record(3),
        record(4),
        record(5),
        ret(),
    ]]);
    let no_such_register = QirError::NoSuchRegister {
        register: 4,
        registers: 4,
    };
    assert_eq!(
        faults(&bytecode),
        [0, 3, 4, 5].map(|index| at_instruction(index, no_such_register.clone()))
    );
}

/// A program with a fault of each site, its instructions' faults in
/// reverse order of their kinds: the program's, then its blocks', its
/// functions', and its instructions', in their order.
#[test]
fn every_fault_is_reported_in_the_order_of_the_sites() {
    let mut bytecode = program(&[&[op(0x01, 0), gate(OPID_X, 4, 0), ret()]]);
    bytecode.entry_block = 2;
    bytecode.block_table.push(Block::from_tuple((3, 0)));
    bytecode.function_table = vec![Function::from_tuple((5, 0, 0))];
    assert_eq!(
        faults(&bytecode),
        [
            at(
                QirFaultSite::Program,
                QirError::NoSuchBlock {
                    block: 2,
                    blocks: 2
                }
            ),
            at(QirFaultSite::Block(1), QirError::EmptyBlock),
            at(
                QirFaultSite::Function(0),
                QirError::NoSuchBlock {
                    block: 5,
                    blocks: 2
                }
            ),
            at_instruction(0, QirError::UnknownOpcode { opcode: 0x01 }),
            at_instruction(
                1,
                QirError::NoSuchQubit {
                    qubit: 4,
                    qubits: 4
                }
            ),
        ]
    );
}

/// Two faults, read as one message.
#[test]
fn faults_read_as_one_message_of_their_sites_and_errors() {
    let faults = QirFaults(vec![
        at(QirFaultSite::Block(1), QirError::EmptyBlock),
        at_instruction(2, QirError::DivisionByZero),
    ]);
    assert_eq!(
        faults.to_string(),
        "block 1: it holds no instruction; instruction 2: a division by the constant zero"
    );
}
