// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for [`PackageAssigners`], the per-package id assigner pool.
//!
//! These cover the core correctness hazard the pool exists to prevent: each
//! package owns an independent id space, so an assigner seeded from one package
//! must never mint ids into another package's arena (which would silently
//! overwrite existing nodes). The pool seeds a fresh assigner per package from
//! that package's own watermark.

use super::PackageAssigners;
use crate::{
    collapse_simulatable_intrinsics, gc_unreachable,
    test_utils::{compile_to_fir, compile_to_fir_with_library},
};
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{CallableImpl, ItemKind, PackageId, PackageStore};

const SOURCE: &str = "
    operation Helper(x : Int) : Int { x + 1 }
    operation Main() : Int {
        let a = Helper(1);
        let b = Helper(a);
        a + b
    }
";

/// Returns the foreign package (not `entry`) with the most expressions, so the
/// entry package's watermark is guaranteed to fall inside the foreign arena.
fn largest_foreign_package(store: &PackageStore, entry: PackageId) -> PackageId {
    store
        .iter()
        .filter(|(id, _)| *id != entry)
        .max_by_key(|(_, pkg)| pkg.exprs.iter().count())
        .map(|(id, _)| id)
        .expect("the standard library packages should be present")
}

/// The entry package's pooled assigner must reproduce the previous
/// single-assigner behavior exactly: it is seeded from the entry package's
/// watermark, so it mints the same ids a fresh `Assigner::from_package` would.
#[test]
fn entry_seed_matches_single_assigner() {
    let (store, pkg_id) = compile_to_fir(SOURCE);

    let mut pool = PackageAssigners::new(&store, pkg_id);
    let pooled = pool.get_mut(&store, pkg_id);
    let mut direct = Assigner::from_package(store.get(pkg_id));

    assert_eq!(pooled.next_expr(), direct.next_expr());
    assert_eq!(pooled.next_pat(), direct.next_pat());
    assert_eq!(pooled.next_stmt(), direct.next_stmt());
    assert_eq!(pooled.next_block(), direct.next_block());
    assert_eq!(pooled.next_item(), direct.next_item());
    assert_eq!(pooled.next_local(), direct.next_local());
}

/// A foreign package is seeded lazily from *that package's* watermark on first
/// access, not from the entry package's.
#[test]
fn lazily_seeds_external_package_from_its_own_watermark() {
    let (store, pkg_id) = compile_to_fir(SOURCE);
    let foreign = largest_foreign_package(&store, pkg_id);

    let mut pool = PackageAssigners::new(&store, pkg_id);
    let pooled = pool.get_mut(&store, foreign);
    let mut direct = Assigner::from_package(store.get(foreign));

    assert_eq!(pooled.next_expr(), direct.next_expr());
    assert_eq!(pooled.next_pat(), direct.next_pat());
    assert_eq!(pooled.next_item(), direct.next_item());
}

/// Minting into a foreign package through the pool never produces an id that
/// already exists in that package's arena, so no existing node is overwritten.
#[test]
fn minting_into_external_package_never_overwrites_existing_arena_entry() {
    let (store, pkg_id) = compile_to_fir(SOURCE);
    let foreign = largest_foreign_package(&store, pkg_id);

    let mut pool = PackageAssigners::new(&store, pkg_id);
    let assigner = pool.get_mut(&store, foreign);
    let package = store.get(foreign);

    for _ in 0..100 {
        let minted = assigner.next_expr();
        assert!(
            package.exprs.get(minted).is_none(),
            "pool-minted ExprId {minted} already exists in the foreign package arena"
        );
    }
}

/// Negative guard for the core hazard: an assigner seeded from the *entry*
/// package and used to mint into a larger foreign package collides with that
/// package's existing arena. This is exactly the silent overwrite the pool
/// prevents by seeding per package.
#[test]
fn entry_seeded_assigner_minting_into_foreign_package_collides() {
    let (store, pkg_id) = compile_to_fir(SOURCE);
    let foreign = largest_foreign_package(&store, pkg_id);

    // Deliberately mis-seed: use the entry package's watermark to mint ids that
    // are then interpreted against the foreign package's arena.
    let mut mis_seeded = Assigner::from_package(store.get(pkg_id));
    let foreign_package = store.get(foreign);

    let collided = (0..100).any(|_| foreign_package.exprs.get(mis_seeded.next_expr()).is_some());
    assert!(
        collided,
        "an entry-seeded assigner must collide with the larger foreign package's arena, \
         demonstrating the overwrite hazard the per-package pool prevents"
    );
}

#[test]
fn eager_seed_preserves_all_foreign_package_watermarks_after_gc() {
    let library_source = r#"
        namespace TestLib {
            @SimulatableIntrinsic()
            operation ForeignSimulatableIntrinsic(value : (Int, Int)) : Unit {
                let (left, right) = value;
                mutable total = left + right;
                set total += 1;
            }
        }
    "#;
    let user_source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {}
        }
    "#;
    let (mut store, entry_package_id) = compile_to_fir_with_library(library_source, user_source);
    let foreign_package_id = store
        .iter()
        .find_map(|(package_id, package)| {
            package
                .items
                .values()
                .any(|item| {
                    matches!(
                        &item.kind,
                        ItemKind::Callable(decl)
                            if decl.name.name.as_ref() == "ForeignSimulatableIntrinsic"
                                && matches!(
                                    decl.implementation,
                                    CallableImpl::SimulatableIntrinsic(_)
                                )
                    )
                })
                .then_some(package_id)
        })
        .expect("foreign simulatable intrinsic should exist");

    let mut expected = Assigner::from_package(store.get(foreign_package_id));
    let expected_block = expected.next_block();
    let expected_expr = expected.next_expr();
    let expected_pat = expected.next_pat();
    let expected_stmt = expected.next_stmt();
    let expected_local = expected.next_local();
    let expected_item = expected.next_item();

    let mut pool = PackageAssigners::new(&store, entry_package_id);
    pool.seed_all(&store);
    collapse_simulatable_intrinsics(&mut store);
    let removed = gc_unreachable::gc_unreachable(store.get_mut(foreign_package_id));
    assert!(removed > 0, "simulation body nodes should be collected");

    let mut post_gc = Assigner::from_package(store.get(foreign_package_id));
    assert_ne!(post_gc.next_block(), expected_block);
    assert_ne!(post_gc.next_expr(), expected_expr);
    assert_ne!(post_gc.next_pat(), expected_pat);
    assert_ne!(post_gc.next_stmt(), expected_stmt);
    assert_ne!(post_gc.next_local(), expected_local);
    assert_eq!(post_gc.next_item(), expected_item);

    let seeded = pool.get_mut(&store, foreign_package_id);
    assert_eq!(seeded.next_block(), expected_block);
    assert_eq!(seeded.next_expr(), expected_expr);
    assert_eq!(seeded.next_pat(), expected_pat);
    assert_eq!(seeded.next_stmt(), expected_stmt);
    assert_eq!(seeded.next_local(), expected_local);
    assert_eq!(seeded.next_item(), expected_item);
}
