// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The faults the checks of a caller (an adapter or a driver) share: both
//! read them in the batches the caller sent the kit's executor, logged as
//! [`BatchBelow`]s. Here are a shot in two batches of a call, readout
//! slots misstated, and whether the caller sent anything.

use std::collections::BTreeSet;

use executor_protocol::{DriveError, ExecuteRefusal, Failure, RunError};

use crate::findings::Findings;
use crate::nonconformance::{Fault, shot_list};
use crate::rule::Rule;
use crate::shared_run::{BatchBelow, RunLog};

/// Notes that the check cannot judge `sender` (the adapter, the driver) if
/// it sent nothing below.
pub(crate) fn sends_something(findings: &mut Findings, log: &RunLog, sender: &str) {
    if log.below.is_empty() {
        findings.cannot_judge(format!(
            "{sender} sent nothing below, so there is nothing to check"
        ));
    }
}

/// A fault at each batch below that holds a shot an earlier batch of its
/// call holds.
pub(crate) fn repeated_shots(below: &[BatchBelow]) -> Vec<Fault> {
    let mut faults = Vec::new();
    for (index, batch) in below.iter().enumerate() {
        let earlier = below[..index]
            .iter()
            .filter(|earlier| (earlier.execute, earlier.call) == (batch.execute, batch.call));
        let held: BTreeSet<_> = earlier.flat_map(|earlier| earlier.shots.iter()).collect();
        let repeated: Vec<String> = batch
            .shots
            .iter()
            .filter(|shot| held.contains(shot))
            .map(ToString::to_string)
            .collect();
        if !repeated.is_empty() {
            faults.push(Fault {
                place: batch.place(),
                found: format!(
                    "holds {} that an earlier batch of its call holds",
                    shot_list(&repeated)
                ),
            });
        }
    }
    faults
}

/// Notes a fault of [`Rule::StatesReadoutSlots`] at each batch below whose
/// stated readout slots are not those its instructions count; and, for a
/// batch holding an instruction the kit's executor does not accept, whose
/// slots it cannot count, that the check cannot judge the sender.
pub(crate) fn note_misstated(findings: &mut Findings, below: &[BatchBelow]) {
    for below in below {
        match below.counted {
            Some(counted) if counted != below.stated => findings.fault(
                Rule::StatesReadoutSlots,
                below.place(),
                format!(
                    "states {} readout slots; its instructions count {counted}",
                    below.stated
                ),
            ),
            Some(_) => {}
            None => findings.cannot_judge(format!(
                "the batch at {} holds an instruction the kit's executor below does not \
                 accept, so the check cannot count its readout slots",
                below.place()
            )),
        }
    }
}

/// Whether `failure` is the refusal of a batch whose readout slots are
/// misstated.
pub(crate) fn misstates_slots(failure: &RunError) -> bool {
    matches!(
        failure,
        RunError::Failed(DriveError {
            failure: Failure::Refused(ExecuteRefusal::ReadoutSlotsMisstated { .. }),
            ..
        })
    )
}
