// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::VecDeque;

use rustc_hash::FxHashMap;

use crate::{
    rir::{BlockId, Instruction, InstructionWithMetadata, Program},
    utils::{get_all_block_successors, get_block_successors},
};

#[cfg(test)]
mod tests;

/// Remaps block IDs in the given program to be contiguous, starting from 0,
/// and in a topological ordering if the program is Directed Acyclic Graph (DAG).
/// Topological ordering is useful for passes that assume each block's successors
/// have higher IDs than the block itself. This is best effort; if the program has a cycle,
/// the function will still remap block IDs but the ordering may not be topological.
pub fn remap_block_ids(program: &mut Program) {
    // Check if the program is acyclic, which lets us construct a topological ordering.
    let is_acyclic = check_acyclic(program);

    // Only update the entry point.
    let entry_block_id = program
        .get_callable(program.entry)
        .body
        .expect("entry point should have a body block");

    // Because we know the program is acyclic, we can keep a list as the map from old block IDs to new block IDs, where
    // the new block ID is the index in the list.
    let mut block_id_map = Vec::new();
    let mut blocks_to_visit: VecDeque<BlockId> = vec![entry_block_id].into();
    while let Some(block_id) = blocks_to_visit.pop_front() {
        // If we've already visited this block, remove it from the previous ordering so that we can insert it at the end.
        // This effectively remaps all the blocks in the list and updates the mapped id of the current block.
        // This is only safe without cycles, so on a cyclic graph the node is skipped and not remapped.
        if is_acyclic {
            block_id_map.retain(|id| *id != block_id);
        } else if block_id_map.contains(&block_id) {
            continue;
        }
        block_id_map.push(block_id);

        let successors = get_block_successors(program.get_block(block_id));
        if blocks_to_visit.len() >= successors.len()
            && blocks_to_visit
                .iter()
                .skip(blocks_to_visit.len() - successors.len())
                .eq(successors.iter())
        {
            // All successors are already at the end of the queue in same order, so avoid adding them and reprocessing
            // the same blocks back-to-back.
            continue;
        }
        // Since we are going to extend the blocks to visit using the successors of the current block, we can remove them from
        // anywhere else in the list to visit so we avoid visiting them multiple times (only the last visit to a block is
        // significant, so others can be skipped).
        blocks_to_visit.retain(|id| !successors.contains(id));
        blocks_to_visit.extend(successors);
    }

    let block_id_map = block_id_map
        .into_iter()
        .enumerate()
        .map(|(new_id, old_id)| (old_id, new_id))
        .collect::<FxHashMap<_, _>>();

    let blocks = program.blocks.drain().collect::<Vec<_>>();
    for (old_block_id, mut block) in blocks {
        let new_block_id = block_id_map[&old_block_id];
        update_phi_nodes(&block_id_map, &mut block.0);
        update_terminator(
            &block_id_map,
            block
                .0
                .last_mut()
                .expect("block should have at least one instruction"),
        );
        program
            .blocks
            .insert_with_metadata(new_block_id.into(), block);
    }
    program
        .callables
        .get_mut(program.entry)
        .expect("entry should exist")
        .body = Some(block_id_map[&entry_block_id].into());
}

fn check_acyclic(program: &Program) -> bool {
    for (block_id, _) in program.blocks.iter() {
        if get_all_block_successors(block_id, program).contains(&block_id) {
            return false;
        }
    }
    true
}

fn update_phi_nodes(
    block_id_map: &FxHashMap<BlockId, usize>,
    instrs: &mut [InstructionWithMetadata],
) {
    for instr in instrs.iter_mut() {
        if let Instruction::Phi(args, _) = &mut instr.instruction {
            for arg in args.iter_mut() {
                arg.1 = (*block_id_map
                    .get(&arg.1)
                    .expect("block ids in phi node should exist in block id map"))
                .into();
            }
        } else {
            // Since phi nodes are always at the top of the block, we can break early.
            return;
        }
    }
}

fn update_terminator(
    block_id_map: &FxHashMap<BlockId, usize>,
    instruction: &mut InstructionWithMetadata,
) {
    match &mut instruction.instruction {
        Instruction::Jump(target) => {
            *target = block_id_map[target].into();
        }
        Instruction::Branch(_, target1, target2) => {
            *target1 = block_id_map[target1].into();
            *target2 = block_id_map[target2].into();
        }
        _ => {}
    }
}
