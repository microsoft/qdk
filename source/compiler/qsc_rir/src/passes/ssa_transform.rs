// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::{
    rir::{Block, BlockId, Instruction, Operand, Program, Variable, VariableId},
    utils::get_variable_assignments,
};
use qsc_data_structures::index_map::IndexMap;
use rustc_hash::FxHashMap;

/// Transforms the program into Single Static Assignment (SSA) form by inserting phi nodes
/// at the beginning of blocks where necessary, allowing the removal of store instructions.
pub fn transform_to_ssa(program: &mut Program, preds: &IndexMap<BlockId, Vec<BlockId>>) {
    // Ensure that the graph is acyclic before proceeding. Current approach does not support cycles.
    ensure_acyclic(preds);

    // Get the next available variable ID for use in newly generated phi nodes.
    let mut next_var_id = get_variable_assignments(program)
        .iter()
        .next_back()
        .map(|(var_id, _)| var_id.successor())
        .unwrap_or_default();

    // First, remove store instructions and propagate variables through individual blocks.
    // This produces a per-block map of dynamic variables to their values.
    // Orphan variables may be left behind where a variable is defined in one block and used in another, which
    // will be resolved by inserting phi nodes.
    let mut block_var_map = map_store_to_dominated_ssa(program, preds);

    // Insert phi nodes where necessary, mapping any remaining orphaned uses to the new variable
    // created by the phi node.
    // This can be done in one pass because the graph is assumed to be acyclic.
    for (block_id, block) in program.blocks.iter_mut() {
        let Some(block_preds) = preds.get(block_id) else {
            // The block with no predecessors is the entry block and has no phi nodes.
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
                        variable_id: next_var_id,
                        ty: operand.get_type(),
                    };
                    let phi_node = Instruction::Phi(args, new_var);
                    block.0.insert(0, phi_node);
                    var_map_updates.insert(variable_id, Operand::Variable(new_var));
                    next_var_id = next_var_id.successor();
                }
            }
        }

        // Now that the block has finished processing, apply any updates to the block and
        // merge those updates into the stored variable map to propagate to successors.
        map_variable_use_in_block(block, &mut var_map_updates);
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
// are live across the block.
fn map_store_to_dominated_ssa(
    program: &mut Program,
    preds: &IndexMap<BlockId, Vec<BlockId>>,
) -> IndexMap<BlockId, FxHashMap<VariableId, Operand>> {
    let mut block_var_map = IndexMap::default();
    for (block_id, block) in program.blocks.iter_mut() {
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
        map_variable_use_in_block(block, &mut var_map);
        block_var_map.insert(block_id, var_map);
    }
    block_var_map
}

// Propagates stored variables through a block, tracking the latest stored value and replacing
// usage of the variable with the stored value.
fn map_variable_use_in_block(block: &mut Block, var_map: &mut FxHashMap<VariableId, Operand>) {
    let instrs = block.0.drain(..).collect::<Vec<_>>();

    for mut instr in instrs {
        match &mut instr {
            // Track the new value of the variable and omit the store instruction.
            Instruction::Store(operand, var) => {
                // Note this uses the mapped operand to make sure this variable points to whatever root literal or variable
                // this operand corresponds to at this point in the block. This makes the new variable respect a point-in-time
                // copy of the operand.
                var_map.insert(var.variable_id, operand.mapped(var_map));
                continue;
            }

            // Replace any arguments with the new values of stored variables.
            Instruction::Call(_, args, _) => {
                *args = args
                    .iter()
                    .map(|arg| match arg {
                        Operand::Variable(var) => {
                            // If the variable is not in the map, it is not something whose value has been updated via store in this block,
                            // so just fallback to use the `arg` value directly.
                            // `map_to_operand` does this automatically by returning `self`` when the variable is not in the map.
                            var.map_to_operand(var_map)
                        }
                        Operand::Literal(_) => *arg,
                    })
                    .collect();
            }

            // Replace the branch condition with the new value of the variable.
            Instruction::Branch(var, _, _) => {
                *var = var.map_to_variable(var_map);
            }

            // Two variable instructions, replace left and right operands with new values.
            Instruction::Add(lhs, rhs, _)
            | Instruction::Sub(lhs, rhs, _)
            | Instruction::Mul(lhs, rhs, _)
            | Instruction::Sdiv(lhs, rhs, _)
            | Instruction::Srem(lhs, rhs, _)
            | Instruction::Shl(lhs, rhs, _)
            | Instruction::Ashr(lhs, rhs, _)
            | Instruction::Fadd(lhs, rhs, _)
            | Instruction::Fsub(lhs, rhs, _)
            | Instruction::Fmul(lhs, rhs, _)
            | Instruction::Fdiv(lhs, rhs, _)
            | Instruction::Fcmp(_, lhs, rhs, _)
            | Instruction::Icmp(_, lhs, rhs, _)
            | Instruction::LogicalAnd(lhs, rhs, _)
            | Instruction::LogicalOr(lhs, rhs, _)
            | Instruction::BitwiseAnd(lhs, rhs, _)
            | Instruction::BitwiseOr(lhs, rhs, _)
            | Instruction::BitwiseXor(lhs, rhs, _) => {
                *lhs = lhs.mapped(var_map);
                *rhs = rhs.mapped(var_map);
            }

            // Single variable instructions, replace operand with new value.
            Instruction::BitwiseNot(operand, _) | Instruction::LogicalNot(operand, _) => {
                *operand = operand.mapped(var_map);
            }

            // Phi nodes are handled separately in the SSA transformation, but need to be passed through
            // like the unconditional terminators.
            Instruction::Phi(..) | Instruction::Jump(..) | Instruction::Return => {}

            Instruction::Alloca(..) => panic!("alloca not supported in ssa transformation"),
            Instruction::Load(..) => panic!("load not supported in ssa transformation"),
        }
        block.0.push(instr);
    }
}

impl Operand {
    fn mapped(&self, var_map: &FxHashMap<VariableId, Operand>) -> Operand {
        match self {
            Operand::Literal(_) => *self,
            Operand::Variable(var) => var.map_to_operand(var_map),
        }
    }
}

impl Variable {
    fn map_to_operand(self, var_map: &FxHashMap<VariableId, Operand>) -> Operand {
        let mut var = self;
        while let Some(operand) = var_map.get(&var.variable_id) {
            if let Operand::Variable(new_var) = operand {
                var = *new_var;
            } else {
                return *operand;
            }
        }
        Operand::Variable(var)
    }

    fn map_to_variable(self, var_map: &FxHashMap<VariableId, Operand>) -> Variable {
        let mut var = self;
        while let Some(operand) = var_map.get(&var.variable_id) {
            let Operand::Variable(new_var) = operand else {
                panic!("literal not supported in this context");
            };
            var = *new_var;
        }
        var
    }
}
