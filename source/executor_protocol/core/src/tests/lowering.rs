// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A lowering's default `read`, which copies an instruction's readouts one
//! for one; how [`Lowered`](crate::Lowered) runs any lowering is in
//! `lowering/lowered.rs`.

mod lowered;

use std::any::type_name;

use crate::prelude::*;
use crate::tests::example_sets::ToffoliGate;
use crate::tests::fake_parts::{CliffordTRegister, Log, ReadTwiceUnread, ToffoliProgram};
use crate::{DriveError, Failure, Lowered, PartId, ReadoutValue, RunError, ShotSet};

/// A lowering reading each `mz` twice below, which keeps the default
/// `read`: its run fails at once, as a panic of its adapter whose message
/// names the lowering, rather than its readouts being copied wrong, so its
/// writer learns to write its own `read`.
#[test]
fn the_default_read_refuses_a_lowering_of_other_readout_slots_naming_it() {
    let log = Log::default();
    let Ok(mut runner) = ToffoliProgram::new(&[ToffoliGate::Mz(0)], 1)
        .through(Lowered::new(ReadTwiceUnread))
        .on(CliffordTRegister::new(1, &[ReadoutValue::One; 2], &log))
    else {
        panic!("the chain composes");
    };
    let Err(RunError::Failed(DriveError {
        failure: Failure::Panicked { part, message },
        ..
    })) = runner.run(&ShotSet::ids([0]), 7)
    else {
        panic!("the lowering's adapter panics");
    };
    assert_eq!(
        part,
        PartId::new(1, type_name::<Lowered<ReadTwiceUnread>>())
    );
    let message = message.expect("a message");
    assert!(message.contains("ReadTwiceUnread"), "{message}");
}
