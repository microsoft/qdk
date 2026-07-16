// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines, clippy::needless_raw_string_hashes)]

use crate::{
    builder::{
        bell_program, new_program, two_body_mutable_param_program, two_body_program,
        two_body_program_with_branch,
    },
    passes::remap_block_ids,
    rir::{
        Block, BlockId, Callable, CallableId, CallableType, Instruction, Prim, Program, Ty,
        Variable, VariableId,
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
    program
        .blocks
        .insert(BlockId(0), Block(vec![Instruction::Return(None)]));

    let doms = build_doms(&mut program);

    expect![[r#"
        Block 0 dominated by block 0,
    "#]]
    .assert_eq(&display_dominator_graph(&doms));
}

#[test]
fn dominator_graph_sequential_blocks_dominated_by_predecessor() {
    let mut program = new_program();
    program
        .blocks
        .insert(BlockId(0), Block(vec![Instruction::Jump(BlockId(1))]));
    program
        .blocks
        .insert(BlockId(1), Block(vec![Instruction::Jump(BlockId(2))]));
    program
        .blocks
        .insert(BlockId(2), Block(vec![Instruction::Return(None)]));

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
            Instruction::Jump(BlockId(1)),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![Instruction::Branch(
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Prim(Prim::Boolean),
            },
            BlockId(2),
            BlockId(3),
            None,
        )]),
    );
    program
        .blocks
        .insert(BlockId(2), Block(vec![Instruction::Return(None)]));
    program
        .blocks
        .insert(BlockId(3), Block(vec![Instruction::Return(None)]));

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
    program
        .blocks
        .insert(BlockId(0), Block(vec![Instruction::Jump(BlockId(1))]));
    program
        .blocks
        .insert(BlockId(1), Block(vec![Instruction::Jump(BlockId(1))]));

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
            Instruction::Jump(BlockId(1)),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![Instruction::Branch(
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Prim(Prim::Boolean),
            },
            BlockId(2),
            BlockId(3),
            None,
        )]),
    );
    program
        .blocks
        .insert(BlockId(2), Block(vec![Instruction::Jump(BlockId(4))]));
    program
        .blocks
        .insert(BlockId(3), Block(vec![Instruction::Jump(BlockId(1))]));
    program
        .blocks
        .insert(BlockId(4), Block(vec![Instruction::Return(None)]));

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
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            input_vars: Vec::new(),
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
                BlockId(5),
                BlockId(4),
                None,
            ),
        ]),
    );
    program
        .blocks
        .insert(BlockId(5), Block(vec![Instruction::Jump(BlockId(1))]));
    program.blocks.insert(
        BlockId(4),
        Block(vec![Instruction::Branch(
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Prim(Prim::Boolean),
            },
            BlockId(2),
            BlockId(3),
            None,
        )]),
    );
    program
        .blocks
        .insert(BlockId(1), Block(vec![Instruction::Jump(BlockId(2))]));
    program.blocks.insert(
        BlockId(2),
        Block(vec![Instruction::Branch(
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Prim(Prim::Boolean),
            },
            BlockId(3),
            BlockId(1),
            None,
        )]),
    );
    program
        .blocks
        .insert(BlockId(3), Block(vec![Instruction::Jump(BlockId(2))]));

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
        Block(vec![Instruction::Branch(
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Prim(Prim::Boolean),
            },
            BlockId(3),
            BlockId(4),
            None,
        )]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![Instruction::Branch(
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Prim(Prim::Boolean),
            },
            BlockId(5),
            BlockId(6),
            None,
        )]),
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
    program
        .blocks
        .insert(BlockId(6), Block(vec![Instruction::Jump(BlockId(7))]));
    program
        .blocks
        .insert(BlockId(7), Block(vec![Instruction::Return(None)]));

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

#[test]
fn build_dominator_graph_two_single_block_bodies() {
    let mut program = two_body_program();

    let doms = build_doms(&mut program);

    // Each body is a single block that dominates itself; the merged map covers both roots.
    expect![[r#"
        Block 0 dominated by block 0,
        Block 1 dominated by block 1,
    "#]]
    .assert_eq(&display_dominator_graph(&doms));
}

#[test]
fn build_dominator_graph_second_body_with_branch() {
    let mut program = two_body_program_with_branch();

    let doms = build_doms(&mut program);

    // The entry body is block 0 (single block). The second body is a diamond rooted at its own entry,
    // which has no predecessors yet is processed without panicking.
    expect![[r#"
        Block 0 dominated by block 0,
        Block 1 dominated by block 1,
        Block 2 dominated by block 1,
        Block 3 dominated by block 1,
    "#]]
    .assert_eq(&display_dominator_graph(&doms));
}

#[test]
fn build_dominator_graph_body_with_parameters() {
    // The second body reads and stores into its own `input_vars` parameter. Parameters are variables,
    // not blocks, so they never appear in the dominator map or as predecessors.
    let mut program = two_body_mutable_param_program();

    let doms = build_doms(&mut program);

    expect![[r#"
        Block 0 dominated by block 0,
        Block 1 dominated by block 1,
    "#]]
    .assert_eq(&display_dominator_graph(&doms));
}

#[test]
fn build_dominator_graph_intrinsic_callables_skipped() {
    // A bodyless intrinsic interleaved between two bodied callables must be ignored: it contributes no
    // root and no blocks to the dominator map.
    let mut program = new_program();
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "intrinsic".to_string(),
            input_type: vec![Ty::Prim(Prim::Qubit)],
            input_vars: Vec::new(),
            output_type: None,
            body: None,
            call_type: CallableType::Regular,
        },
    );
    program.callables.insert(
        CallableId(2),
        Callable {
            name: "second_body".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Integer)),
            body: Some(BlockId(1)),
            call_type: CallableType::Regular,
        },
    );

    program
        .blocks
        .insert(BlockId(0), Block(vec![Instruction::Return(None)]));
    program
        .blocks
        .insert(BlockId(1), Block(vec![Instruction::Return(None)]));

    let preds = build_predecessors_map(&program);
    let doms = build_dominator_graph(&program, &preds);

    expect![[r#"
        Block 0 dominated by block 0,
        Block 1 dominated by block 1,
    "#]]
    .assert_eq(&display_dominator_graph(&doms));
}

#[test]
fn build_dominator_graph_single_body_unchanged_regression() {
    // A realistic single-body program still produces a single-root dominator map, confirming the
    // per-callable driver leaves single-body output unchanged.
    let mut program = bell_program();

    let doms = build_doms(&mut program);

    expect![[r#"
        Block 0 dominated by block 0,
    "#]]
    .assert_eq(&display_dominator_graph(&doms));
}
