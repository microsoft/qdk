// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use miette::Diagnostic;
use qsc_data_structures::span::Span;
use qsc_hir::{
    hir::{Block, Expr, ExprKind},
    visit::{self, Visitor},
};
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error("break and continue expressions can only be used inside a loop")]
    #[diagnostic(code("Qdk.Qsc.LoopControl.BreakContinueOutsideLoop"))]
    OutsideLoop(#[label] Span),

    #[error("break and continue expressions cannot be used in a loop condition")]
    #[diagnostic(code("Qdk.Qsc.LoopControl.BreakContinueInLoopHeader"))]
    InLoopHeader(#[label] Span),

    #[error("break and continue expressions cannot be used in a repeat-loop fixup block")]
    #[diagnostic(code("Qdk.Qsc.LoopControl.BreakContinueInFixup"))]
    InFixup(#[label] Span),
}

/// A position where `break`/`continue` are forbidden even inside a loop, tracked
/// only while it is not separated from the current expression by a nested loop body.
#[derive(Clone, Copy)]
enum ForbiddenPosition {
    /// A `while` condition or a `repeat`-loop `until` condition.
    Condition,
    /// A `repeat`-loop `fixup` block.
    Fixup,
}

/// Validates the placement of `break` and `continue` expressions.
///
/// `break`/`continue` bind to the innermost enclosing loop body and are errors
/// outside any loop, in a loop condition, or in a `repeat`-loop fixup block. This
/// runs on HIR after lambda lifting, so each callable is walked with a fresh loop
/// depth of zero and needs no lambda special-casing: a `break`/`continue` in a
/// lambda body binds only to a loop within that same lambda.
#[derive(Default)]
pub(super) struct LoopControl {
    pub(super) errors: Vec<Error>,
    loop_depth: u32,
    forbidden: Option<ForbiddenPosition>,
}

impl LoopControl {
    fn control_flow_error(&self, span: Span) -> Option<Error> {
        match self.forbidden {
            Some(ForbiddenPosition::Condition) => Some(Error::InLoopHeader(span)),
            Some(ForbiddenPosition::Fixup) => Some(Error::InFixup(span)),
            None if self.loop_depth == 0 => Some(Error::OutsideLoop(span)),
            None => None,
        }
    }

    fn visit_loop_body(&mut self, body: &Block) {
        // A break/continue inside a loop body binds to this loop, so increase the
        // depth and clear any forbidden position inherited from an enclosing header.
        let saved = self.forbidden.take();
        self.loop_depth += 1;
        self.visit_block(body);
        self.loop_depth -= 1;
        self.forbidden = saved;
    }

    fn visit_condition(&mut self, cond: &Expr) {
        let saved = self.forbidden.replace(ForbiddenPosition::Condition);
        self.visit_expr(cond);
        self.forbidden = saved;
    }

    fn visit_fixup(&mut self, fixup: &Block) {
        let saved = self.forbidden.replace(ForbiddenPosition::Fixup);
        self.visit_block(fixup);
        self.forbidden = saved;
    }
}

impl<'a> Visitor<'a> for LoopControl {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match &expr.kind {
            ExprKind::Break | ExprKind::Continue => {
                if let Some(error) = self.control_flow_error(expr.span) {
                    self.errors.push(error);
                }
            }
            ExprKind::For(_, iter, body) => {
                // The iterable is evaluated once before iteration, so a break/continue
                // there binds to the enclosing loop, not this one.
                self.visit_expr(iter);
                self.visit_loop_body(body);
            }
            ExprKind::While(cond, body) => {
                self.visit_condition(cond);
                self.visit_loop_body(body);
            }
            ExprKind::Repeat(body, until, fixup) => {
                self.visit_loop_body(body);
                self.visit_condition(until);
                if let Some(fixup) = fixup {
                    self.visit_fixup(fixup);
                }
            }
            _ => visit::walk_expr(self, expr),
        }
    }
}
