// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::index_map::IndexMap;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    Block, CallableImpl, ExecGraph, ExecGraphIdx, ExecGraphNode, Expr, Pat, SpecImpl, Stmt,
};
use qsc_fir::{
    fir::{self, BlockId, ExprId, LocalItemId, PatId, StmtId},
    ty::{Arrow, InferFunctorId, ParamId, Ty},
};
use qsc_hir::hir::{self, SpecBody, SpecGen};
use std::iter::once;
use std::{clone::Clone, rc::Rc};

#[must_use]
pub fn map_hir_package_to_fir(package: hir::PackageId) -> fir::PackageId {
    fir::PackageId::from(Into::<usize>::into(package))
}

#[must_use]
pub fn map_fir_package_to_hir(package: fir::PackageId) -> hir::PackageId {
    hir::PackageId::from(Into::<usize>::into(package))
}

#[must_use]
pub fn map_hir_local_item_to_fir(local_item: hir::LocalItemId) -> fir::LocalItemId {
    fir::LocalItemId::from(Into::<usize>::into(local_item))
}

#[must_use]
pub fn map_fir_local_item_to_hir(local_item: fir::LocalItemId) -> hir::LocalItemId {
    hir::LocalItemId::from(Into::<usize>::into(local_item))
}

#[derive(Debug, Default)]
struct FirIncrement {
    blocks: Vec<BlockId>,
    exprs: Vec<ExprId>,
    pats: Vec<PatId>,
    stmts: Vec<StmtId>,
    items: Vec<LocalItemId>,
}

#[derive(Default)]
pub struct ExecGraphBuilder {
    no_debug: Vec<ExecGraphNode>,
    debug: Vec<ExecGraphNode>,
}

impl ExecGraphBuilder {
    /// Takes the built execution graph and resets the builder.
    fn take(&mut self) -> ExecGraph {
        let debug_exec_graph = self
            .debug
            .drain(..)
            .chain(once(ExecGraphNode::RetFrame))
            .collect::<Vec<_>>()
            .into();

        let no_debug_exec_graph = self
            .no_debug
            .drain(..)
            .chain(once(ExecGraphNode::Ret))
            .collect::<Vec<_>>()
            .into();

        ExecGraph::new(no_debug_exec_graph, debug_exec_graph)
    }

    /// Pushes a node to *only* the debug execution graph.
    fn debug_push(&mut self, node: ExecGraphNode) {
        self.debug.push(node);
    }

    /// Pushes a node to the execution graph.
    fn push(&mut self, node: ExecGraphNode) {
        self.no_debug.push(node);
        self.debug.push(node);
    }

    /// Pushes a node with the given argument to the execution graph.
    fn push_with_arg<F>(&mut self, node_fn: F, arg: ExecGraphIdx)
    where
        F: Fn(u32) -> ExecGraphNode,
    {
        self.debug.push(node_fn(
            arg.debug_idx.try_into().expect("nodes should fit into u32"),
        ));

        self.no_debug.push(node_fn(
            arg.no_debug_idx
                .try_into()
                .expect("nodes should fit into u32"),
        ));
    }

    /// Pushes a return node to the execution graph.
    fn push_ret(&mut self) {
        self.no_debug.push(ExecGraphNode::Ret);
        self.debug.push(ExecGraphNode::RetFrame);
    }

    /// Returns the current length of the execution graph.
    fn len(&self) -> ExecGraphIdx {
        ExecGraphIdx {
            no_debug_idx: self.no_debug.len(),
            debug_idx: self.debug.len(),
        }
    }

    /// Inserts a node at the given index with the given argument.
    fn insert_with_arg<F>(&mut self, node_fn: F, index: ExecGraphIdx, arg: ExecGraphIdx)
    where
        F: Fn(u32) -> ExecGraphNode,
    {
        let debug_idx = index.debug_idx;
        let no_debug_idx = index.no_debug_idx;

        self.debug[debug_idx] =
            node_fn(arg.debug_idx.try_into().expect("nodes should fit into u32"));

        self.no_debug[no_debug_idx] = node_fn(
            arg.no_debug_idx
                .try_into()
                .expect("nodes should fit into u32"),
        );
    }

    /// Removes all nodes after and including the given index.
    fn pop_from(&mut self, idx: ExecGraphIdx) {
        self.no_debug.drain(idx.no_debug_idx..);
        self.debug.drain(idx.debug_idx..);
    }

    /// Removes the last pushed node.
    fn pop(&mut self) {
        self.no_debug.pop();
        self.debug.pop();
    }
}

pub struct Lowerer {
    nodes: IndexMap<hir::NodeId, fir::NodeId>,
    locals: IndexMap<hir::NodeId, fir::LocalVarId>,
    exprs: IndexMap<ExprId, Expr>,
    pats: IndexMap<PatId, Pat>,
    stmts: IndexMap<StmtId, Stmt>,
    blocks: IndexMap<BlockId, Block>,
    assigner: Assigner,
    exec_graph: ExecGraphBuilder,
    fir_increment: FirIncrement,
}

impl Default for Lowerer {
    fn default() -> Self {
        Self::new()
    }
}

impl Lowerer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: IndexMap::new(),
            locals: IndexMap::new(),
            exprs: IndexMap::new(),
            pats: IndexMap::new(),
            stmts: IndexMap::new(),
            blocks: IndexMap::new(),
            assigner: Assigner::new(),
            exec_graph: ExecGraphBuilder::default(),
            fir_increment: FirIncrement::default(),
        }
    }

    pub fn take_exec_graph(&mut self) -> ExecGraph {
        self.exec_graph.take()
    }

    pub fn lower_package(
        &mut self,
        package: &hir::Package,
        store: &fir::PackageStore,
    ) -> fir::Package {
        let entry = package.entry.as_ref().map(|e| self.lower_expr(e));
        let entry_exec_graph = self.exec_graph.take();
        let items: IndexMap<LocalItemId, fir::Item> = package
            .items
            .values()
            .map(|i| self.lower_item(i))
            .map(|i| (i.id, i))
            .collect();

        // Lower top-level statements
        for stmt in &package.stmts {
            self.lower_stmt(stmt);
        }

        let blocks: IndexMap<_, _> = self.blocks.drain().collect();
        let exprs: IndexMap<_, _> = self.exprs.drain().collect();
        let pats: IndexMap<_, _> = self.pats.drain().collect();
        let stmts: IndexMap<_, _> = self.stmts.drain().collect();

        let package = fir::Package {
            items,
            entry,
            entry_exec_graph,
            blocks,
            exprs,
            pats,
            stmts,
        };
        qsc_fir::validate::validate(&package, store);
        package
    }

    /// Used to update the package with the lowered items.
    /// Incremental compilation requires that we update the package
    /// instead of returning a new one.
    #[allow(clippy::similar_names)]
    pub fn lower_and_update_package(
        &mut self,
        fir_package: &mut fir::Package,
        hir_package: &hir::Package,
    ) {
        // Clear the previous increment since we are about to take a new one.
        self.fir_increment = FirIncrement::default();

        let items: IndexMap<LocalItemId, fir::Item> = hir_package
            .items
            .values()
            .map(|i| self.lower_item(i))
            .map(|i| (i.id, i))
            .collect();

        for stmt in &hir_package.stmts {
            let _ = self.lower_stmt(stmt);
        }

        let entry = hir_package.entry.as_ref().map(|e| self.lower_expr(e));

        self.update_package(fir_package);

        for (k, v) in items {
            fir_package.items.insert(k, v);
            self.fir_increment.items.push(k);
        }

        fir_package.entry = entry;
    }

    pub fn revert_last_increment(&mut self, package: &mut fir::Package) {
        for id in self.fir_increment.blocks.drain(..) {
            package.blocks.remove(id);
        }
        for id in self.fir_increment.exprs.drain(..) {
            package.exprs.remove(id);
        }
        for id in self.fir_increment.pats.drain(..) {
            package.pats.remove(id);
        }
        for id in self.fir_increment.stmts.drain(..) {
            package.stmts.remove(id);
        }
        for id in self.fir_increment.items.drain(..) {
            package.items.remove(id);
        }
    }

    fn update_package(&mut self, package: &mut fir::Package) {
        for (id, value) in self.blocks.drain() {
            package.blocks.insert(id, value);
            self.fir_increment.blocks.push(id);
        }

        for (id, value) in self.exprs.drain() {
            package.exprs.insert(id, value);
            self.fir_increment.exprs.push(id);
        }

        for (id, value) in self.pats.drain() {
            package.pats.insert(id, value);
            self.fir_increment.pats.push(id);
        }

        for (id, value) in self.stmts.drain() {
            package.stmts.insert(id, value);
            self.fir_increment.stmts.push(id);
        }
    }

    fn lower_item(&mut self, item: &hir::Item) -> fir::Item {
        let kind = match &item.kind {
            hir::ItemKind::Namespace(name, items) => {
                let name = fir::Ident {
                    id: self.lower_local_id(
                        name.0
                            .last()
                            .expect("should have at least one ident in name")
                            .id,
                    ),
                    span: name.span(),
                    name: name.name(),
                };
                let items = items.iter().map(|i| lower_local_item_id(*i)).collect();
                fir::ItemKind::Namespace(name, items)
            }
            hir::ItemKind::Callable(callable) => {
                let callable = self.lower_callable_decl(callable, &item.attrs);

                fir::ItemKind::Callable(callable.into())
            }
            hir::ItemKind::Ty(name, udt) => {
                let name = self.lower_ident(name);
                let udt = self.lower_udt(udt);

                fir::ItemKind::Ty(name, udt)
            }
            hir::ItemKind::Export(name, res) => {
                let name = self.lower_ident(name);
                let res = self.lower_res(res);
                fir::ItemKind::Export(name, res)
            }
        };
        let attrs = lower_attrs(&item.attrs);
        fir::Item {
            id: lower_local_item_id(item.id),
            span: item.span,
            parent: item.parent.map(lower_local_item_id),
            doc: Rc::clone(&item.doc),
            attrs,
            visibility: lower_visibility(item.visibility),
            kind,
        }
    }

    fn lower_callable_decl(
        &mut self,
        decl: &hir::CallableDecl,
        attrs: &[hir::Attr],
    ) -> fir::CallableDecl {
        self.assigner.stash_local();
        let locals = self.locals.drain().collect::<Vec<_>>();

        let id = self.lower_id(decl.id);
        let kind = lower_callable_kind(decl.kind);
        let name = self.lower_ident(&decl.name);
        let input = self.lower_pat(&decl.input);
        let generics = self.lower_type_parameters(&decl.generics);
        let output = self.lower_ty(&decl.output);
        let functors = lower_functors(decl.functors);
        let implementation = if decl.body.body == SpecBody::Gen(SpecGen::Intrinsic) {
            assert!(
                !(decl.adj.is_some() || decl.ctl.is_some() || decl.ctl_adj.is_some()),
                "intrinsic callables should not have specializations"
            );
            CallableImpl::Intrinsic
        } else if attrs.contains(&hir::Attr::SimulatableIntrinsic) {
            let body = self.lower_spec_decl(&decl.body);
            CallableImpl::SimulatableIntrinsic(body)
        } else {
            let body = self.lower_spec_decl(&decl.body);
            let adj = decl.adj.as_ref().map(|f| self.lower_spec_decl(f));
            let ctl = decl.ctl.as_ref().map(|f| self.lower_spec_decl(f));
            let ctl_adj = decl.ctl_adj.as_ref().map(|f| self.lower_spec_decl(f));
            let specialized_implementation = SpecImpl {
                body,
                adj,
                ctl,
                ctl_adj,
            };
            CallableImpl::Spec(specialized_implementation)
        };
        let attrs = lower_attrs(&decl.attrs);

        self.assigner.reset_local();
        self.locals.clear();
        for (k, v) in locals {
            self.locals.insert(k, v);
        }

        fir::CallableDecl {
            id,
            span: decl.span,
            kind,
            name,
            generics,
            input,
            output,
            functors,
            implementation,
            attrs,
        }
    }

    fn lower_spec_decl(&mut self, decl: &hir::SpecDecl) -> fir::SpecDecl {
        let SpecBody::Impl(pat, block) = &decl.body else {
            panic!("if a SpecDecl is some, then it must be an implementation");
        };
        let input = pat.as_ref().map(|p| self.lower_spec_decl_pat(p));
        let block = self.lower_block(block);
        fir::SpecDecl {
            id: self.lower_id(decl.id),
            span: decl.span,
            block,
            input,
            exec_graph: self.exec_graph.take(),
        }
    }

    fn lower_spec_decl_pat(&mut self, pat: &hir::Pat) -> PatId {
        let id = self.assigner.next_pat();
        let span = pat.span;
        let ty = self.lower_ty(&pat.ty);

        let kind = match &pat.kind {
            hir::PatKind::Bind(ident) => fir::PatKind::Bind(self.lower_ident(ident)),
            hir::PatKind::Discard => fir::PatKind::Discard,
            hir::PatKind::Tuple(elems) => {
                fir::PatKind::Tuple(elems.iter().map(|pat| self.lower_pat(pat)).collect())
            }
            hir::PatKind::Err => unreachable!("error pat should not be present"),
        };

        let pat = fir::Pat { id, span, ty, kind };
        self.pats.insert(id, pat);
        id
    }

    fn lower_block(&mut self, block: &hir::Block) -> BlockId {
        let id = self.assigner.next_block();
        // When lowering for debugging, we need to be more strict about scoping for variables
        // otherwise variables that are not in scope will be visible in the locals view.
        // We push a scope entry marker, `PushScope`, here and then a `PopScope` marker at the
        // end of the block, which will cause the evaluation logic to track local variables
        // for this block in the innermost scope matching their actual accessibility.
        // When not in debug mode, variables may persist across block boundaries, but all access
        // is performed via their lowered local variable ID, so they cannot be accessed outside of
        // their scope. Associated memory is still cleaned up at callable exit rather than block
        // exit.
        self.exec_graph.debug_push(ExecGraphNode::PushScope);
        let set_unit = block.stmts.is_empty()
            || !matches!(
                block.stmts.last().expect("block should be non-empty").kind,
                hir::StmtKind::Expr(..)
            );
        let block = fir::Block {
            id,
            span: block.span,
            ty: self.lower_ty(&block.ty),
            stmts: block.stmts.iter().map(|s| self.lower_stmt(s)).collect(),
        };
        if set_unit {
            self.exec_graph.push(ExecGraphNode::Unit);
        }
        self.exec_graph.debug_push(ExecGraphNode::BlockEnd(id));
        self.exec_graph.debug_push(ExecGraphNode::PopScope);
        self.blocks.insert(id, block);
        id
    }

    fn lower_stmt(&mut self, stmt: &hir::Stmt) -> fir::StmtId {
        let id = self.assigner.next_stmt();
        let graph_start_idx = self.exec_graph.len();
        self.exec_graph.debug_push(ExecGraphNode::Stmt(id));
        let kind = match &stmt.kind {
            hir::StmtKind::Expr(expr) => fir::StmtKind::Expr(self.lower_expr(expr)),
            hir::StmtKind::Item(item) => fir::StmtKind::Item(lower_local_item_id(*item)),
            hir::StmtKind::Local(mutability, pat, expr) => {
                let pat = self.lower_pat(pat);
                let expr = self.lower_expr(expr);
                self.exec_graph.push(ExecGraphNode::Bind(pat));
                fir::StmtKind::Local(lower_mutability(*mutability), pat, expr)
            }
            hir::StmtKind::Qubit(_, _, _, _) => {
                panic!("qubit statements should have been eliminated by passes");
            }
            hir::StmtKind::Semi(expr) => {
                let expr = self.lower_expr(expr);
                fir::StmtKind::Semi(expr)
            }
        };
        let stmt = fir::Stmt {
            id,
            span: stmt.span,
            kind,
            exec_graph_range: graph_start_idx..self.exec_graph.len(),
        };
        self.stmts.insert(id, stmt);
        id
    }

    #[allow(clippy::too_many_lines)]
    fn lower_expr(&mut self, expr: &hir::Expr) -> ExprId {
        let id = self.assigner.next_expr();
        let graph_start_idx = self.exec_graph.len();
        let ty = self.lower_ty(&expr.ty);

        let kind = match &expr.kind {
            hir::ExprKind::Array(items) => {
                if items
                    .iter()
                    .all(|i| matches!(i.kind, hir::ExprKind::Lit(..)))
                {
                    fir::ExprKind::ArrayLit(
                        items
                            .iter()
                            .map(|i| {
                                let i = self.lower_expr(i);
                                self.exec_graph.pop();
                                i
                            })
                            .collect(),
                    )
                } else {
                    fir::ExprKind::Array(
                        items
                            .iter()
                            .map(|i| {
                                let i = self.lower_expr(i);
                                self.exec_graph.push(ExecGraphNode::Store);
                                i
                            })
                            .collect(),
                    )
                }
            }
            hir::ExprKind::ArrayRepeat(value, size) => {
                let value = self.lower_expr(value);
                self.exec_graph.push(ExecGraphNode::Store);
                let size = self.lower_expr(size);
                fir::ExprKind::ArrayRepeat(value, size)
            }
            hir::ExprKind::Assign(lhs, rhs) => {
                let idx = self.exec_graph.len();
                let lhs = self.lower_expr(lhs);
                // The left-hand side of an assignment is not really an expression to be executed,
                // so remove any added nodes from the execution graph.
                self.exec_graph.pop_from(idx);
                fir::ExprKind::Assign(lhs, self.lower_expr(rhs))
            }
            hir::ExprKind::AssignOp(op, lhs, rhs) => {
                let idx = self.exec_graph.len();
                let is_array = matches!(lhs.ty, qsc_hir::ty::Ty::Array(..));
                let lhs = self.lower_expr(lhs);
                if is_array {
                    // The left-hand side of an array append is not really an expression to be
                    // executed, so remove any added nodes from the execution graph.
                    self.exec_graph.pop_from(idx);
                }
                let idx = self.exec_graph.len();
                if matches!(op, hir::BinOp::AndL | hir::BinOp::OrL) {
                    // Put in a placeholder jump for what will be the short-circuit
                    self.exec_graph.push(ExecGraphNode::Jump(0));
                } else if !is_array {
                    self.exec_graph.push(ExecGraphNode::Store);
                }
                let rhs = self.lower_expr(rhs);
                match op {
                    hir::BinOp::AndL => {
                        self.exec_graph.insert_with_arg(
                            ExecGraphNode::JumpIfNot,
                            idx,
                            self.exec_graph.len(),
                        );
                    }
                    hir::BinOp::OrL => {
                        self.exec_graph.insert_with_arg(
                            ExecGraphNode::JumpIf,
                            idx,
                            self.exec_graph.len(),
                        );
                    }
                    _ => {}
                }
                fir::ExprKind::AssignOp(lower_binop(*op), lhs, rhs)
            }
            hir::ExprKind::AssignField(container, field, replace) => {
                let field = lower_field(field);
                let replace = self.lower_expr(replace);
                self.exec_graph.push(ExecGraphNode::Store);
                let container = self.lower_expr(container);
                fir::ExprKind::AssignField(container, field, replace)
            }
            hir::ExprKind::AssignIndex(container, index, replace) => {
                let index = self.lower_expr(index);
                self.exec_graph.push(ExecGraphNode::Store);
                let replace = self.lower_expr(replace);
                let idx = self.exec_graph.len();
                let container = self.lower_expr(container);
                // The left-hand side of an array index assignment is not really an expression to be
                // executed, so remove any added nodes from the execution graph.
                self.exec_graph.pop_from(idx);
                fir::ExprKind::AssignIndex(container, index, replace)
            }
            hir::ExprKind::BinOp(op, lhs, rhs) => {
                let lhs = self.lower_expr(lhs);
                let idx = self.exec_graph.len();
                if matches!(op, hir::BinOp::AndL | hir::BinOp::OrL) {
                    // Put in a placeholder jump for what will be the short-circuit
                    self.exec_graph.push(ExecGraphNode::Jump(0));
                } else {
                    self.exec_graph.push(ExecGraphNode::Store);
                }
                let rhs = self.lower_expr(rhs);
                match op {
                    // If the operator is logical AND, update the placeholder to skip the
                    // right-hand side if the left-hand side is false
                    hir::BinOp::AndL => {
                        self.exec_graph.insert_with_arg(
                            ExecGraphNode::JumpIfNot,
                            idx,
                            self.exec_graph.len(),
                        );
                    }
                    // If the operator is logical OR, update the placeholder to skip the
                    // right-hand side if the left-hand side is true
                    hir::BinOp::OrL => {
                        self.exec_graph.insert_with_arg(
                            ExecGraphNode::JumpIf,
                            idx,
                            self.exec_graph.len(),
                        );
                    }
                    _ => {}
                }
                fir::ExprKind::BinOp(lower_binop(*op), lhs, rhs)
            }
            hir::ExprKind::Block(block) => fir::ExprKind::Block(self.lower_block(block)),
            hir::ExprKind::Call(callee, arg) => {
                let call = self.lower_expr(callee);
                self.exec_graph.push(ExecGraphNode::Store);
                let arg = self.lower_expr(arg);
                fir::ExprKind::Call(call, arg)
            }
            hir::ExprKind::Fail(message) => {
                // Ensure the right-hand side expression is lowered first so that it
                // is executed before the fail node, if any.
                fir::ExprKind::Fail(self.lower_expr(message))
            }
            hir::ExprKind::Field(container, field) => {
                let container = self.lower_expr(container);
                let field = lower_field(field);
                fir::ExprKind::Field(container, field)
            }
            hir::ExprKind::If(cond, if_true, if_false) => {
                let cond = self.lower_expr(cond);
                let branch_idx = self.exec_graph.len();
                // Put a placeholder in the execution graph for the jump past the true branch
                self.exec_graph.push(ExecGraphNode::Jump(0));
                let if_true = self.lower_expr(if_true);
                let (if_false, else_idx) = if let Some(if_false) = if_false.as_ref() {
                    // Put a placeholder in the execution graph for the jump past the false branch
                    let idx = self.exec_graph.len();
                    self.exec_graph.push(ExecGraphNode::Jump(0));
                    let if_false = self.lower_expr(if_false);
                    // Update the placeholder to skip over the false branch
                    self.exec_graph.insert_with_arg(
                        ExecGraphNode::Jump,
                        idx,
                        self.exec_graph.len(),
                    );
                    (Some(if_false), idx + 1)
                } else {
                    // An if-expr without an else cannot return a value, so we need to
                    // insert a no-op to ensure a Unit value is returned for the expr.
                    let idx = self.exec_graph.len();
                    self.exec_graph.push(ExecGraphNode::Unit);
                    (None, idx)
                };
                // Update the placeholder to skip the true branch if the condition is false
                self.exec_graph
                    .insert_with_arg(ExecGraphNode::JumpIfNot, branch_idx, else_idx);
                fir::ExprKind::If(cond, if_true, if_false)
            }
            hir::ExprKind::Index(container, index) => {
                let container = self.lower_expr(container);
                self.exec_graph.push(ExecGraphNode::Store);
                let index = self.lower_expr(index);
                fir::ExprKind::Index(container, index)
            }
            hir::ExprKind::Lit(lit) => lower_lit(lit),
            hir::ExprKind::Range(start, step, end) => {
                let start = start.as_ref().map(|s| self.lower_expr(s));
                if start.is_some() {
                    self.exec_graph.push(ExecGraphNode::Store);
                }
                let step = step.as_ref().map(|s| self.lower_expr(s));
                if step.is_some() {
                    self.exec_graph.push(ExecGraphNode::Store);
                }
                let end = end.as_ref().map(|e| self.lower_expr(e));
                fir::ExprKind::Range(start, step, end)
            }
            hir::ExprKind::Return(expr) => {
                let expr = self.lower_expr(expr);
                self.exec_graph.push_ret();
                fir::ExprKind::Return(expr)
            }
            hir::ExprKind::Struct(name, copy, fields) => {
                let res = self.lower_res(name);
                let copy = copy.as_ref().map(|c| {
                    let id = self.lower_expr(c);
                    self.exec_graph.push(ExecGraphNode::Store);
                    id
                });
                let fields = fields
                    .iter()
                    .map(|f| {
                        let f = self.lower_field_assign(f);
                        self.exec_graph.push(ExecGraphNode::Store);
                        f
                    })
                    .collect();
                fir::ExprKind::Struct(res, copy, fields)
            }
            hir::ExprKind::Tuple(items) => fir::ExprKind::Tuple(
                items
                    .iter()
                    .map(|i| {
                        let i = self.lower_expr(i);
                        self.exec_graph.push(ExecGraphNode::Store);
                        i
                    })
                    .collect(),
            ),
            hir::ExprKind::UnOp(op, operand) => {
                fir::ExprKind::UnOp(lower_unop(*op), self.lower_expr(operand))
            }
            hir::ExprKind::While(cond, body) => {
                let cond_idx = self.exec_graph.len();
                let cond = self.lower_expr(cond);
                let idx = self.exec_graph.len();
                // Put a placeholder in the execution graph for the jump past the loop
                self.exec_graph.push(ExecGraphNode::Jump(0));
                let body = self.lower_block(body);
                self.exec_graph.push_with_arg(ExecGraphNode::Jump, cond_idx);
                // Update the placeholder to skip the loop if the condition is false
                self.exec_graph.insert_with_arg(
                    ExecGraphNode::JumpIfNot,
                    idx,
                    self.exec_graph.len(),
                );
                // While-exprs never have a return value, so we need to insert a no-op to ensure
                // a Unit value is returned for the expr.
                self.exec_graph.push(ExecGraphNode::Unit);
                fir::ExprKind::While(cond, body)
            }
            hir::ExprKind::Closure(ids, id) => {
                let ids = ids.iter().map(|id| self.lower_local_id(*id)).collect();
                fir::ExprKind::Closure(ids, lower_local_item_id(*id))
            }
            hir::ExprKind::String(components) => fir::ExprKind::String(
                components
                    .iter()
                    .map(|c| self.lower_string_component(c))
                    .collect(),
            ),
            hir::ExprKind::UpdateIndex(lhs, mid, rhs) => {
                let mid = self.lower_expr(mid);
                self.exec_graph.push(ExecGraphNode::Store);
                let rhs = self.lower_expr(rhs);
                self.exec_graph.push(ExecGraphNode::Store);
                let lhs = self.lower_expr(lhs);
                fir::ExprKind::UpdateIndex(lhs, mid, rhs)
            }
            hir::ExprKind::UpdateField(record, field, replace) => {
                let field = lower_field(field);
                let replace = self.lower_expr(replace);
                self.exec_graph.push(ExecGraphNode::Store);
                let record = self.lower_expr(record);
                fir::ExprKind::UpdateField(record, field, replace)
            }
            hir::ExprKind::Var(res, args) => {
                let res = self.lower_res(res);
                let args = args.iter().map(|arg| self.lower_generic_arg(arg)).collect();
                fir::ExprKind::Var(res, args)
            }
            hir::ExprKind::Conjugate(..) => panic!("conjugate should be eliminated by passes"),
            hir::ExprKind::Err => panic!("error expr should not be present"),
            hir::ExprKind::For(..) => panic!("for-loop should be eliminated by passes"),
            hir::ExprKind::Hole => fir::ExprKind::Hole, // allowed for discards
            hir::ExprKind::Repeat(..) => panic!("repeat-loop should be eliminated by passes"),
        };

        match kind {
            // These expressions express specific control flow that is handled above.
            fir::ExprKind::BinOp(fir::BinOp::AndL | fir::BinOp::OrL, _, _)
            | fir::ExprKind::Block(..)
            | fir::ExprKind::If(..)
            | fir::ExprKind::Return(..)
            | fir::ExprKind::While(..) => {}

            fir::ExprKind::Assign(..)
            | fir::ExprKind::AssignField(..)
            | fir::ExprKind::AssignIndex(..)
            | fir::ExprKind::AssignOp(..) => {
                // Assignments are expressions that always produce the value `Unit`,
                // so we need to push the expr first and then follow up with an explicit
                // `Unit` node.
                self.exec_graph.push(ExecGraphNode::Expr(id));
                self.exec_graph.push(ExecGraphNode::Unit);
            }

            // All other expressions should be added to the execution graph.
            _ => self.exec_graph.push(ExecGraphNode::Expr(id)),
        }

        let expr = fir::Expr {
            id,
            span: expr.span,
            ty,
            kind,
            exec_graph_range: graph_start_idx..self.exec_graph.len(),
        };
        self.exprs.insert(id, expr);
        id
    }

    fn lower_field_assign(&mut self, field_assign: &hir::FieldAssign) -> fir::FieldAssign {
        fir::FieldAssign {
            id: self.lower_id(field_assign.id),
            span: field_assign.span,
            field: lower_field(&field_assign.field),
            value: self.lower_expr(&field_assign.value),
        }
    }

    fn lower_string_component(&mut self, component: &hir::StringComponent) -> fir::StringComponent {
        match component {
            hir::StringComponent::Expr(expr) => {
                let expr = self.lower_expr(expr);
                self.exec_graph.push(ExecGraphNode::Store);
                fir::StringComponent::Expr(expr)
            }
            hir::StringComponent::Lit(str) => fir::StringComponent::Lit(Rc::clone(str)),
        }
    }

    fn lower_pat(&mut self, pat: &hir::Pat) -> PatId {
        let id = self.assigner.next_pat();
        let ty = self.lower_ty(&pat.ty);
        let kind = match &pat.kind {
            hir::PatKind::Bind(name) => {
                let name = self.lower_ident(name);
                fir::PatKind::Bind(name)
            }
            hir::PatKind::Discard => fir::PatKind::Discard,
            hir::PatKind::Tuple(items) => {
                fir::PatKind::Tuple(items.iter().map(|i| self.lower_pat(i)).collect())
            }
            hir::PatKind::Err => unreachable!("error pat should not be present"),
        };

        let pat = fir::Pat {
            id,
            span: pat.span,
            ty,
            kind,
        };
        self.pats.insert(id, pat);
        id
    }

    fn lower_id(&mut self, id: hir::NodeId) -> fir::NodeId {
        self.nodes.get(id).copied().unwrap_or_else(|| {
            let new_id = self.assigner.next_node();
            self.nodes.insert(id, new_id);
            new_id
        })
    }

    fn lower_local_id(&mut self, id: hir::NodeId) -> fir::LocalVarId {
        self.locals.get(id).copied().unwrap_or_else(|| {
            let new_id = self.assigner.next_local();
            self.locals.insert(id, new_id);
            new_id
        })
    }

    fn lower_res(&mut self, res: &hir::Res) -> fir::Res {
        match res {
            hir::Res::Item(item) => fir::Res::Item(lower_item_id(item)),
            hir::Res::Local(node) => fir::Res::Local(self.lower_local_id(*node)),
            hir::Res::Err => fir::Res::Err,
        }
    }

    fn lower_ident(&mut self, ident: &hir::Ident) -> fir::Ident {
        fir::Ident {
            id: self.lower_local_id(ident.id),
            span: ident.span,
            name: ident.name.clone(),
        }
    }

    fn lower_udt(&mut self, udt: &qsc_hir::ty::Udt) -> qsc_fir::ty::Udt {
        let span = udt.span;
        let name = udt.name.clone();
        let definition = self.lower_udt_defn(&udt.definition);
        qsc_fir::ty::Udt {
            span,
            name,
            definition,
        }
    }

    fn lower_generic_arg(&mut self, arg: &qsc_hir::ty::GenericArg) -> qsc_fir::ty::GenericArg {
        match &arg {
            qsc_hir::ty::GenericArg::Ty(ty) => qsc_fir::ty::GenericArg::Ty(self.lower_ty(ty)),
            qsc_hir::ty::GenericArg::Functor(functors) => {
                qsc_fir::ty::GenericArg::Functor(lower_functor_set(functors))
            }
        }
    }

    fn lower_udt_defn(&mut self, definition: &qsc_hir::ty::UdtDef) -> qsc_fir::ty::UdtDef {
        let span = definition.span;
        let kind = match &definition.kind {
            qsc_hir::ty::UdtDefKind::Field(field) => {
                qsc_fir::ty::UdtDefKind::Field(self.lower_udt_field(field))
            }
            qsc_hir::ty::UdtDefKind::Tuple(tup) => qsc_fir::ty::UdtDefKind::Tuple(
                tup.iter().map(|def| self.lower_udt_defn(def)).collect(),
            ),
        };
        qsc_fir::ty::UdtDef { span, kind }
    }

    fn lower_arrow(&mut self, arrow: &qsc_hir::ty::Arrow) -> Arrow {
        Arrow {
            kind: lower_callable_kind(arrow.kind),
            input: Box::new(self.lower_ty(&arrow.input.borrow())),
            output: Box::new(self.lower_ty(&arrow.output.borrow())),
            functors: lower_functor_set(&arrow.functors.borrow()),
        }
    }

    fn lower_ty(&mut self, ty: &qsc_hir::ty::Ty) -> Ty {
        match ty {
            qsc_hir::ty::Ty::Array(array) => qsc_fir::ty::Ty::Array(Box::new(self.lower_ty(array))),
            qsc_hir::ty::Ty::Arrow(arrow) => {
                qsc_fir::ty::Ty::Arrow(Box::new(self.lower_arrow(arrow)))
            }
            qsc_hir::ty::Ty::Infer(id) => {
                qsc_fir::ty::Ty::Infer(qsc_fir::ty::InferTyId::from(usize::from(*id)))
            }
            qsc_hir::ty::Ty::Param { id, .. } => {
                qsc_fir::ty::Ty::Param(qsc_fir::ty::ParamId::from(usize::from(*id)))
            }
            qsc_hir::ty::Ty::Prim(prim) => qsc_fir::ty::Ty::Prim(lower_ty_prim(*prim)),
            qsc_hir::ty::Ty::Tuple(tys) => {
                qsc_fir::ty::Ty::Tuple(tys.iter().map(|ty| self.lower_ty(ty)).collect())
            }
            qsc_hir::ty::Ty::Udt(_, res) => qsc_fir::ty::Ty::Udt(self.lower_res(res)),
            qsc_hir::ty::Ty::Err => qsc_fir::ty::Ty::Err,
        }
    }

    fn lower_udt_field(&mut self, field: &qsc_hir::ty::UdtField) -> qsc_fir::ty::UdtField {
        qsc_fir::ty::UdtField {
            ty: self.lower_ty(&field.ty),
            name: field.name.clone(),
            name_span: field.name_span,
        }
    }

    fn lower_type_parameters(
        &mut self,
        generics: &[qsc_hir::ty::TypeParameter],
    ) -> Vec<qsc_fir::ty::TypeParameter> {
        generics
            .iter()
            .map(|x| self.lower_generic_param(x))
            .collect()
    }

    fn lower_generic_param(
        &mut self,
        g: &qsc_hir::ty::TypeParameter,
    ) -> qsc_fir::ty::TypeParameter {
        match g {
            qsc_hir::ty::TypeParameter::Ty { name, bounds } => qsc_fir::ty::TypeParameter::Ty {
                name: name.clone(),
                bounds: self.lower_class_constraints(bounds),
            },
            qsc_hir::ty::TypeParameter::Functor(value) => {
                qsc_fir::ty::TypeParameter::Functor(lower_functor_set_value(*value))
            }
        }
    }

    fn lower_class_constraints(
        &mut self,
        bounds: &qsc_hir::ty::ClassConstraints,
    ) -> qsc_fir::ty::ClassConstraints {
        qsc_fir::ty::ClassConstraints(bounds.0.iter().map(|x| self.lower_ty_bound(x)).collect())
    }

    fn lower_ty_bound(&mut self, b: &qsc_hir::ty::ClassConstraint) -> qsc_fir::ty::ClassConstraint {
        use qsc_fir::ty::ClassConstraint as FirClass;
        use qsc_hir::ty::ClassConstraint as HirClass;
        match b {
            HirClass::Eq => FirClass::Eq,
            HirClass::Exp { power } => FirClass::Exp {
                power: self.lower_ty(power),
            },
            HirClass::Add => FirClass::Add,
            HirClass::NonNativeClass(name) => FirClass::NonNativeClass(name.clone()),
            HirClass::Iterable { item } => FirClass::Iterable {
                item: self.lower_ty(item),
            },
            HirClass::Integral => FirClass::Integral,
            HirClass::Show => FirClass::Show,
            HirClass::Mul => FirClass::Mul,
            HirClass::Div => FirClass::Div,
            HirClass::Sub => FirClass::Sub,
            HirClass::Mod => FirClass::Mod,
            HirClass::Signed => FirClass::Signed,
            HirClass::Ord => FirClass::Ord,
        }
    }
}

fn lower_attrs(attrs: &[hir::Attr]) -> Vec<fir::Attr> {
    attrs
        .iter()
        .filter_map(|attr| match attr {
            hir::Attr::EntryPoint => Some(fir::Attr::EntryPoint),
            hir::Attr::Measurement => Some(fir::Attr::Measurement),
            hir::Attr::Reset => Some(fir::Attr::Reset),
            hir::Attr::Test => Some(fir::Attr::Test),
            hir::Attr::SimulatableIntrinsic | hir::Attr::Unimplemented | hir::Attr::Config => None,
        })
        .collect()
}

fn lower_functors(functors: qsc_hir::ty::FunctorSetValue) -> qsc_fir::ty::FunctorSetValue {
    lower_functor_set_value(functors)
}

fn lower_field(field: &hir::Field) -> fir::Field {
    match field {
        hir::Field::Err => fir::Field::Err,
        hir::Field::Path(path) => fir::Field::Path(fir::FieldPath {
            indices: path.indices.clone(),
        }),
        hir::Field::Prim(field) => fir::Field::Prim(lower_prim_field(*field)),
    }
}

fn lower_functor_set(functors: &qsc_hir::ty::FunctorSet) -> qsc_fir::ty::FunctorSet {
    match *functors {
        qsc_hir::ty::FunctorSet::Value(v) => {
            qsc_fir::ty::FunctorSet::Value(lower_functor_set_value(v))
        }
        qsc_hir::ty::FunctorSet::Param(p, _) => {
            qsc_fir::ty::FunctorSet::Param(ParamId::from(usize::from(p)))
        }
        qsc_hir::ty::FunctorSet::Infer(i) => {
            qsc_fir::ty::FunctorSet::Infer(InferFunctorId::from(usize::from(i)))
        }
    }
}

fn lower_prim_field(field: hir::PrimField) -> fir::PrimField {
    match field {
        hir::PrimField::Start => fir::PrimField::Start,
        hir::PrimField::Step => fir::PrimField::Step,
        hir::PrimField::End => fir::PrimField::End,
    }
}

fn lower_item_id(id: &hir::ItemId) -> fir::ItemId {
    fir::ItemId {
        item: lower_local_item_id(id.item),
        package: id.package.map(|p| fir::PackageId::from(usize::from(p))),
    }
}

fn lower_ty_prim(prim: qsc_hir::ty::Prim) -> qsc_fir::ty::Prim {
    match prim {
        qsc_hir::ty::Prim::Bool => qsc_fir::ty::Prim::Bool,
        qsc_hir::ty::Prim::Double => qsc_fir::ty::Prim::Double,
        qsc_hir::ty::Prim::Int => qsc_fir::ty::Prim::Int,
        qsc_hir::ty::Prim::Qubit => qsc_fir::ty::Prim::Qubit,
        qsc_hir::ty::Prim::Result => qsc_fir::ty::Prim::Result,
        qsc_hir::ty::Prim::String => qsc_fir::ty::Prim::String,
        qsc_hir::ty::Prim::BigInt => qsc_fir::ty::Prim::BigInt,
        qsc_hir::ty::Prim::RangeTo => qsc_fir::ty::Prim::RangeTo,
        qsc_hir::ty::Prim::RangeFrom => qsc_fir::ty::Prim::RangeFrom,
        qsc_hir::ty::Prim::Pauli => qsc_fir::ty::Prim::Pauli,
        qsc_hir::ty::Prim::RangeFull => qsc_fir::ty::Prim::RangeFull,
        qsc_hir::ty::Prim::Range => qsc_fir::ty::Prim::Range,
    }
}

fn lower_visibility(visibility: hir::Visibility) -> fir::Visibility {
    match visibility {
        hir::Visibility::Public => fir::Visibility::Public,
        hir::Visibility::Internal => fir::Visibility::Internal,
    }
}

fn lower_callable_kind(kind: hir::CallableKind) -> fir::CallableKind {
    match kind {
        hir::CallableKind::Function => fir::CallableKind::Function,
        hir::CallableKind::Operation => fir::CallableKind::Operation,
    }
}

fn lower_mutability(mutability: hir::Mutability) -> fir::Mutability {
    match mutability {
        hir::Mutability::Immutable => fir::Mutability::Immutable,
        hir::Mutability::Mutable => fir::Mutability::Mutable,
    }
}

fn lower_unop(op: hir::UnOp) -> fir::UnOp {
    match op {
        hir::UnOp::Functor(f) => fir::UnOp::Functor(lower_functor(f)),
        hir::UnOp::Neg => fir::UnOp::Neg,
        hir::UnOp::NotB => fir::UnOp::NotB,
        hir::UnOp::NotL => fir::UnOp::NotL,
        hir::UnOp::Pos => fir::UnOp::Pos,
        hir::UnOp::Unwrap => fir::UnOp::Unwrap,
    }
}

fn lower_binop(op: hir::BinOp) -> fir::BinOp {
    match op {
        hir::BinOp::Add => fir::BinOp::Add,
        hir::BinOp::AndB => fir::BinOp::AndB,
        hir::BinOp::AndL => fir::BinOp::AndL,
        hir::BinOp::Div => fir::BinOp::Div,
        hir::BinOp::Eq => fir::BinOp::Eq,
        hir::BinOp::Exp => fir::BinOp::Exp,
        hir::BinOp::Gt => fir::BinOp::Gt,
        hir::BinOp::Gte => fir::BinOp::Gte,
        hir::BinOp::Lt => fir::BinOp::Lt,
        hir::BinOp::Lte => fir::BinOp::Lte,
        hir::BinOp::Mod => fir::BinOp::Mod,
        hir::BinOp::Mul => fir::BinOp::Mul,
        hir::BinOp::Neq => fir::BinOp::Neq,
        hir::BinOp::OrB => fir::BinOp::OrB,
        hir::BinOp::OrL => fir::BinOp::OrL,
        hir::BinOp::Shl => fir::BinOp::Shl,
        hir::BinOp::Shr => fir::BinOp::Shr,
        hir::BinOp::Sub => fir::BinOp::Sub,
        hir::BinOp::XorB => fir::BinOp::XorB,
    }
}

fn lower_lit(lit: &hir::Lit) -> fir::ExprKind {
    match lit {
        hir::Lit::BigInt(value) => fir::ExprKind::Lit(fir::Lit::BigInt(value.clone())),
        &hir::Lit::Bool(value) => fir::ExprKind::Lit(fir::Lit::Bool(value)),
        &hir::Lit::Double(value) => fir::ExprKind::Lit(fir::Lit::Double(value)),
        &hir::Lit::Int(value) => fir::ExprKind::Lit(fir::Lit::Int(value)),
        hir::Lit::Pauli(hir::Pauli::I) => fir::ExprKind::Lit(fir::Lit::Pauli(fir::Pauli::I)),
        hir::Lit::Pauli(hir::Pauli::X) => fir::ExprKind::Lit(fir::Lit::Pauli(fir::Pauli::X)),
        hir::Lit::Pauli(hir::Pauli::Y) => fir::ExprKind::Lit(fir::Lit::Pauli(fir::Pauli::Y)),
        hir::Lit::Pauli(hir::Pauli::Z) => fir::ExprKind::Lit(fir::Lit::Pauli(fir::Pauli::Z)),
        hir::Lit::Result(hir::Result::One) => {
            fir::ExprKind::Lit(fir::Lit::Result(fir::Result::One))
        }
        hir::Lit::Result(hir::Result::Zero) => {
            fir::ExprKind::Lit(fir::Lit::Result(fir::Result::Zero))
        }
    }
}

fn lower_functor(functor: hir::Functor) -> fir::Functor {
    match functor {
        hir::Functor::Adj => fir::Functor::Adj,
        hir::Functor::Ctl => fir::Functor::Ctl,
    }
}

fn lower_functor_set_value(value: qsc_hir::ty::FunctorSetValue) -> qsc_fir::ty::FunctorSetValue {
    match value {
        qsc_hir::ty::FunctorSetValue::Empty => qsc_fir::ty::FunctorSetValue::Empty,
        qsc_hir::ty::FunctorSetValue::Adj => qsc_fir::ty::FunctorSetValue::Adj,
        qsc_hir::ty::FunctorSetValue::Ctl => qsc_fir::ty::FunctorSetValue::Ctl,
        qsc_hir::ty::FunctorSetValue::CtlAdj => qsc_fir::ty::FunctorSetValue::CtlAdj,
    }
}

#[must_use]
fn lower_local_item_id(id: qsc_hir::hir::LocalItemId) -> LocalItemId {
    LocalItemId::from(usize::from(id))
}
