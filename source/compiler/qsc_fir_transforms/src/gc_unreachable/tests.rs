// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Proptest applicability: Low — gc_unreachable operates on FIR arena nodes (mark-and-sweep),
// not on Q# semantics. Its correctness is a structural invariant (no surviving node references
// a tombstoned node) rather than behavioral equivalence. Q# template generation doesn't add
// much beyond targeted snapshots that create known orphan patterns.

use crate::PipelineStage;
use crate::test_utils::{compile_and_run_pipeline_to, compile_and_run_pipeline_to_with_library};
use indoc::indoc;

/// Counts total live entries across all four arena types.
fn arena_live_count(package: &qsc_fir::fir::Package) -> usize {
    package.blocks.iter().count()
        + package.stmts.iter().count()
        + package.exprs.iter().count()
        + package.pats.iter().count()
}

#[test]
fn gc_no_orphans_preserves_all_entries() {
    // A simple program with one operation, no closures, no multiple returns.
    // After arg_promote, there should be no orphans.
    let source = indoc! {"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                H(q);
                Reset(q);
            }
        }
    "};
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);
    let before = arena_live_count(store.get(pkg_id));
    let removed = super::gc_unreachable(store.get_mut(pkg_id));
    let after = arena_live_count(store.get(pkg_id));
    assert_eq!(removed, 0, "simple program should have no orphans");
    assert_eq!(before, after, "arena sizes should be unchanged");
}

#[test]
fn gc_removes_return_unify_orphans() {
    // A program with multiple return paths triggers return_unify rewrites,
    // which leaves the original return-path stmts/exprs as orphans.
    let source = indoc! {"
        namespace Test {
            @EntryPoint()
            function Main() : Int {
                if true {
                    return 1;
                }
                return 2;
            }
        }
    "};
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);
    let before = arena_live_count(store.get(pkg_id));
    let removed = super::gc_unreachable(store.get_mut(pkg_id));
    let after = arena_live_count(store.get(pkg_id));
    assert!(
        removed > 0,
        "return_unify should leave orphans that GC removes"
    );
    // The reported count must match the actual arena shrinkage.
    assert_eq!(
        after,
        before - removed,
        "live count must drop by exactly the removed count"
    );
    // Verify post-GC integrity (PostArgPromote: checks arena links without
    // requiring exec_graph_rebuild to have run).
    crate::invariants::check(
        &store,
        pkg_id,
        crate::invariants::InvariantLevel::PostArgPromote,
    );
}

#[test]
fn gc_removes_defunc_orphans() {
    // A program with closures triggers defunctionalization body cloning,
    // which leaves original closure bodies as orphans.
    let source = indoc! {"
        namespace Test {
            function Apply(f : Int -> Int, x : Int) : Int { f(x) }
            @EntryPoint()
            function Main() : Int { Apply(x -> x + 1, 5) }
        }
    "};
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);
    let before = arena_live_count(store.get(pkg_id));
    let removed = super::gc_unreachable(store.get_mut(pkg_id));
    let after = arena_live_count(store.get(pkg_id));
    assert!(removed > 0, "defunc should leave orphans that GC removes");
    // The reported count must match the actual arena shrinkage.
    assert_eq!(
        after,
        before - removed,
        "live count must drop by exactly the removed count"
    );
    // Verify post-GC integrity (PostArgPromote: checks arena links without
    // requiring exec_graph_rebuild to have run).
    crate::invariants::check(
        &store,
        pkg_id,
        crate::invariants::InvariantLevel::PostArgPromote,
    );
}

#[test]
fn gc_on_entry_less_package_is_noop() {
    // Compile a source with entry, then target the core package (no entry).
    let source = indoc! {"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {}
        }
    "};
    let (mut store, _pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);
    let core_id = qsc_fir::fir::PackageId::CORE;
    assert!(
        store.get(core_id).entry.is_none(),
        "core package should have no entry expression"
    );
    let removed = super::gc_unreachable(store.get_mut(core_id));
    assert_eq!(removed, 0, "entry-less core package should have no orphans");
}

#[test]
fn gc_is_idempotent() {
    // Multiple return paths leave orphaned arena nodes after return_unify.
    let source = indoc! {"
        namespace Test {
            @EntryPoint()
            function Main() : Int {
                if true {
                    return 1;
                }
                return 2;
            }
        }
    "};
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);
    let first_pass = super::gc_unreachable(store.get_mut(pkg_id));
    assert!(first_pass > 0, "first GC pass should remove orphans");
    let second_pass = super::gc_unreachable(store.get_mut(pkg_id));
    assert_eq!(
        second_pass, 0,
        "second GC pass should find nothing to remove"
    );
}

#[test]
fn entry_only_reachable_item_survives_dead_sibling_removed() {
    // `Used` is reachable from the entry; `Dead` is not. `gc_unreachable` never
    // removes items itself, so a dead sibling's body only becomes orphaned once
    // `item_dce` tombstones the item. This pins the identity-level outcome: the
    // live item's body block survives the sweep while the dead sibling's body
    // block is tombstoned (not merely `removed > 0`).
    use qsc_fir::fir::{BlockId, CallableImpl, ItemKind};

    fn body_block(package: &qsc_fir::fir::Package, name: &str) -> BlockId {
        package
            .items
            .values()
            .find_map(|item| match &item.kind {
                ItemKind::Callable(decl) if decl.name.name.as_ref() == name => {
                    match &decl.implementation {
                        CallableImpl::Spec(spec) => Some(spec.body.block),
                        _ => None,
                    }
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("callable {name} not found"))
    }

    let source = indoc! {"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                Used(q);
                Reset(q);
            }
            operation Used(q : Qubit) : Unit { H(q); }
            operation Dead(q : Qubit) : Unit { X(q); }
        }
    "};

    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ArgPromote);

    let used_block = body_block(store.get(pkg_id), "Used");
    let dead_block = body_block(store.get(pkg_id), "Dead");
    assert_ne!(
        used_block, dead_block,
        "the two callables should have distinct body blocks"
    );

    // Both bodies occupy their arena slots before item DCE.
    assert!(store.get(pkg_id).blocks.get(used_block).is_some());
    assert!(store.get(pkg_id).blocks.get(dead_block).is_some());

    // Item DCE drops `Dead` (entry-unreachable), orphaning its body block while
    // leaving the live `Used` item intact.
    let reachable = crate::reachability::collect_reachable_from_entry(&store, pkg_id);
    let removed_items =
        crate::item_dce::eliminate_dead_items(pkg_id, store.get_mut(pkg_id), &reachable);
    assert!(
        removed_items >= 1,
        "item_dce should remove the entry-unreachable `Dead` item"
    );
    assert!(
        store.get(pkg_id).blocks.get(dead_block).is_some(),
        "dead body block should still occupy its slot before GC"
    );

    let removed = super::gc_unreachable(store.get_mut(pkg_id));
    assert!(removed > 0, "GC should sweep the orphaned dead body");

    // Identity-level survivorship: the entry-reachable item's body survives the
    // sweep, and the dead sibling's body is tombstoned.
    assert!(
        store.get(pkg_id).blocks.get(used_block).is_some(),
        "entry-reachable `Used` body block must survive GC"
    );
    assert!(
        store.get(pkg_id).blocks.get(dead_block).is_none(),
        "dead sibling `Dead` body block must be tombstoned by GC"
    );
}

/// Locates the FIR package id of the separately-compiled library package by
/// finding the package (other than the user package) that defines the given
/// namespace. The fixture uses a uniquely-named namespace so this never
/// collides with core/std.
fn library_package_id(
    store: &qsc_fir::fir::PackageStore,
    user_pkg: qsc_fir::fir::PackageId,
    namespace: &str,
) -> qsc_fir::fir::PackageId {
    for (id, package) in store {
        if id == user_pkg {
            continue;
        }
        let defines_namespace = package.items.values().any(|item| {
            matches!(&item.kind, qsc_fir::fir::ItemKind::Namespace(name, _)
                if name.name.as_ref() == namespace)
        });
        if defines_namespace {
            return id;
        }
    }
    panic!("could not locate the {namespace} library package in the store");
}

#[test]
fn gc_removes_foreign_package_return_unify_orphans() {
    // A multi-return callable defined in a LIBRARY package and reached from the
    // user entry point is rewritten by return_unify in its OWNING (foreign)
    // package, leaving orphaned stmts/exprs behind there. The closure-wide node
    // GC in `run_pipeline_to_impl` must tombstone those foreign-package orphans,
    // not just orphans in the entry package.
    let lib_source = indoc! {r#"
        namespace TestLib {
            function Choose(cond : Bool) : Int {
                if cond {
                    return 1;
                }
                return 2;
            }
            export Choose;
        }
    "#};
    let user_source = indoc! {r#"
        import TestLib.*;
        @EntryPoint()
        function Main() : Int {
            Choose(true)
        }
    "#};

    // Before node GC: `PipelineStage::ArgPromote` runs return_unify (which
    // rewrites the reachable library callable in place) but stops before the
    // closure-wide GC. The library package therefore still holds the
    // return_unify orphans, so a manual GC over it reclaims them. This guards
    // against a vacuous test where the fixture produces no foreign orphans.
    let (mut pre_gc_store, pre_user_pkg) = compile_and_run_pipeline_to_with_library(
        lib_source,
        user_source,
        PipelineStage::ArgPromote,
    );
    let pre_lib_pkg = library_package_id(&pre_gc_store, pre_user_pkg, "TestLib");
    let pre_gc_removed = super::gc_unreachable(pre_gc_store.get_mut(pre_lib_pkg));
    assert!(
        pre_gc_removed > 0,
        "library package should hold return_unify orphans before node GC runs"
    );

    // After node GC: `PipelineStage::Gc` runs the closure-wide GC across the
    // whole reachable package closure. The foreign library package must already
    // be orphan-free, so a second manual GC over it reclaims nothing. If GC
    // reverted to entry-package-only, foreign orphans would survive and this
    // re-run would report removals.
    let (mut post_gc_store, post_user_pkg) =
        compile_and_run_pipeline_to_with_library(lib_source, user_source, PipelineStage::Gc);
    let post_lib_pkg = library_package_id(&post_gc_store, post_user_pkg, "TestLib");

    // Closure-wide arena integrity holds after the GC stage.
    crate::invariants::check(
        &post_gc_store,
        post_user_pkg,
        crate::invariants::InvariantLevel::PostGc,
    );

    let post_gc_removed = super::gc_unreachable(post_gc_store.get_mut(post_lib_pkg));
    assert_eq!(
        post_gc_removed, 0,
        "closure-wide node GC must tombstone foreign-package orphans"
    );
}
