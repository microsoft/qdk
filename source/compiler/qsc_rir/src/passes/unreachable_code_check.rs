// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    rir::{Instruction, Program},
    utils,
};
use rustc_hash::FxHashSet;

#[cfg(test)]
mod tests;

/// Checks for unreachable code in a program.
/// This check first verifies that all callables are reachable from the entry point of the program.
/// Then, it checks that all blocks are reachable from at least one callable in the program.
/// Finally, it checks that no instruction follows a terminator instruction in a block,
/// and that each block ends with a terminator instruction.
pub fn check_unreachable_code(program: &Program) {
    check_unreachable_callable(program);
    check_unreachable_blocks(program);
    check_unreachable_instrs(program);
}

/// Checks for unreachable instructions in each block of a program.
/// Specifically, this function checks that no instruction follows a terminator instruction in a block,
/// and that each block ends with a terminator instruction.
pub fn check_unreachable_instrs(program: &Program) {
    for (block_id, block) in program.blocks.iter() {
        match block.0.iter().position(|i| {
            matches!(
                i,
                Instruction::Return(..) | Instruction::Jump(..) | Instruction::Branch(..)
            )
        }) {
            Some(idx) => {
                assert!(
                    idx == block.0.len() - 1,
                    "{block_id:?} has unreachable instructions after a terminator",
                );
            }
            None => panic!("{block_id:?} does not end with a terminator instruction"),
        }
    }
}

/// Checks for unreachable blocks in a program.
/// Specifically, this function checks that all blocks are reachable from at least one callable in the program.
pub fn check_unreachable_blocks(program: &Program) {
    let mut live_blocks = FxHashSet::default();
    for (_, callable) in program.callables.iter() {
        if let Some(block_id) = callable.body {
            live_blocks.insert(block_id);
        }
    }
    for (_, block) in &program.blocks {
        live_blocks.extend(utils::get_block_successors(block));
    }
    for (block_id, _) in program.blocks.iter() {
        assert!(
            live_blocks.contains(&block_id),
            "Unreachable block found: {block_id:?}"
        );
    }
}

/// Checks for unreachable callables in a program.
/// Specifically, this function checks that all callables are reachable from the entry point of the program.
/// Note that calls from unreachable blocks are not included as only successor blocks are considered.
pub fn check_unreachable_callable(program: &Program) {
    let mut live_callables = FxHashSet::default();
    live_callables.insert(program.entry);
    for (_, block) in &program.blocks {
        for instr in &block.0 {
            if let Instruction::Call(callable_id, ..) = instr {
                live_callables.insert(*callable_id);
            }
        }
    }
    for (callable_id, _) in program.callables.iter() {
        assert!(
            live_callables.contains(&callable_id),
            "Unreachable callable found: {callable_id:?}"
        );
    }
}
