// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `Return` detection for the return-unification pass.
//!
//! Delegates to the shared pre-order walker in [`crate::walk_utils`] so
//! that `ExprKind` variant coverage is maintained in a single location.
//!
//! `ExprKind::Closure` is treated as a leaf by [`crate::walk_utils::for_each_expr`]:
//! closure captures are [`qsc_fir::fir::LocalVarId`]s rather than
//! expressions, and the closure body lives in a separate callable that
//! `return_unify` visits independently.

use crate::walk_utils;
use qsc_fir::fir::{BlockId, ExprId, ExprKind, Package, PackageLookup, StmtId, StmtKind};

/// Returns `true` when `block_id` contains any `ExprKind::Return` at any depth.
pub(super) fn contains_return_in_block(package: &Package, block_id: BlockId) -> bool {
    let mut found = false;
    walk_utils::for_each_expr_in_block(package, block_id, &mut |_id, expr| {
        if matches!(expr.kind, ExprKind::Return(_)) {
            found = true;
        }
    });
    found
}

/// Returns `true` when the statement's initializer/expression contains any
/// `ExprKind::Return`.
pub(super) fn contains_return_in_stmt(package: &Package, stmt_id: StmtId) -> bool {
    let stmt = package.get_stmt(stmt_id);
    match &stmt.kind {
        StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) => {
            contains_return_in_expr(package, *expr_id)
        }
        StmtKind::Local(_, _, expr_id) => contains_return_in_expr(package, *expr_id),
        StmtKind::Item(_) => false,
    }
}

/// Return `true` when any sub-expression of `expr_id` is an `ExprKind::Return`.
///
/// Delegates to [`walk_utils::for_each_expr`], which walks every
/// sub-expression in pre-order and treats [`ExprKind::Closure`] as a
/// leaf (closure bodies live in separate callables).
///
/// Does not short-circuit: the walker visits every reachable node even
/// after a `Return` is found. This is less efficient than the manual
/// recursive implementation but semantically equivalent.
pub(super) fn contains_return_in_expr(package: &Package, expr_id: ExprId) -> bool {
    let mut found = false;
    walk_utils::for_each_expr(package, expr_id, &mut |_id, expr| {
        if matches!(expr.kind, ExprKind::Return(_)) {
            found = true;
        }
    });
    found
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
