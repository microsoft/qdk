// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Proptest applicability: Low — gc_unreachable operates on FIR arena nodes (mark-and-sweep),
// not on Q# semantics. Its correctness is a structural invariant (no surviving node references
// a tombstoned node) rather than behavioral equivalence. Q# template generation doesn't add
// much beyond targeted snapshots that create known orphan patterns.

use crate::PipelineStage;
use crate::test_utils::compile_and_run_pipeline_to;
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
fn gc_then_check_id_references_passes() {
    // A non-trivial program exercising multiple transform passes.
    // After GC, check_id_references (via PostAll invariants) should not panic.
    let source = indoc! {"
        namespace Test {
            operation ApplyIfOne(q : Qubit, op : Qubit => Unit) : Unit {
                op(q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                ApplyIfOne(q, H);
                if M(q) == One {
                    X(q);
                }
                Reset(q);
            }
        }
    "};
    // Run full pipeline — this runs GC then PostAll invariants (including check_id_references).
    let (_store, _pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Full);
    // If we reach here, check_id_references passed post-GC.
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
