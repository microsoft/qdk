// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines)]

use expect_test::expect;

use crate::rir::{
    AdvancedInstr, Block, BlockId, CallableId, Instruction, Literal, Operand, Program, Variable,
    VariableId,
};

use super::insert_alloca_load_instrs;

#[test]
fn inserts_alloca_and_load_for_branch_and_call() {
    let stored_var = Variable::new_boolean(VariableId(0));
    let mut program = Program::with_blocks(vec![
        (
            BlockId(0),
            Block(vec![
                Instruction::Store(Operand::Literal(Literal::Bool(true)), stored_var.clone()),
                Instruction::Call(
                    CallableId(1),
                    vec![Operand::Variable(stored_var.clone())],
                    None,
                ),
                Instruction::Branch(stored_var, BlockId(1), BlockId(2)),
            ]),
        ),
        (BlockId(1), Block(vec![Instruction::Return])),
        (BlockId(2), Block(vec![Instruction::Return])),
    ]);

    // Before
    expect![[r#"
        Block:
            Variable(0, Boolean) = Store Bool(true)
            Call id(1), args( Variable(0, Boolean), )
            Branch Variable(0, Boolean), 1, 2"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    insert_alloca_load_instrs(&mut program);

    // After
    expect![[r#"
        Block:
            Variable(0, Boolean) = Alloca
            Variable(0, Boolean) = Store Bool(true)
            Variable(2, Boolean) = Load Variable(0, Boolean)
            Call id(1), args( Variable(2, Boolean), )
            Branch Variable(2, Boolean), 1, 2"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
}

#[test]
fn reuses_single_load_within_block() {
    let stored_var = Variable::new_integer(VariableId(0));
    let sum_var = Variable::new_integer(VariableId(1));
    let mut program = Program::with_blocks(vec![(
        BlockId(0),
        Block(vec![
            Instruction::Store(Operand::Literal(Literal::Integer(5)), stored_var.clone()),
            Instruction::Add(
                Operand::Variable(stored_var.clone()),
                Operand::Variable(stored_var.clone()),
                sum_var,
            ),
            Instruction::Call(CallableId(1), vec![Operand::Variable(stored_var)], None),
            Instruction::Return,
        ]),
    )]);

    // Before
    expect![[r#"
        Block:
            Variable(0, Integer) = Store Integer(5)
            Variable(1, Integer) = Add Variable(0, Integer), Variable(0, Integer)
            Call id(1), args( Variable(0, Integer), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    insert_alloca_load_instrs(&mut program);

    let block = program.get_block(BlockId(0));
    let load_count = block
        .0
        .iter()
        .filter(|instr| matches!(instr, Instruction::Advanced(AdvancedInstr::Load(..))))
        .count();
    assert_eq!(
        load_count, 1,
        "expected a single load for all uses within the block"
    );

    // After
    expect![[r#"
        Block:
            Variable(0, Integer) = Alloca
            Variable(0, Integer) = Store Integer(5)
            Variable(3, Integer) = Load Variable(0, Integer)
            Variable(1, Integer) = Add Variable(3, Integer), Variable(3, Integer)
            Call id(1), args( Variable(3, Integer), )
            Return"#]]
    .assert_eq(&block.to_string());
}

#[test]
fn inserts_load_in_successor_block() {
    let stored_var = Variable::new_boolean(VariableId(0));
    let result_var = Variable::new_boolean(VariableId(1));
    let mut program = Program::with_blocks(vec![
        (
            BlockId(0),
            Block(vec![
                Instruction::Store(Operand::Literal(Literal::Bool(false)), stored_var.clone()),
                Instruction::Jump(BlockId(1)),
            ]),
        ),
        (
            BlockId(1),
            Block(vec![
                Instruction::LogicalNot(Operand::Variable(stored_var), result_var),
                Instruction::Return,
            ]),
        ),
    ]);

    // Before
    expect![[r#"
        Block:
            Variable(0, Boolean) = Store Bool(false)
            Jump(1)"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
    expect![[r#"
        Block:
            Variable(1, Boolean) = LogicalNot Variable(0, Boolean)
            Return"#]]
    .assert_eq(&program.get_block(BlockId(1)).to_string());

    insert_alloca_load_instrs(&mut program);

    // After block 0
    expect![[r#"
        Block:
            Variable(0, Boolean) = Alloca
            Variable(0, Boolean) = Store Bool(false)
            Jump(1)"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    // After block 1
    expect![[r#"
        Block:
            Variable(3, Boolean) = Load Variable(0, Boolean)
            Variable(1, Boolean) = LogicalNot Variable(3, Boolean)
            Return"#]]
    .assert_eq(&program.get_block(BlockId(1)).to_string());
}

#[test]
fn leaves_unrelated_operands_unloaded() {
    let stored_var = Variable::new_boolean(VariableId(0));
    let unrelated_var = Variable::new_boolean(VariableId(1));
    let mut program = Program::with_blocks(vec![(
        BlockId(0),
        Block(vec![
            Instruction::Store(Operand::Literal(Literal::Bool(true)), stored_var),
            Instruction::Call(CallableId(1), vec![Operand::Variable(unrelated_var)], None),
            Instruction::Return,
        ]),
    )]);

    // Before
    expect![[r#"
        Block:
            Variable(0, Boolean) = Store Bool(true)
            Call id(1), args( Variable(1, Boolean), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    insert_alloca_load_instrs(&mut program);

    let block = program.get_block(BlockId(0));
    assert!(
        block
            .0
            .iter()
            .all(|instr| !matches!(instr, Instruction::Advanced(AdvancedInstr::Load(..)))),
        "no loads should be inserted for operands unrelated to stored variables",
    );

    // After
    expect![[r#"
        Block:
            Variable(0, Boolean) = Alloca
            Variable(0, Boolean) = Store Bool(true)
            Call id(1), args( Variable(1, Boolean), )
            Return"#]]
    .assert_eq(&block.to_string());
}
