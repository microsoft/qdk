// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use num_bigint::BigInt;
use qsc_data_structures::{namespaces::NamespaceId, span::Span};
use qsc_hir::{
    assigner::Assigner,
    global::Table,
    hir::{
        Expr, ExprKind, Field, Ident, Lit, Mutability, NodeId, Pat, PatKind, Pauli, PrimField, Res,
        Result, Stmt, StmtKind,
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

/// Builds a classical default value of `ty`, or `None` when no default can be
/// synthesized, such as for a qubit, arrow, or user-defined type.
pub(crate) fn build_default(assigner: &mut Assigner, ty: &Ty) -> Option<Expr> {
    let kind = build_default_kind(assigner, ty)?;
    Some(Expr {
        id: assigner.next_node(),
        span: Span::default(),
        ty: ty.clone(),
        kind,
    })
}

fn build_default_kind(assigner: &mut Assigner, ty: &Ty) -> Option<ExprKind> {
    match ty {
        Ty::Prim(Prim::Bool) => Some(ExprKind::Lit(Lit::Bool(false))),
        Ty::Prim(Prim::Int) => Some(ExprKind::Lit(Lit::Int(0))),
        Ty::Prim(Prim::BigInt) => Some(ExprKind::Lit(Lit::BigInt(BigInt::from(0)))),
        Ty::Prim(Prim::Double) => Some(ExprKind::Lit(Lit::Double(0.0))),
        Ty::Prim(Prim::Pauli) => Some(ExprKind::Lit(Lit::Pauli(Pauli::I))),
        Ty::Prim(Prim::Result) => Some(ExprKind::Lit(Lit::Result(Result::Zero))),
        Ty::Prim(Prim::String) => Some(ExprKind::String(Vec::new())),
        // Preserve each range type's structural shape: `...`, `0...`, `...0`,
        // or `0..0`. The concrete bounds only seed a never-observed path.
        Ty::Prim(Prim::RangeFull) => Some(ExprKind::Range(None, None, None)),
        Ty::Prim(Prim::RangeFrom) => Some(ExprKind::Range(
            Some(Box::new(build_default(assigner, &Ty::Prim(Prim::Int))?)),
            None,
            None,
        )),
        Ty::Prim(Prim::RangeTo) => Some(ExprKind::Range(
            None,
            None,
            Some(Box::new(build_default(assigner, &Ty::Prim(Prim::Int))?)),
        )),
        Ty::Prim(Prim::Range) => Some(ExprKind::Range(
            Some(Box::new(build_default(assigner, &Ty::Prim(Prim::Int))?)),
            None,
            Some(Box::new(build_default(assigner, &Ty::Prim(Prim::Int))?)),
        )),
        Ty::Array(_) => Some(ExprKind::Array(Vec::new())),
        Ty::Tuple(elems) => {
            let exprs = elems
                .iter()
                .map(|elem| build_default(assigner, elem))
                .collect::<Option<Vec<_>>>()?;
            Some(ExprKind::Tuple(exprs))
        }
        Ty::Prim(Prim::Qubit)
        | Ty::Arrow(_)
        | Ty::Udt(_, _)
        | Ty::Infer(_)
        | Ty::Param { .. }
        | Ty::Err => None,
    }
}

/// Returns whether `ty` has a classical default that [`build_default`] can
/// synthesize.
pub(crate) fn is_defaultable(ty: &Ty) -> bool {
    match ty {
        Ty::Prim(Prim::Qubit)
        | Ty::Arrow(_)
        | Ty::Udt(..)
        | Ty::Infer(_)
        | Ty::Param { .. }
        | Ty::Err => false,
        Ty::Prim(_) | Ty::Array(_) => true,
        Ty::Tuple(elems) => elems.iter().all(is_defaultable),
    }
}

/// Returns whether a resolved value of `ty` can be represented in generated
/// HIR, including behind an array-backed temporary.
pub(crate) fn is_representable(ty: &Ty) -> bool {
    match ty {
        Ty::Prim(_) | Ty::Array(_) | Ty::Arrow(_) | Ty::Udt(_, Res::Item(_)) => true,
        Ty::Tuple(elems) => elems.iter().all(is_representable),
        Ty::Udt(_, _) | Ty::Infer(_) | Ty::Param { .. } | Ty::Err => false,
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
