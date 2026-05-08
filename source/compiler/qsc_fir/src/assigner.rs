// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! FIR node-ID allocator.
//!
//! [`Assigner`] provides monotonically increasing IDs for every FIR arena type
//! (`BlockId`, `StmtId`, `ExprId`, `PatId`, `LocalItemId`, `LocalVarId`,
//! `NodeId`). IDs are **never reused or decremented**.
//!
//! # Append-only arena contract
//!
//! FIR arenas (`Package.blocks`, `.stmts`, `.exprs`, `.pats`) are backed by
//! `IndexMap<K, V>` which stores `Vec<Option<V>>`. FIR transform passes
//! create new nodes via `Assigner::next_*()` and may mutate existing nodes
//! in-place, but they **never remove entries** from the arenas. This means
//! pre-transform nodes remain as populated-but-unreachable entries ("orphans")
//! after transforms complete.
//!
//! Any code that iterates a FIR arena directly (via `IndexMap::iter()`) will
//! encounter orphan entries alongside live entries. Analyzers must either:
//! - Filter to reachable nodes before processing (see `qsc_rca::common`), or
//! - Tolerate orphan entries gracefully (e.g., in-place type mutations).
//!
//! The `gc_unreachable` pass in `qsc_fir_transforms` can tombstone orphan
//! entries after the pipeline completes, making `iter()` skip them.

use crate::fir::{
    BlockId, CallableImpl, ExprId, ExprKind, LocalItemId, LocalVarId, NodeId, Package, PatId,
    PatKind, Res, StmtId,
};

#[derive(Debug)]
pub struct Assigner {
    next_node: NodeId,
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
            next_node: NodeId::FIRST,
            next_block: BlockId::default(),
            next_expr: ExprId::default(),
            next_pat: PatId::default(),
            next_stmt: StmtId::default(),
            next_local: LocalVarId::default(),
            stashed_local: LocalVarId::default(),
            next_item: LocalItemId::default(),
        }
    }

    pub fn next_node(&mut self) -> NodeId {
        let id = self.next_node;
        self.next_node = id.successor();
        id
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

    pub fn set_next_node(&mut self, id: NodeId) {
        self.next_node = id;
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
        let max_block = package.blocks.iter().map(|(id, _)| u32::from(id)).max();
        if let Some(max) = max_block {
            assigner.set_next_block(BlockId::from(max + 1));
        }

        // ExprId
        let max_expr = package.exprs.iter().map(|(id, _)| u32::from(id)).max();
        if let Some(max) = max_expr {
            assigner.set_next_expr(ExprId::from(max + 1));
        }

        // PatId
        let max_pat = package.pats.iter().map(|(id, _)| u32::from(id)).max();
        if let Some(max) = max_pat {
            assigner.set_next_pat(PatId::from(max + 1));
        }

        // StmtId
        let max_stmt = package.stmts.iter().map(|(id, _)| u32::from(id)).max();
        if let Some(max) = max_stmt {
            assigner.set_next_stmt(StmtId::from(max + 1));
        }

        // NodeId — scan callable and spec decls
        let mut max_node: u32 = 0;
        for item in package.items.values() {
            if let crate::fir::ItemKind::Callable(decl) = &item.kind {
                let decl_node: u32 = decl.id.into();
                max_node = max_node.max(decl_node);
                Self::max_node_from_impl(&decl.implementation, &mut max_node);
            }
        }
        assigner.set_next_node(NodeId::from(max_node + 1));

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
        let max_item = package
            .items
            .iter()
            .map(|(k, _)| -> usize { k.into() })
            .max();
        if let Some(max) = max_item {
            assigner.set_next_item(LocalItemId::from(max + 1));
        }

        assigner
    }

    fn max_node_from_impl(callable_impl: &CallableImpl, max_node: &mut u32) {
        match callable_impl {
            CallableImpl::Intrinsic => {}
            CallableImpl::Spec(spec_impl) => {
                let body_node: u32 = spec_impl.body.id.into();
                *max_node = (*max_node).max(body_node);
                for spec in [&spec_impl.adj, &spec_impl.ctl, &spec_impl.ctl_adj]
                    .into_iter()
                    .flatten()
                {
                    let n: u32 = spec.id.into();
                    *max_node = (*max_node).max(n);
                }
            }
            CallableImpl::SimulatableIntrinsic(spec) => {
                let n: u32 = spec.id.into();
                *max_node = (*max_node).max(n);
            }
        }
    }
}

impl Default for Assigner {
    fn default() -> Self {
        Self::new()
    }
}
