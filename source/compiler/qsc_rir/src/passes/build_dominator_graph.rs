// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::index_map::IndexMap;

use crate::{
    rir::{Block, BlockId, Program},
    utils::get_block_successors,
};

#[cfg(test)]
mod tests;

/// Given a program, return a map from block IDs to the block ID of its immediate dominator. From this,
/// the dominator tree can be constructed by treating the map as a directed graph where the keys are the
/// children and the values are the parents.
/// This algorithm is from [A Simple, Fast Dominance Algorithm](http://www.hipersoft.rice.edu/grads/publications/dom14.pdf)
/// by Cooper, Harvey, and Kennedy, with two notable differences:
/// - Blocks are assumed to be sequentially numbered starting from 0 in reverse postorder rather than depth first order.
/// - Given that reversal, intersection between nodes uses the lesser of the two nodes rather than the greater.
#[must_use]
pub fn build_dominator_graph(
    program: &Program,
    preds: &IndexMap<BlockId, Vec<BlockId>>,
) -> IndexMap<BlockId, BlockId> {
    let entry_block_id = program
        .get_callable(program.entry)
        .body
        .expect("entry point should have a body");

    // Collect all block IDs except the entry, in ascending order (reverse postorder).
    let block_ids: Vec<BlockId> = program
        .blocks
        .iter()
        .map(|(id, _)| id)
        .filter(|&id| id != entry_block_id)
        .collect();

    build_dominator_graph_core(entry_block_id, &block_ids, preds, |a, b| a > b)
}

/// Given a set of blocks, return a map from block IDs to the block ID of its immediate post-dominator.
/// The given `exit_block_id` is the unique block with a `Return` terminator and serves as the root
/// of the post-dominator tree.
///
/// The post-dominator of a block B is the block that every path from B to the exit must pass through.
/// This is computed by running the dominator algorithm on the reversed control flow graph.
///
/// Blocks are assumed to be sequentially numbered starting from 0, matching the topological order
/// of the control flow DAG.
#[must_use]
pub fn build_post_dominator_graph(
    blocks: &IndexMap<BlockId, Block>,
    exit_block_id: BlockId,
) -> IndexMap<BlockId, BlockId> {
    // For post-dominators, we reverse the control flow graph:
    // - The exit block becomes the root of the dominator tree.
    // - Successors in the original graph become predecessors in the reversed graph.
    // - Blocks are iterated in descending order, since the reversed graph's
    //   reverse postorder is the reverse of the original topological order.
    let reversed_preds = build_reversed_predecessors_map(blocks);

    let block_ids: Vec<BlockId> = blocks
        .iter()
        .map(|(id, _)| id)
        .filter(|&id| id != exit_block_id)
        .rev()
        .collect();

    build_dominator_graph_core(exit_block_id, &block_ids, &reversed_preds, |a, b| a < b)
}

/// Core dominator algorithm parameterized over:
/// - `root_id`: the root of the dominator tree (entry for dominators, exit for post-dominators)
/// - `block_ids`: all block IDs except the root, in iteration order
/// - `preds`: predecessor map (original for dominators, reversed for post-dominators)
/// - `further_from_root`: comparison returning true when the first block is further from the root
///   than the second, used by intersection to walk toward the root
fn build_dominator_graph_core(
    root_id: BlockId,
    block_ids: &[BlockId],
    preds: &IndexMap<BlockId, Vec<BlockId>>,
    further_from_root: impl Fn(BlockId, BlockId) -> bool,
) -> IndexMap<BlockId, BlockId> {
    let mut doms = IndexMap::default();

    // The root dominates itself.
    doms.insert(root_id, root_id);

    // The algorithm needs to run until the dominance map stabilizes, ie: no block's immediate dominator changes.
    let mut changed = true;
    while changed {
        changed = false;
        // Always skip the root, as it is the only block that by definition dominates itself.
        for &block_id in block_ids {
            // The immediate dominator of a block is the intersection of the dominators of its predecessors.
            // Start from an assumption that the first predecessor is the dominator, and intersect with the rest.
            let (first_pred, rest_preds) = preds
                .get(block_id)
                .expect("block should be present")
                .split_first()
                .expect("every block should have at least one predecessor");
            let mut new_dom = *first_pred;

            // If there are no other predecessors, the immediate dominator is the first predecessor.
            for pred in rest_preds {
                // For each predecessor whose dominator is known, intersect with the current best guess.
                // Note that the dominator of the predecessor may be a best guess that gets updated in
                // a later iteration.
                if doms.contains_key(*pred) {
                    new_dom = intersect(&doms, new_dom, *pred, &further_from_root);
                }
            }

            // If the immediate dominator has changed, update the map and mark that the map has changed
            // so that the algorithm will run again.
            if doms.get(block_id) != Some(&new_dom) {
                doms.insert(block_id, new_dom);
                changed = true;
            }
        }
    }

    doms
}

/// Builds the predecessor map for the reversed control flow graph.
/// In the reversed graph, edges go from successors to predecessors,
/// so the predecessors of block X in the reversed graph are the successors
/// of X in the original graph.
fn build_reversed_predecessors_map(
    blocks: &IndexMap<BlockId, Block>,
) -> IndexMap<BlockId, Vec<BlockId>> {
    let mut reversed_preds: IndexMap<BlockId, Vec<BlockId>> = IndexMap::default();

    for (block_id, block) in blocks.iter() {
        let mut succs = get_block_successors(block);
        succs.sort_unstable();
        reversed_preds.insert(block_id, succs);
    }

    reversed_preds
}

/// Calculates the closest intersection of two blocks in the current dominator tree.
/// This is the block that dominates both block1 and block2, and is the closest to both.
/// This is done by walking up the dominator tree from both blocks until they meet, and
/// can take advantage of the ordering in the block ids to walk only as far as necessary
/// and avoid membership checks in favor of simple comparisons.
fn intersect(
    doms: &IndexMap<BlockId, BlockId>,
    mut block1: BlockId,
    mut block2: BlockId,
    further_from_root: &impl Fn(BlockId, BlockId) -> bool,
) -> BlockId {
    while block1 != block2 {
        while further_from_root(block1, block2) {
            block1 = *doms.get(block1).expect("block should be present");
        }
        while further_from_root(block2, block1) {
            block2 = *doms.get(block2).expect("block should be present");
        }
    }
    block1
}
