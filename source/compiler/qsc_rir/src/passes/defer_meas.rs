// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::rir::{CallableType, Instruction, Program};
use rustc_hash::FxHashSet;

/// Defers measurements in each block of a program to the end of that block.
/// Specifically, this function reorders instructions within a block to follow the given order:
///
/// 1. All instructions except for measurements and output recordings.
/// 2. All measurements.
/// 3. All output recordings.
/// 4. Return, branch, and jump instructions (terminators).
///
/// Relative ordering within each section is maintained. Note that the scope is limited to each
/// block, so measurements are not necessarily deferred to the end of the entire program unless the
/// program consists of a single block.
pub fn defer_measurements(program: &mut Program) {
    let mut measure_call_ids = FxHashSet::default();
    let mut output_recording_ids = FxHashSet::default();
    for (id, callable) in program.callables.iter() {
        match callable.call_type {
            CallableType::Measurement | CallableType::Readout => {
                measure_call_ids.insert(id);
            }
            CallableType::OutputRecording => {
                output_recording_ids.insert(id);
            }
            CallableType::Regular | CallableType::Initialize => {}
            CallableType::Reset => panic!(
                "Reset callables should not be present in the RIR when deferring measurements"
            ),
        }
    }

    for (_, block) in program.blocks.iter_mut() {
        block
            .0
            .sort_by(|a, b| match (&a.instruction, &b.instruction) {
                // Return, branch, and jump instructions are terminators and should come last.
                (Instruction::Return | Instruction::Branch(..) | Instruction::Jump(..), _) => {
                    std::cmp::Ordering::Greater
                }
                (_, Instruction::Return | Instruction::Branch(..) | Instruction::Jump(..)) => {
                    std::cmp::Ordering::Less
                }

                // Measurements and output recordings should maintain their order relative to the same type.
                (Instruction::Call(a_id, _, _), Instruction::Call(b_id, _, _))
                    if measure_call_ids.contains(a_id) && measure_call_ids.contains(b_id) =>
                {
                    std::cmp::Ordering::Equal
                }
                (Instruction::Call(a_id, _, _), Instruction::Call(b_id, _, _))
                    if output_recording_ids.contains(a_id)
                        && output_recording_ids.contains(b_id) =>
                {
                    std::cmp::Ordering::Equal
                }

                // Output recording should come after any other instruction except for terminator instructions,
                // which are handled above.
                (Instruction::Call(a_id, _, _), _) if output_recording_ids.contains(a_id) => {
                    std::cmp::Ordering::Greater
                }
                (_, Instruction::Call(b_id, _, _)) if output_recording_ids.contains(b_id) => {
                    std::cmp::Ordering::Less
                }

                // Measurements should come after any other instruction except for terminator instructions,
                // and output recording instructions, which are handled above.
                (Instruction::Call(a_id, _, _), Instruction::Call(..))
                    if measure_call_ids.contains(a_id) =>
                {
                    std::cmp::Ordering::Greater
                }
                (Instruction::Call(..), Instruction::Call(b_id, _, _))
                    if measure_call_ids.contains(b_id) =>
                {
                    std::cmp::Ordering::Less
                }

                // All other instructions should maintain their relative order.
                _ => std::cmp::Ordering::Equal,
            });
    }
}
