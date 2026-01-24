// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use rustc_hash::FxHashMap;

use crate::{
    rir::{Instruction, Program},
    utils::build_predecessors_map,
};

/// Simplify control flow of the program.
/// For now, this only removes single redundant blocks where the block is the only successor of its only predecessor.
pub fn simplify_control_flow(program: &mut Program) {
    // Use a map to track which blocks have been merged into their predecessors. This helps
    // find the ultimate target block for later merges.
    let mut merge_map = FxHashMap::default();

    // For each block, check if it has a single predecessor and that predecessor ends with a jump to the current block.
    // If so, the block is redundant and can be merged.
    let preds_map = build_predecessors_map(program);
    for (block_id, preds) in preds_map.iter() {
        if preds.len() == 1
            && program.get_block(preds[0]).0.last().map(|i| &i.instruction)
                == Some(&Instruction::Jump(block_id))
        {
            merge_map.insert(block_id, preds[0]);

            // The block to merge into may itself have been merged into another block.
            // Find the ultimate target block by iterating through the merge map until we find a predecessor
            // block that is missing from the map indicating it hasn't been merged.
            let mut target_block_id = preds[0];
            while let Some(mapped_block) = merge_map.get(&target_block_id) {
                target_block_id = *mapped_block;
            }

            // Clone the instructions for the current block. This is done via clone in part to satisfy the
            // borrow checker, and in part so the check for last instruction above doesn't need to traverse
            // to the ultimate target block via the merge map.
            let mut instrs = program.get_block_mut(block_id).0.clone();
            let target_block = program.get_block_mut(target_block_id);

            // Remove the existing terminator instruction and append the instructions copied from the current block.
            target_block.0.pop();
            target_block.0.append(&mut instrs);
        }
    }

    // Since each key in the merge map is a block that has been merged into another block, the keys can be used
    // as the list of blocks to remove from the program.
    for block_id in merge_map.keys() {
        program.blocks.remove(*block_id);
    }
}
