// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines, clippy::needless_raw_string_hashes)]

use expect_test::expect;

use crate::{
    builder::{bell_program, teleport_program},
    rir::{Block, BlockId, Instruction, Literal, Operand, Program, Ty, Variable, VariableId},
};

use super::simplify_control_flow;

#[test]
fn simplify_control_flow_leaves_program_with_single_block_unchanged() {
    let mut program = bell_program();
    let program_before = program.to_string();
    simplify_control_flow(&mut program);
    assert_eq!(program.to_string(), program_before);
}

#[test]
fn simplify_control_flow_leaves_program_with_branching_and_no_extra_blocks_unchanged() {
    let mut program = teleport_program();
    let program_before = program.to_string();
    simplify_control_flow(&mut program);
    assert_eq!(program.to_string(), program_before);
}

#[test]
fn simplify_control_flow_removes_single_redundant_block() {
    let mut program = Program::new();
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Jump(BlockId(1)),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Return,
        ]),
    );

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Store Bool(true)
                    Jump(1)
                Block 1: Block:
                    Variable(1, Boolean) = Store Bool(true)
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    simplify_control_flow(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Store Bool(true)
                    Variable(1, Boolean) = Store Bool(true)
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());
}

#[test]
fn simplify_control_flow_removes_multiple_redundant_blocks() {
    let mut program = Program::new();
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Jump(BlockId(1)),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Jump(BlockId(2)),
        ]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Return,
        ]),
    );

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Store Bool(true)
                    Jump(1)
                Block 1: Block:
                    Variable(1, Boolean) = Store Bool(true)
                    Jump(2)
                Block 2: Block:
                    Variable(2, Boolean) = Store Bool(true)
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    simplify_control_flow(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Store Bool(true)
                    Variable(1, Boolean) = Store Bool(true)
                    Variable(2, Boolean) = Store Bool(true)
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());
}

#[test]
fn simplify_control_flow_removes_redundant_blocks_across_branches() {
    let mut program = Program::new();
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                },
                BlockId(1),
                BlockId(6),
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Jump(BlockId(2)),
        ]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Jump(BlockId(4)),
        ]),
    );
    program.blocks.insert(
        BlockId(4),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Jump(BlockId(6)),
        ]),
    );
    program.blocks.insert(
        BlockId(6),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Jump(BlockId(7)),
        ]),
    );
    program.blocks.insert(
        BlockId(7),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(5),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Return,
        ]),
    );

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Store Bool(true)
                    Branch Variable(0, Boolean), 1, 6
                Block 1: Block:
                    Variable(1, Boolean) = Store Bool(true)
                    Jump(2)
                Block 2: Block:
                    Variable(2, Boolean) = Store Bool(true)
                    Jump(4)
                Block 4: Block:
                    Variable(3, Boolean) = Store Bool(true)
                    Jump(6)
                Block 6: Block:
                    Variable(4, Boolean) = Store Bool(true)
                    Jump(7)
                Block 7: Block:
                    Variable(5, Boolean) = Store Bool(true)
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    simplify_control_flow(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Store Bool(true)
                    Branch Variable(0, Boolean), 1, 6
                Block 1: Block:
                    Variable(1, Boolean) = Store Bool(true)
                    Variable(2, Boolean) = Store Bool(true)
                    Variable(3, Boolean) = Store Bool(true)
                    Jump(6)
                Block 6: Block:
                    Variable(4, Boolean) = Store Bool(true)
                    Variable(5, Boolean) = Store Bool(true)
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());
}

#[test]
fn simplify_control_flow_removes_redundant_blocks_across_out_of_order_branches() {
    let mut program = Program::new();
    // 0 -> 3
    // 1 -> 0
    // 6 -> 2
    // 2 -> 1
    // 4 -> 4
    // 7 -> 5
    program.blocks.insert(
        BlockId(3),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                },
                BlockId(0),
                BlockId(2),
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Jump(BlockId(1)),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Jump(BlockId(4)),
        ]),
    );
    program.blocks.insert(
        BlockId(4),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Jump(BlockId(2)),
        ]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Jump(BlockId(5)),
        ]),
    );
    program.blocks.insert(
        BlockId(5),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(5),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Return,
        ]),
    );

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
            blocks:
                Block 0: Block:
                    Variable(1, Boolean) = Store Bool(true)
                    Jump(1)
                Block 1: Block:
                    Variable(2, Boolean) = Store Bool(true)
                    Jump(4)
                Block 2: Block:
                    Variable(4, Boolean) = Store Bool(true)
                    Jump(5)
                Block 3: Block:
                    Variable(0, Boolean) = Store Bool(true)
                    Branch Variable(0, Boolean), 0, 2
                Block 4: Block:
                    Variable(3, Boolean) = Store Bool(true)
                    Jump(2)
                Block 5: Block:
                    Variable(5, Boolean) = Store Bool(true)
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    simplify_control_flow(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
            blocks:
                Block 0: Block:
                    Variable(1, Boolean) = Store Bool(true)
                    Variable(2, Boolean) = Store Bool(true)
                    Variable(3, Boolean) = Store Bool(true)
                    Jump(2)
                Block 2: Block:
                    Variable(4, Boolean) = Store Bool(true)
                    Variable(5, Boolean) = Store Bool(true)
                    Return
                Block 3: Block:
                    Variable(0, Boolean) = Store Bool(true)
                    Branch Variable(0, Boolean), 0, 2
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());
}
