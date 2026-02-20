// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines, clippy::needless_raw_string_hashes)]

use crate::{
    builder::new_program,
    passes::remap_block_ids,
    rir::{
        Block, BlockId, Callable, CallableId, CallableType, InstructionKind, Program, Ty, Variable,
        VariableId,
    },
    utils::build_predecessors_map,
};
use expect_test::expect;
use qsc_data_structures::index_map::IndexMap;
use std::fmt::Write;

use super::build_dominator_graph;

fn display_dominator_graph(doms: &IndexMap<BlockId, BlockId>) -> String {
    let mut result = String::new();
    for (block_id, dom) in doms.iter() {
        writeln!(result, "Block {} dominated by block {},", block_id.0, dom.0)
            .expect("writing to string should succeed");
    }
    result
}

fn build_doms(program: &mut Program) -> IndexMap<BlockId, BlockId> {
    remap_block_ids(program);
    let preds = build_predecessors_map(program);
    build_dominator_graph(program, &preds)
}

#[test]
fn dominator_graph_single_block_dominates_itself() {
    let mut program = new_program();
    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );

    let doms = build_doms(&mut program);

    expect![[r#"
        Block 0 dominated by block 0,
    "#]]
    .assert_eq(&display_dominator_graph(&doms));
}

#[test]
fn dominator_graph_sequential_blocks_dominated_by_predecessor() {
    let mut program = new_program();
    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![InstructionKind::Jump(BlockId(1))]),
    );
    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![InstructionKind::Jump(BlockId(2))]),
    );
    program.blocks.insert(
        BlockId(2),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );

    let doms = build_doms(&mut program);

    expect![[r#"
        Block 0 dominated by block 0,
        Block 1 dominated by block 0,
        Block 2 dominated by block 1,
    "#]]
    .assert_eq(&display_dominator_graph(&doms));
}

#[test]
fn dominator_graph_branching_blocks_dominated_by_common_predecessor() {
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
            InstructionKind::Jump(BlockId(1)),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![InstructionKind::Branch(
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Boolean,
            },
            BlockId(2),
            BlockId(3),
        )]),
    );
    program.blocks.insert(
        BlockId(2),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );
    program.blocks.insert(
        BlockId(3),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );

    let doms = build_doms(&mut program);

    expect![[r#"
        Block 0 dominated by block 0,
        Block 1 dominated by block 0,
        Block 2 dominated by block 1,
        Block 3 dominated by block 1,
    "#]]
    .assert_eq(&display_dominator_graph(&doms));
}

#[test]
fn dominator_graph_infinite_loop() {
    let mut program = new_program();
    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![InstructionKind::Jump(BlockId(1))]),
    );
    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![InstructionKind::Jump(BlockId(1))]),
    );

    let doms = build_doms(&mut program);

    expect![[r#"
        Block 0 dominated by block 0,
        Block 1 dominated by block 0,
    "#]]
    .assert_eq(&display_dominator_graph(&doms));
}

#[test]
fn dominator_graph_branch_and_loop() {
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
            InstructionKind::Jump(BlockId(1)),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![InstructionKind::Branch(
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Boolean,
            },
            BlockId(2),
            BlockId(3),
        )]),
    );
    program.blocks.insert(
        BlockId(2),
        Block::from_instruction_kinds(vec![InstructionKind::Jump(BlockId(4))]),
    );
    program.blocks.insert(
        BlockId(3),
        Block::from_instruction_kinds(vec![InstructionKind::Jump(BlockId(1))]),
    );
    program.blocks.insert(
        BlockId(4),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );

    let doms = build_doms(&mut program);

    expect![[r#"
        Block 0 dominated by block 0,
        Block 1 dominated by block 0,
        Block 2 dominated by block 1,
        Block 3 dominated by block 1,
        Block 4 dominated by block 2,
    "#]]
    .assert_eq(&display_dominator_graph(&doms));
}

#[test]
fn dominator_graph_complex_structure_only_dominated_by_entry() {
    // This example comes from the paper from [A Simple, Fast Dominance Algorithm](http://www.hipersoft.rice.edu/grads/publications/dom14.pdf)
    // by Cooper, Harvey, and Kennedy and uses the node numbering from the paper. However, the resulting dominator graph
    // is different due to the numbering of the blocks, such that each block is numbered in reverse postorder.
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

    program
        .callables
        .get_mut(CallableId(0))
        .expect("callable should be present")
        .body = Some(BlockId(6));
    program.blocks.insert(
        BlockId(6),
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
                BlockId(5),
                BlockId(4),
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(5),
        Block::from_instruction_kinds(vec![InstructionKind::Jump(BlockId(1))]),
    );
    program.blocks.insert(
        BlockId(4),
        Block::from_instruction_kinds(vec![InstructionKind::Branch(
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Boolean,
            },
            BlockId(2),
            BlockId(3),
        )]),
    );
    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![InstructionKind::Jump(BlockId(2))]),
    );
    program.blocks.insert(
        BlockId(2),
        Block::from_instruction_kinds(vec![InstructionKind::Branch(
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Boolean,
            },
            BlockId(3),
            BlockId(1),
        )]),
    );
    program.blocks.insert(
        BlockId(3),
        Block::from_instruction_kinds(vec![InstructionKind::Jump(BlockId(2))]),
    );

    let doms = build_doms(&mut program);

    expect![[r#"
        Block 0 dominated by block 0,
        Block 1 dominated by block 0,
        Block 2 dominated by block 0,
        Block 3 dominated by block 0,
        Block 4 dominated by block 0,
        Block 5 dominated by block 0,
    "#]]
    .assert_eq(&display_dominator_graph(&doms));
}

#[test]
fn dominator_graph_with_node_having_many_predicates() {
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
        Block::from_instruction_kinds(vec![InstructionKind::Branch(
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Boolean,
            },
            BlockId(3),
            BlockId(4),
        )]),
    );
    program.blocks.insert(
        BlockId(2),
        Block::from_instruction_kinds(vec![InstructionKind::Branch(
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Boolean,
            },
            BlockId(5),
            BlockId(6),
        )]),
    );
    program.blocks.insert(
        BlockId(3),
        Block::from_instruction_kinds(vec![InstructionKind::Jump(BlockId(7))]),
    );
    program.blocks.insert(
        BlockId(4),
        Block::from_instruction_kinds(vec![InstructionKind::Jump(BlockId(7))]),
    );
    program.blocks.insert(
        BlockId(5),
        Block::from_instruction_kinds(vec![InstructionKind::Jump(BlockId(7))]),
    );
    program.blocks.insert(
        BlockId(6),
        Block::from_instruction_kinds(vec![InstructionKind::Jump(BlockId(7))]),
    );
    program.blocks.insert(
        BlockId(7),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );

    let doms = build_doms(&mut program);

    expect![[r#"
        Block 0 dominated by block 0,
        Block 1 dominated by block 0,
        Block 2 dominated by block 0,
        Block 3 dominated by block 1,
        Block 4 dominated by block 1,
        Block 5 dominated by block 2,
        Block 6 dominated by block 2,
        Block 7 dominated by block 0,
    "#]]
    .assert_eq(&display_dominator_graph(&doms));
}
