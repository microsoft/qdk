// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::logic_sep;
use miette::Diagnostic;
use qsc_data_structures::span::Span;
use qsc_hir::{
    hir::{CallableKind, Expr, ExprKind, Functor, NodeId, UnOp},
    mut_visit::{MutVisitor, walk_expr},
    ty::Ty,
};
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error("operation does not support the adjoint functor")]
    #[diagnostic(help(
        "each operation called inside an operation with compiler-generated adjoint specializations must support the adjoint functor"
    ))]
    #[diagnostic(code("Qsc.AdjGen.MissingAdjFunctor"))]
    MissingAdjFunctor(#[label] Span),

    #[error(transparent)]
    #[diagnostic(transparent)]
    LogicSep(logic_sep::Error),
}

pub(crate) struct AdjDistrib {
    pub(crate) errors: Vec<Error>,
}

impl MutVisitor for AdjDistrib {
    fn visit_expr(&mut self, expr: &mut Expr) {
        match &mut expr.kind {
            ExprKind::Call(op, _) => {
                match &op.ty {
                    Ty::Arrow(arrow) if arrow.kind == CallableKind::Operation => {
                        let functors = arrow
                            .functors
                            .borrow()
                            .expect_value("arrow type should have known functors");

                        if functors.contains(&Functor::Adj) {
                            **op = Expr {
                                id: NodeId::default(),
                                span: op.span,
                                ty: op.ty.clone(),
                                kind: ExprKind::UnOp(UnOp::Functor(Functor::Adj), op.clone()),
                            };
                        } else {
                            self.errors.push(Error::MissingAdjFunctor(op.span));
                        }
                    }
                    _ => {}
                }

                walk_expr(self, expr);
            }
            ExprKind::Conjugate(_, apply) => {
                // Only transform the apply block, the within block can remain as-is.
                self.visit_block(apply);
            }
            _ => walk_expr(self, expr),
        }
    }
}
