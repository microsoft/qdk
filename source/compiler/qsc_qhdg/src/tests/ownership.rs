// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::BTreeSet;

use crate::{
    AliasDecision, Authority, BorrowKind, CleanupState, Epoch, OwnerPath, OwnershipError,
    OwnershipState, ResourceId, SplitCertificate,
};

fn authority() -> Authority {
    Authority {
        path: OwnerPath::root(7),
        resources: BTreeSet::from([ResourceId(10), ResourceId(11)]),
        epoch: Epoch(0),
        cleanup: CleanupState::CleanZ,
    }
}

#[test]
fn ownership_paths_normalize_fields_indices_dynamic_indices_and_slices() {
    let path = OwnerPath::root(7)
        .field("items")
        .index(-1, Some(4))
        .expect("negative index")
        .dynamic_index("i", "0 <= i && i < 4")
        .slice(0, 4, 2)
        .expect("slice");
    assert_eq!(path.selectors.len(), 4);
    assert!(matches!(path.selectors[1], crate::PathSelector::Index(3)));
}

#[test]
fn ownership_shared_and_exclusive_borrows_remain_affine() {
    let mut state = OwnershipState::default();
    state.add_authority(authority()).expect("authority");
    let first = state
        .borrow(&OwnerPath::root(7), BorrowKind::Shared)
        .expect("shared");
    let second = state
        .borrow(&OwnerPath::root(7), BorrowKind::Shared)
        .expect("shared");
    assert_eq!(
        state.borrow(&OwnerPath::root(7), BorrowKind::Exclusive),
        Err(OwnershipError::BorrowConflict)
    );
    state.end_borrow(&first).expect("end first");
    state.end_borrow(&second).expect("end second");
    assert!(
        state
            .borrow(&OwnerPath::root(7), BorrowKind::Exclusive)
            .is_ok()
    );
}

#[test]
fn ownership_split_and_join_require_a_complete_certificate() {
    let parent = authority();
    let left = OwnerPath::root(7).index(0, Some(2)).expect("left");
    let right = OwnerPath::root(7).index(1, Some(2)).expect("right");
    let certificate = SplitCertificate::checked(
        &parent,
        vec![
            (left, BTreeSet::from([ResourceId(10)])),
            (right, BTreeSet::from([ResourceId(11)])),
        ],
    )
    .expect("certificate");
    let mut state = OwnershipState::default();
    state.add_authority(parent).expect("authority");
    state
        .split(&OwnerPath::root(7), &certificate)
        .expect("split");
    state.join(&certificate).expect("join");
    assert_eq!(state.authorities[0].resources.len(), 2);
    assert_eq!(state.authorities[0].epoch, Epoch(1));
}

#[test]
fn ownership_runtime_disjointness_is_disabled() {
    assert_eq!(
        OwnershipState::classify_alias(AliasDecision::RuntimeCheck),
        Err(OwnershipError::RuntimeDisjointnessDisabled)
    );
}

#[test]
fn ownership_branch_theta_return_and_measurement_transitions_advance_epochs() {
    let first = authority().quantum_transition("H");
    let second = authority().measure(true);
    let joined = OwnershipState::join_exits(crate::CleanupExitKind::Branch, &[first, second])
        .expect("matching domains join");
    assert_eq!(joined.epoch, Epoch(2));
    assert_eq!(joined.cleanup, CleanupState::LiveUnknown);
    assert_eq!(authority().reset(true).cleanup, CleanupState::CleanZ);
}
