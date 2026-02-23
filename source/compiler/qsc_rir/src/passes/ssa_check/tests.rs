// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines, clippy::needless_raw_string_hashes)]

use crate::{
    builder::{bell_program, new_program, teleport_program},
    passes::{build_dominator_graph, remap_block_ids},
    rir::{
        Block, BlockId, Callable, CallableId, CallableType, InstructionKind, Literal, Operand,
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
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Return,
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
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Return,
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
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Return,
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
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Jump(BlockId(1)),
        ]),
    );

    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Return,
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
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Return,
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
            output_type: Some(Ty::Boolean),
            body: None,
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![
            InstructionKind::Call(
                CallableId(1),
                Vec::new(),
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

    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Jump(BlockId(3)),
        ]),
    );

    program.blocks.insert(
        BlockId(2),
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Jump(BlockId(3)),
        ]),
    );

    program.blocks.insert(
        BlockId(3),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );

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
            output_type: Some(Ty::Boolean),
            body: None,
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![
            InstructionKind::Call(
                CallableId(1),
                Vec::new(),
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

    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Jump(BlockId(3)),
        ]),
    );

    program.blocks.insert(
        BlockId(2),
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Jump(BlockId(3)),
        ]),
    );

    program.blocks.insert(
        BlockId(3),
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Return,
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
            output_type: Some(Ty::Boolean),
            body: None,
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![
            InstructionKind::Call(
                CallableId(1),
                Vec::new(),
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

    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Jump(BlockId(3)),
        ]),
    );

    program.blocks.insert(
        BlockId(2),
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Jump(BlockId(3)),
        ]),
    );

    program.blocks.insert(
        BlockId(3),
        Block::from_instruction_kinds(vec![
            InstructionKind::Phi(
                vec![
                    (
                        Operand::Variable(Variable {
                            variable_id: VariableId(1),
                            ty: Ty::Boolean,
                        }),
                        BlockId(1),
                    ),
                    (
                        Operand::Variable(Variable {
                            variable_id: VariableId(2),
                            ty: Ty::Boolean,
                        }),
                        BlockId(2),
                    ),
                ],
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Return,
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
            output_type: Some(Ty::Boolean),
            body: None,
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![
            InstructionKind::Call(
                CallableId(1),
                Vec::new(),
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

    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Jump(BlockId(3)),
        ]),
    );

    program.blocks.insert(
        BlockId(2),
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Jump(BlockId(4)),
        ]),
    );

    program.blocks.insert(
        BlockId(4),
        Block::from_instruction_kinds(vec![InstructionKind::Jump(BlockId(3))]),
    );

    program.blocks.insert(
        BlockId(3),
        Block::from_instruction_kinds(vec![
            InstructionKind::Phi(
                vec![
                    (
                        Operand::Variable(Variable {
                            variable_id: VariableId(1),
                            ty: Ty::Boolean,
                        }),
                        BlockId(1),
                    ),
                    (
                        Operand::Variable(Variable {
                            variable_id: VariableId(2),
                            ty: Ty::Boolean,
                        }),
                        BlockId(4),
                    ),
                ],
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Return,
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
            output_type: Some(Ty::Boolean),
            body: None,
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![
            InstructionKind::Call(
                CallableId(1),
                Vec::new(),
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

    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Jump(BlockId(3)),
        ]),
    );

    program.blocks.insert(
        BlockId(2),
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Branch(
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                },
                BlockId(4),
                BlockId(5),
            ),
        ]),
    );

    program.blocks.insert(
        BlockId(4),
        Block::from_instruction_kinds(vec![InstructionKind::Jump(BlockId(6))]),
    );

    program.blocks.insert(
        BlockId(5),
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Jump(BlockId(6)),
        ]),
    );

    program.blocks.insert(
        BlockId(6),
        Block::from_instruction_kinds(vec![InstructionKind::Jump(BlockId(3))]),
    );

    program.blocks.insert(
        BlockId(3),
        Block::from_instruction_kinds(vec![
            InstructionKind::Phi(
                vec![
                    (
                        Operand::Variable(Variable {
                            variable_id: VariableId(1),
                            ty: Ty::Boolean,
                        }),
                        BlockId(1),
                    ),
                    (
                        Operand::Variable(Variable {
                            variable_id: VariableId(3),
                            ty: Ty::Boolean,
                        }),
                        BlockId(6),
                    ),
                ],
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(5),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Return,
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
            output_type: Some(Ty::Boolean),
            body: None,
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![
            InstructionKind::Call(
                CallableId(1),
                Vec::new(),
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

    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Jump(BlockId(3)),
        ]),
    );

    program.blocks.insert(
        BlockId(2),
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Jump(BlockId(3)),
        ]),
    );

    program.blocks.insert(
        BlockId(3),
        Block::from_instruction_kinds(vec![
            InstructionKind::Phi(
                vec![
                    (
                        Operand::Variable(Variable {
                            variable_id: VariableId(1),
                            ty: Ty::Boolean,
                        }),
                        BlockId(0),
                    ),
                    (
                        Operand::Variable(Variable {
                            variable_id: VariableId(2),
                            ty: Ty::Boolean,
                        }),
                        BlockId(1),
                    ),
                ],
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Return,
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
            output_type: Some(Ty::Boolean),
            body: None,
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![
            InstructionKind::Call(
                CallableId(1),
                Vec::new(),
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

    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Jump(BlockId(3)),
        ]),
    );

    program.blocks.insert(
        BlockId(2),
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Jump(BlockId(3)),
        ]),
    );

    program.blocks.insert(
        BlockId(3),
        Block::from_instruction_kinds(vec![
            InstructionKind::Phi(
                vec![
                    (
                        Operand::Variable(Variable {
                            variable_id: VariableId(1),
                            ty: Ty::Boolean,
                        }),
                        BlockId(1),
                    ),
                    (
                        Operand::Variable(Variable {
                            variable_id: VariableId(3),
                            ty: Ty::Boolean,
                        }),
                        BlockId(2),
                    ),
                ],
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Boolean,
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
            output_type: Some(Ty::Boolean),
            body: None,
            call_type: CallableType::Regular,
        },
    );

    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![
            InstructionKind::Call(
                CallableId(1),
                Vec::new(),
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

    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Jump(BlockId(3)),
        ]),
    );

    program.blocks.insert(
        BlockId(2),
        Block::from_instruction_kinds(vec![
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Jump(BlockId(3)),
        ]),
    );

    program.blocks.insert(
        BlockId(3),
        Block::from_instruction_kinds(vec![
            InstructionKind::Phi(
                vec![(
                    Operand::Variable(Variable {
                        variable_id: VariableId(1),
                        ty: Ty::Boolean,
                    }),
                    BlockId(1),
                )],
                Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::LogicalNot(
                Operand::Variable(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Boolean,
                }),
                Variable {
                    variable_id: VariableId(4),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Return,
        ]),
    );

    perform_ssa_check(&mut program);
}
