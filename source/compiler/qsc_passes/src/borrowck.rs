// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use miette::Diagnostic;
use qsc_data_structures::span::Span;
use qsc_hir::{
    hir::{Expr, ExprKind, Mutability, NodeId, Pat, PatKind, Res, Stmt, StmtKind},
    visit::{Visitor, walk_expr, walk_stmt},
};
use rustc_hash::FxHashSet;
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error("cannot update immutable variable")]
    #[diagnostic(help(
        "mutable variables must be declared with the keyword `mutable` instead of `let`"
    ))]
    #[diagnostic(code("Qdk.Qsc.BorrowCk.Mutability"))]
    Mutability(#[label] Span),

    #[error("lambdas cannot close over mutable variables")]
    #[diagnostic(code("Qdk.Qsc.BorrowCk.MutableClosure"))]
    MutableClosure(#[label] Span),

    #[error("invalid left-hand side of assignment")]
    #[diagnostic(help("the left-hand side must be a variable or tuple of variables"))]
    #[diagnostic(code("Qdk.Qsc.BorrowCk.Unassignable"))]
    Unassignable(#[label("not assignable")] Span),
}

#[derive(Default)]
pub(super) struct Checker {
    mutable: FxHashSet<NodeId>,
    pub(super) errors: Vec<Error>,
}

impl Checker {
    fn track_pat(&mut self, pat: &Pat) {
        match &pat.kind {
            PatKind::Bind(ident) => {
                self.mutable.insert(ident.id);
            }
            PatKind::Discard | PatKind::Err => {}
            PatKind::Tuple(tup) => {
                for pat in tup {
                    self.track_pat(pat);
                }
            }
        }
    }

    fn verify_assignment(&mut self, lhs: &Expr) {
        match &lhs.kind {
            ExprKind::Hole => {}
            ExprKind::Var(Res::Local(id), _) => {
                if !self.mutable.contains(id) {
                    self.errors.push(Error::Mutability(lhs.span));
                }
            }
            ExprKind::Tuple(tup) => tup.iter().for_each(|lhs| self.verify_assignment(lhs)),
            _ => self.errors.push(Error::Unassignable(lhs.span)),
        }
    }
}

impl Visitor<'_> for Checker {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        if let StmtKind::Local(Mutability::Mutable, pat, _) = &stmt.kind {
            self.track_pat(pat);
        }
        walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Assign(lhs, _)
            | ExprKind::AssignField(lhs, _, _)
            | ExprKind::AssignIndex(lhs, _, _)
            | ExprKind::AssignOp(_, lhs, _) => self.verify_assignment(lhs),
            ExprKind::Closure(captures, _)
                if captures.iter().any(|cap| self.mutable.contains(cap)) =>
            {
                self.errors.push(Error::MutableClosure(expr.span));
            }
            _ => {}
        }
        walk_expr(self, expr);
    }
}
