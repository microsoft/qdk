// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Exhaustive `Return` detection for the return-unification pass.
//!
//! Mirrors the exhaustive `ExprKind` walker in
//! [`crate::walk_utils`]: every variant is matched explicitly, with no
//! wildcard arm, so adding a new FIR `ExprKind` variant produces a compile
//! error here rather than silently missing a detection site.
//!
//! `ExprKind::Closure` is treated as a leaf: closure captures are
//! [`qsc_fir::fir::LocalVarId`]s rather than expressions, and the closure
//! body lives in a separate callable that `return_unify` visits
//! independently.

use qsc_fir::fir::{BlockId, ExprId, ExprKind, PackageLookup, StmtId, StmtKind, StringComponent};

/// Returns `true` when `block_id` contains any `ExprKind::Return` at any depth.
pub(super) fn contains_return_in_block(lookup: &impl PackageLookup, block_id: BlockId) -> bool {
    let block = lookup.get_block(block_id);
    block
        .stmts
        .iter()
        .any(|&stmt_id| contains_return_in_stmt(lookup, stmt_id))
}

/// Returns `true` when the statement's initializer/expression contains any
/// `ExprKind::Return`.
pub(super) fn contains_return_in_stmt(lookup: &impl PackageLookup, stmt_id: StmtId) -> bool {
    let stmt = lookup.get_stmt(stmt_id);
    match &stmt.kind {
        StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) => {
            contains_return_in_expr(lookup, *expr_id)
        }
        StmtKind::Local(_, _, expr_id) => contains_return_in_expr(lookup, *expr_id),
        StmtKind::Item(_) => false,
    }
}

/// Return `true` when any sub-expression of `expr_id` is an `ExprKind::Return`.
///
/// # Before
/// ```text
/// expr tree rooted at expr_id
/// ```
/// # After
/// ```text
/// unchanged
/// ```
/// # Requires
/// - `expr_id` is valid in `lookup`.
///
/// # Ensures
/// - Returns `true` iff `ExprKind::Return(_)` appears at any depth outside
///   closure boundaries.
/// - Does not recurse into `ExprKind::Closure`: captures are
///   [`qsc_fir::fir::LocalVarId`]s, not sub-expressions, and the closure
///   body lives in a separate callable.
///
/// # Mutations
/// - None (read-only).
pub(super) fn contains_return_in_expr(lookup: &impl PackageLookup, expr_id: ExprId) -> bool {
    let expr = lookup.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Return(_) => true,
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            exprs.iter().any(|&e| contains_return_in_expr(lookup, e))
        }
        ExprKind::ArrayRepeat(a, b)
        | ExprKind::Assign(a, b)
        | ExprKind::AssignOp(_, a, b)
        | ExprKind::BinOp(_, a, b)
        | ExprKind::Call(a, b)
        | ExprKind::Index(a, b)
        | ExprKind::AssignField(a, _, b)
        | ExprKind::UpdateField(a, _, b) => {
            contains_return_in_expr(lookup, *a) || contains_return_in_expr(lookup, *b)
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            contains_return_in_expr(lookup, *a)
                || contains_return_in_expr(lookup, *b)
                || contains_return_in_expr(lookup, *c)
        }
        ExprKind::Block(block_id) => contains_return_in_block(lookup, *block_id),
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => false,
        ExprKind::Fail(e) | ExprKind::Field(e, _) | ExprKind::UnOp(_, e) => {
            contains_return_in_expr(lookup, *e)
        }
        ExprKind::If(cond, body, otherwise) => {
            contains_return_in_expr(lookup, *cond)
                || contains_return_in_expr(lookup, *body)
                || otherwise.is_some_and(|e| contains_return_in_expr(lookup, e))
        }
        ExprKind::Range(start, step, end) => [start, step, end]
            .into_iter()
            .flatten()
            .any(|&e| contains_return_in_expr(lookup, e)),
        ExprKind::Struct(_, copy, fields) => {
            copy.is_some_and(|c| contains_return_in_expr(lookup, c))
                || fields
                    .iter()
                    .any(|fa| contains_return_in_expr(lookup, fa.value))
        }
        ExprKind::String(components) => components.iter().any(|c| match c {
            StringComponent::Expr(e) => contains_return_in_expr(lookup, *e),
            StringComponent::Lit(_) => false,
        }),
        ExprKind::While(cond, body) => {
            contains_return_in_expr(lookup, *cond) || contains_return_in_block(lookup, *body)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{contains_return_in_block, contains_return_in_expr, contains_return_in_stmt};
    use crate::test_utils::{PipelineStage, compile_and_run_pipeline_to};
    use indoc::indoc;
    use qsc_fir::fir::{
        BlockId, CallableImpl, ExprKind, ItemKind, Package, PackageLookup, StmtKind,
    };

    fn find_body_block_id(package: &Package, callable_name: &str) -> BlockId {
        let decl = package
            .items
            .values()
            .find_map(|item| match &item.kind {
                ItemKind::Callable(decl) if decl.name.name.as_ref() == callable_name => Some(decl),
                _ => None,
            })
            .unwrap_or_else(|| panic!("callable '{callable_name}' not found"));

        let CallableImpl::Spec(spec_impl) = &decl.implementation else {
            panic!("callable '{callable_name}' should have a body")
        };

        spec_impl.body.block
    }

    #[test]
    fn contains_return_in_stmt_detects_local_initializer_return() {
        let source = indoc! {r#"
            namespace Test {
                function Main() : Int {
                    let x = if true {
                        return 1;
                    } else {
                        0
                    };
                    x
                }
            }
        "#};

        let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Mono);
        let package = store.get(pkg_id);
        let main_block_id = find_body_block_id(package, "Main");
        let main_block = package.get_block(main_block_id);

        let local_stmt_id = main_block
            .stmts
            .iter()
            .copied()
            .find(|stmt_id| matches!(package.get_stmt(*stmt_id).kind, StmtKind::Local(_, _, _)))
            .expect("expected Main body to contain a Local initializer statement");

        assert!(
            contains_return_in_stmt(package, local_stmt_id),
            "Local initializer with a return-bearing if-expression should be detected"
        );
        assert!(
            contains_return_in_block(package, main_block_id),
            "Main block should report a reachable return through the Local initializer"
        );
    }

    #[test]
    fn contains_return_in_expr_does_not_descend_into_closure_body() {
        let source = indoc! {r#"
            namespace Test {
                function Add(a : Int, b : Int) : Int {
                    if a == 0 {
                        return b;
                    }
                    a + b
                }

                function Main() : Int {
                    let f = x -> Add(x, 1);
                    f(2)
                }
            }
        "#};

        let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Mono);
        let package = store.get(pkg_id);

        let main_block_id = find_body_block_id(package, "Main");
        let main_block = package.get_block(main_block_id);
        let closure_expr_id = main_block
            .stmts
            .iter()
            .find_map(|stmt_id| match package.get_stmt(*stmt_id).kind {
                StmtKind::Local(_, _, init_expr_id)
                    if matches!(package.get_expr(init_expr_id).kind, ExprKind::Closure(_, _)) =>
                {
                    Some(init_expr_id)
                }
                _ => None,
            })
            .expect("expected Main body to contain a closure initializer");

        assert!(
            !contains_return_in_expr(package, closure_expr_id),
            "closure expressions should be treated as leaves by return detection"
        );

        let add_block_id = find_body_block_id(package, "Add");
        assert!(
            contains_return_in_block(package, add_block_id),
            "sanity check: Add should still contain a return before return_unify"
        );
    }
}
