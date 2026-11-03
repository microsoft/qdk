// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An outcome given conditions carries them in place of its own, and
//! carries them at no cost in size.

use crate::{Outcome, ShotConditions, ShotId, ShotOutcome, Value};

/// A code's runtime copies its readouts from a template, giving each the
/// conditions it found on its shot: copied from a template that carries a
/// rejection and given none, a readout carries none, as the conditions
/// given replace an outcome's own rather than join them.
#[test]
fn an_outcome_given_conditions_carries_them_in_place_of_its_own() {
    let template = ShotOutcome::new(1, 0, Value::One).with_conditions(ShotConditions::REJECTED);
    assert_eq!(
        template.with_conditions(ShotConditions::NONE).conditions,
        ShotConditions::NONE,
        "a shot's outcome"
    );
    let template = Outcome::new(0, Value::One).with_conditions(ShotConditions::REJECTED);
    assert_eq!(
        template.with_conditions(ShotConditions::NONE).conditions,
        ShotConditions::NONE,
        "a logic's outcome"
    );
}

/// A run of a million shots holds a million outcomes at once: the byte of
/// conditions each carries fits in the room its other fields leave, so an
/// outcome, a shot's or a logic's, is as small as it was without them.
#[test]
fn an_outcome_is_no_larger_for_carrying_its_shots_conditions() {
    let padded = |bytes: usize, align: usize| bytes.next_multiple_of(align);
    assert_eq!(
        size_of::<ShotOutcome>(),
        padded(
            size_of::<ShotId>() + size_of::<u32>() + size_of::<Value>(),
            align_of::<ShotOutcome>()
        )
    );
    assert_eq!(
        size_of::<Outcome>(),
        padded(size_of::<u32>() + size_of::<Value>(), align_of::<Outcome>())
    );
}
