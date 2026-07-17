// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Item-level dead code elimination — runs after the tuple-decompose and
//! argument-promotion fixed point, before node-level GC and exec graph rebuild.
//!
//! Removes items from [`Package::items`](qsc_fir::fir::Package) that became
//! unreachable after monomorphization and defunctionalization (original
//! generics replaced by monomorphized copies, fully-specialized closure items)
//! plus dead type items left after UDT erasure.
//!
//! # What to know before diving in
//!
//! - **Separate from [`gc_unreachable`](crate::gc_unreachable) because
//!   reachability is cross-package.** Library items may be referenced from user
//!   code, so this needs a [`PackageStore`] for the
//!   walk, whereas `gc_unreachable` works on a single package's arena nodes.
//! - **`StmtKind::Item` edge case.** Removing an item whose declaring
//!   `StmtKind::Item` stmt sits in a still-reachable block would trip
//!   `invariants::check_id_references`. The pipeline mitigates by running
//!   `gc_unreachable` immediately after item DCE, tombstoning the deleted
//!   items' arena nodes. The `StmtKind::Item` stmts survive as harmless
//!   dangling references (allowed post-DCE; ignored by `exec_graph_rebuild`).
//! - Accepts entry-rooted or seed-expanded (pinned-callable) reachability.

#[cfg(test)]
mod tests;

use qsc_fir::fir::{ItemKind, LocalItemId, Package, PackageId, PackageStore, StoreItemId};
use rustc_hash::FxHashSet;

/// Eliminates unreachable items from the package's item map.
///
/// The `reachable` set should be the output of entry-rooted reachability or
/// seed-expanded reachability, such as
/// [`collect_reachable_from_entry`](crate::reachability::collect_reachable_from_entry)
/// or [`collect_reachable_with_seeds`](crate::reachability::collect_reachable_with_seeds).
/// Only items local to this package are considered; cross-package items in the
/// reachable set are ignored.
///
/// Type items are unconditionally removed: `udt_erase` (which must precede this
/// pass) inlined every UDT reference in the *reachable* callables, and this
/// pass drops the unreachable callables that may still reference a UDT, so no
/// surviving callable references a type item.
///
/// Returns the number of items removed.
#[allow(clippy::implicit_hasher)]
pub fn eliminate_dead_items(
    package_id: PackageId,
    package: &mut Package,
    reachable: &FxHashSet<StoreItemId>,
) -> usize {
    let local_reachable: FxHashSet<LocalItemId> = reachable
        .iter()
        .filter(|id| id.package == package_id)
        .map(|id| id.item)
        .collect();

    let mut removed = 0;
    package.items.retain(|id, item| {
        let keep = match &item.kind {
            // Callable items: keep only if reachable from entry.
            ItemKind::Callable(_) => local_reachable.contains(&id),
            // Type items: dead because `udt_erase` inlined every UDT reference
            // in the reachable callables, and the unreachable callables that
            // may still reference a UDT are dropped by the `Callable` arm above.
            ItemKind::Ty(..) => false,
        };
        if !keep {
            removed += 1;
        }
        keep
    });
    removed
}

/// Eliminates entry-unreachable items from every **foreign** (non-entry)
/// package reached by the entry closure.
///
/// Whole-closure structural passes transform only the entry-reachable callables
/// inside each foreign package, leaving each package internally inconsistent:
/// the unreachable callables still reference erased UDTs and pre-promotion
/// callable signatures. Because RCA and codegen analyze every item in every
/// package, those stale unreachable callables would be analyzed out of step with
/// the transformed reachable callables they call, producing arity/type-skew
/// panics. Removing them is correctness-required, not an optimization.
///
/// Unlike [`eliminate_dead_items`], this does **not** root on a foreign
/// package's own exports — a library package's public surface is not an entry
/// point for a closed codegen compilation, so an exported-but-entry-unreachable
/// callable is dead here. Pinned **callable** items (and their
/// transitive dependencies, already present in `reachable`) are kept in
/// whichever package they live in: `local_reachable` only retains the
/// `Callable` arm, so pinning a non-callable item (for example a `Ty`) is a
/// contract violation that [`eliminate_foreign_items_in_package`] rejects. The
/// FIR store the pipeline mutates is a throwaway codegen clone, so trimming a
/// library package's unreachable surface cannot affect any other compilation.
///
/// Returns the total number of items removed across all foreign packages.
pub fn eliminate_unreachable_foreign_items(
    store: &mut PackageStore,
    entry_package_id: PackageId,
    reachable: &FxHashSet<StoreItemId>,
    pinned_items: &[StoreItemId],
) -> usize {
    let foreign_packages: Vec<PackageId> =
        crate::reachability::collect_reachable_package_closure(entry_package_id, reachable)
            .into_iter()
            .filter(|package_id| *package_id != entry_package_id)
            .collect();

    let mut removed = 0;
    for foreign_id in foreign_packages {
        removed += eliminate_foreign_items_in_package(
            foreign_id,
            store.get_mut(foreign_id),
            reachable,
            pinned_items,
        );
    }
    removed
}

/// Removes entry-unreachable items from a single foreign package.
///
/// See [`eliminate_unreachable_foreign_items`] for the rooting rationale.
fn eliminate_foreign_items_in_package(
    package_id: PackageId,
    package: &mut Package,
    reachable: &FxHashSet<StoreItemId>,
    pinned_items: &[StoreItemId],
) -> usize {
    let mut local_reachable: FxHashSet<LocalItemId> = reachable
        .iter()
        .filter(|id| id.package == package_id)
        .map(|id| id.item)
        .collect();

    // Pinned items must survive DCE in whatever package they live in. Their
    // transitive callees are already part of `reachable` (seeded reachability),
    // so unioning the pins themselves is sufficient.
    //
    // Only **callable** items may be pinned. The `Ty` arm of the
    // retain below drops every type item unconditionally — sound because
    // `udt_erase` (which must precede this pass) inlined every UDT reference in
    // the reachable callables, and the `Callable` arm drops the unreachable
    // callables that may still reference a UDT. A pinned `Ty` would therefore
    // be dropped despite its pin, so reject that contract violation
    // deterministically rather than silently miscompiling.
    for pin in pinned_items {
        if pin.package == package_id {
            assert!(
                !matches!(
                    package.items.get(pin.item).map(|item| &item.kind),
                    Some(ItemKind::Ty(..))
                ),
                "item DCE: pinned foreign item {:?} is a `Ty`; only callable items may be pinned",
                pin.item
            );
            local_reachable.insert(pin.item);
        }
    }

    let mut removed = 0;
    package.items.retain(|id, item| {
        let keep = match &item.kind {
            // Callable items: keep only if entry-reachable (no export rooting).
            ItemKind::Callable(_) => local_reachable.contains(&id),
            // Type items: dead because `udt_erase` (which must precede this
            // pass) inlined every UDT reference in the reachable callables, and
            // the `Callable` arm above drops the unreachable callables that may
            // still reference a UDT; the pin loop above already rejected any
            // pinned `Ty`.
            ItemKind::Ty(..) => false,
        };
        if !keep {
            removed += 1;
        }
        keep
    });
    removed
}
