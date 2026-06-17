// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Code action: "Convert integer literal to double"
//! Detects when an integer literal is passed where a double is expected and
//! offers to add a trailing `.` to make it into a double literal.

#[cfg(test)]
mod tests;

use qsc::{
    Span,
    ast::{self, Expr, ExprKind, UnOp},
    compile::{ErrorKind, TyInfoKind},
    hir::ty::Prim,
    line_column::Encoding,
};

use super::is_error_relevant;
use crate::{
    compilation::Compilation,
    protocol::{CodeAction, CodeActionKind, TextEdit, WorkspaceEdit},
    qsc_utils::into_range,
};

pub(crate) fn int_to_double_fixes(
    compilation: &Compilation,
    source_name: &str,
    span: Span,
    encoding: Encoding,
) -> Vec<CodeAction> {
    let mut code_actions = Vec::new();

    let unit = compilation.user_unit();
    let package = &unit.ast.package;
    let source_map = &unit.sources;

    let ty_mismatches = compilation
        .compile_errors
        .iter()
        .filter(|error| is_error_relevant(error, span))
        .filter_map(|error| match error.error() {
            ErrorKind::Frontend(frontend_error) => frontend_error.ty_mismatch(),
            _ => None,
        });

    for (expected, actual, error_span) in ty_mismatches {
        // Check if expected is Double and actual is Int.
        if matches!(&expected.kind, TyInfoKind::Prim(Prim::Double))
            && matches!(&actual.kind, TyInfoKind::Prim(Prim::Int))
        {
            // Confirm that it's a literal and not just some expression of type int
            let Some(mut expr) = find_expr_at(package, error_span) else {
                continue;
            };

            // Strip off any + or - unary operators
            while let ExprKind::UnOp(UnOp::Pos | UnOp::Neg, inner) = expr.kind.as_ref() {
                expr = inner;
            }

            if !matches!(expr.kind.as_ref(), ExprKind::Lit(_)) {
                continue;
            }

            // Generate the fix: add a trailing `.`
            // Note that this depends on the error span excluding surrounding parens
            // so we don't end up with something like `(q).`.
            let dot_range = into_range(
                encoding,
                Span {
                    lo: error_span.hi,
                    hi: error_span.hi,
                },
                source_map,
            );

            code_actions.push(CodeAction {
                title: "Convert to double literal".to_string(),
                edit: Some(WorkspaceEdit {
                    changes: vec![(
                        source_name.to_string(),
                        vec![TextEdit {
                            new_text: ".".to_string(),
                            range: dot_range,
                        }],
                    )],
                }),
                kind: Some(CodeActionKind::QuickFix),
                is_preferred: Some(true),
            });
        }
    }

    code_actions
}

/// Finds the AST expression whose span exactly matches `target` and returns its `NodeId`.
fn find_expr_at(package: &ast::Package, target: Span) -> Option<&ast::Expr> {
    let mut finder = ExprSpanFinder {
        target,
        found: None,
    };
    ast::visit::Visitor::visit_package(&mut finder, package);
    finder.found
}

struct ExprSpanFinder<'a> {
    target: Span,
    found: Option<&'a ast::Expr>,
}

impl<'a> ast::visit::Visitor<'a> for ExprSpanFinder<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if expr.span == self.target {
            self.found = Some(expr);
        } else if self.target.intersection(&expr.span).is_some() {
            ast::visit::walk_expr(self, expr);
        }
    }
}
