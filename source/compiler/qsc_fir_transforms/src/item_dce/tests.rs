// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::PipelineStage;
use crate::test_utils::{compile_and_run_pipeline_to, compile_to_fir};
use indoc::indoc;
use qsc_data_structures::span::Span;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{Ident, Item, ItemId, ItemKind, LocalVarId, PackageLookup, Res, Visibility};
use std::rc::Rc;

/// Counts total items in the user package.
fn item_count(package: &qsc_fir::fir::Package) -> usize {
    package.items.iter().count()
}

/// Counts callable items in the user package.
fn callable_count(package: &qsc_fir::fir::Package) -> usize {
    package
        .items
        .iter()
        .filter(|(_, item)| matches!(item.kind, ItemKind::Callable(_)))
        .count()
}

fn callable_id_by_name(package: &qsc_fir::fir::Package, name: &str) -> qsc_fir::fir::LocalItemId {
    package
        .items
        .iter()
        .find_map(|(item_id, item)| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == name => Some(item_id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("callable {name} should exist"))
}

fn make_export_item(
    export_id: qsc_fir::fir::LocalItemId,
    package_id: qsc_fir::fir::PackageId,
    target_id: qsc_fir::fir::LocalItemId,
) -> Item {
    Item {
        id: export_id,
        span: Span::default(),
        parent: None,
        doc: Rc::from(""),
        attrs: vec![],
        visibility: Visibility::Public,
        kind: ItemKind::Export(
            Ident {
                id: LocalVarId::default(),
                span: Span::default(),
                name: Rc::from("ExportedHelper"),
            },
            Res::Item(ItemId {
                package: package_id,
                item: target_id,
            }),
        ),
    }
}

#[test]
fn dce_removes_unreachable_generic_after_monomorphize() {
    // After monomorphization, the original generic callable is unreachable
    // because it has been replaced by monomorphized copies.
    let source = indoc! {"
        namespace Test {
            function Id<'T>(x : 'T) : 'T { x }
            @EntryPoint()
            function Main() : Int { Id(42) }
        }
    "};
    let (store_before, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Gc);
    let items_before = item_count(store_before.get(pkg_id));

    let (store_after, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ItemDce);
    let items_after = item_count(store_after.get(pkg_id));

    assert!(
        items_after < items_before,
        "item DCE should remove unreachable items: before={items_before}, after={items_after}"
    );
}

#[test]
fn dce_preserves_all_reachable_items() {
    // A minimal program where every callable item is reachable.
    let source = indoc! {"
        namespace Test {
            @EntryPoint()
            function Main() : Int { 42 }
        }
    "};
    let (store_before, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Gc);
    let callable_count_before = callable_count(store_before.get(pkg_id));

    let (store_after, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ItemDce);
    let callable_count_after = callable_count(store_after.get(pkg_id));

    assert_eq!(
        callable_count_before, callable_count_after,
        "all callables reachable — nothing should be removed"
    );
}

#[test]
fn dce_with_closure_passes_invariants() {
    // Closures produce StmtKind::Item declarations in outer blocks.
    // After defunc these become specialized items; the original closure item
    // may become unreachable. ItemDce + cascading GC should keep invariants
    // clean.
    let source = indoc! {"
        namespace Test {
            function Apply(f : Int -> Int, x : Int) : Int { f(x) }
            @EntryPoint()
            function Main() : Int { Apply(x -> x + 1, 5) }
        }
    "};
    // Running through Full exercises ItemDce + cascading GC + invariants.
    let (_store, _pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Full);
    // If we reach here, post-DCE invariants (including check_id_references)
    // passed after cascading GC cleaned up any orphaned StmtKind::Item stmts.
}

#[test]
fn dce_on_entry_less_package_is_noop() {
    // Library packages have no entry expression. The pipeline guards against
    // calling collect_reachable_from_entry (which panics) on entry-less
    // packages. Verify the guard works by running the full pipeline — core
    // and std are entry-less, and they must survive untouched.
    let source = indoc! {"
        namespace Test {
            @EntryPoint()
            function Main() : Unit {}
        }
    "};
    let (store, _pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Full);
    // The core package has no entry expression and should still have items.
    let core_id = qsc_fir::fir::PackageId::CORE;
    assert!(
        store.get(core_id).entry.is_none(),
        "core package should have no entry expression"
    );
    assert!(
        item_count(store.get(core_id)) > 0,
        "core package items should be untouched by item DCE"
    );
}

#[test]
fn dce_removes_generic_after_pipeline() {
    // Non-trivial program exercising multiple transform passes.
    // After ItemDce, unreachable original generic callables should be removed.
    let source = indoc! {"
        namespace Test {
            function Id<'T>(x : 'T) : 'T { x }
            operation ApplyOp(q : Qubit, op : Qubit => Unit) : Unit { op(q); }
            @EntryPoint()
            operation Main() : Unit {
                let x = Id(42);
                use q = Qubit();
                ApplyOp(q, H);
                if M(q) == One {
                    X(q);
                }
                Reset(q);
            }
        }
    "};
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ItemDce);
    // Verify the original generic Id callable was removed — the monomorphized
    // copy Id<Int> should remain.
    let package = store.get(pkg_id);
    let remaining_names: Vec<_> = package
        .items
        .iter()
        .filter_map(|(_, item)| match &item.kind {
            ItemKind::Callable(decl) => Some(decl.name.name.to_string()),
            _ => None,
        })
        .collect();
    assert!(
        !remaining_names.iter().any(|n| n == "Id"),
        "generic Id should be removed; remaining: {remaining_names:?}"
    );
    assert!(
        remaining_names.iter().any(|n| n.starts_with("Id<")),
        "monomorphized Id<Int> should survive; remaining: {remaining_names:?}"
    );
}

#[test]
fn dce_benchmark_generic_multiple_instantiations() {
    let source = indoc! {"
        namespace Test {
            function Id<'T>(x : 'T) : 'T { x }
            function Wrap<'T>(x : 'T) : 'T { Id(x) }
            @EntryPoint()
            function Main() : Int { Wrap(42) + Wrap(0) }
        }
    "};
    let (store_before, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Gc);
    let items_before = item_count(store_before.get(pkg_id));

    let (store_after, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ItemDce);
    let items_after = item_count(store_after.get(pkg_id));

    assert!(
        items_after < items_before,
        "DCE should reduce items: before={items_before}, after={items_after}"
    );
    let callables_after = callable_count(store_after.get(pkg_id));
    assert!(
        callables_after > 0,
        "monomorphized callables should survive: {callables_after}"
    );
}

#[test]
fn dce_benchmark_type_declarations_removed() {
    let source = indoc! {"
        namespace Test {
            newtype Pair = (First : Int, Second : Int);
            @EntryPoint()
            function Main() : Int {
                let p = Pair(1, 2);
                p::First + p::Second
            }
        }
    "};
    let (store_before, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Gc);
    let items_before = item_count(store_before.get(pkg_id));

    let (store_after, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ItemDce);
    let items_after = item_count(store_after.get(pkg_id));

    assert!(
        items_after < items_before,
        "DCE should remove type items: before={items_before}, after={items_after}"
    );
}

#[test]
fn dce_benchmark_closure_and_generic() {
    let source = indoc! {"
        namespace Test {
            function Apply<'T>(f : 'T -> 'T, x : 'T) : 'T { f(x) }
            @EntryPoint()
            function Main() : Int { Apply(x -> x + 1, 5) }
        }
    "};
    let (store_before, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Gc);
    let items_before = item_count(store_before.get(pkg_id));

    let (store_after, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ItemDce);
    let items_after = item_count(store_after.get(pkg_id));

    assert!(
        items_after < items_before,
        "DCE should reduce items with closures+generics: before={items_before}, after={items_after}"
    );
    let callables_after = callable_count(store_after.get(pkg_id));
    assert!(
        callables_after > 0,
        "specialized callables should survive: {callables_after}"
    );
}

#[test]
fn dce_preserves_namespace_items() {
    let source = indoc! {"
        namespace Test {
            @EntryPoint()
            function Main() : Int { 42 }
        }
    "};
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ItemDce);
    let package = store.get(pkg_id);
    let has_namespace = package
        .items
        .iter()
        .any(|(_, item)| matches!(item.kind, ItemKind::Namespace(..)));
    assert!(has_namespace, "namespace items must survive DCE");
}

#[test]
fn dce_preserves_export_targets() {
    let source = indoc! {"
        namespace Test {
            function Helper() : Int { 42 }
            function Dead() : Int { 0 }
            @EntryPoint()
            function Main() : Int { 1 }
        }
    "};
    let (mut store, pkg_id) = crate::test_utils::compile_to_fir(source);
    let helper_id = callable_id_by_name(store.get(pkg_id), "Helper");
    let dead_id = callable_id_by_name(store.get(pkg_id), "Dead");
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    let export_id = assigner.next_item();

    store
        .get_mut(pkg_id)
        .items
        .insert(export_id, make_export_item(export_id, pkg_id, helper_id));

    let reachable = crate::reachability::collect_reachable_from_entry(&store, pkg_id);
    assert!(
        !reachable.contains(&qsc_fir::fir::StoreItemId {
            package: pkg_id,
            item: helper_id,
        }),
        "Helper should be unreachable except through the export"
    );

    crate::item_dce::eliminate_dead_items(pkg_id, store.get_mut(pkg_id), &reachable);
    let package = store.get(pkg_id);

    assert!(
        package.items.contains_key(helper_id),
        "export target callable should survive DCE"
    );
    assert!(
        !package.items.contains_key(dead_id),
        "unexported unreachable callable should still be removed"
    );

    let export = package.get_item(export_id);
    let ItemKind::Export(_, Res::Item(target)) = &export.kind else {
        panic!("export item should survive with an item target")
    };
    assert_eq!(target.package, pkg_id);
    assert_eq!(target.item, helper_id);
    assert!(
        package.items.contains_key(target.item),
        "export target should not dangle after DCE"
    );
}

#[test]
fn item_dce_is_idempotent() {
    let source = indoc! {"
        namespace Test {
            function Id<'T>(x : 'T) : 'T { x }
            @EntryPoint()
            function Main() : Int { Id(42) }
        }
    "};
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ItemDce);
    let items_after_first = item_count(store.get(pkg_id));

    let reachable = crate::reachability::collect_reachable_from_entry(&store, pkg_id);
    let removed = crate::item_dce::eliminate_dead_items(pkg_id, store.get_mut(pkg_id), &reachable);
    assert_eq!(removed, 0, "second item_dce run should remove nothing");
    assert_eq!(
        item_count(store.get(pkg_id)),
        items_after_first,
        "item count should be unchanged after second item_dce run"
    );
}

/// Tests validating `item_dce`'s fragile contract regarding temporary dangling
/// `StmtKind::Item` references and export retention.
///
/// # Contract Summary
///
/// After `item_dce` removes dead items, the declaring `StmtKind::Item` statements
/// may remain in reachable blocks, creating temporary dangling references. This is
/// **intentional and safe** because:
///
/// - **`check_id_references` explicitly allows dangling `StmtKind::Item` references
///   post-DCE.** See [`crate::invariants::check_id_references`] for details.
/// - **`exec_graph_rebuild` ignores `StmtKind::Item` statements**, so dangling refs
///   never participate in execution-graph construction.
/// - **The pipeline cascades `gc_unreachable` after `item_dce`** to tombstone arena
///   nodes belonging to deleted items. This repairs the dangling references by
///   cleaning up the statements.
///
/// This is a **staged-invariant design**: `item_dce` operates only at the item
/// (declaration) level; node-level (block/stmt/expr arena) cleanup is deferred to
/// the downstream garbage-collection pass. Export targets that resolve to local
/// callables are marked reachable by `item_dce` to prevent dangling exports, while
/// unresolved exports are unconditionally preserved.
mod item_dce_contracts {
    use super::*;

    fn dangling_item_refs(package: &qsc_fir::fir::Package) -> Vec<qsc_fir::fir::LocalItemId> {
        let mut refs = Vec::new();
        for stmt in package.stmts.values() {
            if let qsc_fir::fir::StmtKind::Item(item_id) = &stmt.kind
                && package.items.get(*item_id).is_none()
            {
                refs.push(*item_id);
            }
        }
        refs.sort();
        refs
    }

    fn insert_item_stmt_in_main(
        store: &mut qsc_fir::fir::PackageStore,
        pkg_id: qsc_fir::fir::PackageId,
        assigner: &mut Assigner,
        item_id: qsc_fir::fir::LocalItemId,
    ) {
        let stmt_id = assigner.next_stmt();
        let package = store.get_mut(pkg_id);
        package.stmts.insert(
            stmt_id,
            qsc_fir::fir::Stmt {
                id: stmt_id,
                span: Span::default(),
                kind: qsc_fir::fir::StmtKind::Item(item_id),
                exec_graph_range: crate::EMPTY_EXEC_RANGE,
            },
        );

        let main_id = callable_id_by_name(package, "Main");
        let main_item = package.get_item(main_id);
        let ItemKind::Callable(main_decl) = &main_item.kind else {
            panic!("Main should be callable");
        };
        let qsc_fir::fir::CallableImpl::Spec(spec) = &main_decl.implementation else {
            panic!("Main should have a body spec");
        };
        let main_block = spec.body.block;
        package
            .blocks
            .get_mut(main_block)
            .expect("Main body block should exist")
            .stmts
            .insert(0, stmt_id);
    }

    /// Validates that `item_dce` removes dead callables while preserving the
    /// pipeline's ability to handle temporary dangling `StmtKind::Item` references.
    ///
    /// # Contract Being Tested
    ///
    /// - Dead callables are removed from `Package::items`.
    /// - A dead callable declared via `StmtKind::Item` in a reachable block
    ///   becomes a dangling reference temporarily.
    /// - The dangling reference is safe: `check_id_references` post-DCE allows it, and
    ///   `exec_graph_rebuild` ignores `StmtKind::Item` statements.
    /// - The pipeline repairs it by cascading `gc_unreachable` after `item_dce`.
    #[test]
    fn test_temporary_dangling_refs_allowed() {
        let source = indoc! {"
            namespace Test {
                function Dead() : Int { 0 }
                @EntryPoint()
                function Main() : Int { 42 }
            }
        "};

        let (mut store, pkg_id) = compile_to_fir(source);
        let mut assigner = Assigner::from_package(store.get(pkg_id));
        crate::monomorphize::monomorphize(&mut store, pkg_id, &mut assigner);
        let dead_id = callable_id_by_name(store.get(pkg_id), "Dead");
        insert_item_stmt_in_main(&mut store, pkg_id, &mut assigner, dead_id);
        assert!(
            dangling_item_refs(store.get(pkg_id)).is_empty(),
            "pre-DCE package should not yet contain dangling item refs"
        );

        // Directly invoke item_dce without cascading gc_unreachable.
        let reachable = crate::reachability::collect_reachable_from_entry(&store, pkg_id);
        let removed =
            crate::item_dce::eliminate_dead_items(pkg_id, store.get_mut(pkg_id), &reachable);

        // Verify the dead item was removed.
        assert!(
            removed > 0,
            "dead callable should have been removed by item_dce"
        );

        assert!(
            !dangling_item_refs(store.get(pkg_id)).is_empty(),
            "direct item_dce should leave a temporary dangling StmtKind::Item ref"
        );

        // Verify that reachable items (Main) still exist.
        let package = store.get(pkg_id);
        let has_main = package.items.iter().any(|(_, item)| {
            matches!(&item.kind, ItemKind::Callable(decl) if decl.name.name.as_ref() == "Main")
        });
        assert!(
            has_main,
            "reachable callable 'Main' should survive item_dce"
        );

        crate::invariants::check(&store, pkg_id, crate::invariants::InvariantLevel::PostGc);
    }

    /// Validates that `item_dce` preserves exports and marks their resolution targets as
    /// reachable, preventing dangling export targets.
    ///
    /// # Contract Being Tested
    ///
    /// - Export items (structural) are always preserved.
    /// - Export targets that resolve to local callables are marked reachable so the
    ///   preserved export cannot point at a removed item.
    /// - Unresolved export targets (`Res::Err`) are tolerated and do not cause removal
    ///   of the export itself.
    #[test]
    fn test_export_retention_with_unresolved_targets() {
        let source = indoc! {"
            namespace Test {
                function Helper() : Int { 42 }
                @EntryPoint()
                function Main() : Int { 1 }
            }
        "};

        // Compile to FIR and monomorphize.
        let (mut store, pkg_id) = compile_to_fir(source);
        let mut assigner = Assigner::from_package(store.get(pkg_id));
        crate::monomorphize::monomorphize(&mut store, pkg_id, &mut assigner);

        // Manually create an export with an unresolved target to validate the contract.
        let export_id = assigner.next_item();
        store.get_mut(pkg_id).items.insert(
            export_id,
            Item {
                id: export_id,
                span: Span::default(),
                parent: None,
                doc: Rc::from(""),
                attrs: vec![],
                visibility: Visibility::Public,
                kind: ItemKind::Export(
                    Ident {
                        id: LocalVarId::default(),
                        span: Span::default(),
                        name: Rc::from("UnresolvedExport"),
                    },
                    Res::Err, // Unresolved target
                ),
            },
        );

        let items_before = item_count(store.get(pkg_id));

        // Run item_dce.
        let reachable = crate::reachability::collect_reachable_from_entry(&store, pkg_id);
        crate::item_dce::eliminate_dead_items(pkg_id, store.get_mut(pkg_id), &reachable);

        let package = store.get(pkg_id);

        // Contract validation 1: export items are always preserved.
        assert!(
            package.items.contains_key(export_id),
            "export with unresolved target must be retained"
        );

        // Contract validation 2: export structure is unchanged.
        let ItemKind::Export(export_name, export_res) = &package.get_item(export_id).kind else {
            panic!("export_id should still be an export item");
        };
        assert_eq!(
            export_name.name.as_ref(),
            "UnresolvedExport",
            "export name should be preserved"
        );
        assert!(
            matches!(export_res, Res::Err),
            "unresolved target should remain unresolved after item_dce"
        );

        // Verify that DCE still removes truly dead items (any garbage not exported or reachable).
        // The items_before count includes the unresolved export, Main, and possibly others.
        // We just verify the export survived; DCE logic is tested elsewhere.
        assert!(
            item_count(store.get(pkg_id)) <= items_before,
            "item count should not increase after item_dce"
        );
    }

    #[test]
    fn dce_surviving_stmtitem_refs_are_valid() {
        // Regression test: Verify StmtKind::Item refs point to valid items after DCE.
        //
        // Invariant: After item DCE, all surviving StmtKind::Item references within
        // reachable callable bodies must reference items that still exist in the package.
        // No dangling references should remain.
        let source = indoc! {"
            namespace Test {
                operation Dead() : Unit { }
                operation Alive() : Unit {
                    Dead();
                }
                @EntryPoint()
                operation Main() : Unit {
                    Alive();
                }
            }
        "};

        let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ItemDce);
        let package = store.get(pkg_id);

        // Collect all reachable items
        let reachable = crate::reachability::collect_reachable_from_entry(&store, pkg_id);
        let reachable_local: Vec<_> = reachable
            .iter()
            .filter_map(|id| {
                if id.package == pkg_id {
                    Some(id.item)
                } else {
                    None
                }
            })
            .collect();

        // Verify: For each reachable callable, all StmtKind::Item refs point to valid items
        for local_item_id in reachable_local {
            if let ItemKind::Callable(callable) = &package.get_item(local_item_id).kind {
                let spec = match &callable.implementation {
                    qsc_fir::fir::CallableImpl::Spec(spec_impl) => &spec_impl.body,
                    qsc_fir::fir::CallableImpl::SimulatableIntrinsic(spec) => spec,
                    qsc_fir::fir::CallableImpl::Intrinsic => continue,
                };

                // Collect all statements in the callable body block
                let block = package.get_block(spec.block);
                for stmt_id in &block.stmts {
                    let stmt = package.get_stmt(*stmt_id);
                    if let qsc_fir::fir::StmtKind::Item(item_ref) = &stmt.kind {
                        assert!(
                            package.items.contains_key(*item_ref),
                            "StmtKind::Item reference {item_ref:?} points to non-existent item after DCE"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn pinned_item_survives_item_dce() {
    let (mut store, pkg_id) = compile_to_fir(indoc! {"
        namespace Test {
            @EntryPoint()
            operation Main() : Int { 42 }
            // Unreachable from entry but will be pinned
            operation Pinned() : Int { 99 }
        }
    "});
    let package = store.get(pkg_id);
    let pinned_local = callable_id_by_name(package, "Pinned");
    let pinned_store_id = qsc_fir::fir::StoreItemId {
        package: pkg_id,
        item: pinned_local,
    };

    let errors = crate::run_pipeline_to(
        &mut store,
        pkg_id,
        PipelineStage::ItemDce,
        &[pinned_store_id],
    );
    assert!(errors.is_empty());

    // Pinned item should survive DCE.
    let package = store.get(pkg_id);
    assert!(
        package.items.get(pinned_local).is_some(),
        "pinned item should survive DCE"
    );
}

#[test]
fn pinned_item_transitive_deps_survive_item_dce() {
    let (mut store, pkg_id) = compile_to_fir(indoc! {"
        namespace Test {
            @EntryPoint()
            operation Main() : Int { 42 }
            // Unreachable from entry but will be pinned
            operation Pinned() : Int { Helper() }
            // Transitive dep of Pinned, also unreachable from entry
            operation Helper() : Int { 77 }
        }
    "});
    let package = store.get(pkg_id);
    let pinned_local = callable_id_by_name(package, "Pinned");
    let helper_local = callable_id_by_name(package, "Helper");
    let pinned_store_id = qsc_fir::fir::StoreItemId {
        package: pkg_id,
        item: pinned_local,
    };

    let errors = crate::run_pipeline_to(
        &mut store,
        pkg_id,
        PipelineStage::ItemDce,
        &[pinned_store_id],
    );
    assert!(errors.is_empty());

    // Both pinned item and its transitive dep should survive DCE.
    let package = store.get(pkg_id);
    assert!(
        package.items.get(pinned_local).is_some(),
        "pinned item should survive DCE"
    );
    assert!(
        package.items.get(helper_local).is_some(),
        "transitive dependency of pinned item should survive DCE"
    );
}
