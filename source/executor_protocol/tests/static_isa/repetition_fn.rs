// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The repetition code as a logic, [`lower_then_decode`], which
//! `adapter_fn` makes an adapter: what [`Repetition3`] and
//! [`RepetitionCode`] do, as one function. The logic lowers a call's
//! logical instructions to their gadgets, runs them below and waits for
//! their readouts (`Below::run`), then writes each logical readout as the
//! majority of its three.
//!
//! It differs from them where `adapter_fn` cannot yet do what they do:
//!
//! - it is not built against what is below, so it is given the blocks it
//!   checks ([`within`]), where `RepetitionCode` learns them from the
//!   qubits below and offers them upstream;
//! - a physical instruction refused below is named by its index in the
//!   physical call, where `Repetition3` names the logical instruction
//!   whose gadget holds it;
//! - it writes a call's logical readouts once all of its physical ones
//!   came, where `Repetition3`'s reply writes each as soon as its third
//!   readout comes.
//!
//! [`Repetition3`]: crate::repetition3::Repetition3
//! [`RepetitionCode`]: crate::repetition_code::RepetitionCode

use std::collections::BTreeMap;

use executor_protocol::{
    AdapterError, Below, CallInput, Outcomes, Rejected, ShotId, ShotOutcome, Value,
};

use crate::logical::Logical;
use crate::repetition3::gadget;
use crate::toy::Op;

/// The repetition code's logic: a call's logical instructions go below as
/// their gadgets, in one run, and each logical readout comes up as the
/// majority of its block's three readouts. It keeps no state.
///
/// # Errors
///
/// When the run cannot go on ([`Below::run`]), or a logical readout lacks
/// some, not all, of its three readouts below.
pub fn lower_then_decode(
    (): &mut (),
    input: &CallInput<'_, Logical>,
    below: &mut Below<'_, Op>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    let lowered: Vec<Op> = (input.instructions().iter())
        .flat_map(|&instruction| gadget(instruction))
        .collect();
    let read = below.run(&lowered)?;
    outcomes.extend(majorities(&read)?);
    Ok(())
}

/// The logical readouts of `read`, the readouts of a run: logical tag `t`
/// of a shot is the majority of its physical tags `3t`, `3t + 1` and
/// `3t + 2` (a readout with no value counts as no vote for 1), in the order
/// their third votes were read, the order `Repetition3` writes them in. The
/// votes are kept by shot and logical tag, so any order of reading serves:
/// a lowering that reads the blocks' first qubits together, then their
/// second ones, as well as one that reads each block's three together.
///
/// # Errors
///
/// When a logical tag lacks some, not all, of its three readouts: only the
/// code knows what its gadgets read. A logical tag with none of its
/// readouts is not found: it is left out, as `Repetition3` leaves out a
/// vote it lacks.
fn majorities(read: &[ShotOutcome]) -> Result<Vec<ShotOutcome>, String> {
    let mut votes: BTreeMap<(ShotId, u32), Vec<Value>> = BTreeMap::new();
    let mut decoded = Vec::new();
    for &ShotOutcome {
        shot, tag, value, ..
    } in read
    {
        let logical = (shot, tag / 3);
        let of_tag = votes.entry(logical).or_default();
        of_tag.push(value);
        if let [a, b, c] = of_tag[..] {
            let ones = [a, b, c].iter().filter(|&&v| v == Value::One).count();
            decoded.push(ShotOutcome::new(shot, tag / 3, (ones >= 2).into()));
            votes.remove(&logical);
        }
    }
    match votes.into_keys().next() {
        Some((shot, tag)) => Err(format!("shot {shot} lacks a readout of logical tag {tag}")),
        None => Ok(decoded),
    }
}

/// The check of a code of `blocks` blocks: it refuses a call at its first
/// instruction on a block beyond them, before anything goes below, as
/// `Repetition3` does.
pub fn within(blocks: u32) -> impl FnMut(&CallInput<'_, Logical>) -> Result<(), Rejected> {
    move |input| match (input.instructions().iter()).position(|i| i.block() >= blocks) {
        Some(at) => Err(Rejected::Instruction {
            at,
            reason: format!("{blocks} logical qubits"),
        }),
        None => Ok(()),
    }
}
