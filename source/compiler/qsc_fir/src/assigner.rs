// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! FIR node-ID allocator.
//!
//! [`Assigner`] hands out monotonically increasing IDs for each FIR ID type:
//! `BlockId`, `ExprId`, `PatId`, `StmtId`, `LocalVarId`, and `LocalItemId`.
//! Every `next_*()` returns the current value and advances the counter; IDs are
//! **never reused or decremented**.
//!
//! # Reseeding over an existing package
//!
//! [`Assigner::from_package`] advances every counter past the maximum ID
//! already present in a lowered [`Package`], so transform passes that
//! synthesize new nodes never collide with existing ones. Blocks, exprs, pats,
//! stmts, and items are read from their arenas; local-var IDs have no arena and
//! are recovered by scanning `PatKind::Bind` and local/closure `ExprKind`s. The
//! `set_next_*()` methods expose the same per-counter reseeding directly.
//!
//! `stash_local`/`reset_local` save and restore the local-var counter so each
//! callable can number its locals from zero.
//!
//! # Append-only arenas
//!
//! Because IDs are never reused, FIR arena entries are append-only: rewrite
//! passes add and mutate nodes but leave superseded ones behind as unreachable
//! "orphans". Arena `iter()` skips empty/tombstoned (`None`) slots but not
//! orphans, since an orphan is still a populated entry — just no longer
//! reachable from any item or the package entry. The dedicated `gc_unreachable`
//! pass in `qsc_fir_transforms` tombstones orphans (`Some` → `None`) so later
//! `iter()` walks skip them; until it runs, code that iterates an arena
//! directly must filter to reachable nodes itself (as in `qsc_rca::common`).

use crate::fir::{
    BlockId, ExprId, ExprKind, LocalItemId, LocalVarId, Package, PatId, PatKind, Res, StmtId,
};

#[derive(Debug)]
pub struct Assigner {
    next_block: BlockId,
    next_expr: ExprId,
    next_pat: PatId,
    next_stmt: StmtId,
    next_local: LocalVarId,
    stashed_local: LocalVarId,
    next_item: LocalItemId,
}

impl Assigner {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_block: BlockId::default(),
            next_expr: ExprId::default(),
            next_pat: PatId::default(),
            next_stmt: StmtId::default(),
            next_local: LocalVarId::default(),
            stashed_local: LocalVarId::default(),
            next_item: LocalItemId::default(),
        }
    }

    pub fn next_block(&mut self) -> BlockId {
        let id = self.next_block;
        self.next_block = id.successor();
        id
    }

    pub fn next_expr(&mut self) -> ExprId {
        let id = self.next_expr;
        self.next_expr = id.successor();
        id
    }

    pub fn next_pat(&mut self) -> PatId {
        let id = self.next_pat;
        self.next_pat = id.successor();
        id
    }

    pub fn next_stmt(&mut self) -> StmtId {
        let id = self.next_stmt;
        self.next_stmt = id.successor();
        id
    }

    pub fn next_local(&mut self) -> LocalVarId {
        let id = self.next_local;
        self.next_local = id.successor();
        id
    }

    pub fn next_item(&mut self) -> LocalItemId {
        let id = self.next_item;
        self.next_item = id.successor();
        id
    }

    pub fn set_next_block(&mut self, id: BlockId) {
        self.next_block = id;
    }

    pub fn set_next_expr(&mut self, id: ExprId) {
        self.next_expr = id;
    }

    pub fn set_next_pat(&mut self, id: PatId) {
        self.next_pat = id;
    }

    pub fn set_next_stmt(&mut self, id: StmtId) {
        self.next_stmt = id;
    }

    pub fn set_next_local(&mut self, id: LocalVarId) {
        self.next_local = id;
    }

    pub fn set_next_item(&mut self, id: LocalItemId) {
        self.next_item = id;
    }

    pub fn stash_local(&mut self) {
        self.stashed_local = self.next_local;
        self.next_local = LocalVarId::default();
    }

    pub fn reset_local(&mut self) {
        self.next_local = self.stashed_local;
        self.stashed_local = LocalVarId::default();
    }

    /// Creates an `Assigner` whose counters are advanced past the maximum
    /// existing IDs in `package`.
    #[must_use]
    pub fn from_package(package: &Package) -> Self {
        let mut assigner = Self::new();

        // BlockId
        let max_block = package.blocks.iter().next_back();
        if let Some((max, _)) = max_block {
            assigner.set_next_block(max.successor());
        }

        // ExprId
        let max_expr = package.exprs.iter().next_back();
        if let Some((max, _)) = max_expr {
            assigner.set_next_expr(max.successor());
        }

        // PatId
        let max_pat = package.pats.iter().next_back();
        if let Some((max, _)) = max_pat {
            assigner.set_next_pat(max.successor());
        }

        // StmtId
        let max_stmt = package.stmts.iter().next_back();
        if let Some((max, _)) = max_stmt {
            assigner.set_next_stmt(max.successor());
        }

        // LocalVarId — scan PatKind::Bind, ExprKind::Var(Res::Local),
        // ExprKind::Closure
        let mut max_local: u32 = 0;
        for (_, pat) in &package.pats {
            if let PatKind::Bind(ident) = &pat.kind {
                let v: u32 = ident.id.into();
                max_local = max_local.max(v);
            }
        }
        for (_, expr) in &package.exprs {
            if let ExprKind::Var(Res::Local(var), _) = &expr.kind {
                let v: u32 = (*var).into();
                max_local = max_local.max(v);
            }
            if let ExprKind::Closure(vars, _) = &expr.kind {
                for var in vars {
                    let v: u32 = (*var).into();
                    max_local = max_local.max(v);
                }
            }
        }
        assigner.set_next_local(LocalVarId::from(max_local + 1));

        // LocalItemId — scan package.items keys
        let max_item = package.items.iter().next_back();
        if let Some((max, _)) = max_item {
            assigner.set_next_item(max.successor());
        }

        assigner
    }
}

impl Default for Assigner {
    fn default() -> Self {
        Self::new()
    }
}
