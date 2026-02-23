// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::iter::once;

use crate::{
    rir::{InstructionKind, Program},
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
                &i.kind,
                InstructionKind::Return | InstructionKind::Jump(..) | InstructionKind::Branch(..)
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
    let mut start_blocks = FxHashSet::default();
    for (_, callable) in program.callables.iter() {
        if let Some(block_id) = callable.body {
            start_blocks.insert(block_id);
        }
    }
    let mut live_blocks = FxHashSet::default();
    for block in start_blocks {
        live_blocks.insert(block);
        live_blocks.extend(utils::get_all_block_successors(block, program).into_iter());
    }
    let mut dead_blocks = Vec::new();
    for (block_id, _) in program.blocks.iter() {
        if !live_blocks.contains(&block_id) {
            dead_blocks.push(block_id);
        }
    }
    assert!(
        dead_blocks.is_empty(),
        "Unreachable blocks found: {dead_blocks:?}"
    );
}

/// Checks for unreachable callables in a program.
/// Specifically, this function checks that all callables are reachable from the entry point of the program.
/// Note that calls from unreachable blocks are not included as only successor blocks are considered.
pub fn check_unreachable_callable(program: &Program) {
    let mut live_callables = FxHashSet::default();
    let mut callables_to_check = Vec::new();
    callables_to_check.push(program.entry);
    while let Some(callable_id) = callables_to_check.pop() {
        if !live_callables.insert(callable_id) {
            continue;
        }
        let callable = program.get_callable(callable_id);
        let Some(body) = callable.body else {
            continue;
        };
        for block_id in utils::get_all_block_successors(body, program)
            .iter()
            .chain(once(&body))
        {
            let block = program.get_block(*block_id);
            for instr in &block.0 {
                if let InstructionKind::Call(callable_id, ..) = &instr.kind {
                    callables_to_check.push(*callable_id);
                }
            }
        }
    }
    let dead_callables = program
        .callables
        .iter()
        .filter_map(|(callable_id, _)| {
            if live_callables.contains(&callable_id) {
                None
            } else {
                Some(callable_id)
            }
        })
        .collect::<Vec<_>>();
    assert!(
        dead_callables.is_empty(),
        "Unreachable callables found: {dead_callables:?}"
    );
}
