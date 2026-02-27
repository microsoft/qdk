// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use core::panic;
use qsc_data_structures::index_map::IndexMap;
use qsc_partial_eval::{
    Instruction,
    rir::{Block, BlockId, Variable},
};
use qsc_rir::debug::InstructionDbgMetadata;
use qsc_rir::passes::build_post_dominator_graph;
use rustc_hash::FxHashSet;

/// RIR blocks -> Structured Control Flow
pub(super) fn reconstruct_control_flow(
    blocks: &IndexMap<BlockId, Block>,
    entry: BlockId,
) -> StructuredControlFlow {
    let return_block = find_return_block(blocks);
    // The RIR that comes back from passes uses block IDs that already ordered matching the control flow
    // as long as the program is a Directec Acyclic Graph (see source/compiler/qsc_rir/src/passes/remap_block_ids.rs).
    // Further, the `IndexMap` data structure always returns keys in ascending order, matching that DAG.
    // We rely on both assumptions below so that later code can use the ordering to recreate that
    // structure control flow.
    let ordered = blocks.iter().map(|(id, _)| id).collect::<Vec<_>>();
    assert!(
        ordered.is_sorted(),
        "IndexMap iteration order should be deterministic and sorted"
    );

    // Compute immediate post-dominators using the dominator algorithm on the reversed CFG.
    // The post-dominator of a branching block is exactly its merge point.
    let post_doms = build_post_dominator_graph(blocks, return_block);

    build_structured(blocks, &post_doms, entry, None)
}

pub(super) enum StructuredControlFlow {
    Seq(Vec<StructuredControlFlow>),
    BasicBlock(BlockId),
    If {
        cond: Variable,
        then_br: Box<StructuredControlFlow>,
        else_br: Box<StructuredControlFlow>,
        branch_instruction_metadata: Option<Box<InstructionDbgMetadata>>,
    },
    Return,
}

#[derive(Clone, Debug)]
struct Branch {
    condition: Variable,
    true_block: BlockId,
    false_block: BlockId,
    instruction_metadata: Option<Box<InstructionDbgMetadata>>,
}

#[derive(Debug, Clone)]
enum Terminator {
    Unconditional(BlockId),
    Conditional(Branch),
    Return,
}

#[must_use]
fn terminator(block: &Block) -> Terminator {
    // Assume that the block is well-formed and that terminators only appear as the last instruction.
    match &block
        .0
        .last()
        .expect("block should have at least one instruction")
    {
        Instruction::Branch(condition, target1, target2, metadata) => {
            Terminator::Conditional(Branch {
                condition: *condition,
                true_block: *target1,
                false_block: *target2,
                instruction_metadata: metadata.clone(),
            })
        }
        Instruction::Jump(target, ..) => Terminator::Unconditional(*target),
        Instruction::Return => Terminator::Return,
        _ => panic!("unexpected terminator kind"),
    }
}

/// Find the one final "finish" block (Return).
fn find_return_block(blocks: &IndexMap<BlockId, Block>) -> BlockId {
    let mut returns = blocks
        .iter()
        .filter_map(|(id, b)| matches!(terminator(b), Terminator::Return).then_some(id))
        .collect::<Vec<_>>();

    assert_eq!(returns.len(), 1, "expected exactly 1 Return block");
    returns.pop().expect("just checked non-empty")
}

/// `build_structured(entry, stop_at)` produces a structured control flow by:
/// - walking forward normally for straight-line jumps
/// - when it hits a split (conditional), it:
///     1) finds the merge point (the immediate post-dominator of the branching block)
///     2) recursively builds the "then" path until the merge
///     3) recursively builds the "else" path until the merge
///     4) continues after the merge
///
/// `stop_at` means "stop before entering this block" (don't include it).
fn build_structured(
    blocks: &IndexMap<BlockId, Block>,
    post_doms: &IndexMap<BlockId, BlockId>,
    entry: BlockId,
    stop_at: Option<BlockId>,
) -> StructuredControlFlow {
    let mut statements: Vec<StructuredControlFlow> = Vec::new();
    let mut cur = entry;

    // Safety belt: if something is malformed, don't spin.
    let mut visited_here: FxHashSet<BlockId> = FxHashSet::default();

    loop {
        if let Some(stop) = stop_at
            && cur == stop
        {
            break;
        }
        if !visited_here.insert(cur) {
            // In a clean DAG region we shouldn't re-visit blocks.
            break;
        }

        let blk = blocks.get(cur).expect("block should exist");

        // "Do this block's work"
        statements.push(StructuredControlFlow::BasicBlock(cur));

        match terminator(blk) {
            Terminator::Return => {
                statements.push(StructuredControlFlow::Return);
                break;
            }

            Terminator::Unconditional(next) => {
                cur = next;
            }

            Terminator::Conditional(br) => {
                // The merge point is the immediate post-dominator of the branching block:
                // the first block that every path from here must pass through.
                let merge = *post_doms
                    .get(cur)
                    .expect("branching block should have a post-dominator");

                let then_scf = build_structured(blocks, post_doms, br.true_block, Some(merge));
                let else_scf = build_structured(blocks, post_doms, br.false_block, Some(merge));

                statements.push(StructuredControlFlow::If {
                    cond: br.condition,
                    then_br: Box::new(then_scf),
                    else_br: Box::new(else_scf),
                    branch_instruction_metadata: br.instruction_metadata.clone(),
                });

                // After both paths, continue from the merge point.
                cur = merge;
            }
        }
    }

    if statements.len() == 1 {
        statements.pop().expect("just checked non-empty")
    } else {
        StructuredControlFlow::Seq(statements)
    }
}
