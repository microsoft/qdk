// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The first failure's rules: the failure a run keeps is its earliest
//! shot's, in the order of its shots, whatever order the failures are met
//! in, and it comes before exactly the later shots; and the run's error
//! names that shot and keeps how it failed. Which shot a whole run names
//! is in the tests beside `grouped_run.rs`.

use crate::mps_simulator::grouped_run::first_failure::{
    FirstFailure, GroupFailure, engine_failure,
};
use crate::mps_simulator::grouped_run::test_utils::{Failure, failed_at, stopped};
use crate::mps_simulator::test_utils::ids;
use crate::mps_simulator::test_utils::reference_spy::ReferenceSpyError;
use crate::qir::{QirShotId, QirTrap};

/// The failure a run keeps is its earliest shot's, in the order of its
/// shots, whatever order the run meets the failures in: it then comes
/// before exactly the shots after that one. Each row is the places of the
/// shots that fail, in the order the run meets them; the run's shots are
/// 10, 11 and 12.
#[test]
fn the_kept_failure_is_the_earliest_shots_whatever_order_it_is_met_in() {
    let shots = ids(&[10, 11, 12]);
    for met in [[2, 0, 1], [0, 2, 1], [1, 2, 0]] {
        let mut failure = FirstFailure::<ReferenceSpyError>::none();
        for place in met {
            failure.keep(place, stopped());
        }
        let scene = format!("met in order {met:?}");
        assert!(!failure.comes_before(0), "{scene}: before the first shot");
        assert!(failure.comes_before(1), "{scene}: before a later shot");
        let error = failure.into_error(&shots).expect(&scene);
        assert_eq!(
            failed_at(&error, &scene),
            (QirShotId(10), Failure::Stopped),
            "{scene}"
        );
    }
    let no_failure = FirstFailure::<ReferenceSpyError>::none();
    assert!(
        !no_failure.comes_before(0),
        "no failure comes before a shot"
    );
    assert!(
        no_failure.into_error(&shots).is_none(),
        "no failure, no error"
    );
}

/// The run's error names the kept failure's shot among the run's shots,
/// and keeps how it failed: a trap stays a trap, the engine's error stays
/// the engine's, and a stop a stop. Each row is a failure, kept for the
/// shot at place 1 of shots 10, 11 and 12, so the error names shot 11.
#[test]
fn the_error_names_the_kept_shot_and_how_it_failed() {
    let shots = ids(&[10, 11, 12]);
    let a_trap = QirTrap::NotABit {
        instruction: 4,
        value: 2,
    };
    let rows = [
        ("a trap", GroupFailure::Trapped(a_trap), Failure::Trap),
        (
            "the engine's failure",
            engine_failure(ReferenceSpyError::Device),
            Failure::Engine,
        ),
        ("a stop", stopped(), Failure::Stopped),
    ];
    for (scene, kept, how) in rows {
        let mut failure = FirstFailure::none();
        failure.keep(1, kept);
        let error = failure.into_error(&shots).expect(scene);
        assert_eq!(failed_at(&error, scene), (QirShotId(11), how), "{scene}");
    }
}
