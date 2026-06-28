// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines, clippy::needless_raw_string_hashes)]

use crate::{
    builder::{
        bell_program, new_program, teleport_program, two_body_mutable_param_program,
        two_body_program, two_body_program_with_branch,
    },
    passes::{
        build_dominator_graph, check_and_transform, remap_block_ids,
        test_utils::assert_panics_with, transform_to_ssa,
    },
    rir::{
        Block, BlockId, Callable, CallableId, CallableType, Instruction, Literal, Operand, Prim,
        Program, Ty, Variable, VariableId,
    },
    utils::build_predecessors_map,
};

use super::check_ssa_form;

fn perform_ssa_check(program: &mut Program) {
    remap_block_ids(program);
    let preds = build_predecessors_map(program);
    let doms = build_dominator_graph(program, &preds);
    check_ssa_form(program, &preds, &doms);
}

// Runs the SSA sub-sequence of the standard pipeline (remap, store-to-SSA transform, dominator
// build, SSA check) so that multi-body fixtures containing stores are validated against a fully
// transformed program. This mirrors the SSA branch of `check_and_transform` without routing through
// the profile dispatch, which would otherwise send these adaptive fixtures down the non-SSA pipeline.
fn perform_full_ssa_check(program: &mut Program) {
    remap_block_ids(program);
    let preds = build_predecessors_map(program);
    transform_to_ssa(program, &preds);
    let doms = build_dominator_graph(program, &preds);
    check_ssa_form(program, &preds, &doms);
}

#[test]
fn ssa_check_passes_for_base_profile_program() {
    let mut program = bell_program();

    perform_ssa_check(&mut program);
}

#[test]
fn ssa_check_passes_for_adaptive_program_with_all_literals() {
    let mut program = teleport_program();

    perform_ssa_check(&mut program);
}

#[test]
#[should_panic(
    expected = "BlockId(0), instruction 0 has no variables: Variable(0, Boolean) = LogicalNot Bool(true)"
)]
fn ssa_check_fails_for_instruction_on_literal_values() {
    let mut program = new_program();

    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Return(None),
        ]),
    );

    perform_ssa_check(&mut program);
}

#[test]
#[should_panic(
    expected = "VariableId(1) is used before it is assigned in BlockId(0), instruction 0"
)]
fn ssa_check_fails_for_use_before_assignment_in_single_block() {
    let mut program = new_program();

    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Return(None),
        ]),
    );

    perform_ssa_check(&mut program);
}

#[test]
#[should_panic(expected = "VariableId(4) is used but not assigned")]
fn ssa_check_fails_for_use_without_assignment_in_single_block() {
    let mut program = new_program();

    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Return(None),
        ]),
    );

    perform_ssa_check(&mut program);
}

#[test]
#[should_panic(
    expected = "Definition of VariableId(1) in BlockId(1) does not dominate use in BlockId(0), instruction 0"
)]
fn ssa_check_fails_for_use_before_assignment_across_sequential_blocks() {
    let mut program = new_program();

    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(1)),
        ]),
    );

    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Return(None),
        ]),
    );

    perform_ssa_check(&mut program);
}

#[test]
#[should_panic(expected = "Duplicate assignment to VariableId(0) in BlockId(0), instruction 1")]
fn ssa_check_fails_for_multiple_assignment_in_single_block() {
    let mut program = new_program();

    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Return(None),
        ]),
    );

    perform_ssa_check(&mut program);
}

#[test]
fn ssa_check_passes_for_variable_that_dominates_usage() {
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
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
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
                    variable_id: VariableId(0),
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

    program
        .blocks
        .insert(BlockId(3), Block(vec![Instruction::Return(None)]));

    perform_ssa_check(&mut program);
}

#[test]
#[should_panic(
    expected = "Definition of VariableId(2) in BlockId(2) does not dominate use in BlockId(3), instruction 0"
)]
fn ssa_check_fails_when_definition_does_not_dominates_usage() {
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
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
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
                    variable_id: VariableId(0),
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

    perform_ssa_check(&mut program);
}

#[test]
fn ssa_check_succeeds_when_phi_handles_multiple_values_from_branches() {
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
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
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
                    variable_id: VariableId(0),
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
            Instruction::Phi(
                vec![
                    (
                        Operand::Variable(Variable {
                            variable_id: VariableId(1),
                            ty: Ty::Prim(Prim::Boolean),
                        }),
                        BlockId(1),
                    ),
                    (
                        Operand::Variable(Variable {
                            variable_id: VariableId(2),
                            ty: Ty::Prim(Prim::Boolean),
                        }),
                        BlockId(2),
                    ),
                ],
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
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

    perform_ssa_check(&mut program);
}

#[test]
fn ssa_check_succeeds_when_phi_handles_value_from_dominator_of_predecessor() {
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
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
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
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::Jump(BlockId(4)),
        ]),
    );

    program
        .blocks
        .insert(BlockId(4), Block(vec![Instruction::Jump(BlockId(3))]));

    program.blocks.insert(
        BlockId(3),
        Block(vec![
            Instruction::Phi(
                vec![
                    (
                        Operand::Variable(Variable {
                            variable_id: VariableId(1),
                            ty: Ty::Prim(Prim::Boolean),
                        }),
                        BlockId(1),
                    ),
                    (
                        Operand::Variable(Variable {
                            variable_id: VariableId(2),
                            ty: Ty::Prim(Prim::Boolean),
                        }),
                        BlockId(4),
                    ),
                ],
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
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

    perform_ssa_check(&mut program);
}

#[test]
#[should_panic(
    expected = "Definition of VariableId(3) in BlockId(4) does not dominate use in BlockId(5), instruction 18446744073709551615"
)]
fn ssa_check_fails_when_phi_handles_value_from_non_dominator_of_predecessor() {
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
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
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
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(4),
                BlockId(5),
                None,
            ),
        ]),
    );

    program
        .blocks
        .insert(BlockId(4), Block(vec![Instruction::Jump(BlockId(6))]));

    program.blocks.insert(
        BlockId(5),
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
            Instruction::Jump(BlockId(6)),
        ]),
    );

    program
        .blocks
        .insert(BlockId(6), Block(vec![Instruction::Jump(BlockId(3))]));

    program.blocks.insert(
        BlockId(3),
        Block(vec![
            Instruction::Phi(
                vec![
                    (
                        Operand::Variable(Variable {
                            variable_id: VariableId(1),
                            ty: Ty::Prim(Prim::Boolean),
                        }),
                        BlockId(1),
                    ),
                    (
                        Operand::Variable(Variable {
                            variable_id: VariableId(3),
                            ty: Ty::Prim(Prim::Boolean),
                        }),
                        BlockId(6),
                    ),
                ],
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(4),
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

    perform_ssa_check(&mut program);
}

#[test]
#[should_panic(expected = "Phi node in BlockId(3) references a non-predecessor BlockId(0)")]
fn ssa_check_fails_when_phi_lists_non_predecessor_block() {
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
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
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
                    variable_id: VariableId(0),
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
            Instruction::Phi(
                vec![
                    (
                        Operand::Variable(Variable {
                            variable_id: VariableId(1),
                            ty: Ty::Prim(Prim::Boolean),
                        }),
                        BlockId(0),
                    ),
                    (
                        Operand::Variable(Variable {
                            variable_id: VariableId(2),
                            ty: Ty::Prim(Prim::Boolean),
                        }),
                        BlockId(1),
                    ),
                ],
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
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

    perform_ssa_check(&mut program);
}

#[test]
#[should_panic(expected = "Phi node in BlockId(3) assigns to VariableId(3) to itself")]
fn ssa_check_fails_when_phi_assigns_to_itself() {
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
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
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
                    variable_id: VariableId(0),
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
            Instruction::Phi(
                vec![
                    (
                        Operand::Variable(Variable {
                            variable_id: VariableId(1),
                            ty: Ty::Prim(Prim::Boolean),
                        }),
                        BlockId(1),
                    ),
                    (
                        Operand::Variable(Variable {
                            variable_id: VariableId(3),
                            ty: Ty::Prim(Prim::Boolean),
                        }),
                        BlockId(2),
                    ),
                ],
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
        ]),
    );

    perform_ssa_check(&mut program);
}

#[test]
#[should_panic(expected = "Phi node in BlockId(3) has 1 arguments but 2 predecessors")]
fn ssa_check_fails_when_phi_blocks_have_different_predecessors() {
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
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
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
                    variable_id: VariableId(0),
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
            Instruction::Phi(
                vec![(
                    Operand::Variable(Variable {
                        variable_id: VariableId(1),
                        ty: Ty::Prim(Prim::Boolean),
                    }),
                    BlockId(1),
                )],
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Boolean),
                },
            ),
            Instruction::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
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

    perform_ssa_check(&mut program);
}

#[test]
fn ssa_check_two_valid_bodies_passes() {
    // A well-formed program with two bodied callables, transformed to SSA, passes the check. The
    // helper body reads its parameter and the entry body calls it; neither body should trip the
    // use-before-def or dominance checks.
    let mut program = two_body_program();

    perform_full_ssa_check(&mut program);
}

#[test]
fn check_and_transform_processes_multi_body_program() {
    // Drives the full pipeline end to end on a hand-built multi-body program. Multi-body programs do
    // not arise from Q# source on the SSA path today, since the only way to produce a second bodied
    // callable requires `CallSupport`, which routes to the non-SSA pipeline; the fixture is therefore
    // built directly. The fixture is already `AdaptiveRIF`, so `check_and_transform` runs the SSA
    // sub-sequence (`transform_to_ssa` -> `build_dominator_graph` -> `check_ssa_form`). The final
    // `check_ssa_form` self-validates the transformed multi-body program, so completing without a
    // panic is the assertion.
    let mut program = two_body_program();

    check_and_transform(&mut program);
}

#[test]
fn check_and_transform_processes_multi_body_program_with_branch() {
    // The full pipeline on a multi-body program whose secondary body contains a forward branch
    // (a diamond that reconverges on a value return). Running `check_and_transform` end to end
    // exercises `transform_to_ssa`, `build_dominator_graph`, and the self-validating `check_ssa_form`
    // on a branching secondary body; the hand-built fixture is `AdaptiveRIF` so it takes the SSA
    // path, and the pipeline must complete without panic.
    let mut program = two_body_program_with_branch();

    check_and_transform(&mut program);
}

#[test]
fn ssa_check_parameters_are_definitions() {
    // A bodied callable whose body reads its input parameter directly. The parameter has no
    // defining instruction, so it must be recognized as a live-in definition rather than flagged as
    // used but not assigned.
    let mut program = new_program();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: vec![Ty::Prim(Prim::Integer)],
            input_vars: vec![VariableId(0)],
            output_type: Some(Ty::Prim(Prim::Integer)),
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Add(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Integer),
                }),
                Operand::Literal(Literal::Integer(1)),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Integer),
                },
            ),
            Instruction::Return(Some(Operand::Variable(Variable {
                variable_id: VariableId(1),
                ty: Ty::Prim(Prim::Integer),
            }))),
        ]),
    );

    perform_ssa_check(&mut program);
}

#[test]
fn ssa_check_per_body_dominator_lookup() {
    // The second body contains a forward branch, so its blocks are validated against that body's own
    // dominators rather than against the entry body's blocks. A correct per-body dominator lookup
    // lets this well-formed program pass.
    let mut program = two_body_program_with_branch();

    perform_full_ssa_check(&mut program);
}

#[test]
fn ssa_check_multi_body_use_before_def_fails() {
    // A second body that uses a variable which is neither assigned nor one of its parameters is
    // still malformed and must panic with the same use-but-not-assigned message.
    let mut program = new_program();
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                vec![Operand::Literal(Literal::Integer(7))],
                Some(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Integer),
                }),
                None,
            ),
            Instruction::Return(Some(Operand::Variable(Variable {
                variable_id: VariableId(2),
                ty: Ty::Prim(Prim::Integer),
            }))),
        ]),
    );
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "helper".to_string(),
            input_type: vec![Ty::Prim(Prim::Integer)],
            input_vars: vec![VariableId(0)],
            output_type: Some(Ty::Prim(Prim::Integer)),
            body: Some(BlockId(1)),
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            // VariableId(5) is neither assigned in this body nor one of its parameters.
            Instruction::Add(
                Operand::Variable(Variable {
                    variable_id: VariableId(5),
                    ty: Ty::Prim(Prim::Integer),
                }),
                Operand::Literal(Literal::Integer(1)),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Integer),
                },
            ),
            Instruction::Return(Some(Operand::Variable(Variable {
                variable_id: VariableId(1),
                ty: Ty::Prim(Prim::Integer),
            }))),
        ]),
    );

    assert_panics_with("used but not assigned", move || {
        perform_ssa_check(&mut program);
    });
}

#[test]
fn ssa_check_mutable_parameter_no_false_failure() {
    // The second body stores into one of its parameters. After the store-to-SSA transform the
    // parameter is both a live-in definition and a versioned value, which must not produce a false
    // duplicate-assignment or use-before-def failure.
    let mut program = two_body_mutable_param_program();

    perform_full_ssa_check(&mut program);
}

#[test]
fn ssa_check_intrinsic_callables_skipped() {
    // A bodyless (intrinsic) callable alongside a bodied callable. The intrinsic has no blocks and
    // must be ignored entirely, while the bodied callable's parameter use is still recognized.
    let mut program = new_program();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: vec![Ty::Prim(Prim::Integer)],
            input_vars: vec![VariableId(0)],
            output_type: Some(Ty::Prim(Prim::Integer)),
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "dynamic_bool".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Add(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Integer),
                }),
                Operand::Literal(Literal::Integer(1)),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Integer),
                },
            ),
            Instruction::Return(Some(Operand::Variable(Variable {
                variable_id: VariableId(1),
                ty: Ty::Prim(Prim::Integer),
            }))),
        ]),
    );

    perform_ssa_check(&mut program);
}
