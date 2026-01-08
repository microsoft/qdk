// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use std::mem::take;

use miette::Diagnostic;
use qsc_data_structures::span::Span;
use qsc_hir::{
    assigner::Assigner,
    global::Table,
    hir::{
        Block, Expr, ExprKind, Ident, Mutability, NodeId, Package, Pat, PatKind, Res, Stmt,
        StmtKind,
    },
    mut_visit::{self, MutVisitor},
    ty::Ty,
    visit::{self, Visitor},
};
use rustc_hash::FxHashSet;
use thiserror::Error;

use crate::{
    common::generated_name,
    id_update::NodeIdRefresher,
    invert_block::adj_invert_block,
    spec_gen::adj_gen::{self, AdjDistrib},
};

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[diagnostic(transparent)]
    #[error(transparent)]
    AdjGen(adj_gen::Error),

    #[error("variable cannot be assigned in apply-block since it is used in within-block")]
    #[diagnostic(help(
        "updating mutable variables in the apply-block that are used in the within-block can violate logic reversibility"
    ))]
    #[diagnostic(code("Qsc.ConjugateInvert.ApplyAssign"))]
    ApplyAssign(#[label] Span),

    #[error("return expressions are not allowed in apply-blocks")]
    #[diagnostic(code("Qsc.ConjugateInvert.ReturnForbidden"))]
    ReturnForbidden(#[label] Span),
}

/// Generates adjoint inverted blocks for within-blocks across all conjugate expressions,
/// eliminating the conjugate expression from the compilation unit.
pub(super) fn invert_conjugate_exprs(
    core: &Table,
    package: &mut Package,
    assigner: &mut Assigner,
) -> Vec<Error> {
    let mut pass = ConjugateElim {
        core,
        assigner,
        errors: Vec::new(),
    };
    pass.visit_package(package);
    pass.errors
}

struct ConjugateElim<'a> {
    core: &'a Table,
    assigner: &'a mut Assigner,
    errors: Vec<Error>,
}

impl MutVisitor for ConjugateElim<'_> {
    fn visit_expr(&mut self, expr: &mut Expr) {
        match take(&mut expr.kind) {
            ExprKind::Conjugate(within, apply) => {
                let mut usage = Usage {
                    used: FxHashSet::default(),
                };
                usage.visit_block(&within);
                let mut assign_check = AssignmentCheck {
                    used: usage.used,
                    errors: Vec::new(),
                };
                assign_check.visit_block(&apply);
                self.errors.extend(assign_check.errors);

                let mut return_check = ReturnCheck { errors: Vec::new() };
                return_check.visit_block(&apply);
                self.errors.extend(return_check.errors);

                let mut adj_within = within.clone();
                if let Err(invert_errors) =
                    adj_invert_block(self.core, self.assigner, &mut adj_within)
                {
                    self.errors.extend(
                        invert_errors
                            .into_iter()
                            .map(adj_gen::Error::LogicSep)
                            .map(Error::AdjGen),
                    );
                    return;
                }
                let mut distrib = AdjDistrib { errors: Vec::new() };
                distrib.visit_block(&mut adj_within);
                self.errors
                    .extend(distrib.errors.into_iter().map(Error::AdjGen));

                NodeIdRefresher::new(self.assigner).visit_block(&mut adj_within);

                let (bind_id, apply_as_bind) = self.block_as_binding(apply, expr.ty.clone());

                let new_block = Block {
                    id: self.assigner.next_node(),
                    span: Span::default(),
                    ty: expr.ty.clone(),
                    stmts: vec![
                        self.block_as_stmt(within),
                        apply_as_bind,
                        self.block_as_stmt(adj_within),
                        Stmt {
                            id: self.assigner.next_node(),
                            span: Span::default(),
                            kind: StmtKind::Expr(Expr {
                                id: self.assigner.next_node(),
                                span: Span::default(),
                                ty: expr.ty.clone(),
                                kind: ExprKind::Var(Res::Local(bind_id), Vec::new()),
                            }),
                        },
                    ],
                };
                *expr = self.block_as_expr(new_block, expr.ty.clone());
            }
            kind => expr.kind = kind,
        }

        mut_visit::walk_expr(self, expr);
    }
}

impl ConjugateElim<'_> {
    fn block_as_expr(&mut self, block: Block, ty: Ty) -> Expr {
        Expr {
            id: self.assigner.next_node(),
            span: Span::default(),
            ty,
            kind: ExprKind::Block(block),
        }
    }

    fn block_as_stmt(&mut self, block: Block) -> Stmt {
        Stmt {
            id: self.assigner.next_node(),
            span: Span::default(),
            kind: StmtKind::Expr(self.block_as_expr(block, Ty::UNIT)),
        }
    }
    fn block_as_binding(&mut self, block: Block, ty: Ty) -> (NodeId, Stmt) {
        let bind_id = self.assigner.next_node();
        (
            bind_id,
            Stmt {
                id: self.assigner.next_node(),
                span: Span::default(),
                kind: StmtKind::Local(
                    Mutability::Immutable,
                    Pat {
                        id: self.assigner.next_node(),
                        span: Span::default(),
                        ty: ty.clone(),
                        kind: PatKind::Bind(Ident {
                            id: bind_id,
                            span: Span::default(),
                            name: generated_name("apply_res"),
                        }),
                    },
                    self.block_as_expr(block, ty),
                ),
            },
        )
    }
}

struct Usage {
    used: FxHashSet<NodeId>,
}

impl<'a> Visitor<'a> for Usage {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match &expr.kind {
            ExprKind::Var(Res::Local(id), _) => {
                self.used.insert(*id);
            }
            _ => visit::walk_expr(self, expr),
        }
    }
}

struct AssignmentCheck {
    used: FxHashSet<NodeId>,
    errors: Vec<Error>,
}

impl<'a> Visitor<'a> for AssignmentCheck {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match &expr.kind {
            ExprKind::Assign(lhs, rhs) => {
                self.visit_expr(rhs);
                self.check_assign(lhs);
            }
            _ => visit::walk_expr(self, expr),
        }
    }
}

impl AssignmentCheck {
    fn check_assign(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Var(Res::Local(id), _) => {
                if self.used.contains(id) {
                    self.errors.push(Error::ApplyAssign(expr.span));
                }
            }
            ExprKind::Tuple(var_tup) => {
                for expr in var_tup {
                    self.check_assign(expr);
                }
            }
            // Other LHS kinds are not assignable, which is a semantic error
            // handled by the borrow checker pass.
            _ => {}
        }
    }
}

struct ReturnCheck {
    errors: Vec<Error>,
}

impl<'a> Visitor<'a> for ReturnCheck {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if matches!(&expr.kind, ExprKind::Return(..)) {
            self.errors.push(Error::ReturnForbidden(expr.span));
        }
    }
}
