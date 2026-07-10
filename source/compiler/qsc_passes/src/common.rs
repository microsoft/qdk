// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::{namespaces::NamespaceId, span::Span};
use qsc_hir::{
    assigner::Assigner,
    global::Table,
    hir::{
        Expr, ExprKind, Field, Ident, Mutability, NodeId, Pat, PatKind, PrimField, Res, Stmt,
        StmtKind,
    },
    ty::{GenericArg, Prim, Ty},
    visit::{self, Visitor},
};
use std::rc::Rc;

pub(crate) fn generated_name(name: &str) -> Rc<str> {
    Rc::from(format!(".{name}"))
}

pub(crate) fn gen_ident(assigner: &mut Assigner, label: &str, ty: Ty, span: Span) -> IdentTemplate {
    let id = assigner.next_node();
    IdentTemplate {
        id,
        span,
        ty,
        name: generated_name(&format!("{label}_{id}")),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct IdentTemplate {
    pub id: NodeId,
    pub span: Span,
    pub name: Rc<str>,
    pub ty: Ty,
}

impl IdentTemplate {
    pub fn gen_local_ref(&self, assigner: &mut Assigner) -> Expr {
        Expr {
            id: assigner.next_node(),
            span: self.span,
            ty: self.ty.clone(),
            kind: ExprKind::Var(Res::Local(self.id), Vec::new()),
        }
    }

    fn gen_pat(&self, assigner: &mut Assigner) -> Pat {
        Pat {
            id: assigner.next_node(),
            span: self.span,
            ty: self.ty.clone(),
            kind: PatKind::Bind(Ident {
                id: self.id,
                span: self.span,
                name: self.name.clone(),
            }),
        }
    }

    pub fn gen_field_access(&self, field: PrimField, assigner: &mut Assigner) -> Expr {
        Expr {
            id: assigner.next_node(),
            span: self.span,
            ty: Ty::Prim(Prim::Int),
            kind: ExprKind::Field(Box::new(self.gen_local_ref(assigner)), Field::Prim(field)),
        }
    }

    pub fn gen_id_init(&self, mutability: Mutability, expr: Expr, assigner: &mut Assigner) -> Stmt {
        Stmt {
            id: assigner.next_node(),
            span: Span::default(),
            kind: StmtKind::Local(mutability, self.gen_pat(assigner), expr),
        }
    }

    pub fn gen_steppable_id_init(
        &self,
        mutability: Mutability,
        expr: Expr,
        assigner: &mut Assigner,
    ) -> Stmt {
        Stmt {
            id: assigner.next_node(),
            span: self.span,
            kind: StmtKind::Local(mutability, self.gen_pat(assigner), expr),
        }
    }
}

pub(crate) fn create_gen_core_ref(
    core: &Table,
    namespace: NamespaceId,
    name: &str,
    generics: Vec<GenericArg>,
    span: Span,
) -> Expr {
    let callable = core
        .resolve_callable(namespace, name)
        .expect("callable should resolve");

    let ty = callable
        .scheme
        .instantiate(&generics)
        .expect("generic arguments should match type scheme");

    Expr {
        id: NodeId::default(),
        span,
        ty: Ty::Arrow(Rc::new(ty)),
        kind: ExprKind::Var(Res::Item(callable.id), generics),
    }
}

/// A loop-nesting walk shared by scanners that classify a `break`/`continue` by
/// whether it binds to the loop that owns the scanned region.
///
/// The walk tracks a loop depth for post-placement scans. Loop bodies are
/// visited at one greater depth, so a `break`/`continue` reached at depth 0
/// binds to the enclosing loop while a deeper one binds to a loop nested inside
/// the scanned region. A `for` iterable and a `while` condition are evaluated in
/// the enclosing scope and so are visited at the current depth. For `repeat`,
/// the `until` condition and `fixup` block are traversed at the repeat loop's
/// depth because [`LoopControl`](crate::loop_control::LoopControl) has already
/// rejected user `break`/`continue` expressions in those positions.
///
/// Implementors keep their own loop-depth counter and result state; the trait
/// only re-homes the identical nesting traversal. Each implementor supplies the
/// depth bookkeeping, an early-out predicate, and the action taken for a
/// `break`/`continue`.
pub(crate) trait LoopDepthScan<'a>: Visitor<'a> {
    /// Current loop nesting depth relative to the scanned region.
    fn loop_depth(&self) -> u32;

    /// Increments the loop nesting depth when entering a loop body.
    fn enter_loop(&mut self);

    /// Decrements the loop nesting depth when leaving a loop body.
    fn exit_loop(&mut self);

    /// Returns `true` once enough has been found that further walking is moot.
    fn is_done(&self) -> bool;

    /// Records a `break`/`continue`; `at_enclosing_loop` is `true` when it binds
    /// to the loop that owns the scanned region, at loop depth 0.
    fn record_break_continue(&mut self, expr: &Expr, at_enclosing_loop: bool);

    /// Walks `expr`, dispatching loop constructs with depth tracking and invoking
    /// [`record_break_continue`](Self::record_break_continue) for each
    /// `break`/`continue`. Non-loop expressions fall through to the default walk.
    fn walk_loop_depth(&mut self, expr: &'a Expr) {
        if self.is_done() {
            return;
        }
        match &expr.kind {
            ExprKind::Break | ExprKind::Continue => {
                let at_enclosing_loop = self.loop_depth() == 0;
                self.record_break_continue(expr, at_enclosing_loop);
            }
            // A `for` iterable is evaluated in the enclosing loop scope; the body
            // introduces a new innermost loop.
            ExprKind::For(_, iter, body) => {
                self.visit_expr(iter);
                self.enter_loop();
                self.visit_block(body);
                self.exit_loop();
            }
            // A `while` condition is re-evaluated in the enclosing scope; the
            // body introduces a new innermost loop.
            ExprKind::While(cond, body) => {
                self.visit_expr(cond);
                self.enter_loop();
                self.visit_block(body);
                self.exit_loop();
            }
            // The `repeat` body introduces the loop. The `until` condition and
            // `fixup` block are traversed at that depth after placement
            // validation, which rejects user `break`/`continue` there.
            ExprKind::Repeat(body, until, fixup) => {
                self.enter_loop();
                self.visit_block(body);
                self.visit_expr(until);
                if let Some(f) = fixup {
                    self.visit_block(f);
                }
                self.exit_loop();
            }
            _ => visit::walk_expr(self, expr),
        }
    }
}
