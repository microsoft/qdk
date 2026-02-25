// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use core::panic;
use qsc_data_structures::index_map::IndexMap;
use qsc_partial_eval::{
    InstructionKind,
    rir::{Block, BlockId, Instruction, Variable},
};
use qsc_rir::debug::InstructionDbgMetadata;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use std::collections::VecDeque;
use std::vec;

/// RIR blocks -> Structured Control Flow
pub(super) fn reconstruct_control_flow(
    blocks: &IndexMap<BlockId, Block>,
    entry: BlockId,
) -> StructuredControlFlow {
    let return_block = find_return_block(blocks);
    let ordered = execution_order(blocks);

    let topo_index: FxHashMap<BlockId, usize> =
        ordered.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let must_reach = compute_must_reach_sets(blocks, return_block, &ordered);

    build_structured(blocks, &must_reach, &topo_index, entry, None)
}

pub(super) enum StructuredControlFlow {
    Seq(Vec<StructuredControlFlow>),
    /// A single basic block's "payload" (you can expand to instructions later).
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
        Instruction {
            kind: InstructionKind::Branch(condition, target1, target2),
            metadata,
        } => Terminator::Conditional(Branch {
            condition: *condition,
            true_block: *target1,
            false_block: *target2,
            instruction_metadata: metadata.clone(),
        }),
        Instruction {
            kind: InstructionKind::Jump(target),
            ..
        } => Terminator::Unconditional(*target),
        Instruction {
            kind: InstructionKind::Return,
            ..
        } => Terminator::Return,
        _ => panic!("unexpected terminator kind"),
    }
}

/// A block either:
/// - jumps to one next block,
/// - splits into two paths (if/else),
/// - or finishes (return).
fn next_blocks(block: &Block) -> Vec<BlockId> {
    match terminator(block) {
        Terminator::Unconditional(t) => vec![t],
        Terminator::Conditional(br) => vec![br.true_block, br.false_block],
        Terminator::Return => vec![],
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

/// Produce an order where every block appears before anything it can jump to.
/// (This works because you said there are no cycles.)
fn execution_order(blocks: &IndexMap<BlockId, Block>) -> Vec<BlockId> {
    // Count how many incoming edges each block has.
    let mut incoming_count: FxHashMap<BlockId, usize> = FxHashMap::default();
    for id in blocks.iter().map(|(k, _)| k) {
        incoming_count.insert(id, 0);
    }

    for (id, b) in blocks.iter() {
        for nxt in next_blocks(b) {
            *incoming_count.get_mut(&nxt).expect("missing successor") += 1;
        }
        let _ = id;
    }

    // Start with blocks that have no incoming edges.
    let mut ready: VecDeque<BlockId> = incoming_count
        .iter()
        .filter_map(|(id, n)| (*n == 0).then_some(*id))
        .collect();

    // Optional: keep deterministic ordering.
    {
        let mut v: Vec<_> = ready.drain(..).collect();
        v.sort();
        ready.extend(v);
    }

    let mut ordered = Vec::with_capacity(blocks.iter().count());

    while let Some(bid) = ready.pop_front() {
        ordered.push(bid);

        let b = blocks.get(bid).expect("missing block");
        for nxt in next_blocks(b) {
            let n = incoming_count.get_mut(&nxt).expect("missing successor");
            *n -= 1;
            if *n == 0 {
                ready.push_back(nxt);
            }
        }

        // Optional: keep deterministic ordering.
        if ready.len() > 1 {
            let mut v: Vec<_> = ready.drain(..).collect();
            v.sort();
            ready.extend(v);
        }
    }

    assert_eq!(
        ordered.len(),
        blocks.iter().count(),
        "graph has a cycle or inconsistent edges"
    );

    ordered
}

/// For each block b, compute the set of blocks that are guaranteed to happen
/// after b on the way to the final return.
///
/// This is the key trick for turning a split (if/else) into a clean structured
/// region with a well-defined merge point.
///
/// Rules:
/// - The return block must reach itself.
/// - If b unconditionally jumps to n, then b must reach everything n must reach.
/// - If b conditionally jumps to t/f, then b must reach only what BOTH branches
///   must reach (intersection).
fn compute_must_reach_sets(
    blocks: &IndexMap<BlockId, Block>,
    return_block: BlockId,
    ordered: &[BlockId],
) -> FxHashMap<BlockId, FxHashSet<BlockId>> {
    // Walk backwards so successors are already computed.
    let mut must_reach: FxHashMap<BlockId, FxHashSet<BlockId>> = FxHashMap::default();

    for &b in ordered.iter().rev() {
        if b == return_block {
            let mut s = FxHashSet::default();
            s.insert(return_block);
            must_reach.insert(b, s);
            continue;
        }

        let succs = next_blocks(blocks.get(b).expect("block should exist"));
        assert!(
            !succs.is_empty(),
            "non-return block must have a next step under your assumptions"
        );

        // Start with the first successor's must_reach set...
        let mut guaranteed = must_reach
            .get(&succs[0])
            .expect("in a DAG, successors appear later in reverse order walk")
            .clone();

        // ...and if there are multiple successors, keep only what's in ALL of them.
        for s in succs.iter().skip(1) {
            let ss = must_reach
                .get(s)
                .expect("in a DAG, successors appear later in reverse order walk");
            guaranteed.retain(|x| ss.contains(x));
        }

        // A block trivially "must reaches" itself (we include it to simplify joins).
        guaranteed.insert(b);
        must_reach.insert(b, guaranteed);
    }

    must_reach
}

/// Pick the earliest merge point for two paths a and b:
/// - find blocks that both paths are guaranteed to reach
/// - choose the one that happens earliest in the overall forward order
fn earliest_merge_point(
    must_reach: &FxHashMap<BlockId, FxHashSet<BlockId>>,
    order_index: &FxHashMap<BlockId, usize>,
    a: BlockId,
    b: BlockId,
) -> BlockId {
    let sa = must_reach.get(&a).expect("must reach set should exist");
    let sb = must_reach.get(&b).expect("must reach set should exist");

    let mut shared: Vec<BlockId> = sa.intersection(sb).copied().collect();
    assert!(
        !shared.is_empty(),
        "paths should reconverge under your assumptions"
    );

    shared.sort_by_key(|id| order_index[id]);
    shared[0]
}

/// Collect blocks reachable from `start` without stepping through `stop`.
/// Useful if you want to validate that a branch arm is a clean, contained region.
fn reachable_until(
    blocks: &IndexMap<BlockId, Block>,
    start: BlockId,
    stop: BlockId,
) -> FxHashSet<BlockId> {
    let mut seen = FxHashSet::default();
    let mut stack = vec![start];

    while let Some(n) = stack.pop() {
        if n == stop || seen.contains(&n) {
            continue;
        }
        seen.insert(n);

        for nxt in next_blocks(blocks.get(n).expect("block should exist")) {
            if nxt != stop {
                stack.push(nxt);
            }
        }
    }

    seen
}

/// `build_structured(entry, stop_at)` produces a structured control flow by:
/// - walking forward normally for straight-line jumps
/// - when it hits a split (conditional), it:
///     1) finds the merge point
///     2) recursively builds the "then" path until the merge
///     3) recursively builds the "else" path until the merge
///     4) continues after the merge
///
/// `stop_at` means "stop before entering this block" (don't include it).
fn build_structured(
    blocks: &IndexMap<BlockId, Block>,
    must_reach: &FxHashMap<BlockId, FxHashSet<BlockId>>,
    order_index: &FxHashMap<BlockId, usize>,
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
                let merge =
                    earliest_merge_point(must_reach, order_index, br.true_block, br.false_block);

                // Optional: region sanity checks / debugging
                let _then_region = reachable_until(blocks, br.true_block, merge);
                let _else_region = reachable_until(blocks, br.false_block, merge);

                let then_scf =
                    build_structured(blocks, must_reach, order_index, br.true_block, Some(merge));
                let else_scf =
                    build_structured(blocks, must_reach, order_index, br.false_block, Some(merge));

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
