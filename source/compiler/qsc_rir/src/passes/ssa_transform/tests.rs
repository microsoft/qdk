// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines, clippy::needless_raw_string_hashes)]

use expect_test::expect;
use qsc_data_structures::target::Profile;

use crate::{
    builder::{
        bell_program, new_program, teleport_program, two_body_mutable_param_program,
        two_body_program, two_body_program_with_branch, two_body_program_with_loop,
    },
    passes::{check_and_transform, test_utils::assert_panics_with},
    rir::{
        Block, BlockId, Callable, CallableId, CallableType, Instruction, Literal, Operand, Prim,
        Program, Ty, Variable, VariableId,
    },
    utils::build_predecessors_map,
};
fn transform_program(program: &mut Program) {
    program.config.capabilities = Profile::AdaptiveRIF.into();
    check_and_transform(program);
}

// Runs only the store-to-SSA/phi transform on a program, building the predecessor map directly from
// the program. This isolates the transform from the dominator-graph build and SSA checker, which is
// useful for exercising multi-body programs.
fn transform_to_ssa_directly(program: &mut Program) {
    let preds = build_predecessors_map(program);
    super::transform_to_ssa(program, &preds);
}

#[test]
fn ssa_transform_leaves_program_without_store_instruction_unchanged() {
    let mut program = bell_program();
    program.config.capabilities = Profile::AdaptiveRIF.into();
    let program_string_orignal = program.to_string();
    transform_program(&mut program);

    assert_eq!(program_string_orignal, program.to_string());
}

#[test]
fn ssa_transform_leaves_branching_program_without_store_instruction_unchanged() {
    let mut program = teleport_program();
    program.config.capabilities = Profile::AdaptiveRIF.into();
    let program_string_orignal = program.to_string();
    transform_program(&mut program);

    assert_eq!(program_string_orignal, program.to_string());
}

#[test]
fn ssa_transform_removes_store_in_single_block_program() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                Vec::new(),
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Return(None),
        ]),
    );

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Variable(1, Boolean) = Store Variable(0, Boolean)
                    Variable(2, Boolean) = LogicalNot Variable(1, Boolean)
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    transform_program(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Variable(2, Boolean) = LogicalNot Variable(0, Boolean)
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_removes_multiple_stores_in_single_block_program() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                Vec::new(),
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Return(None),
        ]),
    );

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Variable(1, Boolean) = Store Variable(0, Boolean)
                    Variable(2, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(1, Boolean) = Store Variable(2, Boolean)
                    Variable(3, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(1, Boolean) = Store Variable(3, Boolean)
                    Variable(4, Boolean) = LogicalNot Variable(1, Boolean)
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    transform_program(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Variable(2, Boolean) = LogicalNot Variable(0, Boolean)
                    Variable(3, Boolean) = LogicalNot Variable(2, Boolean)
                    Variable(4, Boolean) = LogicalNot Variable(3, Boolean)
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_store_dominating_usage_propagates_to_successor_blocks() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                Vec::new(),
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(1),
                BlockId(2),
                None,
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(3)),
        ]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(3)),
        ]),
    );
    program.blocks.insert(
        BlockId(3),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Return(None),
        ]),
    );

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Variable(1, Boolean) = Store Variable(0, Boolean)
                    Branch Variable(1, Boolean), 1, 2
                Block 1: Block:
                    Variable(2, Boolean) = LogicalNot Variable(1, Boolean)
                    Jump(3)
                Block 2: Block:
                    Variable(3, Boolean) = LogicalNot Variable(1, Boolean)
                    Jump(3)
                Block 3: Block:
                    Variable(4, Boolean) = LogicalNot Variable(1, Boolean)
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    transform_program(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Branch Variable(0, Boolean), 1, 2
                Block 1: Block:
                    Variable(2, Boolean) = LogicalNot Variable(0, Boolean)
                    Jump(3)
                Block 2: Block:
                    Variable(3, Boolean) = LogicalNot Variable(0, Boolean)
                    Jump(3)
                Block 3: Block:
                    Variable(4, Boolean) = LogicalNot Variable(0, Boolean)
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_store_dominating_usage_propagates_to_successor_blocks_without_intermediate_usage()
{
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                Vec::new(),
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(1),
                BlockId(2),
                None,
            ),
        ]),
    );
    program
        .blocks
        .insert(BlockId(1), Block(vec![Instruction::Jump(BlockId(3))]));
    program
        .blocks
        .insert(BlockId(2), Block(vec![Instruction::Jump(BlockId(3))]));
    program.blocks.insert(
        BlockId(3),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Return(None),
        ]),
    );

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Variable(1, Boolean) = Store Variable(0, Boolean)
                    Branch Variable(1, Boolean), 1, 2
                Block 1: Block:
                    Jump(3)
                Block 2: Block:
                    Jump(3)
                Block 3: Block:
                    Variable(4, Boolean) = LogicalNot Variable(1, Boolean)
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    transform_program(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Branch Variable(0, Boolean), 1, 2
                Block 1: Block:
                    Jump(3)
                Block 2: Block:
                    Jump(3)
                Block 3: Block:
                    Variable(4, Boolean) = LogicalNot Variable(0, Boolean)
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_inserts_phi_for_store_not_dominating_usage() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                Vec::new(),
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(1),
                BlockId(2),
                None,
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(3)),
        ]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(3)),
        ]),
    );
    program.blocks.insert(
        BlockId(3),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Return(None),
        ]),
    );

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Variable(1, Boolean) = Store Variable(0, Boolean)
                    Branch Variable(1, Boolean), 1, 2
                Block 1: Block:
                    Variable(2, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(1, Boolean) = Store Variable(2, Boolean)
                    Jump(3)
                Block 2: Block:
                    Variable(3, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(1, Boolean) = Store Variable(3, Boolean)
                    Jump(3)
                Block 3: Block:
                    Variable(4, Boolean) = LogicalNot Variable(1, Boolean)
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    transform_program(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Branch Variable(0, Boolean), 1, 2
                Block 1: Block:
                    Variable(2, Boolean) = LogicalNot Variable(0, Boolean)
                    Jump(3)
                Block 2: Block:
                    Variable(3, Boolean) = LogicalNot Variable(0, Boolean)
                    Jump(3)
                Block 3: Block:
                    Variable(5, Boolean) = Phi ( [Variable(2, Boolean), 1], [Variable(3, Boolean), 2], )
                    Variable(4, Boolean) = LogicalNot Variable(5, Boolean)
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 0
            num_results: 0
            tags:
    "#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_inserts_phi_for_store_not_dominating_usage_in_one_branch() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                Vec::new(),
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(1),
                BlockId(2),
                None,
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(3)),
        ]),
    );
    program
        .blocks
        .insert(BlockId(2), Block(vec![Instruction::Jump(BlockId(3))]));
    program.blocks.insert(
        BlockId(3),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Return(None),
        ]),
    );

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Variable(1, Boolean) = Store Variable(0, Boolean)
                    Branch Variable(1, Boolean), 1, 2
                Block 1: Block:
                    Variable(2, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(1, Boolean) = Store Variable(2, Boolean)
                    Jump(3)
                Block 2: Block:
                    Jump(3)
                Block 3: Block:
                    Variable(4, Boolean) = LogicalNot Variable(1, Boolean)
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    transform_program(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Branch Variable(0, Boolean), 1, 2
                Block 1: Block:
                    Variable(2, Boolean) = LogicalNot Variable(0, Boolean)
                    Jump(3)
                Block 2: Block:
                    Jump(3)
                Block 3: Block:
                    Variable(5, Boolean) = Phi ( [Variable(2, Boolean), 1], [Variable(0, Boolean), 2], )
                    Variable(4, Boolean) = LogicalNot Variable(5, Boolean)
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 0
            num_results: 0
            tags:
    "#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_inserts_phi_for_node_with_many_predecessors() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                Vec::new(),
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(1),
                BlockId(2),
                None,
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(3),
                BlockId(4),
                None,
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(5),
                BlockId(6),
                None,
            ),
        ]),
    );
    program
        .blocks
        .insert(BlockId(3), Block(vec![Instruction::Jump(BlockId(7))]));
    program
        .blocks
        .insert(BlockId(4), Block(vec![Instruction::Jump(BlockId(7))]));
    program
        .blocks
        .insert(BlockId(5), Block(vec![Instruction::Jump(BlockId(7))]));
    program.blocks.insert(
        BlockId(6),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(7)),
        ]),
    );
    program.blocks.insert(
        BlockId(7),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(5),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Return(None),
        ]),
    );

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Variable(1, Boolean) = Store Variable(0, Boolean)
                    Branch Variable(1, Boolean), 1, 2
                Block 1: Block:
                    Variable(2, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(1, Boolean) = Store Variable(2, Boolean)
                    Branch Variable(1, Boolean), 3, 4
                Block 2: Block:
                    Variable(3, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(1, Boolean) = Store Variable(3, Boolean)
                    Branch Variable(1, Boolean), 5, 6
                Block 3: Block:
                    Jump(7)
                Block 4: Block:
                    Jump(7)
                Block 5: Block:
                    Jump(7)
                Block 6: Block:
                    Variable(4, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(1, Boolean) = Store Variable(4, Boolean)
                    Jump(7)
                Block 7: Block:
                    Variable(5, Boolean) = LogicalNot Variable(1, Boolean)
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    transform_program(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Branch Variable(0, Boolean), 1, 2
                Block 1: Block:
                    Variable(2, Boolean) = LogicalNot Variable(0, Boolean)
                    Branch Variable(2, Boolean), 3, 4
                Block 2: Block:
                    Variable(3, Boolean) = LogicalNot Variable(0, Boolean)
                    Branch Variable(3, Boolean), 5, 6
                Block 3: Block:
                    Jump(7)
                Block 4: Block:
                    Jump(7)
                Block 5: Block:
                    Jump(7)
                Block 6: Block:
                    Variable(4, Boolean) = LogicalNot Variable(3, Boolean)
                    Jump(7)
                Block 7: Block:
                    Variable(6, Boolean) = Phi ( [Variable(2, Boolean), 3], [Variable(2, Boolean), 4], [Variable(3, Boolean), 5], [Variable(4, Boolean), 6], )
                    Variable(5, Boolean) = LogicalNot Variable(6, Boolean)
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 0
            num_results: 0
            tags:
    "#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_inserts_phi_for_multiple_stored_values() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                Vec::new(),
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(1),
                BlockId(2),
                None,
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(3)),
        ]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(3)),
        ]),
    );
    program.blocks.insert(
        BlockId(3),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(5),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(6),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Return(None),
        ]),
    );

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Variable(1, Boolean) = Store Variable(0, Boolean)
                    Variable(2, Boolean) = Store Variable(0, Boolean)
                    Branch Variable(1, Boolean), 1, 2
                Block 1: Block:
                    Variable(3, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(1, Boolean) = Store Variable(3, Boolean)
                    Jump(3)
                Block 2: Block:
                    Variable(4, Boolean) = LogicalNot Variable(2, Boolean)
                    Variable(2, Boolean) = Store Variable(4, Boolean)
                    Jump(3)
                Block 3: Block:
                    Variable(5, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(6, Boolean) = LogicalNot Variable(2, Boolean)
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    transform_program(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Branch Variable(0, Boolean), 1, 2
                Block 1: Block:
                    Variable(3, Boolean) = LogicalNot Variable(0, Boolean)
                    Jump(3)
                Block 2: Block:
                    Variable(4, Boolean) = LogicalNot Variable(0, Boolean)
                    Jump(3)
                Block 3: Block:
                    Variable(8, Boolean) = Phi ( [Variable(0, Boolean), 1], [Variable(4, Boolean), 2], )
                    Variable(7, Boolean) = Phi ( [Variable(3, Boolean), 1], [Variable(0, Boolean), 2], )
                    Variable(5, Boolean) = LogicalNot Variable(7, Boolean)
                    Variable(6, Boolean) = LogicalNot Variable(8, Boolean)
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 0
            num_results: 0
            tags:
    "#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_inserts_phi_nodes_in_successive_blocks_for_chained_branches() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                Vec::new(),
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(1),
                BlockId(2),
                None,
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(3),
                BlockId(4),
                None,
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(5)),
        ]),
    );
    program.blocks.insert(
        BlockId(3),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(6)),
        ]),
    );
    program.blocks.insert(
        BlockId(4),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(5),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(5),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(6)),
        ]),
    );
    program.blocks.insert(
        BlockId(5),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(6),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(6),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(7)),
        ]),
    );
    program.blocks.insert(
        BlockId(6),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(7),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(7),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(7)),
        ]),
    );
    program.blocks.insert(
        BlockId(7),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(8),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Return(None),
        ]),
    );

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Variable(1, Boolean) = Store Variable(0, Boolean)
                    Branch Variable(1, Boolean), 1, 2
                Block 1: Block:
                    Variable(2, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(1, Boolean) = Store Variable(2, Boolean)
                    Branch Variable(1, Boolean), 3, 4
                Block 2: Block:
                    Variable(3, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(1, Boolean) = Store Variable(3, Boolean)
                    Jump(5)
                Block 3: Block:
                    Variable(4, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(1, Boolean) = Store Variable(4, Boolean)
                    Jump(6)
                Block 4: Block:
                    Variable(5, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(1, Boolean) = Store Variable(5, Boolean)
                    Jump(6)
                Block 5: Block:
                    Variable(6, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(1, Boolean) = Store Variable(6, Boolean)
                    Jump(7)
                Block 6: Block:
                    Variable(7, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(1, Boolean) = Store Variable(7, Boolean)
                    Jump(7)
                Block 7: Block:
                    Variable(8, Boolean) = LogicalNot Variable(1, Boolean)
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    transform_program(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Branch Variable(0, Boolean), 1, 2
                Block 1: Block:
                    Variable(2, Boolean) = LogicalNot Variable(0, Boolean)
                    Branch Variable(2, Boolean), 3, 4
                Block 2: Block:
                    Variable(3, Boolean) = LogicalNot Variable(0, Boolean)
                    Variable(6, Boolean) = LogicalNot Variable(3, Boolean)
                    Jump(6)
                Block 3: Block:
                    Variable(4, Boolean) = LogicalNot Variable(2, Boolean)
                    Jump(5)
                Block 4: Block:
                    Variable(5, Boolean) = LogicalNot Variable(2, Boolean)
                    Jump(5)
                Block 5: Block:
                    Variable(9, Boolean) = Phi ( [Variable(4, Boolean), 3], [Variable(5, Boolean), 4], )
                    Variable(7, Boolean) = LogicalNot Variable(9, Boolean)
                    Jump(6)
                Block 6: Block:
                    Variable(10, Boolean) = Phi ( [Variable(6, Boolean), 2], [Variable(7, Boolean), 5], )
                    Variable(8, Boolean) = LogicalNot Variable(10, Boolean)
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 0
            num_results: 0
            tags:
    "#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_inerts_phi_nodes_for_early_return_graph_pattern() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                Vec::new(),
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(1),
                BlockId(2),
                None,
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(3)),
        ]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(4),
                BlockId(5),
                None,
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(3),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Return(None),
        ]),
    );
    program.blocks.insert(
        BlockId(4),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(5),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(5),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(6)),
        ]),
    );
    program.blocks.insert(
        BlockId(5),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(6),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(6),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(6)),
        ]),
    );
    program.blocks.insert(
        BlockId(6),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(7),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(3)),
        ]),
    );

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Variable(1, Boolean) = Store Variable(0, Boolean)
                    Branch Variable(1, Boolean), 1, 2
                Block 1: Block:
                    Variable(2, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(1, Boolean) = Store Variable(2, Boolean)
                    Jump(3)
                Block 2: Block:
                    Variable(3, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(1, Boolean) = Store Variable(3, Boolean)
                    Branch Variable(1, Boolean), 4, 5
                Block 3: Block:
                    Variable(4, Boolean) = LogicalNot Variable(1, Boolean)
                    Return
                Block 4: Block:
                    Variable(5, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(1, Boolean) = Store Variable(5, Boolean)
                    Jump(6)
                Block 5: Block:
                    Variable(6, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(1, Boolean) = Store Variable(6, Boolean)
                    Jump(6)
                Block 6: Block:
                    Variable(7, Boolean) = LogicalNot Variable(1, Boolean)
                    Jump(3)
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    transform_program(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Branch Variable(0, Boolean), 1, 2
                Block 1: Block:
                    Variable(2, Boolean) = LogicalNot Variable(0, Boolean)
                    Jump(6)
                Block 2: Block:
                    Variable(3, Boolean) = LogicalNot Variable(0, Boolean)
                    Branch Variable(3, Boolean), 3, 4
                Block 3: Block:
                    Variable(5, Boolean) = LogicalNot Variable(3, Boolean)
                    Jump(5)
                Block 4: Block:
                    Variable(6, Boolean) = LogicalNot Variable(3, Boolean)
                    Jump(5)
                Block 5: Block:
                    Variable(8, Boolean) = Phi ( [Variable(5, Boolean), 3], [Variable(6, Boolean), 4], )
                    Variable(7, Boolean) = LogicalNot Variable(8, Boolean)
                    Jump(6)
                Block 6: Block:
                    Variable(9, Boolean) = Phi ( [Variable(2, Boolean), 1], [Variable(8, Boolean), 5], )
                    Variable(4, Boolean) = LogicalNot Variable(9, Boolean)
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 0
            num_results: 0
            tags:
    "#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_propagates_updates_from_multiple_predecessors_to_later_single_successors() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );

    // Create a program that has a middle block with multiple predecessors and does not update a value from
    // the dominating entry block (in this case, the bool value for the first branch).
    // All successors of the middle block should have the same value for this variable, even if it isn't used,
    // avoiding a panic in the SSA transformation if the value is not propagated through the variable
    // maps used for updates.
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                Vec::new(),
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(1),
                BlockId(2),
                None,
            ),
        ]),
    );
    program
        .blocks
        .insert(BlockId(1), Block(vec![Instruction::Jump(BlockId(2))]));
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                Vec::new(),
                Some(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(3),
                BlockId(4),
                None,
            ),
        ]),
    );
    program
        .blocks
        .insert(BlockId(3), Block(vec![Instruction::Jump(BlockId(4))]));
    program
        .blocks
        .insert(BlockId(4), Block(vec![Instruction::Return(None)]));

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Variable(1, Boolean) = Store Variable(0, Boolean)
                    Branch Variable(1, Boolean), 1, 2
                Block 1: Block:
                    Jump(2)
                Block 2: Block:
                    Variable(2, Boolean) = Call id(1), args( )
                    Variable(3, Boolean) = Store Variable(2, Boolean)
                    Branch Variable(3, Boolean), 3, 4
                Block 3: Block:
                    Jump(4)
                Block 4: Block:
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    transform_program(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Branch Variable(0, Boolean), 1, 2
                Block 1: Block:
                    Jump(2)
                Block 2: Block:
                    Variable(2, Boolean) = Call id(1), args( )
                    Branch Variable(2, Boolean), 3, 4
                Block 3: Block:
                    Jump(4)
                Block 4: Block:
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 0
            num_results: 0
            tags:
    "#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_maps_store_instrs_that_use_values_from_other_store_instrs() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                Vec::new(),
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Return(None),
        ]),
    );

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Variable(1, Boolean) = Store Variable(0, Boolean)
                    Variable(2, Boolean) = Store Variable(1, Boolean)
                    Variable(3, Boolean) = LogicalNot Variable(2, Boolean)
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    transform_program(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Variable(3, Boolean) = LogicalNot Variable(0, Boolean)
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 0
            num_results: 0
            tags:
    "#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_maps_store_with_variable_from_store_in_conditional_to_phi_node() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                Vec::new(),
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(1),
                BlockId(2),
                None,
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(2)),
        ]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Return(None),
        ]),
    );

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Variable(1, Boolean) = Store Variable(0, Boolean)
                    Variable(2, Boolean) = Store Bool(true)
                    Branch Variable(1, Boolean), 1, 2
                Block 1: Block:
                    Variable(2, Boolean) = Store Variable(1, Boolean)
                    Jump(2)
                Block 2: Block:
                    Variable(3, Boolean) = LogicalNot Variable(2, Boolean)
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    transform_program(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Branch Variable(0, Boolean), 1, 2
                Block 1: Block:
                    Jump(2)
                Block 2: Block:
                    Variable(4, Boolean) = Phi ( [Bool(true), 0], [Variable(0, Boolean), 1], )
                    Variable(3, Boolean) = LogicalNot Variable(4, Boolean)
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 0
            num_results: 0
            tags:
    "#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_allows_point_in_time_copy_of_dynamic_variable() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                Vec::new(),
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(5),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Return(None),
        ]),
    );

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Variable(1, Boolean) = Store Variable(0, Boolean)
                    Variable(2, Boolean) = Store Variable(1, Boolean)
                    Variable(3, Boolean) = LogicalNot Variable(1, Boolean)
                    Variable(1, Boolean) = Store Variable(3, Boolean)
                    Variable(4, Boolean) = LogicalNot Variable(2, Boolean)
                    Variable(5, Boolean) = LogicalNot Variable(1, Boolean)
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    transform_program(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Variable(3, Boolean) = LogicalNot Variable(0, Boolean)
                    Variable(4, Boolean) = LogicalNot Variable(0, Boolean)
                    Variable(5, Boolean) = LogicalNot Variable(3, Boolean)
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 0
            num_results: 0
            tags:
    "#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_propagates_phi_var_to_successor_blocks_across_sequential_branches() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );
    program.callables.insert(
        CallableId(2),
        Callable {
            name: "record_bool".to_string(),
            input_type: vec![Ty::Prim(Prim::Boolean)],
            output_type: None,
            body: None,
            input_vars: Vec::new(),
            call_type: CallableType::OutputRecording,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                Vec::new(),
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(1),
                BlockId(2),
                None,
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(4),
                BlockId(5),
                None,
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                Vec::new(),
                Some(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(1)),
        ]),
    );
    program.blocks.insert(
        BlockId(3),
        Block(vec![
            Instruction::Call(
                CallableId(2),
                vec![Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                })],
                None,
                None,
            ),
            Instruction::Return(None),
        ]),
    );
    program
        .blocks
        .insert(BlockId(4), Block(vec![Instruction::Jump(BlockId(3))]));
    program
        .blocks
        .insert(BlockId(5), Block(vec![Instruction::Jump(BlockId(3))]));

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
                Callable 2: Callable:
                    name: record_bool
                    call_type: OutputRecording
                    input_type:
                        [0]: Boolean
                    output_type: <VOID>
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Variable(1, Boolean) = Store Bool(true)
                    Branch Variable(0, Boolean), 1, 2
                Block 1: Block:
                    Variable(3, Boolean) = Store Variable(1, Boolean)
                    Branch Variable(3, Boolean), 4, 5
                Block 2: Block:
                    Variable(2, Boolean) = Call id(1), args( )
                    Variable(1, Boolean) = Store Variable(2, Boolean)
                    Jump(1)
                Block 3: Block:
                    Call id(2), args( Variable(3, Boolean), )
                    Return
                Block 4: Block:
                    Jump(3)
                Block 5: Block:
                    Jump(3)
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    transform_program(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
                Callable 2: Callable:
                    name: record_bool
                    call_type: OutputRecording
                    input_type:
                        [0]: Boolean
                    output_type: <VOID>
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(1), args( )
                    Branch Variable(0, Boolean), 2, 1
                Block 1: Block:
                    Variable(2, Boolean) = Call id(1), args( )
                    Jump(2)
                Block 2: Block:
                    Variable(4, Boolean) = Phi ( [Bool(true), 0], [Variable(2, Boolean), 1], )
                    Branch Variable(4, Boolean), 3, 4
                Block 3: Block:
                    Jump(5)
                Block 4: Block:
                    Jump(5)
                Block 5: Block:
                    Call id(2), args( Variable(4, Boolean), )
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 0
            num_results: 0
            tags:
    "#]]
    .assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_two_bodies_store_to_phi_independent() {
    // Two bodied callables, each a diamond that stores a different value on each side of the branch
    // and reads the merged value afterward. The second body (the helper) uses lower block ids than
    // the entry body, so the arena is not in callable order. The transform must produce an
    // independent loop-free phi for each body, using distinct freshly minted variable versions.
    let mut program = Program::default();
    program.config.capabilities = Profile::AdaptiveRIF.into();

    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Integer)),
            body: Some(BlockId(4)),
            call_type: CallableType::Regular,
        },
    );
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "helper".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Integer)),
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.callables.insert(
        CallableId(2),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            call_type: CallableType::Regular,
        },
    );

    // Helper body: a store-diamond reading the merged counter.
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(2),
                Vec::new(),
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(1),
                BlockId(2),
                None,
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Integer(10)),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Integer),
                },
            ),
            Instruction::Jump(BlockId(3)),
        ]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Integer(20)),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Integer),
                },
            ),
            Instruction::Jump(BlockId(3)),
        ]),
    );
    program.blocks.insert(
        BlockId(3),
        Block(vec![
            Instruction::Add(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Integer),
                }),
                Operand::Literal(Literal::Integer(1)),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Integer),
                },
            ),
            Instruction::Return(Some(Operand::Variable(Variable {
                variable_id: VariableId(2),
                ty: Ty::Prim(Prim::Integer),
            }))),
        ]),
    );

    // Entry body: an independent store-diamond.
    program.blocks.insert(
        BlockId(4),
        Block(vec![
            Instruction::Call(
                CallableId(2),
                Vec::new(),
                Some(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(5),
                BlockId(6),
                None,
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(5),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Integer(100)),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Prim(Prim::Integer),
                },
            ),
            Instruction::Jump(BlockId(7)),
        ]),
    );
    program.blocks.insert(
        BlockId(6),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Integer(200)),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Prim(Prim::Integer),
                },
            ),
            Instruction::Jump(BlockId(7)),
        ]),
    );
    program.blocks.insert(
        BlockId(7),
        Block(vec![
            Instruction::Add(
                Operand::Variable(Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Prim(Prim::Integer),
                }),
                Operand::Literal(Literal::Integer(1)),
                Variable {
                    variable_id: VariableId(5),
                    ty: Ty::Prim(Prim::Integer),
                },
            ),
            Instruction::Return(Some(Operand::Variable(Variable {
                variable_id: VariableId(5),
                ty: Ty::Prim(Prim::Integer),
            }))),
        ]),
    );

    program.entry = CallableId(0);

    transform_to_ssa_directly(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 4
                Callable 1: Callable:
                    name: helper
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 2: Callable:
                    name: dynamic_bool
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Variable(0, Boolean) = Call id(2), args( )
                    Branch Variable(0, Boolean), 1, 2
                Block 1: Block:
                    Jump(3)
                Block 2: Block:
                    Jump(3)
                Block 3: Block:
                    Variable(7, Integer) = Phi ( [Integer(10), 1], [Integer(20), 2], )
                    Variable(2, Integer) = Add Variable(7, Integer), Integer(1)
                    Return Variable(2, Integer)
                Block 4: Block:
                    Variable(3, Boolean) = Call id(2), args( )
                    Branch Variable(3, Boolean), 5, 6
                Block 5: Block:
                    Jump(7)
                Block 6: Block:
                    Jump(7)
                Block 7: Block:
                    Variable(6, Integer) = Phi ( [Integer(100), 5], [Integer(200), 6], )
                    Variable(5, Integer) = Add Variable(6, Integer), Integer(1)
                    Return Variable(5, Integer)
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 0
            num_results: 0
            tags:
    "#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_second_body_entry_identified() {
    // The entry body of `two_body_program` lives in block 2, which is a higher block id than the
    // helper body's blocks. The transform must use each callable's declared `body` as the root rather
    // than assuming block 0 is the entry.
    let mut program = two_body_program();

    transform_to_ssa_directly(&mut program);

    // The entry callable still roots at its declared body.
    assert_eq!(program.get_callable(CallableId(0)).body, Some(BlockId(2)));

    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 2
                Callable 1: Callable:
                    name: helper
                    call_type: Regular
                    input_type:
                        [0]: Integer
                    input_vars:
                        [0]: 0
                    output_type: Integer
                    body: 0
            blocks:
                Block 0: Block:
                    Variable(1, Integer) = Add Variable(0, Integer), Integer(1)
                    Return Variable(1, Integer)
                Block 2: Block:
                    Variable(2, Integer) = Call id(1), args( Integer(7), )
                    Return Variable(2, Integer)
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 0
            num_results: 0
            tags:
    "#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_parameters_seeded_as_entry_defs() {
    // The helper body branches on its boolean `input_vars` parameter without storing into anything.
    // The parameter is live-in with no defining instruction, so the transform must seed it as a
    // definition at the body entry and complete without error.
    let mut program = two_body_program_with_branch();

    transform_to_ssa_directly(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 3
                Callable 1: Callable:
                    name: helper
                    call_type: Regular
                    input_type:
                        [0]: Boolean
                    input_vars:
                        [0]: 0
                    output_type: Integer
                    body: 0
            blocks:
                Block 0: Block:
                    Branch Variable(0, Boolean), 1, 2
                Block 1: Block:
                    Return Integer(1)
                Block 2: Block:
                    Return Integer(0)
                Block 3: Block:
                    Variable(1, Integer) = Call id(1), args( Bool(true), )
                    Return Variable(1, Integer)
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 0
            num_results: 0
            tags:
    "#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_second_body_with_loop() {
    // A backward branch in a secondary body forms a cycle. The transform does not support cycles, so
    // the acyclic guard must reject the program even when the loop lives outside the entry body.
    let mut program = two_body_program_with_loop();

    assert_panics_with("has a cycle", move || {
        transform_to_ssa_directly(&mut program);
    });
}

#[test]
fn ssa_transform_value_returning_body() {
    // A secondary body whose terminator returns a static operand must transform cleanly, exercising
    // the value-returning (IR-function) shape that only ever appears in non-entry bodies.
    let mut program = Program::default();
    program.config.capabilities = Profile::AdaptiveRIF.into();

    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Integer)),
            body: Some(BlockId(1)),
            call_type: CallableType::Regular,
        },
    );
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "helper".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Integer)),
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );

    // Helper body: return a constant.
    program.blocks.insert(
        BlockId(0),
        Block(vec![Instruction::Return(Some(Operand::Literal(
            Literal::Integer(42),
        )))]),
    );
    // Entry body: call the helper and return its result.
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                Vec::new(),
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Integer),
                }),
                None,
            ),
            Instruction::Return(Some(Operand::Variable(Variable {
                variable_id: VariableId(0),
                ty: Ty::Prim(Prim::Integer),
            }))),
        ]),
    );

    program.entry = CallableId(0);

    transform_to_ssa_directly(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 1
                Callable 1: Callable:
                    name: helper
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
            blocks:
                Block 0: Block:
                    Return Integer(42)
                Block 1: Block:
                    Variable(0, Integer) = Call id(1), args( )
                    Return Variable(0, Integer)
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 0
            num_results: 0
            tags:
    "#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_mutable_parameter_versioned() {
    // The helper stores a derived value back into its own `input_vars` parameter. The parameter is
    // both seeded as the entry definition and versioned by the store, so the store must convert to an
    // SSA value without a false duplicate-assignment, while the parameter remains the entry def.
    let mut program = two_body_mutable_param_program();

    transform_to_ssa_directly(&mut program);

    // The parameter remains declared as the body's live-in definition.
    assert_eq!(
        program.get_callable(CallableId(1)).input_vars,
        vec![VariableId(0)]
    );

    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 1
                Callable 1: Callable:
                    name: helper
                    call_type: Regular
                    input_type:
                        [0]: Integer
                    input_vars:
                        [0]: 0
                    output_type: Integer
                    body: 0
            blocks:
                Block 0: Block:
                    Variable(1, Integer) = Add Variable(0, Integer), Integer(1)
                    Return Variable(1, Integer)
                Block 1: Block:
                    Variable(2, Integer) = Call id(1), args( Integer(5), )
                    Return Variable(2, Integer)
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 0
            num_results: 0
            tags:
    "#]].assert_eq(&program.to_string());
}
