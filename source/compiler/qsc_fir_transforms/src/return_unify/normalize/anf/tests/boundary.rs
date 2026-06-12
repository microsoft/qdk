// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The Normalize→Transform hand-off for a `return` buried in a `while`
//! condition.
//!
//! A `return` in the then-branch of an `if` used as a `while` condition is
//! *branch-buried*: it fires before the loop body but is not in a compound
//! operand position the condition itself evaluates eagerly. Normalize (the
//! hoist fixpoint followed by the ANF fixpoint) must treat the while condition
//! as a leaf and leave the `return` in place — lifting it to a spine temp would
//! relocate a callable-level early-exit out of the loop guard. Transform then
//! owns the rewrite, guarding the condition with `not __has_returned`.

use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{ExprId, ExprKind, PackageLookup, StmtKind};

use crate::return_unify::detect::contains_return_in_expr;
use crate::test_utils::compile_and_run_pipeline_to;

use super::*;
use crate::return_unify::symbols;

/// Returns the `(cond, body)` ids of the first `while` loop appearing directly
/// in `block_id`'s statement list.
fn find_while_in_block(
    package: &qsc_fir::fir::Package,
    block_id: qsc_fir::fir::BlockId,
) -> (ExprId, qsc_fir::fir::BlockId) {
    package
        .get_block(block_id)
        .stmts
        .iter()
        .find_map(|&stmt_id| {
            let expr_id = match &package.get_stmt(stmt_id).kind {
                StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) => *expr_id,
                StmtKind::Local(_, _, _) | StmtKind::Item(_) => return None,
            };
            let ExprKind::While(cond_id, body_id) = &package.get_expr(expr_id).kind else {
                return None;
            };
            Some((*cond_id, *body_id))
        })
        .expect("expected body to contain a while loop")
}

#[test]
fn branch_buried_while_condition_return_survives_normalize_and_is_guarded_by_transform() {
    let source = indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable i = 0;
                while (if i < 3 { return 42 } else { false }) {
                    set i += 1;
                }
                i + 5
            }
        }
    "#};

    // --- After Normalize (hoist fixpoint then ANF fixpoint) ---
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Mono);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    let block_id = find_body_block_id(store.get(pkg_id), "Main");

    let mut errors = Vec::new();
    crate::return_unify::normalize::hoist_returns_to_statement_boundary(
        store.get_mut(pkg_id),
        &mut assigner,
        pkg_id,
        block_id,
        &mut errors,
    );
    crate::return_unify::normalize::run_anf_to_fixpoint(
        store.get_mut(pkg_id),
        &mut assigner,
        pkg_id,
        block_id,
        &mut errors,
    );
    assert!(
        errors.is_empty(),
        "normalization produced errors: {errors:?}"
    );

    // Re-fetch the while node after hoisting; the pre-hoist ExprIds may have
    // been rewritten. The condition must still carry the buried `return` —
    // proving the ANF phase treats the while condition as a leaf.
    let (cond_id, _body_id) = find_while_in_block(store.get(pkg_id), block_id);
    assert!(
        contains_return_in_expr(store.get(pkg_id), cond_id),
        "while condition should still contain a Return after Normalize"
    );

    // --- After Transform (full return unification) ---
    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);

    let (has_returned_pat, _) = find_local_init(package, "Main", symbols::HAS_RETURNED);
    let has_returned_var_id = local_var_id_from_named_pat(has_returned_pat, symbols::HAS_RETURNED);

    let body_block_id = find_body_block_id(package, "Main");
    let (cond_id, _body_id) = find_while_in_block(package, body_block_id);

    // The buried `return` is gone and the condition is guarded:
    // `not __has_returned and <cond>`.
    assert!(
        !contains_return_in_expr(package, cond_id),
        "while condition should no longer contain a Return after Transform"
    );
    assert_while_condition_guarded_by_not_flag(package, cond_id, has_returned_var_id);
}
