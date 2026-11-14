// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Helpers for building the Q# AST of a compiled Stim circuit.

use qsc::ast;
use qsc::ast::builder::{
    build_arg_pat, build_assignment_statement, build_attr, build_binary_expr, build_block,
    build_call_args_expr, build_call_expr, build_conjugate_expr, build_continue_stmt,
    build_expr_array_expr, build_if_expr_then_block, build_implicit_return_stmt, build_index_expr,
    build_lit_bool_expr, build_lit_int_expr, build_lit_result_expr, build_local_decl,
    build_operation_with_stmts, build_path_ident_expr, build_path_ident_ty,
    build_stmt_semi_from_expr, build_unary_op_expr, wrap_ty_in_array,
};
use qsc_data_structures::span::Span;

use crate::compiler::{QSharpOp, QUBIT_REGISTER, QubitId, ResultId};

/// `@EntryPoint() operation Main() : Result[] { stmts }`
pub(crate) fn build_main(stmts: Vec<ast::Stmt>, span: Span) -> ast::Item {
    let result_array_ty = wrap_ty_in_array(build_path_ident_ty("Result"));
    build_operation_with_stmts("Main", vec![], result_array_ty, stmts, span, true)
}

/// `@{attr}() operation {name}(arg0 : {param_tys[0]}, …) : {output} { body intrinsic; }`
pub(crate) fn build_intrinsic_decl(
    name: &str,
    attr: &str,
    param_tys: &[&str],
    output: &str,
    span: Span,
) -> ast::Item {
    let mut params: Vec<Box<ast::Pat>> = param_tys
        .iter()
        .enumerate()
        .map(|(i, ty)| {
            Box::new(build_arg_pat(
                format!("arg{i}"),
                span,
                build_path_ident_ty(ty),
            ))
        })
        .collect();
    let input_kind = if params.len() == 1 {
        ast::PatKind::Paren(params.remove(0))
    } else {
        ast::PatKind::Tuple(params.into_boxed_slice())
    };
    let input = ast::Pat {
        kind: Box::new(input_kind),
        span,
        ..Default::default()
    };
    let body = ast::CallableBody::Specs(Box::new([Box::new(ast::SpecDecl {
        id: ast::NodeId::default(),
        span,
        spec: ast::Spec::Body,
        body: ast::SpecBody::Gen(ast::SpecGen::Intrinsic),
    })]));
    let decl = ast::CallableDecl {
        id: ast::NodeId::default(),
        span,
        kind: ast::CallableKind::Operation,
        name: Box::new(ast::Ident {
            span,
            name: name.into(),
            ..Default::default()
        }),
        generics: Box::new([]),
        input: Box::new(input),
        output: Box::new(build_path_ident_ty(output)),
        functors: None,
        body: Box::new(body),
    };
    ast::Item {
        span,
        kind: Box::new(ast::ItemKind::Callable(Box::new(decl))),
        attrs: Box::new([Box::new(build_attr(attr, None::<&str>, span))]),
        ..Default::default()
    }
}

/// `qs[k]`
pub(crate) fn build_qubit_expr(qubit: QubitId, span: Span) -> ast::Expr {
    build_index_expr(
        build_path_ident_expr(QUBIT_REGISTER, span, span),
        build_lit_int_expr(i64::from(qubit), span),
        span,
    )
}

fn record_name(result_id: ResultId) -> String {
    format!("r_{result_id}")
}

/// `r_k`
pub(crate) fn build_record_expr(result_id: ResultId, span: Span) -> ast::Expr {
    build_path_ident_expr(record_name(result_id), span, span)
}

/// `mutable r_k = Zero;`
pub(crate) fn build_record_mutable_decl_stmt(result_id: ResultId, span: Span) -> ast::Stmt {
    let zero = build_lit_result_expr(ast::Result::Zero, span);
    build_local_decl(
        record_name(result_id),
        ast::Mutability::Mutable,
        zero,
        span,
        span,
    )
}

/// `let r_k = value;`
pub(crate) fn build_record_let_stmt(
    result_id: ResultId,
    value: ast::Expr,
    span: Span,
) -> ast::Stmt {
    build_local_decl(
        record_name(result_id),
        ast::Mutability::Immutable,
        value,
        span,
        span,
    )
}

/// `r_k = value;`, for a record declared with [`build_record_mutable_decl_stmt`].
pub(crate) fn build_record_assign_stmt(
    result_id: ResultId,
    value: ast::Expr,
    span: Span,
) -> ast::Stmt {
    build_assignment_statement(build_record_expr(result_id, span), value, span)
}

/// `[r_0, …, r_{count-1}]` as `Main`'s return value.
pub(crate) fn build_records_return_stmt(count: u32, span: Span) -> ast::Stmt {
    let records = (0..count)
        .map(|result_id| build_record_expr(result_id, span))
        .collect();
    build_implicit_return_stmt(build_expr_array_expr(records, span))
}

/// `r_k == value`
pub(crate) fn build_record_equals_expr(
    record: ast::Expr,
    value: ast::Result,
    span: Span,
) -> ast::Expr {
    build_binary_expr(
        false,
        ast::BinOp::Eq,
        record,
        build_lit_result_expr(value, span),
        span,
    )
}

/// `IsLossResult(r_a) or IsLossResult(r_b) or …`
pub(crate) fn build_any_loss_expr(records: Vec<ast::Expr>, span: Span) -> ast::Expr {
    let losses = records
        .into_iter()
        .map(|record| build_qsharp_op_call_expr(QSharpOp::IsLossResult, vec![record], span))
        .collect();
    fold_exprs_with_op(ast::BinOp::OrL, losses, span)
}

/// `{op}`, or `Adjoint {op}` for the adjoint variants such as `SAdj`.
fn build_qsharp_op_callee(op: QSharpOp, span: Span) -> ast::Expr {
    let callee = build_path_ident_expr(op.name(), span, span);
    if op.is_adjoint() {
        build_unary_op_expr(ast::UnOp::Functor(ast::Functor::Adj), callee, span)
    } else {
        callee
    }
}

/// `[Adjoint] {op}(args…)`
pub(crate) fn build_qsharp_op_call_expr(
    op: QSharpOp,
    args: Vec<ast::Expr>,
    span: Span,
) -> ast::Expr {
    let callee = build_qsharp_op_callee(op, span);
    build_call_expr(callee, build_call_args_expr(args), span)
}

/// `Controlled [Adjoint] {op}(controls, target);`
pub(crate) fn build_controlled_qsharp_op_call_stmt(
    op: QSharpOp,
    controls: ast::Expr,
    target: ast::Expr,
    span: Span,
) -> ast::Stmt {
    let callee = build_unary_op_expr(
        ast::UnOp::Functor(ast::Functor::Ctl),
        build_qsharp_op_callee(op, span),
        span,
    );
    let args = build_call_args_expr(vec![controls, target]);
    build_stmt_semi_from_expr(build_call_expr(callee, args, span))
}

/// `{name}(args…)`
pub(crate) fn build_intrinsic_call_expr(name: &str, args: Vec<ast::Expr>, span: Span) -> ast::Expr {
    let callee = build_path_ident_expr(name, span, span);
    build_call_expr(callee, build_call_args_expr(args), span)
}

/// `{name}(args…);`
pub(crate) fn build_intrinsic_call_stmt(name: &str, args: Vec<ast::Expr>, span: Span) -> ast::Stmt {
    build_stmt_semi_from_expr(build_intrinsic_call_expr(name, args, span))
}

/// `within { within } apply { apply };`, or just the `apply` statements if `within` is empty.
pub(crate) fn build_within_apply_stmts(
    within: Vec<ast::Stmt>,
    apply: Vec<ast::Stmt>,
    span: Span,
) -> Vec<ast::Stmt> {
    if within.is_empty() {
        return apply;
    }
    let expr = build_conjugate_expr(build_block(within, span), build_block(apply, span), span);
    vec![build_stmt_semi_from_expr(expr)]
}

/// `within { within } apply { apply; value }`, whose value is `value`'s.
pub(crate) fn build_within_apply_expr(
    within: Vec<ast::Stmt>,
    apply: Vec<ast::Stmt>,
    value: ast::Expr,
    span: Span,
) -> ast::Expr {
    let value_stmt = ast::Stmt {
        span: value.span,
        kind: Box::new(ast::StmtKind::Expr(Box::new(value))),
        ..Default::default()
    };
    let mut apply_stmts = apply;
    apply_stmts.push(value_stmt);
    build_conjugate_expr(
        build_block(within, span),
        build_block(apply_stmts, span),
        span,
    )
}

/// `if condition { continue; }`
pub(crate) fn build_continue_if_stmt(condition: ast::Expr, span: Span) -> ast::Stmt {
    let then_block = build_block(vec![build_continue_stmt(span)], span);
    build_stmt_semi_from_expr(build_if_expr_then_block(condition, then_block, span))
}

/// Combines the expressions left to right with `op`: `a op b op c …`. `op` must be `or` or
/// `!=`, whose identity is `false`, so an empty list folds to `false`.
pub(crate) fn fold_exprs_with_op(op: ast::BinOp, exprs: Vec<ast::Expr>, span: Span) -> ast::Expr {
    let mut exprs = exprs.into_iter();
    let first = exprs
        .next()
        .unwrap_or_else(|| build_lit_bool_expr(false, span));
    exprs.fold(first, |acc, expr| {
        build_binary_expr(false, op, acc, expr, span)
    })
}
