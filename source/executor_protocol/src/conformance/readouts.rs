// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What more than one family of checks uses: the outcomes of a call, read
//! into a buffer that already holds some, the readouts of a run by shot,
//! and a check on each shot that names every shot that breaks it.

use std::collections::BTreeMap;
use std::iter;

#[cfg(doc)]
use crate::Reply;
use crate::{
    ErrorKind, Executor, InstructionBatch, InstructionBatchRef, Rejected, ShotConditions, ShotId,
    ShotOutcome, ShotSet, Value, wait,
};

/// Checks `holds` on each shot of `shots`, and panics once when it fails
/// on some, naming them all, so that a check run once reports every fault:
/// `message` makes the panic message from the shots on which it fails, in
/// the order of `shots`, named as `shot 2` or `shots 2, 4`.
///
/// # Panics
///
/// When `holds` fails on some shot.
pub(super) fn expect_on_each_shot<'s>(
    shots: impl IntoIterator<Item = &'s ShotId>,
    mut holds: impl FnMut(ShotId) -> bool,
    message: impl FnOnce(&str) -> String,
) {
    let failing: Vec<String> = (shots.into_iter())
        .filter(|&&shot| !holds(shot))
        .map(ToString::to_string)
        .collect();
    match failing.as_slice() {
        [] => {}
        [shot] => panic!("{}", message(&format!("shot {shot}"))),
        shots => panic!("{}", message(&format!("shots {}", shots.join(", ")))),
    }
}

/// Each shot's readouts in `readouts`, by tag alone, in the order of the
/// tags, without their conditions: those of a run over several calls
/// compare with those of one batch, whose earlier readouts carry the
/// conditions raised by its later instructions, where a run over several
/// calls marks only the readouts of later calls with them.
pub(super) fn tagged(readouts: Readouts) -> BTreeMap<ShotId, Vec<(u32, Value)>> {
    (readouts.into_iter())
        .map(|(shot, readouts)| {
            let mut tagged: Vec<_> = (readouts.into_iter())
                .map(|(_, tag, value, _)| (tag, value))
                .collect();
            tagged.sort_by_key(|&(tag, _)| tag);
            (shot, tagged)
        })
        .collect()
}

/// Each shot's readouts in a run of several calls: the index of the batch
/// that made each (its call, when each batch is one call), its tag, its
/// value and the conditions it carries, in the order of the batches and the
/// tags. The checks that compare a shot's readouts over the same calls
/// compare their conditions too, since a shot's conditions are its own, as
/// its readouts are.
pub(super) type Readouts = BTreeMap<ShotId, Vec<(usize, u32, Value, ShotConditions)>>;

/// Calls `executor` on `shots` with each of `batches`, in order, waiting
/// for each reply, and gives each shot's readouts, under the index of the
/// batch that made them.
///
/// # Panics
///
/// As [`readouts_of_calls`].
pub(super) fn readouts<I, E: Executor<InstructionBatch<I>>>(
    executor: &mut E,
    shots: &ShotSet,
    batches: &[&[I]],
) -> Readouts {
    let calls: Vec<_> = (batches.iter().enumerate())
        .map(|(call, &batch)| (call, shots.clone(), batch))
        .collect();
    readouts_of_calls(executor, &calls)
}

/// Calls `executor` with each of `calls`, a batch on shots under the index
/// of its batch, in order, waiting for each reply ([`outcomes_of_call`]),
/// and gives each shot's readouts, under the index of the batch that made
/// them.
///
/// # Panics
///
/// When a call is refused, or its reply fails: the checks that run batches
/// need them run; when a reply changes what the buffer held.
pub(super) fn readouts_of_calls<I, E: Executor<InstructionBatch<I>>>(
    executor: &mut E,
    calls: &[(usize, ShotSet, &[I])],
) -> Readouts {
    let mut readouts = Readouts::new();
    for (call, shots, instructions) in calls {
        let outcomes = outcomes_of_call(executor, shots, instructions).unwrap_or_else(|rejected| {
            panic!("the batches must be accepted, but one was refused with `{rejected:?}`")
        });
        for outcome in outcomes {
            (readouts.entry(outcome.shot).or_default()).push((
                *call,
                outcome.tag,
                outcome.value,
                outcome.conditions,
            ));
        }
    }
    for readouts in readouts.values_mut() {
        readouts.sort_by_key(|&(call, tag, ..)| (call, tag));
    }
    readouts
}

/// Calls `executor` on `shots` with `instructions`, and gives the outcomes
/// of its reply, or its refusal. The reply is waited for into a buffer that
/// already holds [`HELD`] and one outcome of each shot of the call
/// ([`held_of`]): a reply appends its events, keeping what the caller held
/// ([`Reply::poll_events`]), so that a caller may gather the events of
/// several replies in one buffer. The held outcomes of the call's shots
/// show a reply that marks the caller's outcomes of a shot it flags,
/// whichever shot of the call that is.
///
/// # Panics
///
/// When the reply fails: the checks that run batches need them run; when
/// the reply changes what the buffer held.
pub(super) fn outcomes_of_call<I, E: Executor<InstructionBatch<I>>>(
    executor: &mut E,
    shots: &ShotSet,
    instructions: &[I],
) -> Result<Vec<ShotOutcome>, Rejected> {
    let held: Vec<ShotOutcome> = iter::once(HELD)
        .chain(shots.iter().map(|&shot| held_of(shot)))
        .collect();
    let reply = executor.call(InstructionBatchRef {
        shots,
        instructions,
    })?;
    let mut outcomes = held.clone();
    if let Err(reason) = wait(reply, &mut outcomes) {
        panic!("the batches must run, but the reply to one failed: `{reason}`");
    }
    assert!(
        outcomes.starts_with(&held),
        "a reply must append its events to the caller's buffer, keeping what it held \
         (`Reply::poll_events`), but the reply to one of the batches changed it"
    );
    Ok(outcomes.split_off(held.len()))
}

/// The outcome that the readout checks' buffer holds before each reply's
/// events: one that no executor gives for a caller's shots, of the last
/// shot and the last tag, with no value.
const HELD: ShotOutcome = ShotOutcome::new(ShotId::MAX, u32::MAX, Value::Error(ErrorKind::Other));

/// An outcome of `shot` that the readout checks' buffer holds before each
/// reply's events, as a caller holds those of the shot's earlier calls:
/// of the last tag, with no value and no condition.
const fn held_of(shot: ShotId) -> ShotOutcome {
    ShotOutcome::new(shot, u32::MAX, Value::Error(ErrorKind::Other))
}
