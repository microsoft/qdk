// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! FIR arena garbage collection.
//!
//! Removes unreachable (orphaned) blocks, stmts, exprs, and pats from
//! a package's [`IndexMap`](qsc_data_structures::index_map::IndexMap) arenas
//! by tombstoning entries that are not reachable from any callable spec body
//! or the package entry expression.
//!
//! # When to run
//!
//! After all FIR transforms that create/orphan arena nodes have completed
//! and before [`exec_graph_rebuild`](crate::exec_graph_rebuild) reconstructs
//! execution graphs from the surviving FIR tree.
//!
//! # Correctness contract
//!
//! The sweep phase tombstones complete unreachable subgraphs: if a node is
//! unreachable, all of its descendants are also unreachable (because the
//! only paths to descendants go through ancestors). The mark phase records
//! every node it visits via the [`Visitor`] trait. The combination guarantees
//! that no surviving node references a tombstoned node, so
//! [`PackageLookup::get_*(..)`](qsc_fir::fir::PackageLookup) calls remain
//! safe.
//!
//! # Transformation shape
//!
//! **Before:** Package arenas contain orphaned blocks, stmts, exprs, and pats
//! left behind by earlier rewrite passes (return unify, defunctionalize, UDT
//! erase, SROA, argument promote).
//!
//! **After:** Only nodes reachable from callable bodies and the entry
//! expression survive. Orphaned entries are tombstoned in the `IndexMap`.

#[cfg(test)]
mod tests;

use qsc_fir::fir::{
    Block, BlockId, Expr, ExprId, Package, PackageLookup, Pat, PatId, Stmt, StmtId,
};
use qsc_fir::visit::{self, Visitor};
use rustc_hash::FxHashSet;

/// Tombstones unreachable blocks, stmts, exprs, and pats in the package's
/// `IndexMap` arenas. Returns the total number of entries removed.
///
/// "Unreachable" means: not visited by a [`Visitor`] walk starting from
/// every item in `package.items` and the `package.entry` expression.
/// Items themselves are never removed.
///
/// # When to call
///
/// After all FIR transforms that create or orphan arena nodes, and before
/// `exec_graph_rebuild`.
pub fn gc_unreachable(package: &mut Package) -> usize {
    let live = mark(package);
    sweep(package, &live)
}

/// Reachable-ID sets for each arena type.
struct LiveSets {
    blocks: FxHashSet<BlockId>,
    stmts: FxHashSet<StmtId>,
    exprs: FxHashSet<ExprId>,
    pats: FxHashSet<PatId>,
}

fn mark(package: &Package) -> LiveSets {
    let mut collector = ReachabilityCollector {
        package,
        live: LiveSets {
            blocks: FxHashSet::default(),
            stmts: FxHashSet::default(),
            exprs: FxHashSet::default(),
            pats: FxHashSet::default(),
        },
    };

    // Walk all items (callable spec bodies, including unreachable callables —
    // item-level DCE is a separate concern). This ensures every spec body's
    // nodes are marked live.
    for (_, item) in &package.items {
        collector.visit_item(item);
    }

    // Walk the entry expression tree (may reference nodes not reachable from
    // any callable spec body, e.g. top-level let bindings in the entry block).
    if let Some(entry_expr_id) = package.entry {
        collector.visit_expr(entry_expr_id);
    }

    collector.live
}

struct ReachabilityCollector<'a> {
    package: &'a Package,
    live: LiveSets,
}

impl<'a> Visitor<'a> for ReachabilityCollector<'a> {
    fn get_block(&self, id: BlockId) -> &'a Block {
        self.package.get_block(id)
    }

    fn get_expr(&self, id: ExprId) -> &'a Expr {
        self.package.get_expr(id)
    }

    fn get_pat(&self, id: PatId) -> &'a Pat {
        self.package.get_pat(id)
    }

    fn get_stmt(&self, id: StmtId) -> &'a Stmt {
        self.package.get_stmt(id)
    }

    fn visit_block(&mut self, id: BlockId) {
        if self.live.blocks.insert(id) {
            visit::walk_block(self, id);
        }
    }

    fn visit_stmt(&mut self, id: StmtId) {
        if self.live.stmts.insert(id) {
            visit::walk_stmt(self, id);
        }
    }

    fn visit_expr(&mut self, id: ExprId) {
        if self.live.exprs.insert(id) {
            visit::walk_expr(self, id);
        }
    }

    fn visit_pat(&mut self, id: PatId) {
        if self.live.pats.insert(id) {
            visit::walk_pat(self, id);
        }
    }
}

/// Deletes every arena node that was not marked live during `mark`.
///
/// Before, dead blocks, statements, expressions, and patterns still occupy the
/// package arenas and can keep stale ids addressable. After, only the nodes in
/// `live` remain and the returned count records how many entries were purged.
fn sweep(package: &mut Package, live: &LiveSets) -> usize {
    let mut removed = 0;

    package.blocks.retain(|id, _| {
        let keep = live.blocks.contains(&id);
        if !keep {
            removed += 1;
        }
        keep
    });
    package.stmts.retain(|id, _| {
        let keep = live.stmts.contains(&id);
        if !keep {
            removed += 1;
        }
        keep
    });
    package.exprs.retain(|id, _| {
        let keep = live.exprs.contains(&id);
        if !keep {
            removed += 1;
        }
        keep
    });
    package.pats.retain(|id, _| {
        let keep = live.pats.contains(&id);
        if !keep {
            removed += 1;
        }
        keep
    });

    removed
}
