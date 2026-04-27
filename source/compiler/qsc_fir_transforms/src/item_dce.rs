// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Item-level dead code elimination.
//!
//! After monomorphization and defunctionalization, many items become
//! unreachable: original generic callables replaced by monomorphized copies,
//! closure items fully specialized, etc. These unreachable items remain in
//! [`Package::items`](qsc_fir::fir::Package). This pass removes them.
//!
//! # Separation from `gc_unreachable`
//!
//! [`gc_unreachable`](crate::gc_unreachable) operates on arena nodes (blocks,
//! stmts, exprs, pats) within a single package. Item-level reachability is
//! cross-package (library items may be referenced from user code), so it
//! requires a [`PackageStore`](qsc_fir::fir::PackageStore) for the
//! reachability walk. This is why item DCE is a separate pass.
//!
//! # `StmtKind::Item` edge case
//!
//! `StmtKind::Item(local_item_id)` stmts declare items inside blocks. If item
//! DCE removes an item but its declaring `StmtKind::Item` stmt is still in a
//! reachable block, `invariants::check_id_references` will panic. The
//! pipeline mitigates this by re-running `gc_unreachable` after item DCE when
//! any items are removed — this tombstones the arena nodes (blocks, stmts,
//! exprs, pats) that belonged to the deleted items' bodies. The
//! `StmtKind::Item` stmts themselves survive as dangling references, which is
//! safe because `check_id_references` explicitly allows them post-DCE and
//! `exec_graph_rebuild` ignores `StmtKind::Item` stmts.
//!
//! # Transformation shape
//!
//! **Before:** `Package::items` contains unreachable callable items (original
//! generics replaced by monomorphized copies, fully-specialized closure items)
//! and dead type items left after UDT erasure.
//!
//! **After:** Unreachable items are removed from `Package::items`. If any
//! items were removed, `gc_unreachable` re-runs to tombstone their arena
//! nodes.

#[cfg(test)]
mod tests;

use qsc_fir::fir::{ItemKind, LocalItemId, Package, PackageId, Res, StoreItemId};
use rustc_hash::FxHashSet;

/// Eliminates unreachable items from the package's item map.
///
/// The `reachable` set should be the output of
/// [`collect_reachable_from_entry`](crate::reachability::collect_reachable_from_entry).
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
        if let ItemKind::Export(_name, res) = &item.kind {
            if let Res::Item(item_id) = res {
                if item_id.package == package_id {
                    local_reachable.insert(item_id.item);
                }
            }
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
