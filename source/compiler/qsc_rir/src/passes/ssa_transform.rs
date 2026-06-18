// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::{
    rir::{BlockId, Instruction, Operand, Program, Ty, Variable, VariableId},
    utils::{get_all_block_successors, get_variable_assignments, map_variable_use_in_block},
};
use qsc_data_structures::index_map::IndexMap;
use rustc_hash::{FxHashMap, FxHashSet};

/// Transforms the program into Single Static Assignment (SSA) form by inserting phi nodes
/// at the beginning of blocks where necessary, allowing the removal of store instructions.
pub fn transform_to_ssa(program: &mut Program, preds: &IndexMap<BlockId, Vec<BlockId>>) {
    // Ensure that the graph is acyclic before proceeding. Current approach does not support cycles.
    ensure_acyclic(preds);

    // Get the next available variable ID for use in newly generated phi nodes. This is computed once
    // across the whole program so that freshly minted SSA versions stay unique across every body.
    let mut next_var_id = get_variable_assignments(program)
        .iter()
        .next_back()
        .map(|(var_id, _)| var_id.successor())
        .unwrap_or_default();

    // The program's blocks form disjoint per-callable subgraphs, since a `Call` is not a control-flow
    // edge. Transform each bodied callable independently, using its declared entry block as the root
    // rather than inferring the entry from blocks that happen to have no predecessors.
    for callable_id in program.all_callable_ids() {
        let callable = program.get_callable(callable_id);
        let Some(entry) = callable.body else {
            continue;
        };

        // Live-in parameters have no defining instruction, so pair each one with its declared type so
        // it can be seeded as a definition at the body entry.
        let input_vars: Vec<(VariableId, Ty)> = callable
            .input_vars
            .iter()
            .copied()
            .zip(callable.input_type.iter().copied())
            .collect();

        transform_body_to_ssa(program, entry, &input_vars, preds, &mut next_var_id);
    }
}

// Transform a single bodied callable into SSA form. Only the blocks reachable from the body's entry
// are processed, and the body's live-in parameters are seeded as definitions at the entry.
fn transform_body_to_ssa(
    program: &mut Program,
    entry: BlockId,
    input_vars: &[(VariableId, Ty)],
    preds: &IndexMap<BlockId, Vec<BlockId>>,
    next_var_id: &mut VariableId,
) {
    // Process the entry block together with every block reachable from it, in ascending block-id
    // order so that each block is handled after its predecessors.
    let mut body_blocks = vec![entry];
    body_blocks.append(&mut get_all_block_successors(entry, program));

    // First, remove store instructions and propagate variables through individual blocks.
    // This produces a per-block map of dynamic variables to their values.
    // Orphan variables may be left behind where a variable is defined in one block and used in another, which
    // will be resolved by inserting phi nodes.
    let mut block_var_map =
        map_store_to_dominated_ssa(program, &body_blocks, entry, input_vars, preds);

    // Insert phi nodes where necessary, mapping any remaining orphaned uses to the new variable
    // created by the phi node.
    // This can be done in one pass because the graph is assumed to be acyclic.
    for &block_id in &body_blocks {
        let block = program.get_block_mut(block_id);
        let Some(block_preds) = preds.get(block_id) else {
            // The body entry has no predecessors and therefore has no phi nodes.
            continue;
        };

        // Use a map to track updates to the variable map for the block. These will be applied after
        // any phi nodes are inserted and will replace any orphaned variables.
        let mut var_map_updates = FxHashMap::default();

        let (first_pred, rest_preds) = block_preds
            .split_first()
            .expect("block should have at least one predecessor");

        // The block is only a candidate for phi nodes if it has multiple predecessors.
        if rest_preds.is_empty() {
            // If the block has only one predecessor, track any updates to the variable map from that
            // predecessor to ensure any phi values that may have been added or inherited in the predecessor
            // are propagated to this block.
            let pred_var_map = block_var_map
                .get(*first_pred)
                .expect("block should have variable map");
            pred_var_map.clone_into(&mut var_map_updates);
        } else {
            // Check each variable in the first predecessor's variable map, and if any other
            // predecessor has a different value for the variable, a phi node is needed.
            let first_pred_map = block_var_map
                .get(*first_pred)
                .expect("block should have variable map");
            'var_loop: for (var_id, operand) in first_pred_map {
                let mut phi_nodes = FxHashMap::default();

                if rest_preds.iter().any(|pred| {
                    block_var_map
                        .get(*pred)
                        .expect("block should have variable map")
                        .get(var_id)
                        != Some(operand)
                }) {
                    // Some predecessors have different values for this variable, so a phi node is needed.
                    // Start with the first predecessor's value and block id, then add the values from the other predecessors.
                    let mut phi_args = vec![(operand.mapped(first_pred_map), *first_pred)];
                    for pred in rest_preds {
                        let pred_var_map = block_var_map
                            .get(*pred)
                            .expect("block should have variable map");
                        let mut pred_operand = match pred_var_map.get(var_id) {
                            Some(operand) => *operand,
                            None => {
                                // If the variable is not defined in this predecessor, it does not dominate this block.
                                // Assume it is not used and skip creating a phi node for this variable. If the variable is used,
                                // the ssa check will detect it and panic later.
                                continue 'var_loop;
                            }
                        };
                        pred_operand = pred_operand.mapped(pred_var_map);
                        phi_args.push((pred_operand, *pred));
                    }
                    phi_nodes.insert(*var_id, phi_args);
                } else {
                    // If all predecessors have the same value for this variable, the value can be propagated.
                    // Update the block variable map with the common operand.
                    var_map_updates.insert(*var_id, *operand);
                }

                // For any phi nodes that need to be inserted, create a new variable and insert
                // the phi node at the beginning of the block. The new variable will be used to replace
                // the original variable in the block's variable map, which will take care of any orphaned uses.
                for (variable_id, args) in phi_nodes {
                    let new_var = Variable {
                        variable_id: *next_var_id,
                        ty: operand.get_type(),
                    };
                    let phi_node = Instruction::Phi(args, new_var);
                    block.0.insert(0, phi_node);
                    var_map_updates.insert(variable_id, Operand::Variable(new_var));
                    *next_var_id = next_var_id.successor();
                }
            }
        }

        // Now that the block has finished processing, apply any updates to the block and
        // merge those updates into the stored variable map to propagate to successors.
        map_variable_use_in_block(block, &mut var_map_updates, &FxHashSet::default());
        for (var_id, operand) in var_map_updates {
            let var_map = block_var_map
                .get_mut(block_id)
                .expect("block should have variable map");
            var_map.entry(var_id).or_insert(operand);
        }
    }
}

// For now, SSA transform assumes the graph is acyclic, so verify that no block has a predecessor with
// a block id less than itself, which would indicate a cycle.
fn ensure_acyclic(preds: &IndexMap<BlockId, Vec<BlockId>>) {
    for (block_id, block_preds) in preds.iter() {
        assert!(
            !block_preds.iter().any(|pred| *pred >= block_id),
            "block {block_id:?} has a cycle in its predecessors"
        );
    }
}

// Remove store instructions and propagate variables through individual blocks.
// This produces a per-block map of dynamic variables to their values.
// Any block with a single predecessor inherits that predecessor's mapped variables, since those
// are live across the block. The body's live-in parameters are seeded as definitions at the entry
// block so that uses of a parameter resolve and later merges can build phi nodes for it.
fn map_store_to_dominated_ssa(
    program: &mut Program,
    body_blocks: &[BlockId],
    entry: BlockId,
    input_vars: &[(VariableId, Ty)],
    preds: &IndexMap<BlockId, Vec<BlockId>>,
) -> IndexMap<BlockId, FxHashMap<VariableId, Operand>> {
    let mut block_var_map = IndexMap::default();
    for &block_id in body_blocks {
        let mut var_map: FxHashMap<VariableId, Operand> = match preds.get(block_id) {
            Some(block_preds) if block_preds.len() == 1 => {
                // Any block with a single predecessor inherits those mapped variables.
                block_var_map
                    .get(block_preds[0])
                    .cloned()
                    .unwrap_or_default()
            }
            _ => FxHashMap::default(),
        };
        if block_id == entry {
            // Each parameter is its own initial SSA version, defined at the body entry.
            for &(var_id, ty) in input_vars {
                var_map.entry(var_id).or_insert(Operand::Variable(Variable {
                    variable_id: var_id,
                    ty,
                }));
            }
        }
        map_variable_use_in_block(
            program.get_block_mut(block_id),
            &mut var_map,
            &FxHashSet::default(),
        );
        block_var_map.insert(block_id, var_map);
    }
    block_var_map
}
