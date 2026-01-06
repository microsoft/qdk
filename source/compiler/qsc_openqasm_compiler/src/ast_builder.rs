// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::rc::Rc;

use num_bigint::BigInt;

use qsc_ast::ast::{
    self, Attr, Block, CallableBody, CallableDecl, CallableKind, Expr, ExprKind, FieldAssign,
    FunctorExpr, FunctorExprKind, Ident, ImportKind, ImportOrExportDecl, ImportOrExportItem, Item,
    ItemKind, Lit, Mutability, NodeId, Pat, PatKind, Path, PathKind, QubitInit, QubitInitKind,
    QubitSource, Stmt, StmtKind, TopLevelNode, Ty, TyKind,
};
use qsc_data_structures::span::Span;

use qsc_openqasm_parser::{
    parser::ast::{List, list_from_iter},
    semantic::types::Type,
    stdlib::angle::Angle,
};

use crate::types::{ArrayDimensions, Complex};

pub(crate) fn build_unmanaged_qubit_alloc<S>(name: S, stmt_span: Span, name_span: Span) -> Stmt
where
    S: AsRef<str>,
{
    let alloc_ident = Ident {
        name: Rc::from("__quantum__rt__qubit_allocate"),
        ..Default::default()
    };
    let path_expr = Expr {
        kind: Box::new(ExprKind::Path(PathKind::Ok(Box::new(Path {
            segments: build_idents(&["QIR", "Runtime"]),
            name: Box::new(alloc_ident),
            id: NodeId::default(),
            span: Span::default(),
        })))),
        ..Default::default()
    };
    let call_expr = Expr {
        span: stmt_span,
        kind: Box::new(ExprKind::Call(
            Box::new(path_expr),
            Box::new(create_unit_expr(Span::default())),
        )),
        ..Default::default()
    };
    let rhs = call_expr;

    Stmt {
        span: stmt_span,
        kind: Box::new(StmtKind::Local(
            Mutability::Immutable,
            Box::new(Pat {
                kind: Box::new(PatKind::Bind(
                    Box::new(Ident {
                        span: name_span,
                        name: Rc::from(name.as_ref()),
                        ..Default::default()
                    }),
                    None,
                )),
                span: name_span,
                ..Default::default()
            }),
            Box::new(rhs),
        )),
        ..Default::default()
    }
}

pub(crate) fn build_unmanaged_qubit_alloc_array<S>(
    name: S,
    size: u32,
    stmt_span: Span,
    name_span: Span,
    designator_span: Span,
) -> Stmt
where
    S: AsRef<str>,
{
    let alloc_ident = Ident {
        name: Rc::from("AllocateQubitArray"),
        ..Default::default()
    };

    let path_expr = Expr {
        kind: Box::new(ExprKind::Path(PathKind::Ok(Box::new(Path {
            segments: build_idents(&["QIR", "Runtime"]),
            name: Box::new(alloc_ident),
            id: NodeId::default(),
            span: Span::default(),
        })))),
        ..Default::default()
    };
    let call_expr = Expr {
        id: NodeId::default(),
        span: stmt_span,
        kind: Box::new(ExprKind::Call(
            Box::new(path_expr),
            Box::new(Expr {
                kind: Box::new(ExprKind::Paren(Box::new(Expr {
                    id: NodeId::default(),
                    span: designator_span,
                    kind: Box::new(ExprKind::Lit(Box::new(Lit::Int(i64::from(size))))),
                }))),
                span: stmt_span,
                ..Default::default()
            }),
        )),
    };

    Stmt {
        id: NodeId::default(),
        span: stmt_span,
        kind: Box::new(StmtKind::Local(
            Mutability::Immutable,
            Box::new(Pat {
                kind: Box::new(PatKind::Bind(
                    Box::new(Ident {
                        id: NodeId::default(),
                        span: name_span,
                        name: Rc::from(name.as_ref()),
                    }),
                    None,
                )),
                span: name_span,
                ..Default::default()
            }),
            Box::new(call_expr),
        )),
    }
}

pub(crate) fn build_managed_qubit_alloc<S>(name: S, stmt_span: Span, name_span: Span) -> Stmt
where
    S: AsRef<str>,
{
    let qubit_init = QubitInitKind::Single;

    let qubit = QubitInit {
        id: NodeId::default(),
        span: stmt_span,
        kind: Box::new(qubit_init),
    };

    let ident: Ident = Ident {
        id: NodeId::default(),
        span: name_span,
        name: Rc::from(name.as_ref()),
    };
    let qubit_kind = StmtKind::Qubit(
        QubitSource::Fresh,
        Box::new(Pat {
            kind: Box::new(PatKind::Bind(Box::new(ident), None)),
            span: name_span,
            ..Default::default()
        }),
        Box::new(qubit),
        None,
    );
    Stmt {
        kind: Box::new(qubit_kind),
        ..Default::default()
    }
}

pub(crate) fn managed_qubit_alloc_array<S>(
    name: S,
    size: u32,
    stmt_span: Span,
    name_span: Span,
    designator_span: Span,
) -> Stmt
where
    S: AsRef<str>,
{
    let qubit_init = QubitInitKind::Array(Box::new(Expr {
        span: designator_span,
        kind: Box::new(ExprKind::Lit(Box::new(Lit::Int(i64::from(size))))),
        ..Default::default()
    }));

    let qubit = QubitInit {
        span: name_span,
        kind: Box::new(qubit_init),
        ..Default::default()
    };

    let ident: Ident = Ident {
        span: name_span,
        name: Rc::from(name.as_ref()),
        ..Default::default()
    };
    let qubit_kind = StmtKind::Qubit(
        QubitSource::Fresh,
        Box::new(Pat {
            kind: Box::new(PatKind::Bind(Box::new(ident), None)),
            span: name_span,
            ..Default::default()
        }),
        Box::new(qubit),
        None,
    );
    Stmt {
        span: stmt_span,
        kind: Box::new(qubit_kind),
        ..Default::default()
    }
}

pub(crate) fn build_lit_result_expr(value: qsc_ast::ast::Result, span: Span) -> Expr {
    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(ExprKind::Lit(Box::new(Lit::Result(value)))),
    }
}

pub(crate) fn build_lit_result_array_expr<I: IntoIterator<Item = ast::Result>>(
    values: I,
    span: Span,
) -> Expr {
    let exprs: Vec<_> = values
        .into_iter()
        .map(|value| build_lit_result_expr(value, Span::default()))
        .collect();
    build_expr_array_expr(exprs, span)
}

pub(crate) fn build_expr_array_expr(values: Vec<qsc_ast::ast::Expr>, span: Span) -> Expr {
    let exprs: Vec<_> = values.into_iter().map(Box::new).collect();
    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(ExprKind::Array(exprs.into_boxed_slice())),
    }
}

// This will be used to compile arrays in the near future.
#[allow(dead_code)]
pub(crate) fn build_default_result_array_expr(len: usize, span: Span) -> Expr {
    let exprs: Vec<_> = (0..len)
        .map(|_| Box::new(build_lit_result_expr(ast::Result::Zero, Span::default())))
        .collect();
    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(ExprKind::Array(exprs.into_boxed_slice())),
    }
}

pub(crate) fn build_lit_bigint_expr(value: BigInt, span: Span) -> Expr {
    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(ExprKind::Lit(Box::new(Lit::BigInt(Box::new(value))))),
    }
}

pub(crate) fn build_lit_bool_expr(value: bool, span: Span) -> Expr {
    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(ExprKind::Lit(Box::new(Lit::Bool(value)))),
    }
}

pub(crate) fn build_lit_int_expr(value: i64, span: Span) -> Expr {
    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(ExprKind::Lit(Box::new(Lit::Int(value)))),
    }
}

fn build_ident(name: &str) -> Ident {
    Ident {
        name: Rc::from(name),
        ..Default::default()
    }
}

pub(crate) fn build_lit_angle_expr(angle: Angle, span: Span) -> Expr {
    let path_kind = build_angle_ident_pathkind();
    let value_expr = Box::new(Expr {
        #[allow(clippy::cast_possible_wrap)]
        kind: Box::new(ExprKind::Lit(Box::new(Lit::Int(angle.value as i64)))),
        ..Default::default()
    });
    let size_expr = Box::new(Expr {
        kind: Box::new(ExprKind::Lit(Box::new(Lit::Int(i64::from(angle.size))))),
        ..Default::default()
    });

    let fields = list_from_iter([
        FieldAssign {
            span,
            field: Box::new(build_ident("Value")),
            value: value_expr,
            ..Default::default()
        },
        FieldAssign {
            span,
            field: Box::new(build_ident("Size")),
            value: size_expr,
            ..Default::default()
        },
    ]);

    let kind = Box::new(ExprKind::Struct(path_kind, None, fields));

    Expr {
        id: NodeId::default(),
        span,
        kind,
    }
}

pub(crate) fn build_lit_complex_expr(value: Complex, span: Span) -> Expr {
    let real = build_lit_double_expr(value.real, Span::default());
    let img = build_lit_double_expr(value.imaginary, Span::default());
    build_math_call_from_exprs("Complex", vec![real, img], span)
}

pub(crate) fn build_complex_from_expr(expr: Expr) -> Expr {
    let img = build_lit_double_expr(0.0, Span::default());
    let span = expr.span;
    build_math_call_from_exprs("Complex", vec![expr, img], span)
}

pub(crate) fn build_binary_expr(
    is_assignment: bool,
    qsop: ast::BinOp,
    lhs: ast::Expr,
    rhs: ast::Expr,
    span: Span,
) -> ast::Expr {
    let expr_kind = if is_assignment {
        ast::ExprKind::AssignOp(qsop, Box::new(lhs), Box::new(rhs))
    } else {
        ast::ExprKind::BinOp(qsop, Box::new(lhs), Box::new(rhs))
    };
    ast::Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(expr_kind),
    }
}

pub(crate) fn build_math_call_from_exprs(name: &str, exprs: Vec<Expr>, span: Span) -> Expr {
    let alloc_ident = Ident {
        name: Rc::from(name),
        ..Default::default()
    };
    let path_expr = Expr {
        kind: Box::new(ExprKind::Path(PathKind::Ok(Box::new(Path {
            segments: build_idents(&["Std", "Math"]),
            name: Box::new(alloc_ident),
            id: NodeId::default(),
            span: Span::default(),
        })))),
        ..Default::default()
    };
    let exprs: Vec<_> = exprs.into_iter().map(Box::new).collect();
    let kind = if exprs.is_empty() {
        ExprKind::Tuple(Box::new([]))
    } else if exprs.len() == 1 {
        ExprKind::Paren(exprs[0].clone())
    } else {
        ExprKind::Tuple(exprs.into_boxed_slice())
    };
    let call = ExprKind::Call(
        Box::new(path_expr),
        Box::new(Expr {
            kind: Box::new(kind),
            ..Default::default()
        }),
    );
    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(call),
    }
}

pub(crate) fn build_path_ident_expr<S: AsRef<str>>(
    name: S,
    name_span: Span,
    expr_span: Span,
) -> ast::Expr {
    let ident = ast::Ident {
        id: NodeId::default(),
        span: name_span,
        name: Rc::from(name.as_ref()),
    };
    let path = ast::Path {
        id: NodeId::default(),
        span: name_span,
        segments: None,
        name: Box::new(ident),
    };
    let path_kind = ast::ExprKind::Path(PathKind::Ok(Box::new(path)));
    ast::Expr {
        id: NodeId::default(),
        span: expr_span,
        kind: Box::new(path_kind),
    }
}

pub(crate) fn build_assignment_statement(lhs: Expr, rhs: Expr, assignment_span: Span) -> ast::Stmt {
    let expr_kind = ast::ExprKind::Assign(Box::new(lhs), Box::new(rhs));
    let expr = ast::Expr {
        id: NodeId::default(),
        span: assignment_span,
        kind: Box::new(expr_kind),
    };
    let semi = ast::StmtKind::Semi(Box::new(expr));
    ast::Stmt {
        id: NodeId::default(),
        span: assignment_span,
        kind: Box::new(semi),
    }
}

pub(crate) fn build_convert_call_expr(expr: Expr, name: &str) -> Expr {
    let span = expr.span;
    let cast_ident = Ident {
        name: Rc::from(name),
        ..Default::default()
    };
    let path_expr = Expr {
        kind: Box::new(ExprKind::Path(PathKind::Ok(Box::new(Path {
            segments: build_idents(&["Std", "Convert"]),
            name: Box::new(cast_ident),
            id: NodeId::default(),
            span: Span::default(),
        })))),
        ..Default::default()
    };
    let call = ExprKind::Call(
        Box::new(path_expr),
        Box::new(Expr {
            kind: Box::new(ExprKind::Paren(Box::new(expr))),
            ..Default::default()
        }),
    );

    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(call),
    }
}

pub(crate) fn build_array_reverse_expr(expr: Expr) -> Expr {
    let span = expr.span;
    let cast_ident = Ident {
        name: Rc::from("Reversed"),
        ..Default::default()
    };
    let path_expr = Expr {
        kind: Box::new(ExprKind::Path(PathKind::Ok(Box::new(Path {
            segments: build_idents(&["Std", "Arrays"]),
            name: Box::new(cast_ident),
            id: NodeId::default(),
            span: Span::default(),
        })))),
        ..Default::default()
    };
    let call = ExprKind::Call(
        Box::new(path_expr),
        Box::new(Expr {
            kind: Box::new(ExprKind::Paren(Box::new(expr))),
            ..Default::default()
        }),
    );

    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(call),
    }
}

#[allow(clippy::similar_names)]
pub(crate) fn build_range_expr(
    start: Option<Expr>,
    step: Option<Expr>,
    stop: Option<Expr>,
    span: Span,
) -> Expr {
    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(ExprKind::Range(
            start.map(Box::new),
            step.map(Box::new),
            stop.map(Box::new),
        )),
    }
}

pub(crate) fn build_if_expr_then_block_else_expr(
    cond: Expr,
    then_expr: Block,
    else_expr: Option<Expr>,
    span: Span,
) -> Expr {
    let else_expr = else_expr.map(Box::new);
    let if_kind = ExprKind::If(Box::new(cond), Box::new(then_expr), else_expr);

    ast::Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(if_kind),
    }
}

pub(crate) fn build_if_expr_then_expr_else_expr(
    cond: Expr,
    then_expr: Expr,
    else_expr: Expr,
    span: Span,
) -> Expr {
    let if_kind = ExprKind::If(
        Box::new(cond),
        Box::new(build_expr_wrapped_block_expr(then_expr)),
        Some(Box::new(build_wrapped_block_expr(
            build_expr_wrapped_block_expr(else_expr),
        ))),
    );

    ast::Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(if_kind),
    }
}

pub(crate) fn build_if_expr_then_block_else_block(
    cond: Expr,
    then_block: Block,
    else_block: Block,
    span: Span,
) -> Expr {
    let if_kind = ExprKind::If(
        Box::new(cond),
        Box::new(then_block),
        Some(Box::new(build_wrapped_block_expr(else_block))),
    );

    ast::Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(if_kind),
    }
}

pub(crate) fn build_if_expr_then_block(cond: Expr, then_block: Block, span: Span) -> Expr {
    let if_kind = ExprKind::If(Box::new(cond), Box::new(then_block), None);

    ast::Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(if_kind),
    }
}

pub(crate) fn build_convert_cast_call_by_name(
    name: &str,
    expr: ast::Expr,
    name_span: Span,
    operand_span: Span,
) -> ast::Expr {
    build_qasm_convert_call_with_one_param(name, expr, name_span, operand_span)
}

pub(crate) fn build_qasm_convert_call_with_one_param<S: AsRef<str>>(
    name: S,
    expr: ast::Expr,
    name_span: Span,
    operand_span: Span,
) -> ast::Expr {
    let expr_span = expr.span;
    let path_kind = build_qasmstd_convert_pathkind(name.as_ref(), name_span);
    let callee_expr = ast::Expr {
        id: NodeId::default(),
        span: name_span,
        kind: Box::new(ast::ExprKind::Path(path_kind)),
    };

    let param_expr_kind = ast::ExprKind::Paren(Box::new(expr));

    let param_expr = ast::Expr {
        kind: Box::new(param_expr_kind),
        span: operand_span,
        ..Default::default()
    };
    let call_kind = ast::ExprKind::Call(Box::new(callee_expr), Box::new(param_expr));
    ast::Expr {
        kind: Box::new(call_kind),
        span: expr_span,
        ..Default::default()
    }
}

pub(crate) fn build_angle_cast_call_by_name(
    name: &str,
    expr: ast::Expr,
    name_span: Span,
    operand_span: Span,
) -> ast::Expr {
    let call_span = expr.span;
    build_angle_call_with_one_param(name, expr, name_span, operand_span, call_span)
}

pub(crate) fn build_angle_call_with_one_param<S: AsRef<str>>(
    name: S,
    expr: ast::Expr,
    name_span: Span,
    operand_span: Span,
    call_span: Span,
) -> ast::Expr {
    let path_kind = build_angle_pathkind(name.as_ref(), name_span);
    let callee_expr = ast::Expr {
        id: NodeId::default(),
        span: name_span,
        kind: Box::new(ast::ExprKind::Path(path_kind)),
    };

    let param_expr_kind = ast::ExprKind::Paren(Box::new(expr));

    let param_expr = ast::Expr {
        kind: Box::new(param_expr_kind),
        span: operand_span,
        ..Default::default()
    };
    let call_kind = ast::ExprKind::Call(Box::new(callee_expr), Box::new(param_expr));
    ast::Expr {
        kind: Box::new(call_kind),
        span: call_span,
        ..Default::default()
    }
}

pub(crate) fn build_measure_call(
    expr: ast::Expr,
    name_span: Span,
    operand_span: Span,
    stmt_span: Span,
) -> ast::Expr {
    build_call_with_param(
        "M",
        &["Std", "Intrinsic"],
        expr,
        name_span,
        operand_span,
        stmt_span,
    )
}

pub(crate) fn build_measureeachz_call(
    expr: ast::Expr,
    name_span: Span,
    operand_span: Span,
    stmt_span: Span,
) -> ast::Expr {
    build_call_with_param(
        "MeasureEachZ",
        &["Std", "Measurement"],
        expr,
        name_span,
        operand_span,
        stmt_span,
    )
}

pub(crate) fn build_reset_call(expr: ast::Expr, name_span: Span, operand_span: Span) -> ast::Expr {
    build_global_call_with_one_param("Reset", expr, name_span, operand_span)
}

pub(crate) fn build_reset_all_call(
    expr: ast::Expr,
    name_span: Span,
    operand_span: Span,
) -> ast::Expr {
    build_global_call_with_one_param("ResetAll", expr, name_span, operand_span)
}

pub(crate) fn build_global_call_with_one_param<S: AsRef<str>>(
    name: S,
    expr: ast::Expr,
    name_span: Span,
    operand_span: Span,
) -> ast::Expr {
    let expr_span = expr.span;
    let ident = ast::Ident {
        id: NodeId::default(),
        span: name_span,
        name: Rc::from(name.as_ref()),
    };
    let callee_expr = ast::Expr {
        id: NodeId::default(),
        span: name_span,
        kind: Box::new(ast::ExprKind::Path(PathKind::Ok(Box::new(ast::Path {
            id: NodeId::default(),
            span: name_span,
            segments: None,
            name: Box::new(ident),
        })))),
    };

    let param_expr_kind = ast::ExprKind::Paren(Box::new(expr));

    let param_expr = ast::Expr {
        kind: Box::new(param_expr_kind),
        span: operand_span,
        ..Default::default()
    };
    let call_kind = ast::ExprKind::Call(Box::new(callee_expr), Box::new(param_expr));
    ast::Expr {
        kind: Box::new(call_kind),
        span: expr_span,
        ..Default::default()
    }
}

pub(crate) fn build_qasmstd_convert_call_with_two_params<S: AsRef<str>>(
    name: S,
    fst: ast::Expr,
    snd: ast::Expr,
    name_span: Span,
    operand_span: Span,
) -> ast::Expr {
    let callee_expr = ast::Expr {
        id: NodeId::default(),
        span: name_span,
        kind: Box::new(ast::ExprKind::Path(build_qasmstd_convert_pathkind(
            name, name_span,
        ))),
    };

    let param_expr_kind = ast::ExprKind::Tuple(Box::new([Box::new(fst), Box::new(snd)]));

    let param_expr = ast::Expr {
        kind: Box::new(param_expr_kind),
        span: operand_span,
        ..Default::default()
    };
    let call_kind = ast::ExprKind::Call(Box::new(callee_expr), Box::new(param_expr));
    ast::Expr {
        kind: Box::new(call_kind),
        span: name_span,
        ..Default::default()
    }
}

pub(crate) fn build_angle_convert_call_with_two_params<S: AsRef<str>>(
    name: S,
    fst: ast::Expr,
    snd: ast::Expr,
    name_span: Span,
    operand_span: Span,
) -> ast::Expr {
    let callee_expr = ast::Expr {
        id: NodeId::default(),
        span: name_span,
        kind: Box::new(ast::ExprKind::Path(build_angle_pathkind(name, name_span))),
    };

    let param_expr_kind = ast::ExprKind::Tuple(Box::new([Box::new(fst), Box::new(snd)]));

    let param_expr = ast::Expr {
        kind: Box::new(param_expr_kind),
        span: operand_span,
        ..Default::default()
    };
    let call_kind = ast::ExprKind::Call(Box::new(callee_expr), Box::new(param_expr));
    ast::Expr {
        kind: Box::new(call_kind),
        span: name_span,
        ..Default::default()
    }
}

pub(crate) fn build_gate_call_with_params_and_callee(
    param_expr: ast::Expr,
    callee_expr: Expr,
    expr_span: Span,
) -> ast::Expr {
    let call_kind = ast::ExprKind::Call(Box::new(callee_expr), Box::new(param_expr));
    ast::Expr {
        kind: Box::new(call_kind),
        span: expr_span,
        ..Default::default()
    }
}

pub fn build_gate_call_param_expr(args: Vec<Expr>, remaining: usize) -> Expr {
    if args.len() == 1 && remaining > 0 {
        return args[0].clone();
    }
    let param_expr_kind = if args.len() == 1 {
        ast::ExprKind::Paren(Box::new(args[0].clone()))
    } else {
        let args: Vec<_> = args.into_iter().map(Box::new).collect();
        ast::ExprKind::Tuple(args.into_boxed_slice())
    };
    ast::Expr {
        kind: Box::new(param_expr_kind),
        ..Default::default()
    }
}

pub(crate) fn build_math_call_no_params(name: &str, span: Span) -> Expr {
    build_call_no_params(name, &["Std", "Math"], span, span)
}

pub(crate) fn build_call_no_params(
    name: &str,
    idents: &[&str],
    fn_call_span: Span,
    fn_name_span: Span,
) -> Expr {
    let segments = build_idents(idents);
    let fn_name = Ident {
        name: Rc::from(name),
        ..Default::default()
    };
    let path_expr = Expr {
        kind: Box::new(ExprKind::Path(PathKind::Ok(Box::new(Path {
            segments,
            name: Box::new(fn_name),
            id: NodeId::default(),
            span: fn_name_span,
        })))),
        span: fn_name_span,
        ..Default::default()
    };
    let call = ExprKind::Call(
        Box::new(path_expr),
        Box::new(create_unit_expr(Span::default())),
    );

    Expr {
        id: NodeId::default(),
        span: fn_call_span,
        kind: Box::new(call),
    }
}

pub(crate) fn build_call_stmt_no_params(
    name: &str,
    idents: &[&str],
    fn_call_span: Span,
    fn_name_span: Span,
) -> Stmt {
    build_stmt_semi_from_expr(build_call_no_params(
        name,
        idents,
        fn_call_span,
        fn_name_span,
    ))
}

pub(crate) fn build_call_with_param(
    name: &str,
    idents: &[&str],
    operand: Expr,
    name_span: Span,
    operand_span: Span,
    call_span: Span,
) -> Expr {
    let segments = build_idents(idents);
    let fn_name = Ident {
        name: Rc::from(name),
        span: name_span,
        ..Default::default()
    };
    let path_expr = Expr {
        kind: Box::new(ExprKind::Path(PathKind::Ok(Box::new(Path {
            segments,
            name: Box::new(fn_name),
            id: NodeId::default(),
            span: name_span,
        })))),
        span: name_span,
        ..Default::default()
    };
    let call = ExprKind::Call(
        Box::new(path_expr),
        Box::new(Expr {
            kind: Box::new(ExprKind::Paren(Box::new(operand))),
            span: operand_span,
            ..Default::default()
        }),
    );

    Expr {
        id: NodeId::default(),
        span: call_span,
        kind: Box::new(call),
    }
}

pub(crate) fn build_call_with_params(
    name: &str,
    idents: &[&str],
    operands: Vec<Expr>,
    name_span: Span,
    call_span: Span,
) -> Expr {
    let segments = build_idents(idents);
    let fn_name = Ident {
        name: Rc::from(name),
        span: name_span,
        ..Default::default()
    };
    let path_expr = Expr {
        kind: Box::new(ExprKind::Path(PathKind::Ok(Box::new(Path {
            segments,
            name: Box::new(fn_name),
            id: NodeId::default(),
            span: name_span,
        })))),
        span: name_span,
        ..Default::default()
    };
    let call = ExprKind::Call(Box::new(path_expr), Box::new(build_tuple_expr(operands)));

    Expr {
        id: NodeId::default(),
        span: call_span,
        kind: Box::new(call),
    }
}

pub(crate) fn build_lit_double_expr(value: f64, span: Span) -> Expr {
    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(ExprKind::Lit(Box::new(Lit::Double(value)))),
    }
}

pub(crate) fn build_stmt_semi_from_expr(expr: Expr) -> Stmt {
    Stmt {
        id: NodeId::default(),
        span: expr.span,
        kind: Box::new(StmtKind::Semi(Box::new(expr))),
    }
}

pub(crate) fn build_stmt_semi_from_expr_with_span(expr: Expr, span: Span) -> Stmt {
    Stmt {
        id: NodeId::default(),
        span,
        kind: Box::new(StmtKind::Semi(Box::new(expr))),
    }
}

pub(crate) fn build_wrapped_block_expr(block: Block) -> Expr {
    Expr {
        id: NodeId::default(),
        span: block.span,
        kind: Box::new(ast::ExprKind::Block(Box::new(block))),
    }
}

pub(crate) fn build_expr_wrapped_block_expr(expr: Expr) -> Block {
    Block {
        id: NodeId::default(),
        span: expr.span,
        stmts: Box::new([Box::new(Stmt {
            span: expr.span,
            kind: Box::new(StmtKind::Expr(Box::new(expr))),
            ..Default::default()
        })]),
    }
}

pub(crate) fn build_qasm_import_decl() -> Vec<Stmt> {
    build_qasm_import_items()
        .into_iter()
        .map(|item| Stmt {
            kind: Box::new(StmtKind::Item(Box::new(item))),
            span: Span::default(),
            id: NodeId::default(),
        })
        .collect()
}

pub(crate) fn build_qasm_import_items() -> Vec<Item> {
    vec![build_qasm_import_decl_intrinsic()]
}

pub(crate) fn build_qasm_import_decl_intrinsic() -> Item {
    let path_kind = Path {
        segments: Some(Box::new([build_ident("Std"), build_ident("OpenQASM")])),
        name: Box::new(build_ident("Intrinsic")),
        id: NodeId::default(),
        span: Span::default(),
    };
    let items = vec![ImportOrExportItem {
        span: Span::default(),
        path: PathKind::Ok(Box::new(path_kind)),
        kind: ImportKind::Wildcard,
    }];
    let decl = ImportOrExportDecl::new(Span::default(), items.into_boxed_slice(), false);
    Item {
        id: NodeId::default(),
        span: Span::default(),
        kind: Box::new(ItemKind::ImportOrExport(decl)),
        doc: "".into(),
        attrs: Box::new([]),
    }
}

pub(crate) fn build_classical_decl<S>(
    name: S,
    is_const: bool,
    ty_span: Span,
    decl_span: Span,
    name_span: Span,
    ty: &crate::types::Type,
    expr: Expr,
) -> Stmt
where
    S: AsRef<str>,
{
    const USE_IMPLICIT_TYPE_DEF: bool = true;

    let ident = Ident {
        id: NodeId::default(),
        span: name_span,
        name: name.as_ref().into(),
    };

    let result_ty_ident = Ident {
        name: ty.to_string().into(),
        span: ty_span,
        ..Default::default()
    };
    let result_ty_path = ast::PathKind::Ok(Box::new(ast::Path {
        name: Box::new(result_ty_ident),
        segments: None,
        id: NodeId::default(),
        span: ty_span,
    }));
    let result_ty_kind = ast::TyKind::Path(result_ty_path);

    let tydef = if USE_IMPLICIT_TYPE_DEF {
        None
    } else {
        Some(Box::new(ast::Ty {
            id: NodeId::default(),
            span: ty_span,
            kind: Box::new(result_ty_kind),
        }))
    };

    let pat = Pat {
        kind: Box::new(PatKind::Bind(Box::new(ident), tydef)),
        span: name_span,
        ..Default::default()
    };
    let mutability = if is_const {
        Mutability::Immutable
    } else {
        Mutability::Mutable
    };
    let local = StmtKind::Local(mutability, Box::new(pat), Box::new(expr));
    Stmt {
        id: NodeId::default(),
        span: decl_span,
        kind: Box::new(local),
    }
}

pub(crate) fn build_tuple_expr(output_exprs: Vec<Expr>) -> Expr {
    let boxed_exprs = output_exprs.into_iter().map(Box::new).collect::<Vec<_>>();
    let tuple_expr_kind = ExprKind::Tuple(boxed_exprs.into_boxed_slice());
    Expr {
        kind: Box::new(tuple_expr_kind),
        ..Default::default()
    }
}

pub(crate) fn build_implicit_return_stmt(output_expr: Expr) -> Stmt {
    Stmt {
        kind: Box::new(StmtKind::Expr(Box::new(output_expr))),
        ..Default::default()
    }
}

pub(crate) fn build_path_ident_ty<S: AsRef<str>>(name: S) -> Ty {
    let ident = ast::Ident {
        name: Rc::from(name.as_ref()),
        ..Default::default()
    };
    let path = ast::PathKind::Ok(Box::new(ast::Path {
        name: Box::new(ident),
        segments: Option::default(),
        id: NodeId::default(),
        span: Span::default(),
    }));
    let kind = TyKind::Path(path);
    Ty {
        kind: Box::new(kind),
        ..Default::default()
    }
}

fn build_angle_ident_pathkind() -> PathKind {
    build_angle_pathkind("Angle", Span::default())
}

fn build_qasmstd_convert_pathkind<S: AsRef<str>>(name: S, span: Span) -> PathKind {
    let alloc_ident = build_ident(name.as_ref());
    PathKind::Ok(Box::new(Path {
        segments: build_idents(&["Std", "OpenQASM", "Convert"]),
        name: Box::new(alloc_ident),
        id: NodeId::default(),
        span,
    }))
}

fn build_angle_pathkind<S: AsRef<str>>(name: S, span: Span) -> PathKind {
    let alloc_ident = build_ident(name.as_ref());
    PathKind::Ok(Box::new(Path {
        segments: build_idents(&["Std", "OpenQASM", "Angle"]),
        name: Box::new(alloc_ident),
        id: NodeId::default(),
        span,
    }))
}

pub(crate) fn build_angle_ty_ident() -> Ty {
    let path_kind = build_angle_ident_pathkind();
    let kind = TyKind::Path(path_kind);
    Ty {
        kind: Box::new(kind),
        ..Default::default()
    }
}

pub(crate) fn build_complex_ty_ident() -> Ty {
    let ident = ast::Ident {
        name: Rc::from("Complex"),
        ..Default::default()
    };
    let path = ast::PathKind::Ok(Box::new(ast::Path {
        name: Box::new(ident),
        segments: build_idents(&["Std", "Math"]),
        id: NodeId::default(),
        span: Span::default(),
    }));
    let kind = TyKind::Path(path);
    Ty {
        kind: Box::new(kind),
        ..Default::default()
    }
}

pub(crate) fn build_top_level_ns_with_items<S: AsRef<str>>(
    whole_span: Span,
    ns: S,
    items: Vec<ast::Item>,
) -> TopLevelNode {
    TopLevelNode::Namespace(qsc_ast::ast::Namespace {
        id: NodeId::default(),
        span: whole_span,
        name: [Ident {
            name: Rc::from(ns.as_ref()),
            span: Span::default(),
            id: NodeId::default(),
        }]
        .into(),
        items: items
            .into_iter()
            .map(Box::new)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
        doc: "".into(),
    })
}

pub(crate) fn build_operation_with_stmts<S: AsRef<str>>(
    name: S,
    input_pats: Vec<Pat>,
    output_ty: Ty,
    stmts: Vec<ast::Stmt>,
    whole_span: Span,
    add_entry_point: bool,
) -> ast::Item {
    let mut attrs = vec![];
    // If there are no input parameters, add an attribute to mark this
    // as an entry point. We will get a Q# compilation error if we
    // attribute an operation with EntryPoint and it has input parameters.
    if input_pats.is_empty() && add_entry_point {
        attrs.push(Box::new(qsc_ast::ast::Attr {
            id: NodeId::default(),
            span: Span::default(),
            name: Box::new(qsc_ast::ast::Ident {
                name: Rc::from("EntryPoint"),
                ..Default::default()
            }),
            arg: Box::new(create_unit_expr(Span::default())),
        }));
    }
    let input_pats = input_pats.into_iter().map(Box::new).collect::<Vec<_>>();
    let input = match input_pats.len() {
        0 => Box::new(Pat {
            kind: Box::new(qsc_ast::ast::PatKind::Tuple(input_pats.into_boxed_slice())),
            ..Default::default()
        }),
        1 => Box::new(Pat {
            kind: Box::new(qsc_ast::ast::PatKind::Paren(input_pats[0].clone())),
            ..Default::default()
        }),
        _ => Box::new(qsc_ast::ast::Pat {
            kind: Box::new(qsc_ast::ast::PatKind::Tuple(input_pats.into_boxed_slice())),
            ..Default::default()
        }),
    };

    let stmts = stmts
        .into_iter()
        .map(Box::new)
        .collect::<Vec<_>>()
        .into_boxed_slice();
    qsc_ast::ast::Item {
        id: NodeId::default(),
        span: whole_span,
        doc: "".into(),
        attrs: attrs.into_boxed_slice(),
        kind: Box::new(qsc_ast::ast::ItemKind::Callable(Box::new(
            qsc_ast::ast::CallableDecl {
                id: NodeId::default(),
                span: whole_span,
                kind: qsc_ast::ast::CallableKind::Operation,
                name: Box::new(qsc_ast::ast::Ident {
                    name: Rc::from(name.as_ref()),
                    ..Default::default()
                }),
                generics: Box::new([]),
                input,
                output: Box::new(output_ty),
                functors: None,
                body: Box::new(qsc_ast::ast::CallableBody::Block(Box::new(
                    qsc_ast::ast::Block {
                        id: NodeId::default(),
                        span: whole_span,
                        stmts,
                    },
                ))),
            },
        ))),
    }
}

pub(crate) fn build_arg_pat(name: String, span: Span, ty: Ty) -> Pat {
    qsc_ast::ast::Pat {
        kind: Box::new(qsc_ast::ast::PatKind::Bind(
            Box::new(qsc_ast::ast::Ident {
                name: Rc::from(name),
                span,
                ..Default::default()
            }),
            Some(Box::new(ty)),
        )),
        span,
        ..Default::default()
    }
}

pub(crate) fn build_unary_op_expr(op: ast::UnOp, expr: ast::Expr, prefix_span: Span) -> ast::Expr {
    ast::Expr {
        span: prefix_span,
        kind: Box::new(ast::ExprKind::UnOp(op, Box::new(expr))),
        ..Default::default()
    }
}

pub(crate) fn map_qsharp_type_to_ast_ty(output_ty: &crate::types::Type, span: Span) -> Ty {
    let mut ty = match output_ty {
        crate::types::Type::Angle(_) => build_angle_ty_ident(),
        crate::types::Type::Result(_) => build_path_ident_ty("Result"),
        crate::types::Type::Qubit => build_path_ident_ty("Qubit"),
        crate::types::Type::BigInt(_) => build_path_ident_ty("BigInt"),
        crate::types::Type::Int(_) => build_path_ident_ty("Int"),
        crate::types::Type::Double(_) => build_path_ident_ty("Double"),
        crate::types::Type::Complex(_) => build_complex_ty_ident(),
        crate::types::Type::Bool(_) => build_path_ident_ty("Bool"),
        crate::types::Type::ResultArray(dims, _) => build_array_type_name("Result", *dims),
        crate::types::Type::QubitArray(dims) => build_array_type_name("Qubit", *dims),
        crate::types::Type::BigIntArray(dims, _) => build_array_type_name("BigInt", *dims),
        crate::types::Type::IntArray(dims, _) => build_array_type_name("Int", *dims),
        crate::types::Type::DoubleArray(dims) => build_array_type_name("Double", *dims),
        crate::types::Type::BoolArray(dims, _) => build_array_type_name("Bool", *dims),
        crate::types::Type::ComplexArray(dims, _) => {
            let ty = build_complex_ty_ident();
            wrap_array_ty_by_dims(*dims, ty)
        }
        crate::types::Type::AngleArray(dims, _) => {
            let ty = build_angle_ty_ident();
            wrap_array_ty_by_dims(*dims, ty)
        }
        crate::types::Type::Callable(_, _, _) | crate::types::Type::Gate(_, _) => {
            unreachable!("Unexpected callable type in AST conversion")
        }
        crate::types::Type::Range => build_path_ident_ty("Range"),
        crate::types::Type::Tuple(tys) => {
            if tys.is_empty() {
                build_path_ident_ty("Unit")
            } else {
                let t = tys
                    .iter()
                    .map(|ty| map_qsharp_type_to_ast_ty(ty, Span::default()))
                    .collect::<Vec<_>>();

                let kind = TyKind::Tuple(t.into_boxed_slice());
                Ty {
                    kind: Box::new(kind),
                    ..Default::default()
                }
            }
        }
        crate::types::Type::Err => Ty::default(),
    };
    ty.span = span;
    ty
}

fn wrap_array_ty_by_dims(dims: impl Into<u32>, mut ty: Ty) -> Ty {
    let dims: u32 = dims.into();
    for _ in 0..dims {
        ty = wrap_ty_in_array(ty);
    }
    ty
}

fn build_array_type_name<S: AsRef<str>>(name: S, dims: ArrayDimensions) -> Ty {
    let name = name.as_ref();
    let ty = build_path_ident_ty(name);
    wrap_array_ty_by_dims(dims, ty)
}

fn wrap_ty_in_array(ty: Ty) -> Ty {
    let kind = TyKind::Array(Box::new(ty));
    Ty {
        kind: Box::new(kind),
        ..Default::default()
    }
}

pub(crate) fn build_for_stmt(
    loop_var_name: &str,
    loop_var_span: Span,
    loop_var_qsharp_ty: &crate::types::Type,
    loop_var_ty_span: Span,
    iter: Expr,
    body: Block,
    stmt_span: Span,
) -> Stmt {
    Stmt {
        kind: Box::new(StmtKind::Expr(Box::new(Expr {
            kind: Box::new(ExprKind::For(
                Box::new(Pat {
                    kind: Box::new(PatKind::Bind(
                        Box::new(Ident {
                            name: loop_var_name.into(),
                            span: loop_var_span,
                            ..Default::default()
                        }),
                        Some(Box::new(map_qsharp_type_to_ast_ty(
                            loop_var_qsharp_ty,
                            loop_var_ty_span,
                        ))),
                    )),
                    span: Span {
                        lo: loop_var_ty_span.lo,
                        hi: loop_var_span.hi,
                    },
                    ..Default::default()
                }),
                Box::new(iter),
                Box::new(body),
            )),
            span: stmt_span,
            ..Default::default()
        }))),
        span: stmt_span,
        ..Default::default()
    }
}

pub(crate) fn wrap_expr_in_parens(expr: Expr, span: Span) -> Expr {
    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(ExprKind::Paren(Box::new(expr))),
    }
}

pub(crate) fn build_while_stmt(expr: Expr, body: Block, stmt_span: Span) -> Stmt {
    Stmt {
        kind: Box::new(StmtKind::Expr(Box::new(Expr {
            kind: Box::new(ExprKind::While(Box::new(expr), Box::new(body))),
            span: stmt_span,
            ..Default::default()
        }))),
        span: stmt_span,
        ..Default::default()
    }
}

pub(crate) fn build_return_expr(expr: Expr, span: Span) -> Expr {
    Expr {
        kind: Box::new(ExprKind::Return(Box::new(expr))),
        span,
        ..Default::default()
    }
}

pub(crate) fn build_return_unit(span: Span) -> Expr {
    build_return_expr(create_unit_expr(span), span)
}

fn create_unit_expr(span: Span) -> Expr {
    Expr {
        span,
        kind: Box::new(ExprKind::Tuple(Box::new([]))),
        ..Default::default()
    }
}

pub(crate) fn build_end_stmt(span: Span) -> Stmt {
    let message = Expr {
        kind: Box::new(ExprKind::Lit(Box::new(Lit::String("end".into())))),
        span,
        ..Default::default()
    };

    let kind = ExprKind::Fail(Box::new(message));

    let expr = Expr {
        kind: Box::new(kind),
        span,
        ..Default::default()
    };

    build_stmt_semi_from_expr_with_span(expr, span)
}

pub(crate) fn build_index_expr(expr: Expr, index_expr: Expr, span: Span) -> Expr {
    let kind = ExprKind::Index(Box::new(expr), Box::new(index_expr));
    Expr {
        kind: Box::new(kind),
        span,
        ..Default::default()
    }
}

pub(crate) fn build_barrier_call(span: Span) -> Stmt {
    let expr = build_call_no_params("__quantum__qis__barrier__body", &[], span, span);
    build_stmt_semi_from_expr(expr)
}

pub(crate) fn build_argument_validation_stmts(name: &String, ty: &Type, span: Span) -> Vec<Stmt> {
    assert!(ty.is_array(), "Expected array type");
    assert!(
        !matches!(ty, Type::DynArrayRef(..)),
        "Unexpected dynamic array type"
    );

    let message = Expr {
        kind: Box::new(ExprKind::Lit(Box::new(Lit::String(
            format!("Argument `{name}` is not compatible with its OpenQASM type `{ty}`.").into(),
        )))),
        span,
        ..Default::default()
    };
    let mut rank = 0;
    let mut stmts = Vec::new();
    let dims = if let Type::BitArray(width, _) = ty {
        vec![*width]
    } else if let Type::QubitArray(width) = ty {
        vec![*width]
    } else if let Type::StaticArrayRef(array_ty) = &ty {
        array_ty.dims.clone().into_iter().collect::<Vec<_>>()
    } else if let Type::Array(array_ty) = &ty {
        array_ty.dims.clone().into_iter().collect::<Vec<_>>()
    } else {
        unreachable!("Expected static array type, got: {ty:?}")
    };

    let fail_expr = Expr {
        kind: Box::new(ExprKind::Fail(Box::new(message.clone()))),
        span: Span::default(),
        id: NodeId::default(),
    };
    let fail_stmt = Stmt {
        kind: Box::new(StmtKind::Expr(Box::new(fail_expr))),
        span: Span::default(),
        id: NodeId::default(),
    };
    let fail_block = Block {
        id: NodeId::default(),
        span,
        stmts: list_from_iter([fail_stmt]),
    };

    for dim in dims {
        if dim == 0 {
            // OpenQASM allows for 0 length arrays and dimension. If we encounter a 0 length dim, stop
            // generating length checks.
            break;
        }

        let len_operand = if rank == 0 {
            rank += 1;
            build_path_ident_expr(name, span, span)
        } else {
            rank += 1;
            let index_expr = build_lit_int_expr(0, span);
            let ident = build_path_ident_expr(name, span, span);
            let mut expr = build_index_expr(ident, index_expr.clone(), span);
            for _ in 2..rank {
                expr = build_index_expr(expr, index_expr.clone(), span);
            }

            expr
        };

        let lhs = build_call_with_param("Length", &["Std", "Core"], len_operand, span, span, span);

        let rhs = build_lit_int_expr(dim.into(), span);
        let binop_expr = build_binary_expr(false, ast::BinOp::Neq, lhs, rhs, span);

        let if_expr = Expr {
            kind: Box::new(ExprKind::If(
                Box::new(binop_expr),
                Box::new(fail_block.clone()),
                None,
            )),
            span: Span::default(), // Ensure the stmt has an empty span for debugging
            id: NodeId::default(),
        };
        let stmt = build_stmt_semi_from_expr(if_expr);
        stmts.push(stmt);
    }

    stmts
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn build_function_or_operation(
    name: String,
    cargs: Vec<(String, Ty, Pat, Type)>,
    qargs: Vec<(String, Ty, Pat, Type)>,
    body: Option<Block>,
    name_span: Span,
    body_span: Span,
    gate_span: Span,
    return_type: Ty,
    kind: CallableKind,
    functors: Option<FunctorExpr>,
    attrs: List<Attr>,
) -> Stmt {
    let args = cargs
        .into_iter()
        .chain(qargs)
        .map(|(_, _, pat, _)| Box::new(pat))
        .collect::<Vec<_>>();

    let lo = args
        .iter()
        .min_by_key(|x| x.span.lo)
        .map(|x| x.span.lo)
        .unwrap_or_default();

    let hi = args
        .iter()
        .max_by_key(|x| x.span.hi)
        .map(|x| x.span.hi)
        .unwrap_or_default();

    let input_pat_kind = if args.len() == 1 {
        PatKind::Paren(args[0].clone())
    } else {
        PatKind::Tuple(args.into_boxed_slice())
    };

    let input_pat = Pat {
        kind: Box::new(input_pat_kind),
        span: Span { lo, hi },
        ..Default::default()
    };

    let body = CallableBody::Block(Box::new(body.unwrap_or_else(|| Block {
        id: NodeId::default(),
        span: body_span,
        stmts: Box::new([]),
    })));

    let decl = CallableDecl {
        id: NodeId::default(),
        span: name_span,
        kind,
        name: Box::new(Ident {
            name: name.into(),
            ..Default::default()
        }),
        generics: Box::new([]),
        input: Box::new(input_pat),
        output: Box::new(return_type),
        functors: functors.map(Box::new),
        body: Box::new(body),
    };
    let item = Item {
        span: gate_span,
        kind: Box::new(ast::ItemKind::Callable(Box::new(decl))),
        attrs,
        ..Default::default()
    };

    Stmt {
        kind: Box::new(StmtKind::Item(Box::new(item))),
        span: gate_span,
        ..Default::default()
    }
}

pub(crate) fn build_adj_plus_ctl_functor() -> FunctorExpr {
    let adj = Box::new(FunctorExpr {
        kind: Box::new(FunctorExprKind::Lit(ast::Functor::Adj)),
        id: Default::default(),
        span: Default::default(),
    });

    let ctl = Box::new(FunctorExpr {
        kind: Box::new(FunctorExprKind::Lit(ast::Functor::Ctl)),
        id: Default::default(),
        span: Default::default(),
    });

    FunctorExpr {
        kind: Box::new(FunctorExprKind::BinOp(ast::SetOp::Union, adj, ctl)),
        id: Default::default(),
        span: Default::default(),
    }
}

fn build_idents(idents: &[&str]) -> Option<Box<[Ident]>> {
    let idents = idents
        .iter()
        .map(|name| Ident {
            name: Rc::from(*name),
            ..Default::default()
        })
        .collect::<Vec<_>>();
    if idents.is_empty() {
        None
    } else {
        Some(idents.into())
    }
}

pub(crate) fn build_attr<S, T>(name: S, value: Option<T>, span: Span) -> Attr
where
    S: AsRef<str>,
    T: AsRef<str>,
{
    let name = Box::new(Ident {
        span,
        name: name.as_ref().into(),
        ..Default::default()
    });

    let arg = if let Some(value) = value {
        Box::new(Expr {
            span,
            kind: Box::new(ExprKind::Paren(Box::new(Expr {
                span,
                kind: Box::new(ExprKind::Path(PathKind::Ok(Box::new(Path {
                    id: Default::default(),
                    span,
                    segments: None,
                    name: Box::new(Ident {
                        span,
                        name: value.as_ref().into(),
                        ..Default::default()
                    }),
                })))),
                id: Default::default(),
            }))),
            id: Default::default(),
        })
    } else {
        Box::new(Expr {
            span,
            kind: Box::new(ExprKind::Tuple(Box::default())),
            id: Default::default(),
        })
    };

    Attr {
        span,
        name,
        arg,
        id: Default::default(),
    }
}
