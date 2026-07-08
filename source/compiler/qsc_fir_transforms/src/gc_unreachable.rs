// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! FIR arena garbage collection — runs after argument promotion, before item
//! DCE (and again after item DCE).
//!
//! Tombstones blocks, stmts, exprs, and pats in a package's `IndexMap` arenas
//! that are no longer reachable from any callable spec body or the entry
//! expression — the orphans left behind by the earlier rewrite passes. Items
//! are never removed (that is [`item_dce`](crate::item_dce)'s job).
//!
//! # What to know before diving in
//!
//! - **Mark-and-sweep correctness.** The mark phase records every node a
//!   [`Visitor`] walk visits; the sweep tombstones whole unreachable subgraphs
//!   (an unreachable node's descendants are also unreachable). Together this
//!   guarantees no surviving node references a tombstoned one, keeping
//!   [`PackageLookup`] `get_*` calls safe.
//! - **Takes `&mut Package`, not the `Assigner` tuple.** It only tombstones
//!   existing entries and never allocates fresh IDs, so — like the other
//!   tail metadata passes — it does not receive the pipeline-global
//!   [`Assigner`](qsc_fir::assigner::Assigner).

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
pub fn gc_unreachable(package: &mut Package) -> usize {
    let live = mark(package);
    sweep(package, &live)
}

/// Reachable-ID sets for each arena type.
#[derive(Debug, Default)]
struct LiveSets {
    blocks: FxHashSet<BlockId>,
    stmts: FxHashSet<StmtId>,
    exprs: FxHashSet<ExprId>,
    pats: FxHashSet<PatId>,
}

fn mark(package: &Package) -> LiveSets {
    let mut collector = ReachabilityCollector {
        package,
        live: LiveSets::default(),
    };

    // Walk all items, including unreachable callables; item-level DCE is a
    // separate concern. This marks every spec body's nodes live.
    for (_, item) in &package.items {
        collector.visit_item(item);
    }

    // Walk the entry expression tree, which may reference nodes not reachable
    // from any callable spec body, e.g. top-level let bindings in the entry block.
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
/// Only the nodes in `live` survive; the returned count records how many
/// entries were purged.
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
