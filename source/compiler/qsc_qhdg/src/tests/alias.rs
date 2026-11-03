// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{AliasDecision, AliasFacts, AliasView, OwnerPath, ResourceId};

#[test]
fn ownership_alias_views_do_not_duplicate_affine_authority() {
    let duplicate = AliasView::new(
        vec![OwnerPath::root(1), OwnerPath::root(1)],
        vec![ResourceId(5), ResourceId(5)],
    );
    assert!(!duplicate.injective);
    assert_eq!(duplicate.classify(&duplicate, false), AliasDecision::Reject);
}

#[test]
fn ownership_alias_classification_is_independent_of_solver_timing() {
    let left = AliasView::new(vec![OwnerPath::root(1)], vec![ResourceId(5)]);
    let right = AliasView::new(vec![OwnerPath::root(2)], vec![ResourceId(6)]);
    assert_eq!(left.classify(&right, false), AliasDecision::ProvenDisjoint);
}

#[test]
fn ownership_alias_facts_cover_proof_runtime_check_escape_and_reject() {
    assert_eq!(
        AliasFacts {
            statically_disjoint: true,
            ..AliasFacts::default()
        }
        .classify(),
        AliasDecision::ProvenDisjoint
    );
    assert_eq!(
        AliasFacts {
            runtime_comparable: true,
            ..AliasFacts::default()
        }
        .classify(),
        AliasDecision::RuntimeCheck
    );
    assert_eq!(
        AliasFacts {
            may_escape: true,
            ..AliasFacts::default()
        }
        .classify(),
        AliasDecision::Escape
    );
    assert_eq!(AliasFacts::default().classify(), AliasDecision::Reject);
}
