// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use miette::Diagnostic;
use qsc_data_structures::span::Span;
use qsc_hir::{
    hir::{Block, CallableKind, Expr, ExprKind, NodeId, StmtKind},
    ty::Ty,
    visit::{Visitor, walk_expr},
};
use rustc_hash::FxHashSet;
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error("cannot generate adjoint with this expression")]
    #[diagnostic(help(
        "assignments, repeat-loops, while-loops, and returns cannot be used in blocks that require generated adjoint"
    ))]
    #[diagnostic(code("Qsc.LogicSeparation.ExprFobidden"))]
    ExprForbidden(#[label] Span),

    #[error("cannot generate adjoint of block with {0} type")]
    #[diagnostic(help("adjoint generation can only be performed with blocks of type Unit"))]
    #[diagnostic(code("Qsc.LogicSeparation.NonUnitBlock"))]
    NonUnitBlock(String, #[label] Span),

    #[error("cannot generate adjoint with operation call in this position")]
    #[diagnostic(help(
        "in blocks that require generated adjoint, operation calls can only appear as top-level statements or in a qubit allocation block, conjugate block, for-loop block, or conditional block"
    ))]
    #[diagnostic(code("Qsc.LogicSeparation.OpCallForbidden"))]
    OpCallForbidden(#[label] Span),
}

/// Checks that the given block is separatable, meaning classical statements and quantum statements
/// across the block and any nested expressions/blocks can be logically separated. On success, returns a `HashSet` of
/// all quantum statement node ids, based on whether any operation calls are present in that statement.
pub(crate) fn find_quantum_stmts(block: &Block) -> Result<FxHashSet<NodeId>, Vec<Error>> {
    let mut pass = SepCheck {
        errors: Vec::new(),
        op_call_present: FxHashSet::default(),
        op_call_allowed: true,
        has_op_call: false,
    };
    pass.visit_block(block);
    if pass.errors.is_empty() {
        Ok(pass.op_call_present)
    } else {
        Err(pass.errors)
    }
}

struct SepCheck {
    errors: Vec<Error>,
    op_call_present: FxHashSet<NodeId>,
    op_call_allowed: bool,
    has_op_call: bool,
}

impl<'a> Visitor<'a> for SepCheck {
    fn visit_block(&mut self, block: &'a Block) {
        match &block.ty {
            Ty::Tuple(tup) if tup.is_empty() => {}
            ty if self.op_call_allowed => {
                self.errors
                    .push(Error::NonUnitBlock(ty.display(), block.span));
                self.op_call_allowed = false;
            }
            _ => {}
        }

        let prior = self.op_call_allowed;
        let mut has_inner_op_call = false;
        for stmt in &block.stmts {
            let has_op_call = match &stmt.kind {
                StmtKind::Item(_) => false,

                StmtKind::Local(..) | StmtKind::Qubit(_, _, _, None) => {
                    self.op_call_allowed = false;
                    self.visit_stmt(stmt);
                    self.op_call_allowed = prior;
                    false
                }

                StmtKind::Qubit(_, _, init, Some(qubit_block)) => {
                    self.op_call_allowed = false;
                    self.visit_qubit_init(init);
                    self.op_call_allowed = prior;
                    self.handle_block(qubit_block)
                }

                StmtKind::Expr(expr) | StmtKind::Semi(expr) => self.handle_expr(expr, prior),
            };
            if has_op_call {
                self.op_call_present.insert(stmt.id);
                has_inner_op_call = true;
            }
        }
        self.has_op_call = self.has_op_call || has_inner_op_call;
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let ExprKind::Call(callee, _) = &expr.kind
            && matches!(&callee.ty, Ty::Arrow(arrow) if arrow.kind == CallableKind::Operation)
        {
            self.errors.push(Error::OpCallForbidden(expr.span));
        }
        walk_expr(self, expr);
    }
}

impl SepCheck {
    fn handle_if_expr(
        &mut self,
        prior: bool,
        cond: &Expr,
        then_expr: &Expr,
        else_expr: Option<&Expr>,
    ) -> bool {
        self.op_call_allowed = false;
        self.visit_expr(cond);
        self.op_call_allowed = prior;

        let then_has_op = self.handle_expr(then_expr, prior);
        let else_has_op = if let Some(else_expr) = else_expr {
            self.handle_expr(else_expr, prior)
        } else {
            false
        };
        then_has_op || else_has_op
    }

    fn handle_block(&mut self, block: &Block) -> bool {
        self.visit_block(block);
        if self.has_op_call {
            self.has_op_call = false;
            return true;
        }
        false
    }

    fn handle_call(&mut self, expr: &Expr, callee: &Expr, args: &Expr, prior: bool) -> bool {
        let is_op_call =
            matches!(&callee.ty, Ty::Arrow(arrow) if arrow.kind == CallableKind::Operation);
        self.op_call_allowed = false;
        self.visit_expr(callee);
        self.visit_expr(args);
        self.op_call_allowed = prior;
        let mut has_inner_op_call = false;
        if is_op_call {
            if self.op_call_allowed {
                has_inner_op_call = true;
            } else {
                self.errors.push(Error::OpCallForbidden(expr.span));
            }
        }
        has_inner_op_call
    }

    fn handle_expr(&mut self, expr: &Expr, prior: bool) -> bool {
        match &expr.kind {
            ExprKind::Block(block) => self.handle_block(block),
            ExprKind::Call(callee, args) => self.handle_call(expr, callee, args, prior),
            ExprKind::Conjugate(within, apply) => {
                let within_has_op = self.handle_block(within);
                self.handle_block(apply) || within_has_op
            }
            ExprKind::For(_, iter, loop_block) => {
                self.op_call_allowed = false;
                self.visit_expr(iter);
                self.op_call_allowed = prior;
                self.handle_block(loop_block)
            }
            ExprKind::If(cond, then_expr, else_expr) => {
                self.handle_if_expr(prior, cond, then_expr, else_expr.as_deref())
            }

            ExprKind::Array(_)
            | ExprKind::ArrayRepeat(..)
            | ExprKind::BinOp(..)
            | ExprKind::Closure(..)
            | ExprKind::Err
            | ExprKind::Fail(..)
            | ExprKind::Field(..)
            | ExprKind::Hole
            | ExprKind::Index(..)
            | ExprKind::Lit(..)
            | ExprKind::Range(..)
            | ExprKind::Struct(..)
            | ExprKind::String(..)
            | ExprKind::UpdateIndex(..)
            | ExprKind::Tuple(..)
            | ExprKind::UnOp(..)
            | ExprKind::UpdateField(..)
            | ExprKind::Var(..) => {
                self.op_call_allowed = false;
                self.visit_expr(expr);
                self.op_call_allowed = prior;
                false
            }

            ExprKind::Assign(..)
            | ExprKind::AssignOp(..)
            | ExprKind::AssignField(..)
            | ExprKind::AssignIndex(..)
            | ExprKind::Repeat(..)
            | ExprKind::Return(..)
            | ExprKind::While(..) => {
                self.errors.push(Error::ExprForbidden(expr.span));
                false
            }
        }
    }
}
