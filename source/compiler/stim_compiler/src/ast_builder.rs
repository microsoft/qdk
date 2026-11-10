// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Helpers for building the Q# AST of a compiled Stim circuit.

use qsc::ast;
use qsc::ast::builder::{
    build_arg_pat, build_attr, build_binary_expr, build_block, build_call_args_expr,
    build_call_expr, build_conjugate_expr, build_continue_stmt, build_if_expr_then_block,
    build_index_expr, build_lit_bool_expr, build_lit_int_expr, build_lit_result_expr,
    build_local_decl, build_operation_with_stmts, build_path_ident_expr, build_path_ident_ty,
    build_stmt_semi_from_expr, build_unary_op_expr,
};
use qsc_data_structures::span::Span;

use crate::compiler::{QSharpOp, QUBIT_REGISTER, QubitId, ResultId};

/// `@EntryPoint() operation Main() : Result[] { stmts }`
pub(crate) fn build_main(stmts: Vec<ast::Stmt>, span: Span) -> ast::Item {
    let result_array_ty = ast::Ty {
        kind: Box::new(ast::TyKind::Array(Box::new(build_path_ident_ty("Result")))),
        ..Default::default()
    };
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

/// `[Adjoint] {op}(args…);`
pub(crate) fn build_qsharp_op_call_stmt(
    op: QSharpOp,
    args: Vec<ast::Expr>,
    span: Span,
) -> ast::Stmt {
    build_stmt_semi_from_expr(build_qsharp_op_call_expr(op, args, span))
}

/// `Controlled [Adjoint] {op}(controls, target);`, where `controls` is an array of qubits.
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

/// `{name}(args…)` for a QDK intrinsic, e.g. `__quantum__qis__peek_loss__body(qs[0])`.
pub(crate) fn build_intrinsic_call_expr(name: &str, args: Vec<ast::Expr>, span: Span) -> ast::Expr {
    let callee = build_path_ident_expr(name, span, span);
    let args = build_call_args_expr(args);
    build_call_expr(callee, args, span)
}

/// `{name}(args…);` for a QDK intrinsic, e.g. a noise intrinsic.
pub(crate) fn build_intrinsic_call_stmt(name: &str, args: Vec<ast::Expr>, span: Span) -> ast::Stmt {
    build_stmt_semi_from_expr(build_intrinsic_call_expr(name, args, span))
}

/// `qs[k]`
pub(crate) fn build_qubit_expr(qubit: QubitId, span: Span) -> ast::Expr {
    build_index_expr(
        build_path_ident_expr(QUBIT_REGISTER, span, span),
        build_lit_int_expr(i64::from(qubit), span),
        span,
    )
}

/// `r_k`
// TODO: change to `r[k]` when mutable arrays are supported.
fn record_name(result_id: ResultId) -> String {
    format!("r_{result_id}")
}

/// `r_k` as an expression.
pub(crate) fn build_record_expr(result_id: ResultId, span: Span) -> ast::Expr {
    build_path_ident_expr(record_name(result_id), span, span)
}

/// `mutable r_k = Zero;`
pub(crate) fn build_record_decl_stmt(result_id: ResultId, span: Span) -> ast::Stmt {
    let zero = build_lit_result_expr(ast::Result::Zero, span);
    build_local_decl(
        record_name(result_id),
        ast::Mutability::Mutable,
        zero,
        span,
        span,
    )
}

/// `r_k == One`, or `r_k == Zero` for a negated record.
pub(crate) fn build_record_bit_expr(result_id: ResultId, negated: bool, span: Span) -> ast::Expr {
    let expected = if negated {
        ast::Result::Zero
    } else {
        ast::Result::One
    };
    build_binary_expr(
        false,
        ast::BinOp::Eq,
        build_record_expr(result_id, span),
        build_lit_result_expr(expected, span),
        span,
    )
}

/// `IsLossResult(r_k)`
pub(crate) fn build_is_loss_expr(result_id: ResultId, span: Span) -> ast::Expr {
    build_qsharp_op_call_expr(
        QSharpOp::IsLossResult,
        vec![build_record_expr(result_id, span)],
        span,
    )
}

/// `within { within } apply { apply };`, or just the `apply` statements if `within` is empty.
pub(crate) fn build_conjugate_stmts(
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

/// `if {name} { continue; }`
pub(crate) fn build_continue_if_stmt(name: &str, span: Span) -> ast::Stmt {
    let then_block = build_block(vec![build_continue_stmt(span)], span);
    build_stmt_semi_from_expr(build_if_expr_then_block(
        build_path_ident_expr(name, span, span),
        then_block,
        span,
    ))
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
