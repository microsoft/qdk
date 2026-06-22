// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::PipelineStage;
use crate::test_utils::{
    assert_panics_with, callable_id_by_name, compile_and_run_pipeline_to,
    compile_and_run_pipeline_to_with_library, compile_to_fir, compile_to_fir_with_library,
};
use indoc::indoc;
use qsc_data_structures::span::Span;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{ItemKind, PackageLookup};

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

/// Collects the names of all `Ty` (newtype) items in the user package.
fn ty_item_names(package: &qsc_fir::fir::Package) -> Vec<String> {
    package
        .items
        .iter()
        .filter_map(|(_, item)| match &item.kind {
            ItemKind::Ty(ident, _) => Some(ident.name.to_string()),
            ItemKind::Callable(_) => None,
        })
        .collect()
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
    let (store_before, pkg_id) =
        compile_and_run_pipeline_to(source, PipelineStage::TupleDecompose2);
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
    let (store_before, pkg_id) =
        compile_and_run_pipeline_to(source, PipelineStage::TupleDecompose2);
    let callable_count_before = callable_count(store_before.get(pkg_id));

    let (store_after, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ItemDce);
    let callable_count_after = callable_count(store_after.get(pkg_id));

    assert_eq!(
        callable_count_before, callable_count_after,
        "all callables reachable — nothing should be removed"
    );
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
            ItemKind::Ty(..) => None,
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
fn dce_removes_unreachable_generic_instantiations() {
    let source = indoc! {"
        namespace Test {
            function Id<'T>(x : 'T) : 'T { x }
            function Wrap<'T>(x : 'T) : 'T { Id(x) }
            @EntryPoint()
            function Main() : Int { Wrap(42) + Wrap(0) }
        }
    "};
    let (store_after, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ItemDce);
    let package = store_after.get(pkg_id);
    let names: Vec<String> = package
        .items
        .iter()
        .filter_map(|(_, item)| match &item.kind {
            ItemKind::Callable(decl) => Some(decl.name.name.to_string()),
            ItemKind::Ty(..) => None,
        })
        .collect();

    // Reachable entry survives.
    assert!(
        names.iter().any(|n| n == "Main"),
        "entry Main must survive DCE; remaining: {names:?}"
    );
    // Generic templates are unreachable after monomorphization → removed.
    assert!(
        !names.iter().any(|n| n == "Id" || n == "Wrap"),
        "generic Id/Wrap templates must be removed; remaining: {names:?}"
    );
    // Their monomorphized instantiations remain reachable from Main.
    assert!(
        names.iter().any(|n| n.starts_with("Id<")),
        "monomorphized Id<Int> must survive; remaining: {names:?}"
    );
    assert!(
        names.iter().any(|n| n.starts_with("Wrap<")),
        "monomorphized Wrap<Int> must survive; remaining: {names:?}"
    );
}

#[test]
fn dce_removes_unreachable_type_declarations() {
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
    let (store_before, pkg_id) =
        compile_and_run_pipeline_to(source, PipelineStage::TupleDecompose2);
    let items_before = item_count(store_before.get(pkg_id));
    // Before DCE the `Pair` newtype is present as a Ty item.
    assert!(
        ty_item_names(store_before.get(pkg_id)).contains(&"Pair".to_string()),
        "Pair newtype should exist before DCE"
    );

    let (store_after, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ItemDce);
    let items_after = item_count(store_after.get(pkg_id));

    assert!(
        items_after < items_before,
        "DCE should remove type items: before={items_before}, after={items_after}"
    );
    // The specific `Pair` Ty item is the one removed: after lowering, its field
    // accesses became tuple index ops, leaving the newtype declaration orphaned.
    assert!(
        !ty_item_names(store_after.get(pkg_id)).contains(&"Pair".to_string()),
        "the unreachable Pair newtype should be the removed Ty item"
    );
    // The reachable Main callable must survive.
    let _ = callable_id_by_name(store_after.get(pkg_id), "Main");
}

#[test]
fn dce_removes_unreachable_closure_and_generic() {
    let source = indoc! {"
        namespace Test {
            function Apply<'T>(f : 'T -> 'T, x : 'T) : 'T { f(x) }
            @EntryPoint()
            function Main() : Int { Apply(x -> x + 1, 5) }
        }
    "};
    let (store_after, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ItemDce);
    let package = store_after.get(pkg_id);
    let names: Vec<String> = package
        .items
        .iter()
        .filter_map(|(_, item)| match &item.kind {
            ItemKind::Callable(decl) => Some(decl.name.name.to_string()),
            ItemKind::Ty(..) => None,
        })
        .collect();

    // Reachable entry survives.
    assert!(
        names.iter().any(|n| n == "Main"),
        "entry Main must survive DCE; remaining: {names:?}"
    );
    // The generic HOF template is unreachable after monomorphization and
    // defunctionalization → removed.
    assert!(
        !names.iter().any(|n| n == "Apply"),
        "generic Apply template must be removed; remaining: {names:?}"
    );
    // A specialized/monomorphized Apply (the concrete callee reachable from
    // Main) survives.
    assert!(
        names.iter().any(|n| n != "Apply" && n.starts_with("Apply")),
        "a specialized Apply callable must survive; remaining: {names:?}"
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
/// `StmtKind::Item` references.
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
/// the downstream garbage-collection pass.
mod item_dce_contracts {
    use crate::package_assigners::PackageAssigners;

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
    fn temporary_dangling_refs_allowed() {
        let source = indoc! {"
            namespace Test {
                function Dead() : Int { 0 }
                @EntryPoint()
                function Main() : Int { 42 }
            }
        "};

        let (mut store, pkg_id) = compile_to_fir(source);
        let mut assigners = PackageAssigners::entry(&store, pkg_id);
        crate::monomorphize::monomorphize(&mut store, pkg_id, &mut assigners);
        let dead_id = callable_id_by_name(store.get(pkg_id), "Dead");
        let assigner = assigners.get_mut(&store, pkg_id);
        insert_item_stmt_in_main(&mut store, pkg_id, assigner, dead_id);
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

        crate::invariants::check(
            &store,
            pkg_id,
            crate::invariants::InvariantLevel::PostItemDce,
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

    let result = crate::run_pipeline_to_with_diagnostics(
        &mut store,
        pkg_id,
        PipelineStage::ItemDce,
        &[pinned_store_id],
    );
    assert!(result.is_success());

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

    let result = crate::run_pipeline_to_with_diagnostics(
        &mut store,
        pkg_id,
        PipelineStage::ItemDce,
        &[pinned_store_id],
    );
    assert!(result.is_success());

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

// ---------------------------------------------------------------------------
// Cross-package foreign item DCE.
//
// Whole-closure structural passes transform only the entry-reachable callables
// inside each foreign (library) package. The entry-unreachable foreign
// callables left behind still reference erased UDTs and pre-promotion
// signatures, so they must be removed to keep each library package internally
// consistent with its transformed reachable callables.
// ---------------------------------------------------------------------------

/// Finds a callable's `StoreItemId` by name across every package in the store.
fn find_callable_store_id(
    store: &qsc_fir::fir::PackageStore,
    name: &str,
) -> qsc_fir::fir::StoreItemId {
    for (package_id, package) in store {
        for (item_id, item) in package.items.iter() {
            if let ItemKind::Callable(decl) = &item.kind
                && decl.name.name.as_ref() == name
            {
                return qsc_fir::fir::StoreItemId {
                    package: package_id,
                    item: item_id,
                };
            }
        }
    }
    panic!("callable {name} not found in any package");
}

/// Whether a callable with the given name exists in `package`.
fn callable_exists(package: &qsc_fir::fir::Package, name: &str) -> bool {
    package.items.iter().any(|(_, item)| {
        matches!(&item.kind, ItemKind::Callable(decl) if decl.name.name.as_ref() == name)
    })
}

/// Whether a newtype with the given name exists in `package`.
fn ty_exists(package: &qsc_fir::fir::Package, name: &str) -> bool {
    package.items.iter().any(
        |(_, item)| matches!(&item.kind, ItemKind::Ty(ident, _) if ident.name.as_ref() == name),
    )
}

/// Finds a newtype's `StoreItemId` by name across every package in the store.
fn find_ty_store_id(store: &qsc_fir::fir::PackageStore, name: &str) -> qsc_fir::fir::StoreItemId {
    for (package_id, package) in store {
        for (item_id, item) in package.items.iter() {
            if let ItemKind::Ty(ident, _) = &item.kind
                && ident.name.as_ref() == name
            {
                return qsc_fir::fir::StoreItemId {
                    package: package_id,
                    item: item_id,
                };
            }
        }
    }
    panic!("newtype {name} not found in any package");
}

#[test]
fn foreign_dce_removes_unreachable_library_callable_and_udt() {
    // `DeadWithUdt` is unreachable from the user entry but references a library
    // newtype `Pair`. After the entry-rooted passes transform only the reachable
    // `LibUsed`, the stale `DeadWithUdt`/`Pair` would skew RCA/codegen. Foreign
    // DCE must remove both while keeping the reachable `LibUsed`.
    let lib = indoc! {"
        namespace Lib {
            operation LibUsed(q : Qubit) : Unit { X(q) }
            newtype Pair = (First : Int, Second : Int);
            operation DeadWithUdt(p : Pair) : Int { p::First + p::Second }
            export LibUsed, DeadWithUdt;
        }
    "};
    let user = indoc! {"
        import Lib.*;
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            LibUsed(q);
        }
    "};

    let (store, user_pkg_id) =
        compile_and_run_pipeline_to_with_library(lib, user, PipelineStage::Full);
    let lib_pkg_id = find_callable_store_id(&store, "LibUsed").package;
    let lib_package = store.get(lib_pkg_id);

    assert!(
        callable_exists(lib_package, "LibUsed"),
        "reachable library callable must survive foreign DCE"
    );
    assert!(
        !callable_exists(lib_package, "DeadWithUdt"),
        "unreachable library callable referencing an erased UDT must be removed"
    );
    assert!(
        !ty_exists(lib_package, "Pair"),
        "library newtype is unconditionally dead after udt_erase"
    );
    assert_ne!(
        lib_pkg_id, user_pkg_id,
        "library and user packages must be distinct"
    );
}

#[test]
fn foreign_dce_prunes_dead_non_pinned_library_callable() {
    // `DeadLibOp` is unreachable from the user entry and not pinned, so foreign
    // DCE removes it while the reachable `LibUsed` survives.
    let lib = indoc! {"
        namespace Lib {
            operation LibUsed(q : Qubit) : Unit { X(q) }
            operation DeadLibOp() : Int { 13 }
            export LibUsed, DeadLibOp;
        }
    "};
    let user = indoc! {"
        import Lib.*;
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            LibUsed(q);
        }
    "};

    let (store, _user_pkg_id) =
        compile_and_run_pipeline_to_with_library(lib, user, PipelineStage::Full);
    let lib_pkg_id = find_callable_store_id(&store, "LibUsed").package;
    let lib_package = store.get(lib_pkg_id);

    assert!(
        callable_exists(lib_package, "LibUsed"),
        "reachable library callable must survive foreign DCE"
    );
    assert!(
        !callable_exists(lib_package, "DeadLibOp"),
        "dead non-pinned library callable must be pruned"
    );
}

#[test]
fn pinned_library_item_survives_dce_in_foreign_package() {
    // `PinnedLibOp` is unreachable from the user entry. Pinning it (a library
    // `StoreItemId`) must keep it alive in its non-entry package.
    let lib = indoc! {"
        namespace Lib {
            operation LibUsed(q : Qubit) : Unit { X(q) }
            operation PinnedLibOp() : Int { 99 }
            export LibUsed, PinnedLibOp;
        }
    "};
    let user = indoc! {"
        import Lib.*;
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            LibUsed(q);
        }
    "};

    let (mut store, user_pkg_id) = compile_to_fir_with_library(lib, user);
    let pinned_store_id = find_callable_store_id(&store, "PinnedLibOp");

    let result = crate::run_pipeline_to_with_diagnostics(
        &mut store,
        user_pkg_id,
        PipelineStage::ItemDce,
        &[pinned_store_id],
    );
    assert!(result.is_success());

    let lib_package = store.get(pinned_store_id.package);
    assert!(
        lib_package.items.get(pinned_store_id.item).is_some(),
        "pinned library item must survive DCE in its non-entry package"
    );
}

#[test]
fn pinned_foreign_ty_panics_in_item_dce() {
    // Only callable items may be pinned. Foreign item DCE drops every `Ty`
    // unconditionally (sound because `udt_erase` precedes it and inlines every
    // UDT reference), so a pinned `Ty` would be silently dropped. The pass
    // asserts instead, turning the latent miscompile into a deterministic panic.
    let lib = indoc! {"
        namespace Lib {
            operation LibUsed(q : Qubit) : Unit { X(q) }
            newtype Pair = (First : Int, Second : Int);
            operation DeadWithUdt(p : Pair) : Int { p::First + p::Second }
            export LibUsed, DeadWithUdt;
        }
    "};
    let user = indoc! {"
        import Lib.*;
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            LibUsed(q);
        }
    "};

    let (mut store, user_pkg_id) = compile_to_fir_with_library(lib, user);
    let pair_ty_id = find_ty_store_id(&store, "Pair");
    assert_ne!(
        pair_ty_id.package, user_pkg_id,
        "the pinned newtype must live in a foreign (library) package"
    );
    let reachable = crate::reachability::collect_reachable_from_entry(&store, user_pkg_id);

    assert_panics_with("only callable items may be pinned", move || {
        crate::item_dce::eliminate_unreachable_foreign_items(
            &mut store,
            user_pkg_id,
            &reachable,
            &[pair_ty_id],
        );
    });
}

#[test]
fn library_item_reachable_only_via_pinned_seed_survives_with_transitive_deps() {
    // `PinnedLibOp` is unreachable from the user entry and calls `LibHelper`.
    // Pinning `PinnedLibOp` must keep both it and its transitive dependency
    // alive in the library package.
    let lib = indoc! {"
        namespace Lib {
            operation LibUsed(q : Qubit) : Unit { X(q) }
            operation PinnedLibOp() : Int { LibHelper() }
            operation LibHelper() : Int { 77 }
            export LibUsed, PinnedLibOp, LibHelper;
        }
    "};
    let user = indoc! {"
        import Lib.*;
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            LibUsed(q);
        }
    "};

    let (mut store, user_pkg_id) = compile_to_fir_with_library(lib, user);
    let pinned_store_id = find_callable_store_id(&store, "PinnedLibOp");
    let helper_store_id = find_callable_store_id(&store, "LibHelper");

    let result = crate::run_pipeline_to_with_diagnostics(
        &mut store,
        user_pkg_id,
        PipelineStage::ItemDce,
        &[pinned_store_id],
    );
    assert!(result.is_success());

    let lib_package = store.get(pinned_store_id.package);
    assert!(
        lib_package.items.get(pinned_store_id.item).is_some(),
        "pinned library seed must survive DCE in its non-entry package"
    );
    assert!(
        lib_package.items.get(helper_store_id.item).is_some(),
        "transitive dependency of a pinned library seed must survive DCE"
    );
}
