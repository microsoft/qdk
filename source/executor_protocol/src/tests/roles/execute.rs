// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an executor's refusal tells the part above: whether the same
//! `execute` may be sent again.

use crate::{ExecuteRefusal, InstructionFault, Opcode, RetryClass, ShotId};

/// A part that retries, such as an adapter above a device that may be
/// busy, asks each refusal whether the same `execute` may be sent again:
/// only a receiver that cannot take it now (`NotReady`) may run it later;
/// every other refusal is refused again.
#[test]
fn only_a_not_ready_refusal_may_be_sent_again() {
    for (refusal, class) in every_refusal_with_its_class(ShotId::new(3)) {
        assert_eq!(refusal.retry_class(), class, "{refusal}");
    }
}

/// Every refusal, naming `shot`, with its class. A refusal added to
/// [`ExecuteRefusal`] fails to compile in [`variant`], and then fails the
/// tests that use it until it is listed here.
pub(crate) fn every_refusal_with_its_class(shot: ShotId) -> Vec<(ExecuteRefusal, RetryClass)> {
    let every = vec![
        (
            ExecuteRefusal::NotReady {
                shot,
                why: "the device is calibrating".into(),
            },
            RetryClass::Later,
        ),
        (
            ExecuteRefusal::Instruction {
                shot,
                batch: 0,
                at: 1,
                fault: InstructionFault::UnknownOpcode(Opcode::__from_macro(99)),
            },
            RetryClass::Never,
        ),
        (
            ExecuteRefusal::TooManyInstructions {
                shot,
                batch: 0,
                max: 2,
                got: 3,
            },
            RetryClass::Never,
        ),
        (
            ExecuteRefusal::TooManyReadoutSlots {
                shot,
                batch: 0,
                at: 1,
            },
            RetryClass::Never,
        ),
        (
            ExecuteRefusal::ReadoutSlotsMisstated {
                shot,
                batch: 0,
                stated: 1,
                counted: 2,
            },
            RetryClass::Never,
        ),
        (ExecuteRefusal::ShotNotHeld(shot), RetryClass::Never),
        (ExecuteRefusal::EarlierFailure(shot), RetryClass::Never),
    ];
    let mut listed = [false; VARIANTS];
    for (refusal, _) in &every {
        listed[variant(refusal)] = true;
    }
    assert!(
        listed.iter().all(|listed| *listed),
        "every refusal is listed: {listed:?}"
    );
    every
}

/// How many variants [`ExecuteRefusal`] has.
const VARIANTS: usize = 7;

/// `refusal`'s variant, as a number below [`VARIANTS`]: an exhaustive
/// match, so that a new variant must be numbered here.
fn variant(refusal: &ExecuteRefusal) -> usize {
    match refusal {
        ExecuteRefusal::NotReady { .. } => 0,
        ExecuteRefusal::Instruction { .. } => 1,
        ExecuteRefusal::TooManyInstructions { .. } => 2,
        ExecuteRefusal::TooManyReadoutSlots { .. } => 3,
        ExecuteRefusal::ReadoutSlotsMisstated { .. } => 4,
        ExecuteRefusal::ShotNotHeld(_) => 5,
        ExecuteRefusal::EarlierFailure(_) => 6,
    }
}
