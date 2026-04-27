// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use qsc_data_structures::functors::FunctorApp;
use qsc_fir::fir::{ExprId, ItemId, LocalItemId, PackageId};

fn global(id: usize) -> ConcreteCallable {
    ConcreteCallable::Global {
        item_id: ItemId {
            package: PackageId::from(0),
            item: LocalItemId::from(id),
        },
        functor: FunctorApp::default(),
    }
}

fn cond() -> ExprId {
    ExprId::from(99u32)
}

#[test]
fn join_with_condition_single_multi_inserts_into_set() {
    let a = global(1);
    let b = global(2);
    let lhs = CalleeLattice::Single(a.clone());
    let rhs = CalleeLattice::Multi(vec![(b.clone(), Some(ExprId::from(50u32)))]);

    let result = lhs.join_with_condition(rhs, cond());

    match result {
        CalleeLattice::Multi(entries) => {
            assert!(entries.iter().any(|(cc, _)| *cc == a));
            assert!(entries.iter().any(|(cc, _)| *cc == b));
        }
        other => panic!("expected Multi, got {other:?}"),
    }
}

#[test]
fn join_with_condition_multi_single_inserts_into_set() {
    let a = global(1);
    let b = global(2);
    let lhs = CalleeLattice::Multi(vec![(a.clone(), Some(ExprId::from(50u32)))]);
    let rhs = CalleeLattice::Single(b.clone());

    let result = lhs.join_with_condition(rhs, cond());

    match result {
        CalleeLattice::Multi(entries) => {
            assert!(entries.iter().any(|(cc, _)| *cc == a));
            assert!(entries.iter().any(|(cc, _)| *cc == b));
        }
        other => panic!("expected Multi, got {other:?}"),
    }
}

#[test]
fn join_with_condition_single_same_stays_single() {
    let a = global(1);
    let result = CalleeLattice::Single(a.clone())
        .join_with_condition(CalleeLattice::Single(a.clone()), cond());

    match result {
        CalleeLattice::Single(cc) => assert_eq!(cc, a),
        other => panic!("expected Single, got {other:?}"),
    }
}

#[test]
fn join_with_condition_single_different_produces_multi() {
    let a = global(1);
    let b = global(2);
    let result = CalleeLattice::Single(a.clone())
        .join_with_condition(CalleeLattice::Single(b.clone()), cond());

    match result {
        CalleeLattice::Multi(entries) => {
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0], (a, Some(cond())));
            assert_eq!(entries[1], (b, None));
        }
        other => panic!("expected Multi, got {other:?}"),
    }
}

#[test]
fn join_with_condition_multi_single_cap_exceeded_becomes_dynamic() {
    let entries: Vec<(ConcreteCallable, Option<ExprId>)> = (0..MULTI_CAP)
        .map(|i| {
            (
                global(i),
                Some(ExprId::from(u32::try_from(i).expect("id must fit"))),
            )
        })
        .collect();
    let extra = global(MULTI_CAP + 10);
    let lhs = CalleeLattice::Multi(entries);
    let rhs = CalleeLattice::Single(extra);

    let result = lhs.join_with_condition(rhs, cond());

    assert!(
        matches!(result, CalleeLattice::Dynamic),
        "expected Dynamic when exceeding MULTI_CAP, got {result:?}"
    );
}
