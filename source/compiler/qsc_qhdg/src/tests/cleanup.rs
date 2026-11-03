// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{CleanupError, CleanupJoin, CleanupState, cleanup::ExitKind};

#[test]
fn ownership_measurement_and_reset_preserve_dirty_restoration_distinction() {
    let pending = CleanupState::PendingRestore {
        delta: 9,
        history: Vec::new(),
    };
    assert_eq!(pending.measure(false), CleanupState::LiveUnknown);
    assert_eq!(pending.reset(false), CleanupState::LiveUnknown);
    assert_eq!(CleanupState::CleanZ.reset(true), CleanupState::CleanZ);
}

#[test]
fn ownership_dirty_restoration_and_release_routes_remain_disabled() {
    assert_eq!(
        CleanupState::PendingRestore {
            delta: 9,
            history: vec!["H".to_string()],
        }
        .prove_restored(),
        Err(CleanupError::DirtyRestorationDisabled)
    );
    assert_eq!(
        CleanupState::CleanZ.release(),
        Err(CleanupError::FailureCleanupDisabled)
    );
}

#[test]
fn ownership_branch_theta_return_and_failure_joins_are_explicit() {
    for kind in [
        ExitKind::Branch,
        ExitKind::Theta,
        ExitKind::Return,
        ExitKind::Handler,
    ] {
        assert!(CleanupJoin::checked(kind, &[CleanupState::CleanZ, CleanupState::CleanZ]).is_ok());
    }
    assert_eq!(
        CleanupJoin::checked(ExitKind::Failure, &[CleanupState::CleanZ]),
        Err(CleanupError::FailureCleanupDisabled)
    );
    assert_eq!(
        CleanupJoin::checked(
            ExitKind::Branch,
            &[CleanupState::CleanZ, CleanupState::Released]
        ),
        Err(CleanupError::JoinMismatch)
    );
    assert_eq!(
        crate::cleanup::join_cleanup([CleanupState::CleanZ, CleanupState::Escaped].iter()),
        crate::cleanup::join_cleanup([CleanupState::Escaped, CleanupState::CleanZ].iter())
    );
}
