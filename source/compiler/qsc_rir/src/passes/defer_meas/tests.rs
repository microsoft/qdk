// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines, clippy::needless_raw_string_hashes)]

use crate::{
    builder,
    rir::{
        Block, BlockId, CallableId, InstructionKind, Literal, Operand, Program, Ty, Variable,
        VariableId,
    },
};
use expect_test::expect;

use super::defer_measurements;

#[test]
fn measurements_deferred_on_return_block() {
    let mut program = Program::default();
    add_simple_measurement_block(&mut program);

    // Before
    expect![[r#"
        Block:
            Call id(0), args( Qubit(0), Result(0), )
            Call id(2), args( Qubit(1), )
            Call id(1), args( Qubit(1), Result(1), )
            Call id(1), args( Qubit(0), Result(2), )
            Call id(3), args( Integer(3), Pointer, )
            Call id(4), args( Result(0), Pointer, )
            Call id(4), args( Result(1), Pointer, )
            Call id(4), args( Result(2), Pointer, )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    // After
    defer_measurements(&mut program);
    expect![[r#"
        Block:
            Call id(2), args( Qubit(1), )
            Call id(0), args( Qubit(0), Result(0), )
            Call id(1), args( Qubit(1), Result(1), )
            Call id(1), args( Qubit(0), Result(2), )
            Call id(3), args( Integer(3), Pointer, )
            Call id(4), args( Result(0), Pointer, )
            Call id(4), args( Result(1), Pointer, )
            Call id(4), args( Result(2), Pointer, )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
}

#[test]
fn branching_block_measurements_deferred_properly() {
    let mut program = Program::default();
    add_branching_measurement_block(&mut program);

    // Before
    expect![[r#"
        Block:
            Call id(0), args( Qubit(1), Result(0), )
            Call id(1), args( Qubit(0), )
            Call id(0), args( Qubit(0), Result(1), )
            Variable(0, Boolean) = Call id(3), args( Result(0), )
            Branch Variable(0, Boolean), 1, 2"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    // After
    defer_measurements(&mut program);
    expect![[r#"
        Block:
            Call id(1), args( Qubit(0), )
            Call id(0), args( Qubit(1), Result(0), )
            Call id(0), args( Qubit(0), Result(1), )
            Variable(0, Boolean) = Call id(3), args( Result(0), )
            Branch Variable(0, Boolean), 1, 2"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
}

fn add_simple_measurement_block(program: &mut Program) {
    program.callables.insert(CallableId(0), builder::m_decl());
    program
        .callables
        .insert(CallableId(1), builder::mresetz_decl());
    program.callables.insert(CallableId(2), builder::x_decl());
    program
        .callables
        .insert(CallableId(3), builder::array_record_decl());
    program
        .callables
        .insert(CallableId(4), builder::result_record_decl());
    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![
            InstructionKind::Call(
                CallableId(0),
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
            ),
            InstructionKind::Call(
                CallableId(2),
                vec![Operand::Literal(Literal::Qubit(1))],
                None,
            ),
            InstructionKind::Call(
                CallableId(1),
                vec![
                    Operand::Literal(Literal::Qubit(1)),
                    Operand::Literal(Literal::Result(1)),
                ],
                None,
            ),
            InstructionKind::Call(
                CallableId(1),
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(2)),
                ],
                None,
            ),
            InstructionKind::Call(
                CallableId(3),
                vec![
                    Operand::Literal(Literal::Integer(3)),
                    Operand::Literal(Literal::Pointer),
                ],
                None,
            ),
            InstructionKind::Call(
                CallableId(4),
                vec![
                    Operand::Literal(Literal::Result(0)),
                    Operand::Literal(Literal::Pointer),
                ],
                None,
            ),
            InstructionKind::Call(
                CallableId(4),
                vec![
                    Operand::Literal(Literal::Result(1)),
                    Operand::Literal(Literal::Pointer),
                ],
                None,
            ),
            InstructionKind::Call(
                CallableId(4),
                vec![
                    Operand::Literal(Literal::Result(2)),
                    Operand::Literal(Literal::Pointer),
                ],
                None,
            ),
            InstructionKind::Return,
        ]),
    );
}

fn add_branching_measurement_block(program: &mut Program) {
    program
        .callables
        .insert(CallableId(0), builder::mresetz_decl());
    program.callables.insert(CallableId(1), builder::x_decl());
    program
        .callables
        .insert(CallableId(3), builder::read_result_decl());
    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![
            InstructionKind::Call(
                CallableId(0),
                vec![
                    Operand::Literal(Literal::Qubit(1)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
            ),
            InstructionKind::Call(
                CallableId(1),
                vec![Operand::Literal(Literal::Qubit(0))],
                None,
            ),
            InstructionKind::Call(
                CallableId(0),
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(1)),
                ],
                None,
            ),
            InstructionKind::Call(
                CallableId(3),
                vec![Operand::Literal(Literal::Result(0))],
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
            ),
            InstructionKind::Branch(
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                },
                BlockId(1),
                BlockId(2),
            ),
        ]),
    );
}
