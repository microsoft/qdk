// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::rir::{
    Block, BlockId, Instruction, Operand, OperandMapping, Program, Variable, VariableId,
};
use qsc_data_structures::index_map::IndexMap;
use rustc_hash::{FxHashMap, FxHashSet};

/// Given a block, return the block IDs of its successors.
#[must_use]
pub fn get_block_successors(block: &Block) -> Vec<BlockId> {
    let mut successors = Vec::new();
    // Assume that the block is well-formed and that terminators only appear as the last instruction.
    match &block
        .0
        .last()
        .expect("block should have at least one instruction")
    {
        Instruction::Branch(_, target1, target2, _) => {
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
            match instr {
                Instruction::Call(_, _, Some(var), _)
                | Instruction::Convert(_, var)
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
                | Instruction::Frem(_, _, var)
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
                    has_phi |= matches!(instr, Instruction::Phi(_, _));
                    assignments.insert(var.variable_id, (block_id, idx));
                }
                Instruction::Store(_, var)
                | Instruction::StoreArray(_, var)
                | Instruction::Alloca(var)
                | Instruction::Load(_, var)
                | Instruction::Index(_, _, var) => {
                    has_store = true;
                    assignments.insert(var.variable_id, (block_id, idx));
                }

                Instruction::Call(_, _, None, _)
                | Instruction::Jump(..)
                | Instruction::Branch(..)
                | Instruction::Return(..) => {}
            }
        }
    }
    assert!(
        !(has_store && has_phi),
        "Program has both store and phi instructions."
    );
    assignments
}

// Propagates stored variables through a block, tracking the latest stored value and replacing
// usage of the variable with the stored value.
pub(crate) fn map_variable_use_in_block(
    block: &mut Block,
    var_map: &mut FxHashMap<VariableId, OperandMapping>,
    var_stor_to_keep: &FxHashSet<VariableId>,
) {
    let instrs = block.0.drain(..).collect::<Vec<_>>();

    // Built lazily on the first store that updates the map, so blocks without such stores don't pay for it.
    let mut deep_mapping_index: Option<DeepMappingIndex> = None;

    for mut instr in instrs {
        match &mut instr {
            // Track the new value of the variable and omit the store instruction.
            Instruction::Store(operand, var) => {
                if var_stor_to_keep.contains(&var.variable_id) {
                    // Only keep stores to variables that are in the set to keep.
                    *operand = operand.mapped(var_map);
                } else if let Operand::Variable(mapped_var) = operand.mapped(var_map)
                    && var_stor_to_keep.contains(&mapped_var.variable_id)
                {
                    *operand = Operand::Variable(mapped_var);
                } else {
                    let deep_mapping_index =
                        deep_mapping_index.get_or_insert_with(|| build_deep_mapping_index(var_map));
                    update_variable_mapping(var_map, deep_mapping_index, operand, var);
                    continue;
                }
            }
            Instruction::StoreArray(operand, var) => {
                if var_stor_to_keep.contains(&var.variable_id) {
                    // Only keep stores to variables that are in the set to keep.
                    *operand = operand.iter().map(|op| op.mapped(var_map)).collect();
                } else {
                    // Otherwise drop the store array by continuing the loop.
                    continue;
                }
            }

            // Replace any arguments with the new values of stored variables.
            Instruction::Call(_, args, _, _) => {
                *args = args.iter().map(|arg| arg.mapped(var_map)).collect();
            }

            // Replace the branch condition with the new value of the variable.
            Instruction::Branch(var, _, _, _) => {
                *var = var.map_to_variable(var_map);
            }

            Instruction::Convert(operand, var) => {
                *operand = operand.mapped(var_map);
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
            | Instruction::Frem(lhs, rhs, _)
            | Instruction::Fcmp(_, lhs, rhs, _)
            | Instruction::Icmp(_, lhs, rhs, _)
            | Instruction::LogicalAnd(lhs, rhs, _)
            | Instruction::LogicalOr(lhs, rhs, _)
            | Instruction::BitwiseAnd(lhs, rhs, _)
            | Instruction::BitwiseOr(lhs, rhs, _)
            | Instruction::BitwiseXor(lhs, rhs, _)
            | Instruction::Index(lhs, rhs, _) => {
                *lhs = lhs.mapped(var_map);
                *rhs = rhs.mapped(var_map);
            }

            // Single variable instructions, replace operand with new value.
            Instruction::BitwiseNot(operand, _)
            | Instruction::LogicalNot(operand, _)
            | Instruction::Return(Some(operand)) => {
                *operand = operand.mapped(var_map);
            }

            // Phi nodes are handled separately in the SSA transformation, but need to be passed through
            // like the unconditional terminators.
            Instruction::Phi(..) | Instruction::Jump(..) | Instruction::Return(None) => {}

            Instruction::Alloca(..) | Instruction::Load(..) => {
                panic!("alloca/load not supported in ssa transformation")
            }
        }
        block.0.push(instr);
    }
}

/// Reverse index from a variable to the variables whose mapping is a deep mapping to it. It lets a store
/// find the deep mappings to downgrade without scanning the whole variable map, which would make
/// processing a block quadratic in the number of stores. Entries can go stale when a variable is
/// remapped, so they are re-checked against the variable map before use.
type DeepMappingIndex = FxHashMap<VariableId, Vec<VariableId>>;

fn build_deep_mapping_index(var_map: &FxHashMap<VariableId, OperandMapping>) -> DeepMappingIndex {
    let mut index = DeepMappingIndex::default();
    for (var_id, mapping) in var_map {
        if let OperandMapping::Deep(Operand::Variable(target)) = mapping {
            index.entry(target.variable_id).or_default().push(*var_id);
        }
    }
    index
}

fn update_variable_mapping(
    var_map: &mut FxHashMap<VariableId, OperandMapping>,
    deep_mapping_index: &mut DeepMappingIndex,
    operand: &Operand,
    var: &Variable,
) {
    // Note this uses the mapped operand to make sure this variable points to whatever root literal or variable
    // this operand corresponds to at this point in the block. This makes the new variable respect a point-in-time
    // copy of the operand. However, it will create a mapping that matches the last mapping of the operand, ensuring
    // that a shallow mapping is not incorrectly treated as a deep mapping.
    let new_mapping = operand.last_mapping(var_map);
    var_map.insert(var.variable_id, new_mapping);
    if let OperandMapping::Deep(Operand::Variable(target)) = new_mapping {
        deep_mapping_index
            .entry(target.variable_id)
            .or_default()
            .push(var.variable_id);
    }

    // For all existing deep mappings to this variable, downgrade them to shallow mappings.
    // This ensures those previous mappings represent the value at the time they were created, rather than the new value being stored.
    // Only the variables recorded in the index for this variable are visited, and each is re-checked since its entry may be stale.
    // Downgraded and stale entries are dropped; an entry is kept only if it is still a deep mapping to this variable id with a
    // different type, mirroring the full-variable comparison used to decide what to downgrade.
    if let Some(mut dependents) = deep_mapping_index.remove(&var.variable_id) {
        dependents.retain(|dependent| {
            let Some(mapping) = var_map.get_mut(dependent) else {
                return false;
            };
            match *mapping {
                OperandMapping::Deep(Operand::Variable(existing)) if existing == *var => {
                    *mapping = OperandMapping::Shallow(Operand::Variable(existing));
                    false
                }
                OperandMapping::Deep(Operand::Variable(existing)) => {
                    existing.variable_id == var.variable_id
                }
                _ => false,
            }
        });
        if !dependents.is_empty() {
            deep_mapping_index.insert(var.variable_id, dependents);
        }
    }
}

impl Operand {
    #[must_use]
    pub(crate) fn mapped(&self, var_map: &FxHashMap<VariableId, OperandMapping>) -> Operand {
        match self {
            Operand::Literal(_) => *self,
            Operand::Variable(var) => var.map_to_operand(var_map),
        }
    }

    #[must_use]
    pub(crate) fn last_mapping(
        &self,
        var_map: &FxHashMap<VariableId, OperandMapping>,
    ) -> OperandMapping {
        match self {
            Operand::Literal(_) => OperandMapping::Deep(*self),
            Operand::Variable(var) => {
                let mut var = *var;
                while let Some(mapping) = var_map.get(&var.variable_id) {
                    if let Operand::Variable(new_var) = mapping.into() {
                        if new_var.variable_id == var.variable_id {
                            break;
                        }
                        var = new_var;
                        if mapping.is_shallow() {
                            return *mapping;
                        }
                    } else {
                        return *mapping;
                    }
                }
                OperandMapping::Deep(Operand::Variable(var))
            }
        }
    }
}

impl Variable {
    #[must_use]
    pub(crate) fn map_to_operand(self, var_map: &FxHashMap<VariableId, OperandMapping>) -> Operand {
        let mut var = self;
        while let Some(mapping) = var_map.get(&var.variable_id) {
            if let Operand::Variable(new_var) = mapping.into() {
                if new_var.variable_id == var.variable_id {
                    // The variable maps to itself, as happens when a live-in parameter is seeded as
                    // its own definition. It is already at its root, so stop following the chain.
                    break;
                }
                var = new_var;
                if mapping.is_shallow() {
                    // Stop following the chain for shallow mappings and use the current mapping as is.
                    break;
                }
            } else {
                return mapping.into();
            }
        }
        Operand::Variable(var)
    }

    #[must_use]
    pub(crate) fn map_to_variable(
        self,
        var_map: &FxHashMap<VariableId, OperandMapping>,
    ) -> Variable {
        let mut var = self;
        while let Some(mapping) = var_map.get(&var.variable_id) {
            let Operand::Variable(new_var) = mapping.into() else {
                panic!("literal not supported in this context");
            };
            if new_var.variable_id == var.variable_id {
                // The variable maps to itself, as happens when a live-in parameter is seeded as its
                // own definition. It is already at its root, so stop following the chain.
                break;
            }
            var = new_var;
            if mapping.is_shallow() {
                // Stop following the chain for shallow mappings and use the current mapping as is.
                break;
            }
        }
        var
    }
}
