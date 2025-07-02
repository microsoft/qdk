// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines, clippy::needless_raw_string_hashes)]

use expect_test::expect;
use qsc_data_structures::target::TargetCapabilityFlags;

use crate::{
    builder::{bell_program, new_program, teleport_program},
    passes::check_and_transform,
    rir::{
        Block, BlockId, Callable, CallableId, CallableType, Instruction, Literal, Operand, Program,
        Ty, Variable, VariableId,
    },
};
fn transform_program(program: &mut Program) {
    program.config.capabilities = TargetCapabilityFlags::all();
    check_and_transform(program);
}

#[test]
fn ssa_transform_leaves_program_without_store_instruction_unchanged() {
    let mut program = bell_program();
    program.config.capabilities = TargetCapabilityFlags::all();
    let program_string_orignal = program.to_string();
    transform_program(&mut program);

    assert_eq!(program_string_orignal, program.to_string());
}

#[test]
fn ssa_transform_leaves_branching_program_without_store_instruction_unchanged() {
    let mut program = teleport_program();
    program.config.capabilities = TargetCapabilityFlags::all();
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
            output_type: Some(Ty::Boolean),
            body: None,
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
                    ty: Ty::Boolean,
                }),
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
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
            num_results: 0"#]]
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
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 0
            num_results: 0"#]]
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
            output_type: Some(Ty::Boolean),
            body: None,
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
                    ty: Ty::Boolean,
                }),
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(4),
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
            num_results: 0"#]]
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
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 0
            num_results: 0"#]]
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
            output_type: Some(Ty::Boolean),
            body: None,
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
                    ty: Ty::Boolean,
                }),
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
                BlockId(1),
                BlockId(2),
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
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
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
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
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(4),
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
            num_results: 0"#]]
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
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 0
            num_results: 0"#]]
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
            output_type: Some(Ty::Boolean),
            body: None,
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
                    ty: Ty::Boolean,
                }),
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
                BlockId(1),
                BlockId(2),
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
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(4),
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
            num_results: 0"#]]
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
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 0
            num_results: 0"#]]
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
            output_type: Some(Ty::Boolean),
            body: None,
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
                    ty: Ty::Boolean,
                }),
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
                BlockId(1),
                BlockId(2),
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
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
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
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
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(4),
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
            num_results: 0"#]]
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
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 0
            num_results: 0"#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_inserts_phi_for_store_not_dominating_usage_in_one_branch() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Boolean),
            body: None,
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
                    ty: Ty::Boolean,
                }),
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
                BlockId(1),
                BlockId(2),
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
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
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(4),
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
            num_results: 0"#]]
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
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 0
            num_results: 0"#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_inserts_phi_for_node_with_many_predecessors() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Boolean),
            body: None,
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
                    ty: Ty::Boolean,
                }),
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
                BlockId(1),
                BlockId(2),
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
                BlockId(3),
                BlockId(4),
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
                BlockId(5),
                BlockId(6),
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
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
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
                    ty: Ty::Boolean,
                }),
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
            num_results: 0"#]]
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
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 0
            num_results: 0"#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_inserts_phi_for_multiple_stored_values() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Boolean),
            body: None,
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
                    ty: Ty::Boolean,
                }),
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
                BlockId(1),
                BlockId(2),
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
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
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
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
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(5),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(6),
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
            num_results: 0"#]]
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
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 0
            num_results: 0"#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_inserts_phi_nodes_in_successive_blocks_for_chained_branches() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Boolean),
            body: None,
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
                    ty: Ty::Boolean,
                }),
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
                BlockId(1),
                BlockId(2),
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
                BlockId(3),
                BlockId(4),
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
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
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
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
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(5),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(5),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
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
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(6),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(6),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
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
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(7),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(7),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
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
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(8),
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
            num_results: 0"#]]
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
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 0
            num_results: 0"#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_inerts_phi_nodes_for_early_return_graph_pattern() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Boolean),
            body: None,
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
                    ty: Ty::Boolean,
                }),
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
                BlockId(1),
                BlockId(2),
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
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
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
                BlockId(4),
                BlockId(5),
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(3),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Return,
        ]),
    );
    program.blocks.insert(
        BlockId(4),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(5),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(5),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
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
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(6),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(6),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
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
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(7),
                    ty: Ty::Boolean,
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
            num_results: 0"#]]
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
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 0
            num_results: 0"#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_propagates_updates_from_multiple_predecessors_to_later_single_successors() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Boolean),
            body: None,
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
                    ty: Ty::Boolean,
                }),
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
                BlockId(1),
                BlockId(2),
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
                    ty: Ty::Boolean,
                }),
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                },
                BlockId(3),
                BlockId(4),
            ),
        ]),
    );
    program
        .blocks
        .insert(BlockId(3), Block(vec![Instruction::Jump(BlockId(4))]));
    program
        .blocks
        .insert(BlockId(4), Block(vec![Instruction::Return]));

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
            num_results: 0"#]]
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
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 0
            num_results: 0"#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_maps_store_instrs_that_use_values_from_other_store_instrs() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Boolean),
            body: None,
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
                    ty: Ty::Boolean,
                }),
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(3),
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
            num_results: 0"#]]
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
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 0
            num_results: 0"#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_maps_store_with_variable_from_store_in_conditional_to_phi_node() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Boolean),
            body: None,
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
                    ty: Ty::Boolean,
                }),
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
                BlockId(1),
                BlockId(2),
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
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
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(3),
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
            num_results: 0"#]]
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
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 0
            num_results: 0"#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_allows_point_in_time_copy_of_dynamic_variable() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Boolean),
            body: None,
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
                    ty: Ty::Boolean,
                }),
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
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
            num_results: 0"#]]
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
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 0
            num_results: 0"#]].assert_eq(&program.to_string());
}

#[test]
fn ssa_transform_propagates_phi_var_to_successor_blocks_across_sequential_branches() {
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Boolean),
            body: None,
            call_type: CallableType::Regular,
        },
    );
    program.callables.insert(
        CallableId(2),
        Callable {
            name: "record_bool".to_string(),
            input_type: vec![Ty::Boolean],
            output_type: None,
            body: None,
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
                    ty: Ty::Boolean,
                }),
            ),
            Instruction::Store(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                },
                BlockId(1),
                BlockId(2),
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                },
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                },
                BlockId(4),
                BlockId(5),
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
                    ty: Ty::Boolean,
                }),
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
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
                    ty: Ty::Boolean,
                })],
                None,
            ),
            Instruction::Return,
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
            num_results: 0"#]]
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
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 0
            num_results: 0"#]]
    .assert_eq(&program.to_string());
}
