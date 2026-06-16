// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::index_map::IndexMap;

use crate::{
    rir::{BlockId, Program},
    utils::get_all_block_successors,
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
    let mut doms = IndexMap::default();

    // Program.blocks is a single flat arena holding the blocks of every callable. Calls are not CFG
    // edges, so the arena decomposes into disjoint per-callable subgraphs, one root per bodied callable.
    // Process each bodied callable independently and merge the results; block ids are globally unique,
    // so the merged map has no collisions.
    for callable_id in program.all_callable_ids() {
        // Intrinsics (and any other bodyless callables) have no blocks and contribute no dominators.
        let Some(entry_block_id) = program.get_callable(callable_id).body else {
            continue;
        };

        // The entry block of a body dominates itself.
        doms.insert(entry_block_id, entry_block_id);

        // The reachable blocks of this body, in ascending id order. The entry block is excluded here
        // because it is the only block that by definition dominates itself, so it never needs updating.
        let body_blocks = get_all_block_successors(entry_block_id, program);

        // The algorithm needs to run until the dominance map stabilizes, ie: no block's immediate
        // dominator changes.
        let mut changed = true;
        while changed {
            changed = false;
            for &block_id in &body_blocks {
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
                        new_dom = intersect(&doms, new_dom, *pred);
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
    }

    doms
}

/// Calculates the closest intersection of two blocks in the current dominator tree.
/// This is the block that dominates both block1 and block2, and is the closest to both.
/// This is done by walking up the dominator tree from both blocks until they meet, and
/// can take advantage of the ordering in the the block ids to walk only as far as necessary
/// and avoid membership checks in favor of simple comparisons.
fn intersect(
    doms: &IndexMap<BlockId, BlockId>,
    mut block1: BlockId,
    mut block2: BlockId,
) -> BlockId {
    while block1 != block2 {
        while block1 > block2 {
            block1 = *doms.get(block1).expect("block should be present");
        }
        while block2 > block1 {
            block2 = *doms.get(block2).expect("block should be present");
        }
    }
    block1
}
