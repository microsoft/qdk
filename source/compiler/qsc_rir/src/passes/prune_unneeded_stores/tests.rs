// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines)]

use expect_test::expect;

use crate::rir::{
    Block, BlockId, CallableId, Instruction, Literal, Operand, Program, Variable, VariableId,
};

use super::prune_unneeded_stores;

#[test]
fn removes_store_without_use() {
    let mut program = Program::with_blocks(vec![(
        BlockId(0),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable::new_boolean(VariableId(0)),
            ),
            Instruction::Return,
        ]),
    )]);

    // Before
    expect![[r#"
        Block:
            Variable(0, Boolean) = Store Bool(true)
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    prune_unneeded_stores(&mut program);

    // After
    expect![[r#"
        Block:
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
}

#[test]
fn propagates_literal_within_block() {
    let stored_var = Variable::new_boolean(VariableId(0));
    let mut program = Program::with_blocks(vec![(
        BlockId(0),
        Block(vec![
            Instruction::Store(Operand::Literal(Literal::Bool(false)), stored_var.clone()),
            Instruction::LogicalNot(
                Operand::Variable(stored_var),
                Variable::new_boolean(VariableId(1)),
            ),
            Instruction::Return,
        ]),
    )]);

    // Before
    expect![[r#"
        Block:
            Variable(0, Boolean) = Store Bool(false)
            Variable(1, Boolean) = LogicalNot Variable(0, Boolean)
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    prune_unneeded_stores(&mut program);

    // After
    expect![[r#"
        Block:
            Variable(1, Boolean) = LogicalNot Bool(false)
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
}

#[test]
fn keeps_store_for_cross_block_use() {
    let stored_var = Variable::new_boolean(VariableId(0));
    let mut program = Program::with_blocks(vec![
        (
            BlockId(0),
            Block(vec![
                Instruction::Store(Operand::Literal(Literal::Bool(true)), stored_var.clone()),
                Instruction::Jump(BlockId(1)),
            ]),
        ),
        (
            BlockId(1),
            Block(vec![
                Instruction::LogicalNot(
                    Operand::Variable(stored_var),
                    Variable::new_boolean(VariableId(1)),
                ),
                Instruction::Return,
            ]),
        ),
    ]);

    // Before
    expect![[r#"
        Block:
            Variable(0, Boolean) = Store Bool(true)
            Jump(1)"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
    expect![[r#"
        Block:
            Variable(1, Boolean) = LogicalNot Variable(0, Boolean)
            Return"#]]
    .assert_eq(&program.get_block(BlockId(1)).to_string());

    prune_unneeded_stores(&mut program);

    // After
    expect![[r#"
        Block:
            Variable(0, Boolean) = Store Bool(true)
            Jump(1)"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    expect![[r#"
        Block:
            Variable(1, Boolean) = LogicalNot Variable(0, Boolean)
            Return"#]]
    .assert_eq(&program.get_block(BlockId(1)).to_string());
}

#[test]
fn removes_overwritten_store_and_keeps_last_value() {
    let stored_var = Variable::new_boolean(VariableId(0));
    let mut program = Program::with_blocks(vec![(
        BlockId(0),
        Block(vec![
            Instruction::Store(Operand::Literal(Literal::Bool(true)), stored_var.clone()),
            Instruction::Store(Operand::Literal(Literal::Bool(false)), stored_var.clone()),
            Instruction::Call(CallableId(1), vec![Operand::Variable(stored_var)], None),
            Instruction::Return,
        ]),
    )]);

    // Before
    expect![[r#"
            Block:
                Variable(0, Boolean) = Store Bool(true)
                Variable(0, Boolean) = Store Bool(false)
                Call id(1), args( Variable(0, Boolean), )
                Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    prune_unneeded_stores(&mut program);

    // After
    expect![[r#"
        Block:
            Call id(1), args( Bool(false), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
}

#[test]
fn propagates_chained_stores() {
    let source_var = Variable::new_boolean(VariableId(0));
    let alias_var = Variable::new_boolean(VariableId(1));
    let mut program = Program::with_blocks(vec![(
        BlockId(0),
        Block(vec![
            Instruction::Store(Operand::Literal(Literal::Bool(true)), source_var.clone()),
            Instruction::Store(Operand::Variable(source_var.clone()), alias_var.clone()),
            Instruction::Call(CallableId(1), vec![Operand::Variable(alias_var)], None),
            Instruction::Return,
        ]),
    )]);

    // Before
    expect![[r#"
            Block:
                Variable(0, Boolean) = Store Bool(true)
                Variable(1, Boolean) = Store Variable(0, Boolean)
                Call id(1), args( Variable(1, Boolean), )
                Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    prune_unneeded_stores(&mut program);

    // After
    expect![[r#"
        Block:
            Call id(1), args( Bool(true), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
}
