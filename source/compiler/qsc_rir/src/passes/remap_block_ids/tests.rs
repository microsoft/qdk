// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines, clippy::needless_raw_string_hashes)]

use expect_test::expect;

use crate::rir::{
    Block, BlockId, Callable, CallableId, CallableType, Instruction, Program, Ty, Variable,
    VariableId,
};

use super::remap_block_ids;

#[test]
fn remap_block_ids_no_changes() {
    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program
        .blocks
        .insert(BlockId(0), Block(vec![Instruction::Jump(BlockId(1))]));
    program
        .blocks
        .insert(BlockId(1), Block(vec![Instruction::Jump(BlockId(2))]));
    program
        .blocks
        .insert(BlockId(2), Block(vec![Instruction::Return]));

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 0
            blocks:
                Block 0: Block:
                    Jump(1)
                Block 1: Block:
                    Jump(2)
                Block 2: Block:
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    remap_block_ids(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 0
            blocks:
                Block 0: Block:
                    Jump(1)
                Block 1: Block:
                    Jump(2)
                Block 2: Block:
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
fn remap_block_ids_out_of_order_no_branches() {
    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            output_type: None,
            body: Some(BlockId(5)),
            call_type: CallableType::Regular,
        },
    );
    program
        .blocks
        .insert(BlockId(5), Block(vec![Instruction::Jump(BlockId(3))]));
    program
        .blocks
        .insert(BlockId(3), Block(vec![Instruction::Jump(BlockId(7))]));
    program
        .blocks
        .insert(BlockId(7), Block(vec![Instruction::Return]));

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 5
            blocks:
                Block 3: Block:
                    Jump(7)
                Block 5: Block:
                    Jump(3)
                Block 7: Block:
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    remap_block_ids(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 0
            blocks:
                Block 0: Block:
                    Jump(1)
                Block 1: Block:
                    Jump(2)
                Block 2: Block:
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
fn remap_block_ids_out_of_order_with_one_branch() {
    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            output_type: None,
            body: Some(BlockId(2)),
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![Instruction::Branch(
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Boolean,
            },
            BlockId(3),
            BlockId(1),
        )]),
    );
    program
        .blocks
        .insert(BlockId(1), Block(vec![Instruction::Jump(BlockId(0))]));
    program
        .blocks
        .insert(BlockId(3), Block(vec![Instruction::Jump(BlockId(1))]));
    program
        .blocks
        .insert(BlockId(0), Block(vec![Instruction::Return]));

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 2
            blocks:
                Block 0: Block:
                    Return
                Block 1: Block:
                    Jump(0)
                Block 2: Block:
                    Branch Variable(0, Boolean), 3, 1
                Block 3: Block:
                    Jump(1)
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    remap_block_ids(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 0
            blocks:
                Block 0: Block:
                    Branch Variable(0, Boolean), 1, 2
                Block 1: Block:
                    Jump(2)
                Block 2: Block:
                    Jump(3)
                Block 3: Block:
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
fn remap_block_ids_simple_loop() {
    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            output_type: None,
            body: Some(BlockId(4)),
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(4),
        Block(vec![Instruction::Branch(
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Boolean,
            },
            BlockId(6),
            BlockId(2),
        )]),
    );
    program
        .blocks
        .insert(BlockId(6), Block(vec![Instruction::Jump(BlockId(4))]));
    program
        .blocks
        .insert(BlockId(2), Block(vec![Instruction::Return]));

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 4
            blocks:
                Block 2: Block:
                    Return
                Block 4: Block:
                    Branch Variable(0, Boolean), 6, 2
                Block 6: Block:
                    Jump(4)
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    remap_block_ids(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 0
            blocks:
                Block 0: Block:
                    Branch Variable(0, Boolean), 1, 2
                Block 1: Block:
                    Jump(0)
                Block 2: Block:
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
fn remap_block_ids_infinite_loop() {
    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            output_type: None,
            body: Some(BlockId(4)),
            call_type: CallableType::Regular,
        },
    );
    program
        .blocks
        .insert(BlockId(4), Block(vec![Instruction::Jump(BlockId(0))]));
    program
        .blocks
        .insert(BlockId(0), Block(vec![Instruction::Jump(BlockId(4))]));

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 4
            blocks:
                Block 0: Block:
                    Jump(4)
                Block 4: Block:
                    Jump(0)
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    remap_block_ids(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 0
            blocks:
                Block 0: Block:
                    Jump(1)
                Block 1: Block:
                    Jump(0)
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());
}

#[test]
fn remap_block_ids_nested_branching_loops() {
    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            output_type: None,
            body: Some(BlockId(4)),
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(4),
        Block(vec![Instruction::Branch(
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Boolean,
            },
            BlockId(6),
            BlockId(2),
        )]),
    );
    program.blocks.insert(
        BlockId(6),
        Block(vec![Instruction::Branch(
            Variable {
                variable_id: VariableId(1),
                ty: Ty::Boolean,
            },
            BlockId(4),
            BlockId(2),
        )]),
    );
    program
        .blocks
        .insert(BlockId(2), Block(vec![Instruction::Return]));

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 4
            blocks:
                Block 2: Block:
                    Return
                Block 4: Block:
                    Branch Variable(0, Boolean), 6, 2
                Block 6: Block:
                    Branch Variable(1, Boolean), 4, 2
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    remap_block_ids(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 0
            blocks:
                Block 0: Block:
                    Branch Variable(0, Boolean), 1, 2
                Block 1: Block:
                    Branch Variable(1, Boolean), 0, 2
                Block 2: Block:
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
fn remap_block_ids_ensures_acyclic_program_gets_topological_ordering() {
    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            output_type: None,
            body: Some(BlockId(4)),
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(4),
        Block(vec![Instruction::Branch(
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Boolean,
            },
            BlockId(6),
            BlockId(2),
        )]),
    );
    program
        .blocks
        .insert(BlockId(6), Block(vec![Instruction::Jump(BlockId(2))]));
    program.blocks.insert(
        BlockId(2),
        Block(vec![Instruction::Branch(
            Variable {
                variable_id: VariableId(1),
                ty: Ty::Boolean,
            },
            BlockId(1),
            BlockId(3),
        )]),
    );
    program
        .blocks
        .insert(BlockId(1), Block(vec![Instruction::Jump(BlockId(7))]));
    program.blocks.insert(
        BlockId(3),
        Block(vec![Instruction::Branch(
            Variable {
                variable_id: VariableId(2),
                ty: Ty::Boolean,
            },
            BlockId(5),
            BlockId(0),
        )]),
    );
    program
        .blocks
        .insert(BlockId(5), Block(vec![Instruction::Jump(BlockId(8))]));
    program
        .blocks
        .insert(BlockId(0), Block(vec![Instruction::Jump(BlockId(8))]));
    program
        .blocks
        .insert(BlockId(8), Block(vec![Instruction::Jump(BlockId(7))]));
    program
        .blocks
        .insert(BlockId(7), Block(vec![Instruction::Return]));

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 4
            blocks:
                Block 0: Block:
                    Jump(8)
                Block 1: Block:
                    Jump(7)
                Block 2: Block:
                    Branch Variable(1, Boolean), 1, 3
                Block 3: Block:
                    Branch Variable(2, Boolean), 5, 0
                Block 4: Block:
                    Branch Variable(0, Boolean), 6, 2
                Block 5: Block:
                    Jump(8)
                Block 6: Block:
                    Jump(2)
                Block 7: Block:
                    Return
                Block 8: Block:
                    Jump(7)
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    remap_block_ids(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 0
            blocks:
                Block 0: Block:
                    Branch Variable(0, Boolean), 1, 2
                Block 1: Block:
                    Jump(2)
                Block 2: Block:
                    Branch Variable(1, Boolean), 3, 4
                Block 3: Block:
                    Jump(8)
                Block 4: Block:
                    Branch Variable(2, Boolean), 5, 6
                Block 5: Block:
                    Jump(7)
                Block 6: Block:
                    Jump(7)
                Block 7: Block:
                    Jump(8)
                Block 8: Block:
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());
}
