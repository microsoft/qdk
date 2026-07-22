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

/// A [`Visitor`] that invokes a callback for each `break`/`continue` binding to
/// the loop (or non-loop region) that owns the visited node, i.e. one not nested
/// inside a loop within that region.
///
/// Loop bodies are not visited, since a `break`/`continue` there binds to the
/// inner loop. A `for` iterable and `while` condition run in the enclosing scope,
/// so they are still walked; a `repeat` loop's `until` and `fixup` bind to that
/// loop and are skipped with its body.
///
/// Callers enter through the [`Visitor`] method for the region they own
/// ([`visit_expr`](Visitor::visit_expr), [`visit_block`](Visitor::visit_block),
/// or [`visit_stmt`](Visitor::visit_stmt)) and read results from the callback.
pub(crate) struct EnclosingBreakContinueScan<F> {
    on_break_continue: F,
}

impl<F: FnMut(&Expr)> EnclosingBreakContinueScan<F> {
    /// Creates a scan that calls `on_break_continue` with each `break`/`continue`
    /// expression that binds to the region owning the visited node.
    pub(crate) fn new(on_break_continue: F) -> Self {
        Self { on_break_continue }
    }
}

impl<'a, F: FnMut(&Expr)> Visitor<'a> for EnclosingBreakContinueScan<F> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match &expr.kind {
            ExprKind::Break | ExprKind::Continue => (self.on_break_continue)(expr),
            // A `for` iterable and a `while` condition run in the enclosing scope,
            // so a `break`/`continue` there binds to the region that owns this
            // loop. The body is a new innermost loop and is left unvisited.
            ExprKind::For(_, iter, _body) => self.visit_expr(iter),
            ExprKind::While(cond, _body) => self.visit_expr(cond),
            // A `repeat` loop's body, `until` condition, and `fixup` block all bind
            // `break`/`continue` to that loop, so none are visited.
            ExprKind::Repeat(..) => {}
            _ => visit::walk_expr(self, expr),
        }
    }
}
