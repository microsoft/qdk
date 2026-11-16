// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines, clippy::needless_raw_string_hashes)]

use crate::{
    builder::{bell_program, new_program},
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
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );

    program
        .blocks
        .insert(BlockId(0), Block(vec![Instruction::Return(None)]));

    let preds = build_predecessors_map(&program);
    let doms = build_dominator_graph(&program, &preds);

    expect![[r#"
        Block 0 dominated by block 0,
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
