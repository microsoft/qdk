// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use miette::Diagnostic;
use qsc_data_structures::span::Span;
use qsc_hir::{
    hir::{CallableDecl, CallableKind, Expr, ExprKind, Package, Stmt, StmtKind},
    ty::{FunctorSetValue, Ty},
    visit::{self, Visitor},
};
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error("functions cannot use conjugate expressions")]
    #[diagnostic(code("Qdk.Qsc.CallableLimits.Conjugate"))]
    Conjugate(#[label] Span),

    #[error("functions cannot have functor expressions")]
    #[diagnostic(code("Qdk.Qsc.CallableLimits.Functor"))]
    Functor(#[label] Span),

    #[error("functions cannot call operations")]
    #[diagnostic(code("Qdk.Qsc.CallableLimits.OpCall"))]
    OpCall(#[label] Span),

    #[error("functions cannot allocate qubits")]
    #[diagnostic(code("Qdk.Qsc.CallableLimits.QubitAlloc"))]
    QubitAlloc(#[label] Span),

    #[error("functions cannot use repeat-loop expressions")]
    #[diagnostic(code("Qdk.Qsc.CallableLimits.Repeat"))]
    Repeat(#[label] Span),

    #[error("functions cannot have specializations")]
    #[diagnostic(code("Qdk.Qsc.CallableLimits.Spec"))]
    Spec(#[label] Span),
}

#[derive(Default)]
pub(super) struct CallableLimits {
    pub(super) errors: Vec<Error>,
}

impl Visitor<'_> for CallableLimits {
    fn visit_package(&mut self, package: &Package) {
        package.items.values().for_each(|i| self.visit_item(i));
    }

    fn visit_callable_decl(&mut self, decl: &CallableDecl) {
        if decl.kind == CallableKind::Function {
            if decl.adj.is_some() || decl.ctl.is_some() || decl.ctl_adj.is_some() {
                self.errors.push(Error::Spec(decl.span));
            }
            if decl.functors != FunctorSetValue::Empty {
                self.errors.push(Error::Functor(decl.name.span));
            }
            visit::walk_callable_decl(self, decl);
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        if let StmtKind::Qubit(..) = &stmt.kind {
            self.errors.push(Error::QubitAlloc(stmt.span));
        }
        visit::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Call(callee, _) => {
                if matches!(&callee.ty, Ty::Arrow(arrow) if arrow.kind == CallableKind::Operation) {
                    self.errors.push(Error::OpCall(expr.span));
                }
            }
            ExprKind::Conjugate(..) => {
                self.errors.push(Error::Conjugate(expr.span));
            }
            ExprKind::Repeat(..) => {
                self.errors.push(Error::Repeat(expr.span));
            }
            _ => {}
        }
        visit::walk_expr(self, expr);
    }
}
