// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{cell::RefCell, rc::Rc};

use miette::Diagnostic;
use qsc_data_structures::span::Span;
use qsc_hir::{
    hir::{CallableKind, Expr, ExprKind, Functor, NodeId, Res, UnOp},
    mut_visit::{MutVisitor, walk_expr},
    ty::{Arrow, FunctorSet, Prim, Ty},
};
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error("operation does not support the controlled functor")]
    #[diagnostic(help(
        "each operation called inside an operation with compiler-generated controlled specializations must support the controlled functor"
    ))]
    #[diagnostic(code("Qdk.Qsc.CtlGen.MissingCtlFunctor"))]
    MissingCtlFunctor(#[label] Span),
}

pub(super) struct CtlDistrib {
    pub(super) ctls: Res,
    pub(super) errors: Vec<Error>,
}

impl MutVisitor for CtlDistrib {
    fn visit_expr(&mut self, expr: &mut Expr) {
        match &mut expr.kind {
            ExprKind::Call(op, args) => {
                match &op.ty {
                    Ty::Arrow(arrow) if arrow.kind == CallableKind::Operation => {
                        let functors = match *arrow.functors.borrow() {
                            FunctorSet::Value(functors) | FunctorSet::Param(_, functors) => {
                                functors
                            }
                            FunctorSet::Infer(_) => panic!("arrow type should have known functors"),
                        };

                        if functors.contains(&Functor::Ctl) {
                            op.kind = ExprKind::UnOp(UnOp::Functor(Functor::Ctl), op.clone());
                            op.id = NodeId::default();
                            let input_ty = Ty::Arrow(Rc::new(Arrow {
                                kind: CallableKind::Operation,
                                input: RefCell::new(Ty::Tuple(vec![
                                    Ty::Array(Box::new(Ty::Prim(Prim::Qubit))),
                                    arrow.input.borrow().clone(),
                                ])),
                                output: arrow.output.clone(),
                                functors: arrow.functors.clone(),
                            }));
                            op.ty = input_ty;

                            args.kind = ExprKind::Tuple(vec![
                                Expr {
                                    id: NodeId::default(),
                                    span: args.span,
                                    ty: Ty::Array(Box::new(Ty::Prim(Prim::Qubit))),
                                    kind: ExprKind::Var(self.ctls, Vec::new()),
                                },
                                Expr::clone(args),
                            ]);
                            args.ty = Ty::Tuple(vec![
                                Ty::Array(Box::new(Ty::Prim(Prim::Qubit))),
                                Ty::clone(&args.ty),
                            ]);
                            args.id = NodeId::default();
                        } else {
                            self.errors.push(Error::MissingCtlFunctor(op.span));
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
