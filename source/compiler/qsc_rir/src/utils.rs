// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::rir::{BlockId, BlockWithMetadata, Instruction, Program, VariableId};
use qsc_data_structures::index_map::IndexMap;
use rustc_hash::FxHashSet;

/// Given a block, return the block IDs of its successors.
#[must_use]
pub fn get_block_successors(block: &BlockWithMetadata) -> Vec<BlockId> {
    let mut successors = Vec::new();
    // Assume that the block is well-formed and that terminators only appear as the last instruction.
    match &block
        .0
        .last()
        .expect("block should have at least one instruction")
        .instruction
    {
        Instruction::Branch(_, target1, target2) => {
            successors.push(*target1);
            successors.push(*target2);
        }
        Instruction::Jump(target) => successors.push(*target),
        _ => {}
    }
    successors
}

/// Given a block ID and a containing program, return the block IDs of all blocks reachable from the given block including itself.
/// The returned block IDs are sorted in ascending order.
#[must_use]
pub fn get_all_block_successors(block: BlockId, program: &Program) -> Vec<BlockId> {
    let mut blocks_to_visit = get_block_successors(program.get_block(block));
    let mut blocks_visited = FxHashSet::default();
    while let Some(block_id) = blocks_to_visit.pop() {
        if blocks_visited.contains(&block_id) {
            continue;
        }
        blocks_visited.insert(block_id);
        let block = program.get_block(block_id);
        let block_successors = get_block_successors(block);
        blocks_to_visit.extend(block_successors.clone());
    }
    let mut successors = blocks_visited.into_iter().collect::<Vec<_>>();
    successors.sort_unstable();
    successors
}

/// Given a program, return a map from block IDs to the block IDs of their predecessors.
/// The vectors used as values in the map are sorted in ascending order, ensuring that block ids
/// for predecessors are listed lowest to highest.
#[must_use]
pub fn build_predecessors_map(program: &Program) -> IndexMap<BlockId, Vec<BlockId>> {
    let mut preds: IndexMap<BlockId, Vec<BlockId>> = IndexMap::default();

    for (block_id, block) in program.blocks.iter() {
        for successor in get_block_successors(block) {
            if let Some(preds_list) = preds.get_mut(successor) {
                preds_list.push(block_id);
            } else {
                preds.insert(successor, vec![block_id]);
            }
        }
    }

    for preds_list in preds.values_mut() {
        preds_list.sort_unstable();
    }

    preds
}

#[must_use]
pub fn get_variable_assignments(program: &Program) -> IndexMap<VariableId, (BlockId, usize)> {
    let mut assignments = IndexMap::default();
    let mut has_store = false;
    let mut has_phi = false;
    for (block_id, block) in program.blocks.iter() {
        for (idx, instr) in block.0.iter().enumerate() {
            match &instr.instruction {
                Instruction::Call(_, _, Some(var))
                | Instruction::Add(_, _, var)
                | Instruction::Sub(_, _, var)
                | Instruction::Mul(_, _, var)
                | Instruction::Sdiv(_, _, var)
                | Instruction::Srem(_, _, var)
                | Instruction::Shl(_, _, var)
                | Instruction::Ashr(_, _, var)
                | Instruction::Fadd(_, _, var)
                | Instruction::Fsub(_, _, var)
                | Instruction::Fmul(_, _, var)
                | Instruction::Fdiv(_, _, var)
                | Instruction::Fcmp(_, _, _, var)
                | Instruction::Icmp(_, _, _, var)
                | Instruction::LogicalNot(_, var)
                | Instruction::LogicalAnd(_, _, var)
                | Instruction::LogicalOr(_, _, var)
                | Instruction::BitwiseNot(_, var)
                | Instruction::BitwiseAnd(_, _, var)
                | Instruction::BitwiseOr(_, _, var)
                | Instruction::BitwiseXor(_, _, var)
                | Instruction::Phi(_, var) => {
                    assert!(
                        !assignments.contains_key(var.variable_id),
                        "Duplicate assignment to {:?} in {block_id:?}, instruction {idx}",
                        var.variable_id
                    );
                    has_phi |= matches!(&instr.instruction, Instruction::Phi(_, _));
                    assignments.insert(var.variable_id, (block_id, idx));
                }
                Instruction::Store(_, var) => {
                    has_store = true;
                    assignments.insert(var.variable_id, (block_id, idx));
                }

                Instruction::Call(_, _, None)
                | Instruction::Jump(..)
                | Instruction::Branch(..)
                | Instruction::Return => {}
            }
        }
    }
    assert!(
        !(has_store && has_phi),
        "Program has both store and phi instructions."
    );
    assignments
}
