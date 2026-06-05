// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Code action: "Convert to single-element array"
// Detects when a value is passed where an array of that type is expected,
// and offers to wrap it in `[...]`.

#[cfg(test)]
mod tests;

use qsc::{
    Span,
    ast::{self, Expr, ExprKind, NodeId},
    display::Lookup,
    hir::ty::Ty,
    line_column::{Encoding, Range},
};

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

    // Find all call expressions overlapping the requested span.
    let mut finder = CallFinder {
        target_span: span,
        found: Vec::new(),
    };
    ast::visit::Visitor::visit_package(&mut finder, package);

    for (callee_id, args) in finder.found {
        // Look up the callee type to get the expected parameter types.
        let Some(callee_ty) = compilation.get_ty(callee_id) else {
            continue;
        };
        let Ty::Arrow(arrow) = callee_ty else {
            continue;
        };

        let expected_input = arrow.input.borrow();
        let param_tys: Vec<&Ty> = match &*expected_input {
            Ty::Tuple(tys) => tys.iter().collect(),
            other => vec![other],
        };

        if args.len() != param_tys.len() {
            continue;
        }

        // Match arguments against parameters.
        for (arg, param_ty) in args.iter().zip(param_tys.iter()) {
            let Some(arg_ty) = compilation.get_ty(arg.id) else {
                continue;
            };
            // Check if expected is Array(T) and actual is T.
            if let Ty::Array(item_ty) = param_ty
                && item_ty.as_ref() == arg_ty
            {
                // Generate the fix: wrap arg in [...]
                let lo = (arg.span.lo - source.offset) as usize;
                let hi = (arg.span.hi - source.offset) as usize;
                let arg_text = &source.contents[lo..hi];
                let new_text = format!("[{arg_text}]");
                let range =
                    Range::from_span(encoding, &source.contents, &(arg.span - source.offset));
                code_actions.push(CodeAction {
                    title: "Convert to single-element array".to_string(),
                    edit: Some(WorkspaceEdit {
                        changes: vec![(
                            source_name.to_string(),
                            vec![TextEdit { new_text, range }],
                        )],
                    }),
                    kind: Some(CodeActionKind::QuickFix),
                    is_preferred: None,
                });
            }
        }
    }

    code_actions
}

/// AST visitor that finds Call expressions overlapping the target span and extracts
/// the callee node id and individual argument expressions.
struct CallFinder<'a> {
    target_span: Span,
    found: Vec<(NodeId, Vec<&'a Expr>)>,
}

impl<'a> ast::visit::Visitor<'a> for CallFinder<'a> {
    fn visit_namespace(&mut self, namespace: &'a ast::Namespace) {
        if self.target_span.intersection(&namespace.span).is_some() {
            ast::visit::walk_namespace(self, namespace);
        }
    }

    fn visit_stmt(&mut self, stmt: &'a ast::Stmt) {
        if self.target_span.intersection(&stmt.span).is_some() {
            ast::visit::walk_stmt(self, stmt);
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if self.target_span.intersection(&expr.span).is_some() {
            if let ExprKind::Call(callee, arg) = &*expr.kind {
                let args = extract_args(arg);
                self.found.push((callee.id, args));
            }
            ast::visit::walk_expr(self, expr);
        }
    }
}

/// Given a call argument expression, extract the individual argument expressions.
/// If the argument is a tuple, returns each element. If it's a paren-wrapped
/// single expression, returns the inner expression. Otherwise returns the expression itself.
fn extract_args(arg: &Expr) -> Vec<&Expr> {
    match &*arg.kind {
        ExprKind::Tuple(items) => items.iter().map(AsRef::as_ref).collect(),
        ExprKind::Paren(inner) => vec![inner.as_ref()],
        _ => vec![arg],
    }
}
