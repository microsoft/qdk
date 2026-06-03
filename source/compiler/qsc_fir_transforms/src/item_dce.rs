// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Item-level dead code elimination — runs after GC, before exec graph
//! rebuild.
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
//!   code, so this needs a [`PackageStore`](qsc_fir::fir::PackageStore) for the
//!   walk, whereas `gc_unreachable` works on a single package's arena nodes.
//! - **`StmtKind::Item` edge case.** Removing an item whose declaring
//!   `StmtKind::Item` stmt sits in a still-reachable block would trip
//!   `invariants::check_id_references`. The pipeline mitigates by re-running
//!   `gc_unreachable` after item DCE when anything was removed, tombstoning the
//!   deleted items' arena nodes. The `StmtKind::Item` stmts survive as harmless
//!   dangling references (allowed post-DCE; ignored by `exec_graph_rebuild`).
//! - Accepts entry-rooted or seed-expanded (pinned-callable) reachability.

#[cfg(test)]
mod tests;

use qsc_fir::fir::{ItemKind, LocalItemId, Package, PackageId, Res, StoreItemId};
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
/// Type items are unconditionally removed (dead after `udt_erase`). Namespace
/// and export items are structural and always preserved.
///
/// Export targets that resolve to local callables are marked reachable so the
/// preserved exports cannot point at removed items.
///
/// Returns the number of items removed.
#[allow(clippy::implicit_hasher)]
pub fn eliminate_dead_items(
    package_id: PackageId,
    package: &mut Package,
    reachable: &FxHashSet<StoreItemId>,
) -> usize {
    let mut local_reachable: FxHashSet<LocalItemId> = reachable
        .iter()
        .filter(|id| id.package == package_id)
        .map(|id| id.item)
        .collect();

    // Mark export targets that resolve to local callables as reachable so
    // the preserved exports don't point at removed items. Cross-package
    // export targets and unresolved (Res::Err) exports are ignored.
    for item in package.items.values() {
        if let ItemKind::Export(_name, Res::Item(item_id)) = &item.kind
            && item_id.package == package_id
        {
            local_reachable.insert(item_id.item);
        }
    }

    let mut removed = 0;
    package.items.retain(|id, item| {
        let keep = match &item.kind {
            // Callable items: keep only if reachable from entry or an export target.
            ItemKind::Callable(_) => local_reachable.contains(&id),
            // Type items: unconditionally dead after `udt_erase`.
            ItemKind::Ty(..) => false,
            // Namespace and export items: structural, always preserved.
            ItemKind::Namespace(..) | ItemKind::Export(..) => true,
        };
        if !keep {
            removed += 1;
        }
        keep
    });
    removed
}
