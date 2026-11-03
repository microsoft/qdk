// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The rule that within a batch a tag names one readout of each shot
//! ([`ShotOutcome::tag`]), kept for an executor written by hand
//! ([`check_unique_tags`]).

use crate::Rejected;
#[cfg(doc)]
use crate::ShotOutcome;

/// Checks that no two readouts of a batch share a tag, since within a batch
/// a tag names one readout of each shot ([`ShotOutcome::tag`]). An executor
/// that reports its readouts by tag calls it before it runs anything, so
/// that a batch that repeats a tag runs nothing.
///
/// `readouts` lists each readout the batch asks for: the index of its
/// instruction in the batch, and its tag. An instruction that makes several
/// readouts is listed once for each.
///
/// # Errors
///
/// [`Rejected::Instruction`] at the first instruction, in the batch's order,
/// whose readout repeats the tag of one before it, or of another of its own
/// readouts. Its reason names the earlier instruction and the planned
/// readout register.
pub fn check_unique_tags(readouts: impl IntoIterator<Item = (usize, u32)>) -> Result<(), Rejected> {
    let mut readouts = readouts.into_iter();
    // No allocation for a batch of at most one readout.
    let (Some(first), Some(second)) = (readouts.next(), readouts.next()) else {
        return Ok(());
    };
    let mut by_tag: Vec<(u32, usize)> = [first, second]
        .into_iter()
        .chain(readouts)
        .map(|(at, tag)| (tag, at))
        .collect();
    by_tag.sort_unstable();
    // Within a tag, the readouts are in the batch's order: a pair's second
    // repeats its first, and the earliest second is the first repeat.
    let repeat = by_tag
        .windows(2)
        .filter_map(|pair| match *pair {
            [(tag, earlier), (again, at)] if tag == again => Some((at, tag, earlier)),
            _ => None,
        })
        .min();
    match repeat {
        None => Ok(()),
        Some((at, tag, earlier)) => {
            let named = if earlier == at {
                "another readout of this instruction".to_string()
            } else {
                format!("the readout of instruction {earlier}")
            };
            Err(Rejected::Instruction {
                at,
                reason: format!(
                    "tag {tag} already names {named}: within a batch a tag names one \
                     readout of each shot (docs/replies.md, \"Readout tags\"). A program that repeats a \
                     tag within a batch is the first case for the planned readout \
                     register: time to implement it"
                ),
            })
        }
    }
}
