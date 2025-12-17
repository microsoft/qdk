// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::rir::{
    AdvancedInstr, Block, BlockId, Instruction, Operand, Program, Variable, VariableId,
};
use qsc_data_structures::index_map::IndexMap;
use rustc_hash::{FxHashMap, FxHashSet};

/// Given a block, return the block IDs of its successors.
#[must_use]
pub fn get_block_successors(block: &Block) -> Vec<BlockId> {
    let mut successors = Vec::new();
    // Assume that the block is well-formed and that terminators only appear as the last instruction.
    match block
        .0
        .last()
        .expect("block should have at least one instruction")
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
            match instr {
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
                | Instruction::Phi(_, var)
                | Instruction::Convert(_, var) => {
                    assert!(
                        !assignments.contains_key(var.variable_id),
                        "Duplicate assignment to {:?} in {block_id:?}, instruction {idx}",
                        var.variable_id
                    );
                    has_phi |= matches!(instr, Instruction::Phi(_, _));
                    assignments.insert(var.variable_id, (block_id, idx));
                }
                Instruction::Store(_, var)
                | Instruction::Advanced(
                    AdvancedInstr::Alloca(var)
                    | AdvancedInstr::Load(_, var)
                    | AdvancedInstr::Index(_, _, var),
                ) => {
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

// Propagates stored variables through a block, tracking the latest stored value and replacing
// usage of the variable with the stored value.
pub(crate) fn map_variable_use_in_block(
    block: &mut Block,
    var_map: &mut FxHashMap<VariableId, Operand>,
    var_stor_to_keep: &FxHashSet<VariableId>,
) {
    let instrs = block.0.drain(..).collect::<Vec<_>>();

    for mut instr in instrs {
        match &mut instr {
            // Track the new value of the variable and omit the store instruction.
            Instruction::Store(operand, var) => {
                if var_stor_to_keep.contains(&var.variable_id) {
                    // Only keep stores to variables that are in the set to keep.
                    *operand = operand.mapped(var_map);
                } else {
                    // Note this uses the mapped operand to make sure this variable points to whatever root literal or variable
                    // this operand corresponds to at this point in the block. This makes the new variable respect a point-in-time
                    // copy of the operand.
                    var_map.insert(var.variable_id, operand.mapped(var_map));
                    continue;
                }
            }

            // Replace any arguments with the new values of stored variables.
            Instruction::Call(_, args, _) => {
                *args = args
                    .iter_mut()
                    .map(|arg| match arg {
                        Operand::Variable(var) => {
                            // If the variable is not in the map, it is not something whose value has been updated via store in this block,
                            // so just fallback to use the `arg` value directly.
                            // `map_to_operand` does this automatically by returning `self`` when the variable is not in the map.
                            var.map_to_operand(var_map)
                        }
                        Operand::Literal(_) => arg.clone(),
                    })
                    .collect();
            }

            // Replace the branch condition with the new value of the variable.
            Instruction::Branch(var, _, _) => {
                *var = var.map_to_variable(var_map);
            }

            Instruction::Advanced(AdvancedInstr::Index(var, operand, _))
            | Instruction::Convert(operand, var) => {
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

            Instruction::Advanced(AdvancedInstr::Alloca(..)) => {
                panic!("alloca not supported in ssa transformation")
            }
            Instruction::Advanced(AdvancedInstr::Load(..)) => {
                panic!("load not supported in ssa transformation")
            }
        }
        block.0.push(instr);
    }
}

impl Operand {
    #[must_use]
    pub fn mapped(&mut self, var_map: &FxHashMap<VariableId, Operand>) -> Operand {
        match self {
            Operand::Literal(_) => self.clone(),
            Operand::Variable(var) => var.map_to_operand(var_map),
        }
    }
}

impl Variable {
    #[must_use]
    pub fn map_to_operand(&mut self, var_map: &FxHashMap<VariableId, Operand>) -> Operand {
        let var = self;
        while let Some(operand) = var_map.get(&var.variable_id) {
            if let Operand::Variable(new_var) = operand {
                *var = new_var.clone();
            } else {
                return operand.clone();
            }
        }
        Operand::Variable(var.clone())
    }

    #[must_use]
    pub fn map_to_variable(&mut self, var_map: &FxHashMap<VariableId, Operand>) -> Variable {
        let var = self;
        while let Some(Operand::Variable(new_var)) = var_map.get(&var.variable_id) {
            *var = new_var.clone();
        }
        var.clone()
    }
}
