// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use qsc_data_structures::functors::FunctorApp;
use qsc_fir::fir;
use qsc_fir::fir::{ExprId, ItemId, LocalItemId, PackageId, StoreItemId};

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
    let rhs = CalleeLattice::Multi(vec![(b.clone(), vec![ExprId::from(50u32)])]);

    let result = lhs.join_with_condition(rhs, cond());

    match result {
        CalleeLattice::Multi(entries) => {
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0], (a, vec![cond()]));
            assert_eq!(entries[1], (b, vec![ExprId::from(50u32)]));
        }
        other => panic!("expected Multi, got {other:?}"),
    }
}

#[test]
fn join_with_condition_multi_single_inserts_into_set() {
    let a = global(1);
    let b = global(2);
    let lhs = CalleeLattice::Multi(vec![(a.clone(), vec![ExprId::from(50u32)])]);
    let rhs = CalleeLattice::Single(b.clone());

    let result = lhs.join_with_condition(rhs, cond());

    match result {
        CalleeLattice::Multi(entries) => {
            assert_eq!(entries.len(), 2);
            // The outer `condition` is prepended onto the inherited guard
            // list, and the false-branch callable becomes the trailing
            // default with an empty guard list.
            assert_eq!(entries[0], (a, vec![cond(), ExprId::from(50u32)]));
            assert_eq!(entries[1], (b, vec![]));
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
            assert_eq!(entries[0], (a, vec![cond()]));
            assert_eq!(entries[1], (b, vec![]));
        }
        other => panic!("expected Multi, got {other:?}"),
    }
}

#[test]
fn join_with_condition_multi_single_cap_exceeded_becomes_dynamic() {
    let entries: Vec<(ConcreteCallable, Vec<ExprId>)> = (0..MULTI_CAP)
        .map(|i| {
            (
                global(i),
                vec![ExprId::from(u32::try_from(i).expect("id must fit"))],
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

#[test]
fn join_with_condition_multi_multi_merges_nested_branches() {
    // `if cOuter { if a {W} else {X} } else { if b {Y} else {Z} }`.
    let inner_a = ExprId::from(10u32);
    let inner_b = ExprId::from(20u32);
    let cc_w = global(1);
    let cc_x = global(2);
    let cc_y = global(3);
    let cc_z = global(4);

    // True branch: `if a {W} else {X}` → outermost-first guard lists.
    let lhs = CalleeLattice::Multi(vec![(cc_w.clone(), vec![inner_a]), (cc_x.clone(), vec![])]);
    // False branch: `if b {Y} else {Z}`.
    let rhs = CalleeLattice::Multi(vec![(cc_y.clone(), vec![inner_b]), (cc_z.clone(), vec![])]);

    let result = lhs.join_with_condition(rhs, cond());

    match result {
        CalleeLattice::Multi(entries) => {
            assert_eq!(entries.len(), 4);
            // s1 entries gain the outer condition prepended (outermost-first).
            assert_eq!(entries[0], (cc_w, vec![cond(), inner_a]));
            assert_eq!(entries[1], (cc_x, vec![cond()]));
            // s2 entries keep their inner guards; the empty-guard default is
            // last and unconditional.
            assert_eq!(entries[2], (cc_y, vec![inner_b]));
            assert_eq!(entries[3], (cc_z, vec![]));
        }
        other => panic!("expected Multi, got {other:?}"),
    }
}

#[test]
fn join_with_condition_multi_multi_same_set_stays_unchanged() {
    let a = global(1);
    let b = global(2);
    let set = vec![(a.clone(), vec![ExprId::from(50u32)]), (b.clone(), vec![])];
    let lhs = CalleeLattice::Multi(set.clone());
    let rhs = CalleeLattice::Multi(set.clone());

    let result = lhs.join_with_condition(rhs, cond());

    match result {
        CalleeLattice::Multi(entries) => assert_eq!(entries, set),
        other => panic!("expected Multi, got {other:?}"),
    }
}

#[test]
fn join_with_condition_multi_multi_shared_callable_keeps_both_arms() {
    // `if cOuter { if a {W} else {X} } else { if b {W} else {Z} }`: `W`
    // reaches the local from both branches under different guards. The merge
    // must keep both `W` arms — collapsing them by callable identity would
    // drop the `!cOuter && b` arm and reroute that path to the trailing
    // default `Z`.
    let inner_a = ExprId::from(10u32);
    let inner_b = ExprId::from(20u32);
    let cc_w = global(1);
    let cc_x = global(2);
    let cc_z = global(4);

    let lhs = CalleeLattice::Multi(vec![(cc_w.clone(), vec![inner_a]), (cc_x.clone(), vec![])]);
    let rhs = CalleeLattice::Multi(vec![(cc_w.clone(), vec![inner_b]), (cc_z.clone(), vec![])]);

    let result = lhs.join_with_condition(rhs, cond());

    match result {
        CalleeLattice::Multi(entries) => {
            assert_eq!(entries.len(), 4);
            assert_eq!(entries[0], (cc_w.clone(), vec![cond(), inner_a]));
            assert_eq!(entries[1], (cc_x, vec![cond()]));
            // The shared `W` from the false branch keeps its own guard and is
            // not deduplicated against the true-branch `W`.
            assert_eq!(entries[2], (cc_w, vec![inner_b]));
            assert_eq!(entries[3], (cc_z, vec![]));
        }
        other => panic!("expected Multi, got {other:?}"),
    }
}

#[test]
fn join_with_condition_multi_multi_cap_exceeded_becomes_dynamic() {
    let half = MULTI_CAP / 2 + 1;
    let s1: Vec<(ConcreteCallable, Vec<ExprId>)> = (0..half)
        .map(|i| {
            (
                global(i),
                vec![ExprId::from(u32::try_from(i).expect("id must fit"))],
            )
        })
        .collect();
    let s2: Vec<(ConcreteCallable, Vec<ExprId>)> = (0..half)
        .map(|i| {
            (
                global(i + 100),
                vec![ExprId::from(u32::try_from(i + 100).expect("id must fit"))],
            )
        })
        .collect();
    let lhs = CalleeLattice::Multi(s1);
    let rhs = CalleeLattice::Multi(s2);

    let result = lhs.join_with_condition(rhs, cond());

    assert!(
        matches!(result, CalleeLattice::Dynamic),
        "expected Dynamic when exceeding MULTI_CAP, got {result:?}"
    );
}

#[test]
fn compose_functors_identity() {
    let a = FunctorApp::default();
    let b = FunctorApp::default();
    let result = compose_functors(&a, &b);
    assert_eq!(result, FunctorApp::default());
}

#[test]
fn compose_functors_adj_toggle() {
    let a = FunctorApp {
        adjoint: true,
        controlled: 0,
    };
    let b = FunctorApp {
        adjoint: true,
        controlled: 0,
    };
    let result = compose_functors(&a, &b);
    assert!(!result.adjoint, "adj XOR adj should cancel");
    assert_eq!(result.controlled, 0);
}

#[test]
fn compose_functors_ctl_stack() {
    let a = FunctorApp {
        adjoint: false,
        controlled: 1,
    };
    let b = FunctorApp {
        adjoint: false,
        controlled: 1,
    };
    let result = compose_functors(&a, &b);
    assert!(!result.adjoint);
    assert_eq!(result.controlled, 2);
}

#[test]
fn compose_functors_adj_and_ctl() {
    let a = FunctorApp {
        adjoint: true,
        controlled: 1,
    };
    let b = FunctorApp {
        adjoint: false,
        controlled: 1,
    };
    let result = compose_functors(&a, &b);
    assert!(result.adjoint, "true XOR false = true");
    assert_eq!(result.controlled, 2);
}

#[test]
fn spec_key_equality() {
    let key1 = SpecKey {
        hof_id: StoreItemId::from((PackageId::from(1usize), LocalItemId::from(5usize))),
        concrete_args: vec![ConcreteCallableKey::Global {
            item_id: ItemId {
                package: fir::PackageId::from(1usize),
                item: LocalItemId::from(10usize),
            },
            functor: FunctorApp::default(),
        }],
    };
    let key2 = SpecKey {
        hof_id: StoreItemId::from((PackageId::from(1usize), LocalItemId::from(5usize))),
        concrete_args: vec![ConcreteCallableKey::Global {
            item_id: ItemId {
                package: fir::PackageId::from(1usize),
                item: LocalItemId::from(10usize),
            },
            functor: FunctorApp::default(),
        }],
    };
    assert_eq!(key1, key2);
}

#[test]
fn spec_key_different() {
    let key1 = SpecKey {
        hof_id: StoreItemId::from((PackageId::from(1usize), LocalItemId::from(5usize))),
        concrete_args: vec![ConcreteCallableKey::Global {
            item_id: ItemId {
                package: fir::PackageId::from(1usize),
                item: LocalItemId::from(10usize),
            },
            functor: FunctorApp::default(),
        }],
    };
    let key2 = SpecKey {
        hof_id: StoreItemId::from((PackageId::from(1usize), LocalItemId::from(5usize))),
        concrete_args: vec![ConcreteCallableKey::Global {
            item_id: ItemId {
                package: fir::PackageId::from(1usize),
                item: LocalItemId::from(20usize),
            },
            functor: FunctorApp::default(),
        }],
    };
    assert_ne!(key1, key2);
}
