// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Deep-clone + ID-remap infrastructure for FIR subtrees.
//!
//! [`FirCloner`] copies blocks, expressions, patterns, and statements from a
//! source package into a target package while assigning fresh IDs to every
//! cloned node. All internal references (sub-expression IDs, block IDs, pattern
//! IDs, etc.) are remapped so the cloned subtree is self-consistent and does
//! not collide with existing IDs in the target package.

#[cfg(test)]
mod tests;

use qsc_fir::{
    assigner::Assigner,
    fir::{
        Block, BlockId, CallableDecl, CallableImpl, ExecGraph, ExecGraphDebugNode, ExecGraphNode,
        Expr, ExprId, ExprKind, FieldAssign, Ident, Item, ItemId, ItemKind, LocalItemId,
        LocalVarId, NodeId, Package, Pat, PatId, PatKind, Res, SpecDecl, SpecImpl, Stmt, StmtId,
        StmtKind, StringComponent,
    },
};
use rustc_hash::FxHashMap;
use std::rc::Rc;

/// Deep-clones FIR subtrees with full ID remapping.
///
/// All package-global IDs (`BlockId`, `ExprId`, `PatId`, `StmtId`, `NodeId`)
/// are replaced with fresh values allocated from the internal `Assigner`.
/// `LocalVarId`s are remapped per-clone to avoid collisions when the cloned
/// body is placed into a different callable scope.
pub struct FirCloner {
    /// Assigner for allocating fresh IDs above the target package's maximum.
    assigner: Assigner,
    /// Old → new remap tables.
    block_map: FxHashMap<BlockId, BlockId>,
    expr_map: FxHashMap<ExprId, ExprId>,
    pat_map: FxHashMap<PatId, PatId>,
    stmt_map: FxHashMap<StmtId, StmtId>,
    local_map: FxHashMap<LocalVarId, LocalVarId>,
    /// Reserved for future use. `NodeId` remapping is currently a no-op
    /// delegated to [`Assigner::next_node`]; the field is retained so lookups
    /// from `Old` → `New` can be added without changing the public surface.
    node_map: FxHashMap<NodeId, NodeId>,
    /// Old → new remap for nested items (`StmtKind::Item` / `ExprKind::Closure`).
    item_map: FxHashMap<LocalItemId, LocalItemId>,
    /// Per-clone local variable counter.
    next_local: u32,
    /// Optional remap for self-referencing recursive callables.
    /// When set, `Res::Item(old)` matching the first element is remapped to
    /// `Res::Item(new)` with the second element.
    self_item_remap: Option<(ItemId, ItemId)>,
}

impl FirCloner {
    /// Creates a new cloner whose counters start above the maximum existing IDs
    /// in `package`.
    #[must_use]
    pub fn new(package: &Package) -> Self {
        let assigner = Assigner::from_package(package);
        Self {
            assigner,
            block_map: FxHashMap::default(),
            expr_map: FxHashMap::default(),
            pat_map: FxHashMap::default(),
            stmt_map: FxHashMap::default(),
            local_map: FxHashMap::default(),
            node_map: FxHashMap::default(),
            item_map: FxHashMap::default(),
            next_local: 0,
            self_item_remap: None,
        }
    }

    /// Creates a new cloner initialized with the provided `Assigner`.
    ///
    /// Use this when an `Assigner` with correct watermarks is already
    /// available (e.g., captured from the lowerer), avoiding the O(n)
    /// scan performed by [`FirCloner::new`].
    #[must_use]
    pub fn from_assigner(assigner: Assigner) -> Self {
        Self {
            assigner,
            block_map: FxHashMap::default(),
            expr_map: FxHashMap::default(),
            pat_map: FxHashMap::default(),
            stmt_map: FxHashMap::default(),
            local_map: FxHashMap::default(),
            node_map: FxHashMap::default(),
            item_map: FxHashMap::default(),
            next_local: 0,
            self_item_remap: None,
        }
    }

    /// Creates a cloner whose `LocalVarId` counter starts at `local_offset`.
    ///
    /// Use this when inlining a callee body into a caller: set `local_offset`
    /// to one past the caller's maximum `LocalVarId` so the inlined locals do
    /// not shadow the caller's variables.
    #[must_use]
    pub fn with_local_offset(package: &Package, local_offset: LocalVarId) -> Self {
        let assigner = Assigner::from_package(package);
        Self {
            assigner,
            block_map: FxHashMap::default(),
            expr_map: FxHashMap::default(),
            pat_map: FxHashMap::default(),
            stmt_map: FxHashMap::default(),
            local_map: FxHashMap::default(),
            node_map: FxHashMap::default(),
            item_map: FxHashMap::default(),
            next_local: local_offset.into(),
            self_item_remap: None,
        }
    }

    /// Sets the self-item remap so that `Res::Item(old)` references are
    /// rewritten to `Res::Item(new)`. Used when cloning a recursive callable
    /// to point self-calls at the newly created specialization.
    pub fn set_self_item_remap(&mut self, old: ItemId, new: ItemId) {
        self.self_item_remap = Some((old, new));
    }

    /// Resets the per-clone remap tables and the local counter.
    ///
    /// Call this between successive clone operations to start a fresh mapping
    /// (e.g., when cloning multiple callables with the same `FirCloner`).
    pub fn reset_maps(&mut self) {
        self.block_map.clear();
        self.expr_map.clear();
        self.pat_map.clear();
        self.stmt_map.clear();
        self.local_map.clear();
        self.node_map.clear();
        self.item_map.clear();
        self.next_local = 0;
        self.self_item_remap = None;
    }

    /// Clones all specializations of a `CallableImpl`, inserting cloned nodes
    /// into `target`.
    pub fn clone_callable_impl(
        &mut self,
        source: &Package,
        callable_impl: &CallableImpl,
        target: &mut Package,
    ) -> CallableImpl {
        match callable_impl {
            CallableImpl::Intrinsic => CallableImpl::Intrinsic,
            CallableImpl::Spec(spec_impl) => {
                CallableImpl::Spec(self.clone_spec_impl(source, spec_impl, target))
            }
            CallableImpl::SimulatableIntrinsic(spec_decl) => {
                CallableImpl::SimulatableIntrinsic(self.clone_spec_decl(source, spec_decl, target))
            }
        }
    }

    /// Clones a `SpecImpl` (body + optional adj / ctl / ctl-adj specializations).
    pub fn clone_spec_impl(
        &mut self,
        source: &Package,
        spec_impl: &SpecImpl,
        target: &mut Package,
    ) -> SpecImpl {
        let body = self.clone_spec_decl(source, &spec_impl.body, target);
        let adj = spec_impl
            .adj
            .as_ref()
            .map(|s| self.clone_spec_decl(source, s, target));
        let ctl = spec_impl
            .ctl
            .as_ref()
            .map(|s| self.clone_spec_decl(source, s, target));
        let ctl_adj = spec_impl
            .ctl_adj
            .as_ref()
            .map(|s| self.clone_spec_decl(source, s, target));
        SpecImpl {
            body,
            adj,
            ctl,
            ctl_adj,
        }
    }

    /// Clones a single `SpecDecl` (one specialization body) into `target`.
    pub fn clone_spec_decl(
        &mut self,
        source: &Package,
        spec: &SpecDecl,
        target: &mut Package,
    ) -> SpecDecl {
        let new_node = self.alloc_node(spec.id);
        // Clone input BEFORE block so that `local_map` contains input
        // parameter mappings when body expressions are walked.
        let new_input = spec
            .input
            .map(|pat_id| self.clone_pat(source, pat_id, target));
        let new_block = self.clone_block(source, spec.block, target);
        let new_exec_graph = self.remap_exec_graph(&spec.exec_graph);
        SpecDecl {
            id: new_node,
            span: spec.span,
            block: new_block,
            input: new_input,
            exec_graph: new_exec_graph,
        }
    }

    /// Clones a block and all its transitive children into `target`.
    pub fn clone_block(
        &mut self,
        source: &Package,
        block_id: BlockId,
        target: &mut Package,
    ) -> BlockId {
        if let Some(&mapped) = self.block_map.get(&block_id) {
            return mapped;
        }
        let new_id = self.assigner.next_block();
        self.block_map.insert(block_id, new_id);

        let block = source
            .blocks
            .get(block_id)
            .expect("block should exist in source package");
        let new_stmts: Vec<StmtId> = block
            .stmts
            .iter()
            .map(|&stmt_id| self.clone_stmt(source, stmt_id, target))
            .collect();
        let new_block = Block {
            id: new_id,
            span: block.span,
            ty: block.ty.clone(),
            stmts: new_stmts,
        };
        target.blocks.insert(new_id, new_block);
        new_id
    }

    /// Clones a statement into `target`.
    pub fn clone_stmt(
        &mut self,
        source: &Package,
        stmt_id: StmtId,
        target: &mut Package,
    ) -> StmtId {
        if let Some(&mapped) = self.stmt_map.get(&stmt_id) {
            return mapped;
        }
        let new_id = self.assigner.next_stmt();
        self.stmt_map.insert(stmt_id, new_id);

        let stmt = source
            .stmts
            .get(stmt_id)
            .expect("stmt should exist in source package");
        let new_kind = match &stmt.kind {
            StmtKind::Expr(expr_id) => StmtKind::Expr(self.clone_expr(source, *expr_id, target)),
            StmtKind::Semi(expr_id) => StmtKind::Semi(self.clone_expr(source, *expr_id, target)),
            StmtKind::Local(mutability, pat_id, expr_id) => StmtKind::Local(
                *mutability,
                self.clone_pat(source, *pat_id, target),
                self.clone_expr(source, *expr_id, target),
            ),
            StmtKind::Item(item_id) => {
                let new_item_id = self.clone_nested_item(source, *item_id, target);
                StmtKind::Item(new_item_id)
            }
        };
        let new_stmt = Stmt {
            id: new_id,
            span: stmt.span,
            kind: new_kind,
            exec_graph_range: stmt.exec_graph_range.clone(),
        };
        target.stmts.insert(new_id, new_stmt);
        new_id
    }

    /// Clones a nested item (e.g., from `StmtKind::Item` or `ExprKind::Closure`)
    /// into `target`, allocating a fresh `LocalItemId` and remapping its body.
    ///
    /// Returns the new `LocalItemId` in the target package.
    pub fn clone_nested_item(
        &mut self,
        source: &Package,
        item_id: LocalItemId,
        target: &mut Package,
    ) -> LocalItemId {
        // Return existing mapping if already cloned.
        if let Some(&mapped) = self.item_map.get(&item_id) {
            return mapped;
        }

        let new_id = self.alloc_item();
        self.item_map.insert(item_id, new_id);

        let item = source
            .items
            .get(item_id)
            .expect("item should exist in source package");

        let new_kind = match &item.kind {
            ItemKind::Callable(decl) => {
                // Save the outer scope's local_map and counter so that the
                // nested item's parameters don't overwrite them. LocalVarIds
                // are scoped per-callable and commonly reuse the same values
                // across different scopes.
                let saved_local_map = self.local_map.clone();
                let saved_next_local = self.next_local;
                self.local_map = FxHashMap::default();
                self.next_local = 0;

                let new_input = self.clone_pat(source, decl.input, target);
                let new_impl = self.clone_callable_impl(source, &decl.implementation, target);

                // Restore the outer scope's local_map and counter.
                self.local_map = saved_local_map;
                self.next_local = saved_next_local;

                let new_node = self.alloc_node(decl.id);
                ItemKind::Callable(Box::new(CallableDecl {
                    id: new_node,
                    span: decl.span,
                    kind: decl.kind,
                    name: Ident {
                        id: LocalVarId::default(),
                        span: decl.name.span,
                        name: Rc::clone(&decl.name.name),
                    },
                    generics: decl.generics.clone(),
                    input: new_input,
                    output: decl.output.clone(),
                    functors: decl.functors,
                    implementation: new_impl,
                    attrs: decl.attrs.clone(),
                }))
            }
            ItemKind::Namespace(ident, items) => ItemKind::Namespace(ident.clone(), items.clone()),
            ItemKind::Ty(ident, udt) => ItemKind::Ty(ident.clone(), udt.clone()),
            ItemKind::Export(ident, res) => ItemKind::Export(ident.clone(), *res),
        };

        let new_item = Item {
            id: new_id,
            span: item.span,
            parent: item.parent,
            doc: Rc::clone(&item.doc),
            attrs: item.attrs.clone(),
            visibility: item.visibility,
            kind: new_kind,
        };
        target.items.insert(new_id, new_item);
        new_id
    }

    /// Clones an expression into `target`, remapping all sub-expression and
    /// block references.
    pub fn clone_expr(
        &mut self,
        source: &Package,
        expr_id: ExprId,
        target: &mut Package,
    ) -> ExprId {
        if let Some(&mapped) = self.expr_map.get(&expr_id) {
            return mapped;
        }
        let new_id = self.assigner.next_expr();
        self.expr_map.insert(expr_id, new_id);

        let expr = source
            .exprs
            .get(expr_id)
            .expect("expr should exist in source package");
        let new_kind = self.clone_expr_kind(source, &expr.kind, target);
        let new_expr = Expr {
            id: new_id,
            span: expr.span,
            ty: expr.ty.clone(),
            kind: new_kind,
            exec_graph_range: expr.exec_graph_range.clone(),
        };
        target.exprs.insert(new_id, new_expr);
        new_id
    }

    /// Clones a pattern into `target`, remapping `LocalVarId` in bindings.
    pub fn clone_pat(&mut self, source: &Package, pat_id: PatId, target: &mut Package) -> PatId {
        if let Some(&mapped) = self.pat_map.get(&pat_id) {
            return mapped;
        }
        let new_id = self.assigner.next_pat();
        self.pat_map.insert(pat_id, new_id);

        let pat = source
            .pats
            .get(pat_id)
            .expect("pat should exist in source package");
        let new_kind = match &pat.kind {
            PatKind::Bind(ident) => {
                let new_local = self.alloc_local(ident.id);
                PatKind::Bind(Ident {
                    id: new_local,
                    span: ident.span,
                    name: Rc::clone(&ident.name),
                })
            }
            PatKind::Discard => PatKind::Discard,
            PatKind::Tuple(pats) => {
                let new_pats: Vec<PatId> = pats
                    .iter()
                    .map(|&p| self.clone_pat(source, p, target))
                    .collect();
                PatKind::Tuple(new_pats)
            }
        };
        let new_pat = Pat {
            id: new_id,
            span: pat.span,
            ty: pat.ty.clone(),
            kind: new_kind,
        };
        target.pats.insert(new_id, new_pat);
        new_id
    }

    /// Clones the input pattern of a callable. This is a convenience that
    /// delegates to [`clone_pat`](Self::clone_pat).
    pub fn clone_input_pat(
        &mut self,
        source: &Package,
        pat_id: PatId,
        target: &mut Package,
    ) -> PatId {
        self.clone_pat(source, pat_id, target)
    }

    /// Remaps a `Res` reference.
    ///
    /// - `Res::Local(var)` → remapped local
    /// - `Res::Item(id)` → remapped only when matching `self_item_remap`
    /// - `Res::Err` → unchanged
    ///
    /// Item references inside [`ExprKind::Closure(_, id)`](ExprKind::Closure)
    /// are not routed through this helper. `clone_expr_kind` remaps them
    /// through a parallel path: first consulting `item_map`, then falling
    /// back to [`clone_nested_item`](Self::clone_nested_item) when the
    /// referenced item lives in the source package, and finally consulting
    /// `self_item_remap` for the recursive self-item case. Both paths must
    /// agree on the resulting `LocalItemId`.
    #[must_use]
    pub fn remap_res(&self, res: &Res) -> Res {
        match res {
            Res::Local(var) => Res::Local(*self.local_map.get(var).unwrap_or(var)),
            Res::Item(item_id) => {
                if let Some((old, new)) = &self.self_item_remap
                    && item_id == old
                {
                    return Res::Item(*new);
                }
                Res::Item(*item_id)
            }
            Res::Err => Res::Err,
        }
    }

    /// Remaps all typed IDs embedded in an `ExecGraph`.
    #[must_use]
    pub fn remap_exec_graph(&self, graph: &ExecGraph) -> ExecGraph {
        let remap_configured = |nodes: &[ExecGraphNode]| -> Rc<[ExecGraphNode]> {
            nodes
                .iter()
                .map(|node| self.remap_exec_graph_node(*node))
                .collect::<Vec<_>>()
                .into()
        };

        // ExecGraph stores its fields as Rc<[ExecGraphNode]>. We need to
        // extract, remap, and reconstruct.
        let no_debug = remap_configured(graph.select_ref(qsc_fir::fir::ExecGraphConfig::NoDebug));
        let debug = remap_configured(graph.select_ref(qsc_fir::fir::ExecGraphConfig::Debug));
        ExecGraph::new(no_debug, debug)
    }

    /// Returns a reference to the current block remap table.
    #[must_use]
    pub fn block_map(&self) -> &FxHashMap<BlockId, BlockId> {
        &self.block_map
    }

    /// Returns a reference to the current expression remap table.
    #[must_use]
    pub fn expr_map(&self) -> &FxHashMap<ExprId, ExprId> {
        &self.expr_map
    }

    /// Returns a reference to the current local variable remap table.
    #[must_use]
    pub fn local_map(&self) -> &FxHashMap<LocalVarId, LocalVarId> {
        &self.local_map
    }

    /// Returns a reference to the current pattern remap table.
    #[must_use]
    pub fn pat_map(&self) -> &FxHashMap<PatId, PatId> {
        &self.pat_map
    }

    /// Returns a reference to the current item remap table.
    #[must_use]
    pub fn item_map(&self) -> &FxHashMap<LocalItemId, LocalItemId> {
        &self.item_map
    }

    /// Allocates a fresh `ExprId`.
    pub fn alloc_expr(&mut self) -> ExprId {
        self.assigner.next_expr()
    }

    /// Allocates a fresh `PatId`.
    pub fn alloc_pat(&mut self) -> PatId {
        self.assigner.next_pat()
    }

    /// Allocates a fresh `LocalItemId`.
    pub fn alloc_item(&mut self) -> LocalItemId {
        self.assigner.next_item()
    }

    /// Consumes the cloner and returns the internal `Assigner` with its
    /// counters advanced past all IDs allocated during cloning.
    #[must_use]
    pub fn into_assigner(self) -> Assigner {
        self.assigner
    }

    fn alloc_node(&mut self, _old: NodeId) -> NodeId {
        // `_old` is reserved for future use. Today every cloned node receives
        // a fresh id with no lookup against `node_map`; the parameter is kept
        // so a remap table can be wired in without changing call sites.
        self.assigner.next_node()
    }

    pub(crate) fn next_node(&mut self) -> NodeId {
        self.assigner.next_node()
    }

    pub(crate) fn alloc_local(&mut self, old: LocalVarId) -> LocalVarId {
        let new = LocalVarId::from(self.next_local);
        self.next_local += 1;
        self.local_map.insert(old, new);
        new
    }

    /// Clones one expression kind into `target`, recursively remapping every
    /// referenced child id.
    ///
    /// Before, `kind` points at blocks, expressions, and patterns owned by the
    /// source package. After, the returned `ExprKind` has the same shape but all
    /// referenced children have been cloned into `target` and replaced with the
    /// freshly allocated ids from this cloner.
    #[allow(clippy::too_many_lines)]
    fn clone_expr_kind(
        &mut self,
        source: &Package,
        kind: &ExprKind,
        target: &mut Package,
    ) -> ExprKind {
        match kind {
            ExprKind::Array(exprs) => ExprKind::Array(
                exprs
                    .iter()
                    .map(|&e| self.clone_expr(source, e, target))
                    .collect(),
            ),
            ExprKind::ArrayLit(exprs) => ExprKind::ArrayLit(
                exprs
                    .iter()
                    .map(|&e| self.clone_expr(source, e, target))
                    .collect(),
            ),
            ExprKind::ArrayRepeat(val, size) => ExprKind::ArrayRepeat(
                self.clone_expr(source, *val, target),
                self.clone_expr(source, *size, target),
            ),
            ExprKind::Assign(lhs, rhs) => ExprKind::Assign(
                self.clone_expr(source, *lhs, target),
                self.clone_expr(source, *rhs, target),
            ),
            ExprKind::AssignOp(op, lhs, rhs) => ExprKind::AssignOp(
                *op,
                self.clone_expr(source, *lhs, target),
                self.clone_expr(source, *rhs, target),
            ),
            ExprKind::AssignField(record, field, replace) => ExprKind::AssignField(
                self.clone_expr(source, *record, target),
                field.clone(),
                self.clone_expr(source, *replace, target),
            ),
            ExprKind::AssignIndex(container, index, replace) => ExprKind::AssignIndex(
                self.clone_expr(source, *container, target),
                self.clone_expr(source, *index, target),
                self.clone_expr(source, *replace, target),
            ),
            ExprKind::BinOp(op, lhs, rhs) => ExprKind::BinOp(
                *op,
                self.clone_expr(source, *lhs, target),
                self.clone_expr(source, *rhs, target),
            ),
            ExprKind::Block(block_id) => {
                ExprKind::Block(self.clone_block(source, *block_id, target))
            }
            ExprKind::Call(callee, arg) => ExprKind::Call(
                self.clone_expr(source, *callee, target),
                self.clone_expr(source, *arg, target),
            ),
            ExprKind::Closure(vars, local_item_id) => {
                let new_vars: Vec<LocalVarId> = vars
                    .iter()
                    .map(|v| *self.local_map.get(v).unwrap_or(v))
                    .collect();
                let new_item_id = if let Some(&mapped) = self.item_map.get(local_item_id) {
                    mapped
                } else if source.items.contains_key(*local_item_id) {
                    self.clone_nested_item(source, *local_item_id, target)
                } else if let Some((old, new)) = &self.self_item_remap {
                    if *local_item_id == old.item {
                        new.item
                    } else {
                        *local_item_id
                    }
                } else {
                    *local_item_id
                };
                ExprKind::Closure(new_vars, new_item_id)
            }
            ExprKind::Fail(expr) => ExprKind::Fail(self.clone_expr(source, *expr, target)),
            ExprKind::Field(expr, field) => {
                ExprKind::Field(self.clone_expr(source, *expr, target), field.clone())
            }
            ExprKind::Hole => ExprKind::Hole,
            ExprKind::If(cond, body, otherwise) => ExprKind::If(
                self.clone_expr(source, *cond, target),
                self.clone_expr(source, *body, target),
                otherwise.map(|e| self.clone_expr(source, e, target)),
            ),
            ExprKind::Index(array, index) => ExprKind::Index(
                self.clone_expr(source, *array, target),
                self.clone_expr(source, *index, target),
            ),
            ExprKind::Lit(lit) => ExprKind::Lit(lit.clone()),
            ExprKind::Range(start, step, end) => ExprKind::Range(
                start.map(|e| self.clone_expr(source, e, target)),
                step.map(|e| self.clone_expr(source, e, target)),
                end.map(|e| self.clone_expr(source, e, target)),
            ),
            ExprKind::Return(expr) => ExprKind::Return(self.clone_expr(source, *expr, target)),
            ExprKind::Struct(res, copy, fields) => {
                let new_res = self.remap_res(res);
                let new_copy = copy.map(|e| self.clone_expr(source, e, target));
                let new_fields: Vec<FieldAssign> = fields
                    .iter()
                    .map(|fa| FieldAssign {
                        id: self.assigner.next_node(),
                        span: fa.span,
                        field: fa.field.clone(),
                        value: self.clone_expr(source, fa.value, target),
                    })
                    .collect();
                ExprKind::Struct(new_res, new_copy, new_fields)
            }
            ExprKind::String(components) => {
                let new_components: Vec<StringComponent> = components
                    .iter()
                    .map(|c| match c {
                        StringComponent::Expr(expr) => {
                            StringComponent::Expr(self.clone_expr(source, *expr, target))
                        }
                        StringComponent::Lit(s) => StringComponent::Lit(Rc::clone(s)),
                    })
                    .collect();
                ExprKind::String(new_components)
            }
            ExprKind::UpdateIndex(e1, e2, e3) => ExprKind::UpdateIndex(
                self.clone_expr(source, *e1, target),
                self.clone_expr(source, *e2, target),
                self.clone_expr(source, *e3, target),
            ),
            ExprKind::Tuple(exprs) => ExprKind::Tuple(
                exprs
                    .iter()
                    .map(|&e| self.clone_expr(source, e, target))
                    .collect(),
            ),
            ExprKind::UnOp(op, expr) => ExprKind::UnOp(*op, self.clone_expr(source, *expr, target)),
            ExprKind::UpdateField(record, field, replace) => ExprKind::UpdateField(
                self.clone_expr(source, *record, target),
                field.clone(),
                self.clone_expr(source, *replace, target),
            ),
            ExprKind::Var(res, generic_args) => {
                ExprKind::Var(self.remap_res(res), generic_args.clone())
            }
            ExprKind::While(cond, block) => ExprKind::While(
                self.clone_expr(source, *cond, target),
                self.clone_block(source, *block, target),
            ),
        }
    }

    fn remap_exec_graph_node(&self, node: ExecGraphNode) -> ExecGraphNode {
        match node {
            ExecGraphNode::Bind(pat_id) => {
                ExecGraphNode::Bind(*self.pat_map.get(&pat_id).unwrap_or(&pat_id))
            }
            ExecGraphNode::Expr(expr_id) => {
                ExecGraphNode::Expr(*self.expr_map.get(&expr_id).unwrap_or(&expr_id))
            }
            // Jump targets are graph-relative indices, not IDs — preserve them.
            ExecGraphNode::Jump(_)
            | ExecGraphNode::JumpIf(_)
            | ExecGraphNode::JumpIfNot(_)
            | ExecGraphNode::Store
            | ExecGraphNode::Unit
            | ExecGraphNode::Ret => node,
            ExecGraphNode::Debug(debug_node) => {
                ExecGraphNode::Debug(self.remap_debug_node(debug_node))
            }
        }
    }

    fn remap_debug_node(&self, node: ExecGraphDebugNode) -> ExecGraphDebugNode {
        match node {
            ExecGraphDebugNode::Stmt(stmt_id) => {
                ExecGraphDebugNode::Stmt(*self.stmt_map.get(&stmt_id).unwrap_or(&stmt_id))
            }
            ExecGraphDebugNode::PushLoopScope(expr_id) => {
                ExecGraphDebugNode::PushLoopScope(*self.expr_map.get(&expr_id).unwrap_or(&expr_id))
            }
            ExecGraphDebugNode::BlockEnd(block_id) => {
                ExecGraphDebugNode::BlockEnd(*self.block_map.get(&block_id).unwrap_or(&block_id))
            }
            ExecGraphDebugNode::PushScope
            | ExecGraphDebugNode::PopScope
            | ExecGraphDebugNode::RetFrame
            | ExecGraphDebugNode::LoopIteration => node,
        }
    }
}
