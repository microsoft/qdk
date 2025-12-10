// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use core::panic;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    rir::{CallableId, Instruction, Program, VariableId},
    utils::{get_block_successors, map_variable_use_in_block},
};

pub fn prune_unneeded_stores(program: &mut Program) {
    for callable_id in program.all_callable_ids() {
        process_callable(program, callable_id);
    }
}

fn process_callable(program: &mut Program, callable_id: CallableId) {
    let callable = program.get_callable(callable_id);

    let Some(entry_block_id) = callable.body else {
        return;
    };

    // Walk all the blocks to track which variables are stored and which are used.
    let mut stored_vars = FxHashSet::default();
    let mut used_vars = FxHashSet::default();
    let mut cross_block_used_vars = FxHashSet::default();
    let mut visited_blocks = FxHashSet::default();
    let mut blocks_to_visit = vec![entry_block_id];
    while let Some(block_id) = blocks_to_visit.pop() {
        visited_blocks.insert(block_id);
        let mut used_vars_in_block = FxHashSet::default();
        let stored_vars_before_block = stored_vars.clone();
        check_var_usage(program, block_id, &mut stored_vars, &mut used_vars_in_block);

        for var in used_vars_in_block {
            if !used_vars.insert(var) || stored_vars_before_block.contains(&var) {
                // This variable was already marked as used, which means it is used cross-block.
                // Alternatively, the variable was stored before this block and is used here.
                // Either means we shouldn't try to transform stores to this variable away.
                cross_block_used_vars.insert(var);
            }
        }

        for successor_id in get_block_successors(program.get_block(block_id)) {
            if !visited_blocks.contains(&successor_id) {
                blocks_to_visit.push(successor_id);
            }
        }
    }

    // Perform a intra-block-only version of the SSA transform to eliminate stores to variables that
    // are only used within a single block.
    visited_blocks.clear();
    blocks_to_visit.push(entry_block_id);
    while let Some(block_id) = blocks_to_visit.pop() {
        visited_blocks.insert(block_id);
        let block = program.get_block_mut(block_id);
        let mut last_store_map = FxHashMap::default();
        map_variable_use_in_block(block, &mut last_store_map, &cross_block_used_vars);

        for successor_id in get_block_successors(program.get_block(block_id)) {
            if !visited_blocks.contains(&successor_id) {
                blocks_to_visit.push(successor_id);
            }
        }
    }

    // Now that we know which variables are used, we can remove the stores to unused variables.
    // Filtered stored_vars to only those that are used, then revisit the blocks to remove stores to unused variables.
    stored_vars.retain(|var| used_vars.contains(var));
    visited_blocks.clear();
    blocks_to_visit.push(entry_block_id);
    while let Some(block_id) = blocks_to_visit.pop() {
        visited_blocks.insert(block_id);
        let block = program.get_block_mut(block_id);
        block.0.retain(|instr| match instr {
            Instruction::Store(_, variable) => stored_vars.contains(&variable.variable_id),
            _ => true,
        });
        for successor_id in get_block_successors(program.get_block(block_id)) {
            if !visited_blocks.contains(&successor_id) {
                blocks_to_visit.push(successor_id);
            }
        }
    }
}

fn check_var_usage(
    program: &mut Program,
    block_id: crate::rir::BlockId,
    stored_vars: &mut FxHashSet<VariableId>,
    used_vars: &mut FxHashSet<VariableId>,
) {
    let block = program.get_block(block_id);
    for instr in &block.0 {
        match instr {
            Instruction::Store(operand, variable) => {
                if let crate::rir::Operand::Variable(var) = operand {
                    used_vars.insert(var.variable_id);
                }
                stored_vars.insert(variable.variable_id);
            }

            Instruction::Call(_, operands, variable) => {
                if let Some(var) = variable
                    && stored_vars.contains(&var.variable_id)
                {
                    panic!("calls should not use stored variables for capturing return values");
                }
                for operand in operands {
                    if let crate::rir::Operand::Variable(var) = operand {
                        used_vars.insert(var.variable_id);
                    }
                }
            }
            Instruction::Branch(variable, _, _) => {
                used_vars.insert(variable.variable_id);
            }
            Instruction::Fcmp(_, operand0, operand1, variable)
            | Instruction::Icmp(_, operand0, operand1, variable)
            | Instruction::Add(operand0, operand1, variable)
            | Instruction::Sub(operand0, operand1, variable)
            | Instruction::Mul(operand0, operand1, variable)
            | Instruction::Sdiv(operand0, operand1, variable)
            | Instruction::Srem(operand0, operand1, variable)
            | Instruction::Shl(operand0, operand1, variable)
            | Instruction::Ashr(operand0, operand1, variable)
            | Instruction::Fadd(operand0, operand1, variable)
            | Instruction::Fsub(operand0, operand1, variable)
            | Instruction::Fmul(operand0, operand1, variable)
            | Instruction::Fdiv(operand0, operand1, variable)
            | Instruction::LogicalAnd(operand0, operand1, variable)
            | Instruction::LogicalOr(operand0, operand1, variable)
            | Instruction::BitwiseAnd(operand0, operand1, variable)
            | Instruction::BitwiseOr(operand0, operand1, variable)
            | Instruction::BitwiseXor(operand0, operand1, variable) => {
                for op in [operand0, operand1] {
                    if let crate::rir::Operand::Variable(var) = op {
                        used_vars.insert(var.variable_id);
                    }
                }
                assert!(
                    !stored_vars.contains(&variable.variable_id),
                    "arithmetic instructions should not use stored variables for capturing return values"
                );
                used_vars.insert(variable.variable_id);
            }
            Instruction::LogicalNot(operand, variable)
            | Instruction::BitwiseNot(operand, variable) => {
                if let crate::rir::Operand::Variable(var) = operand {
                    used_vars.insert(var.variable_id);
                }
                assert!(
                    !stored_vars.contains(&variable.variable_id),
                    "not instructions should not use stored variables for capturing return values"
                );
                used_vars.insert(variable.variable_id);
            }

            Instruction::Load(..) => panic!("loads should not be present during store pruning"),
            Instruction::Alloca(..) => panic!("allocas should not be present during store pruning"),
            Instruction::Phi(..) => panic!("phis should not be present during store pruning"),

            Instruction::Return | Instruction::Jump(..) => {}
        }
    }
}
