// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A shot's conditions merge by union, and none is ever cleared.

use crate::ShotConditions;

/// Once raised, a condition stays raised: merging another, or none, keeps
/// it.
#[test]
fn merging_conditions_keeps_every_one_raised() {
    let mut conditions = ShotConditions::REJECTED;
    conditions |= ShotConditions::NONE;
    assert_eq!(conditions, ShotConditions::REJECTED);
    conditions |= ShotConditions::INCONSISTENT;
    assert!(conditions.contains(ShotConditions::REJECTED));
    assert!(conditions.contains(ShotConditions::INCONSISTENT));
    assert_eq!(
        conditions | ShotConditions::NONE,
        ShotConditions::REJECTED | ShotConditions::INCONSISTENT
    );
}

/// `contains` holds when every condition asked for is raised, not any of
/// them; asking for none holds for every shot, clear or not.
#[test]
fn contains_holds_when_every_condition_asked_is_raised() {
    let rejected = ShotConditions::REJECTED;
    for (row, raised, asked, holds) in [
        (
            "one raised, one not",
            rejected,
            ShotConditions::REJECTED | ShotConditions::UNRESOLVED,
            false,
        ),
        (
            "one not raised",
            rejected,
            ShotConditions::UNRESOLVED,
            false,
        ),
        ("the one raised", rejected, ShotConditions::REJECTED, true),
        (
            "none, of a raised set",
            rejected,
            ShotConditions::NONE,
            true,
        ),
        (
            "none, of a clear set",
            ShotConditions::NONE,
            ShotConditions::NONE,
            true,
        ),
    ] {
        assert_eq!(raised.contains(asked), holds, "{row}");
    }
}

/// Only a shot with no condition is clear.
#[test]
fn only_no_condition_is_empty() {
    assert!(ShotConditions::NONE.is_empty());
    assert!(ShotConditions::default().is_empty());
    for raised in [
        ShotConditions::REJECTED,
        ShotConditions::INCONSISTENT,
        ShotConditions::UNRESOLVED,
    ] {
        assert!(!raised.is_empty());
    }
}

/// A failing test names the conditions raised.
#[test]
fn debug_names_the_conditions() {
    assert_eq!(
        format!("{:?}", ShotConditions::NONE),
        "ShotConditions(NONE)"
    );
    assert_eq!(
        format!(
            "{:?}",
            ShotConditions::UNRESOLVED | ShotConditions::REJECTED
        ),
        "ShotConditions(REJECTED | UNRESOLVED)"
    );
}
