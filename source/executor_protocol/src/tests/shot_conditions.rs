// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::ShotConditions;

/// A shot's conditions print as the flags raised, each under its own
/// name, so that a test's failure names them.
#[test]
fn conditions_print_as_their_flags() {
    assert_eq!(
        format!("{:?}", ShotConditions::NONE),
        "ShotConditions(NONE)"
    );
    assert_eq!(
        format!("{:?}", ShotConditions::REJECTED),
        "ShotConditions(REJECTED)"
    );
    assert_eq!(
        format!("{:?}", ShotConditions::UNRESOLVED),
        "ShotConditions(UNRESOLVED)"
    );
    assert_eq!(
        format!(
            "{:?}",
            ShotConditions::INCONSISTENT | ShotConditions::REJECTED
        ),
        "ShotConditions(REJECTED | INCONSISTENT)"
    );
}

/// A caller that discards a shot only when it is both rejected and
/// inconsistent asks whether its conditions contain both: a set contains
/// another only when it holds every condition of it, not one of them
/// alone.
#[test]
fn a_shots_conditions_contain_a_set_only_with_every_condition_of_it() {
    let both = ShotConditions::REJECTED | ShotConditions::INCONSISTENT;
    assert!(both.contains(both), "a set contains itself");
    assert!(
        (both | ShotConditions::UNRESOLVED).contains(both),
        "a larger set contains it"
    );
    assert!(!ShotConditions::REJECTED.contains(both));
}
