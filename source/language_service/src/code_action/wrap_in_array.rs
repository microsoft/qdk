// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Code action: "Convert to single-element array"
// Detects when a value is passed where an array of that type is expected,
// and offers to wrap it in `[...]`.

#[cfg(test)]
mod tests;

use qsc::{
    Span,
    ast::{self, Expr},
    compile::{ErrorKind, TyInfo},
    display::Lookup,
    hir::ty::Ty,
    line_column::{Encoding, Range},
};

use super::is_error_relevant;
use crate::{
    compilation::Compilation,
    protocol::{CodeAction, CodeActionKind, TextEdit, WorkspaceEdit},
};

pub(crate) fn wrap_in_array_fixes(
    compilation: &Compilation,
    source_name: &str,
    span: Span,
    encoding: Encoding,
) -> Vec<CodeAction> {
    let mut code_actions = Vec::new();

    let unit = compilation.user_unit();
    let package = &unit.ast.package;

    let source = unit
        .sources
        .find_by_name(source_name)
        .expect("source should exist");

    let ty_mismatches = compilation
        .compile_errors
        .iter()
        .filter(|error| is_error_relevant(error, span))
        .filter_map(|error| match error.error() {
            ErrorKind::Frontend(frontend_error) => frontend_error.ty_mismatch(),
            _ => None,
        });

    for (expected, actual, error_span) in ty_mismatches {
        // Check if expected is Array(T) and actual is a matching primitive T.
        // Scoped to primitives to include Qubit, exclude tuples, and provide an intelligible stopping point.
        if let TyInfo::Array(item_ty) = expected
            && item_ty.as_ref() == actual
            && matches!(actual, TyInfo::Prim(_))
        {
            // Verify via the type table that the expression is truly a primitive type.
            // The error's `actual` field can be simplified (e.g. an array mismatch
            // decomposes to element-level), so we check the real expression type.
            let Some(expr_id) = find_expr_at(package, error_span) else {
                continue;
            };
            let Some(ty) = compilation.get_ty(expr_id) else {
                continue;
            };
            if !matches!(ty, Ty::Prim(_)) {
                continue;
            }

            // Generate the fix: wrap the expression in [...]
            // Note that this depends on the error span excluding surrounding parens
            // so we don't end up with something like `F[(q)]`.
            let lo = (error_span.lo - source.offset) as usize;
            let hi = (error_span.hi - source.offset) as usize;
            let arg_text = &source.contents[lo..hi];
            let new_text = format!("[{arg_text}]");
            let range = Range::from_span(encoding, &source.contents, &(error_span - source.offset));
            code_actions.push(CodeAction {
                title: "Convert to single-element array".to_string(),
                edit: Some(WorkspaceEdit {
                    changes: vec![(source_name.to_string(), vec![TextEdit { new_text, range }])],
                }),
                kind: Some(CodeActionKind::QuickFix),
                is_preferred: None,
            });
        }
    }

    code_actions
}

/// Finds the AST expression whose span exactly matches `target` and returns its `NodeId`.
fn find_expr_at(package: &ast::Package, target: Span) -> Option<ast::NodeId> {
    let mut finder = ExprSpanFinder {
        target,
        found: None,
    };
    ast::visit::Visitor::visit_package(&mut finder, package);
    finder.found
}

struct ExprSpanFinder {
    target: Span,
    found: Option<ast::NodeId>,
}

impl<'a> ast::visit::Visitor<'a> for ExprSpanFinder {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if expr.span == self.target {
            self.found = Some(expr.id);
        } else if self.target.intersection(&expr.span).is_some() {
            ast::visit::walk_expr(self, expr);
        }
    }
}
