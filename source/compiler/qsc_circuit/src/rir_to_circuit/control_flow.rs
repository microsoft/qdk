// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use core::panic;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use std::vec;

use super::QuantumProgram;
use super::instruction_types::{BlockIdx, DbgLocationIdx, Instr, Var};

/// RIR blocks -> Structured Control Flow
pub(super) fn reconstruct_control_flow(
    program: &impl QuantumProgram,
    entry: BlockIdx,
) -> StructuredControlFlow {
    let return_block = find_return_block(program);
    // The RIR that comes back from passes uses block IDs that already ordered matching the control flow
    // as long as the program is a Directec Acyclic Graph (see source/compiler/qsc_rir/src/passes/remap_block_ids.rs).
    // Further, the `IndexMap` data structure always returns keys in ascending order, matching that DAG.
    // We rely on both assumptions below so that later code can use the ordering to recreate that
    // structure control flow.
    let ordered = program.block_ids();
    assert!(
        ordered.is_sorted(),
        "block IDs should be deterministic and sorted"
    );

    let must_reach = compute_must_reach_sets(program, return_block, &ordered);

    build_structured(program, &must_reach, entry, None)
}

pub(super) enum StructuredControlFlow {
    Seq(Vec<StructuredControlFlow>),
    BasicBlock(BlockIdx),
    If {
        cond: Var,
        then_br: Box<StructuredControlFlow>,
        else_br: Box<StructuredControlFlow>,
        branch_dbg_location: Option<DbgLocationIdx>,
    },
    Return,
}

#[derive(Clone, Debug)]
struct Branch {
    condition: Var,
    true_block: BlockIdx,
    false_block: BlockIdx,
    dbg_location: Option<DbgLocationIdx>,
}

#[derive(Debug, Clone)]
enum Terminator {
    Unconditional(BlockIdx),
    Conditional(Branch),
    Return,
}

#[must_use]
fn terminator(instructions: &[Instr]) -> Terminator {
    // Assume that the block is well-formed and that terminators only appear as the last instruction.
    match instructions
        .last()
        .expect("block should have at least one instruction")
    {
        Instr::Branch {
            condition,
            true_block,
            false_block,
            dbg_location,
        } => Terminator::Conditional(Branch {
            condition: *condition,
            true_block: *true_block,
            false_block: *false_block,
            dbg_location: *dbg_location,
        }),
        Instr::Jump(target) => Terminator::Unconditional(*target),
        Instr::Return => Terminator::Return,
        _ => panic!("unexpected terminator kind"),
    }
}

/// A block either:
/// - jumps to one next block,
/// - splits into two paths (if/else),
/// - or finishes (return).
fn next_blocks(instructions: &[Instr]) -> Vec<BlockIdx> {
    match terminator(instructions) {
        Terminator::Unconditional(t) => vec![t],
        Terminator::Conditional(br) => vec![br.true_block, br.false_block],
        Terminator::Return => vec![],
    }
}

/// Find the one final "finish" block (Return).
fn find_return_block(program: &impl QuantumProgram) -> BlockIdx {
    let mut returns = program
        .block_ids()
        .into_iter()
        .filter(|id| {
            matches!(
                terminator(&program.get_block_instructions(*id)),
                Terminator::Return
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(returns.len(), 1, "expected exactly 1 Return block");
    returns.pop().expect("just checked non-empty")
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
    program: &impl QuantumProgram,
    return_block: BlockIdx,
    ordered: &[BlockIdx],
) -> FxHashMap<BlockIdx, FxHashSet<BlockIdx>> {
    // Walk backwards so successors are already computed.
    let mut must_reach: FxHashMap<BlockIdx, FxHashSet<BlockIdx>> = FxHashMap::default();

    for &b in ordered.iter().rev() {
        if b == return_block {
            let mut s = FxHashSet::default();
            s.insert(return_block);
            must_reach.insert(b, s);
            continue;
        }

        let succs = next_blocks(&program.get_block_instructions(b));
        assert!(!succs.is_empty(), "non-return block must have a next step");

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
    must_reach: &FxHashMap<BlockIdx, FxHashSet<BlockIdx>>,
    a: BlockIdx,
    b: BlockIdx,
) -> BlockIdx {
    let sa = must_reach.get(&a).expect("must reach set should exist");
    let sb = must_reach.get(&b).expect("must reach set should exist");

    *sa.intersection(sb)
        .min()
        .expect("there should be at least the return block in common")
}

/// Collect blocks reachable from `start` without stepping through `stop`.
fn reachable_until(
    program: &impl QuantumProgram,
    start: BlockIdx,
    stop: BlockIdx,
) -> FxHashSet<BlockIdx> {
    let mut seen = FxHashSet::default();
    let mut stack = vec![start];

    while let Some(n) = stack.pop() {
        if n == stop || seen.contains(&n) {
            continue;
        }
        seen.insert(n);

        for nxt in next_blocks(&program.get_block_instructions(n)) {
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
    program: &impl QuantumProgram,
    must_reach: &FxHashMap<BlockIdx, FxHashSet<BlockIdx>>,
    entry: BlockIdx,
    stop_at: Option<BlockIdx>,
) -> StructuredControlFlow {
    let mut statements: Vec<StructuredControlFlow> = Vec::new();
    let mut cur = entry;

    // Safety belt: if something is malformed, don't spin.
    let mut visited_here: FxHashSet<BlockIdx> = FxHashSet::default();

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

        let blk = program.get_block_instructions(cur);

        // "Do this block's work"
        statements.push(StructuredControlFlow::BasicBlock(cur));

        match terminator(&blk) {
            Terminator::Return => {
                statements.push(StructuredControlFlow::Return);
                break;
            }

            Terminator::Unconditional(next) => {
                cur = next;
            }

            Terminator::Conditional(br) => {
                let merge = earliest_merge_point(must_reach, br.true_block, br.false_block);

                // Optional: region sanity checks / debugging
                let _then_region = reachable_until(program, br.true_block, merge);
                let _else_region = reachable_until(program, br.false_block, merge);

                let then_scf = build_structured(program, must_reach, br.true_block, Some(merge));
                let else_scf = build_structured(program, must_reach, br.false_block, Some(merge));

                statements.push(StructuredControlFlow::If {
                    cond: br.condition,
                    then_br: Box::new(then_scf),
                    else_br: Box::new(else_scf),
                    branch_dbg_location: br.dbg_location,
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
