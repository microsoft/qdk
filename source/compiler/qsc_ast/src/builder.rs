// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Helpers for building Q# AST nodes, shared by the compilers that generate Q# ASTs.

use std::rc::Rc;

use num_bigint::BigInt;
use qsc_data_structures::span::Span;

use crate::ast::{
    self, Attr, BinOp, Block, CallableBody, CallableDecl, CallableKind, Expr, ExprKind,
    FieldAccess, Functor, FunctorExpr, FunctorExprKind, Ident, Item, ItemKind, Lit, Mutability,
    Namespace, NamespaceKind, NodeId, Pat, PatKind, Path, PathKind, QubitInit, QubitInitKind,
    QubitSource, SetOp, Stmt, StmtKind, TopLevelNode, Ty, TyKind, UnOp,
};

#[must_use]
pub fn boxed_list_from_iter<T>(iter: impl IntoIterator<Item = T>) -> Box<[Box<T>]> {
    iter.into_iter().map(Box::new).collect()
}

#[must_use]
pub fn build_managed_qubit_alloc<S: AsRef<str>>(
    name: S,
    stmt_span: Span,
    name_span: Span,
    source: QubitSource,
) -> Stmt {
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
        source,
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

#[must_use]
pub fn managed_qubit_alloc_array<S: AsRef<str>>(
    name: S,
    size: u32,
    stmt_span: Span,
    name_span: Span,
    designator_span: Span,
    source: QubitSource,
) -> Stmt {
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
        source,
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

#[must_use]
pub fn build_lit_result_expr(value: ast::Result, span: Span) -> Expr {
    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(ExprKind::Lit(Box::new(Lit::Result(value)))),
    }
}

#[must_use]
pub fn build_lit_result_array_expr<I: IntoIterator<Item = ast::Result>>(
    values: I,
    span: Span,
) -> Expr {
    let exprs: Vec<_> = values
        .into_iter()
        .map(|value| build_lit_result_expr(value, Span::default()))
        .collect();
    build_expr_array_expr(exprs, span)
}

#[must_use]
pub fn build_expr_array_expr(values: Vec<Expr>, span: Span) -> Expr {
    let exprs: Vec<_> = values.into_iter().map(Box::new).collect();
    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(ExprKind::Array(exprs.into_boxed_slice())),
    }
}

// This will be used to compile arrays in the near future.
#[must_use]
pub fn build_default_result_array_expr(len: usize, span: Span) -> Expr {
    let exprs: Vec<_> = (0..len)
        .map(|_| Box::new(build_lit_result_expr(ast::Result::Zero, Span::default())))
        .collect();
    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(ExprKind::Array(exprs.into_boxed_slice())),
    }
}

#[must_use]
pub fn build_lit_bigint_expr(value: BigInt, span: Span) -> Expr {
    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(ExprKind::Lit(Box::new(Lit::BigInt(Box::new(value))))),
    }
}

#[must_use]
pub fn build_lit_bool_expr(value: bool, span: Span) -> Expr {
    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(ExprKind::Lit(Box::new(Lit::Bool(value)))),
    }
}

#[must_use]
pub fn build_lit_int_expr(value: i64, span: Span) -> Expr {
    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(ExprKind::Lit(Box::new(Lit::Int(value)))),
    }
}

#[must_use]
pub fn build_ident(name: &str) -> Ident {
    Ident {
        name: Rc::from(name),
        ..Default::default()
    }
}

#[must_use]
pub fn build_complex_from_expr(expr: Expr) -> Expr {
    let img = build_lit_double_expr(0.0, Span::default());
    let span = expr.span;
    build_math_call_from_exprs("Complex", vec![expr, img], span)
}

#[must_use]
pub fn build_field_expr(expr: Expr, field: &str, span: Span) -> Expr {
    let field = FieldAccess::Ok(Box::new(build_ident(field)));
    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(ExprKind::Field(Box::new(expr), field)),
    }
}

#[must_use]
pub fn build_binary_expr(
    is_assignment: bool,
    qsop: BinOp,
    lhs: Expr,
    rhs: Expr,
    span: Span,
) -> Expr {
    let expr_kind = if is_assignment {
        ExprKind::AssignOp(qsop, Box::new(lhs), Box::new(rhs))
    } else {
        ExprKind::BinOp(qsop, Box::new(lhs), Box::new(rhs))
    };
    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(expr_kind),
    }
}

#[must_use]
pub fn build_math_call_from_exprs(name: &str, exprs: Vec<Expr>, span: Span) -> Expr {
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

#[must_use]
pub fn build_path_ident_expr<S: AsRef<str>>(name: S, name_span: Span, expr_span: Span) -> Expr {
    let ident = Ident {
        id: NodeId::default(),
        span: name_span,
        name: Rc::from(name.as_ref()),
    };
    let path = Path {
        id: NodeId::default(),
        span: name_span,
        segments: None,
        name: Box::new(ident),
    };
    let path_kind = ExprKind::Path(PathKind::Ok(Box::new(path)));
    Expr {
        id: NodeId::default(),
        span: expr_span,
        kind: Box::new(path_kind),
    }
}

#[must_use]
pub fn build_assignment_statement(lhs: Expr, rhs: Expr, assignment_span: Span) -> Stmt {
    let expr_kind = ExprKind::Assign(Box::new(lhs), Box::new(rhs));
    let expr = Expr {
        id: NodeId::default(),
        span: assignment_span,
        kind: Box::new(expr_kind),
    };
    let semi = StmtKind::Semi(Box::new(expr));
    Stmt {
        id: NodeId::default(),
        span: assignment_span,
        kind: Box::new(semi),
    }
}

#[must_use]
pub fn build_convert_call_expr(expr: Expr, name: &str) -> Expr {
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

#[must_use]
pub fn build_array_reverse_expr(expr: Expr) -> Expr {
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
#[must_use]
pub fn build_range_expr(
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

#[must_use]
pub fn build_if_expr_then_block_else_expr(
    cond: Expr,
    then_expr: Block,
    else_expr: Option<Expr>,
    span: Span,
) -> Expr {
    let else_expr = else_expr.map(Box::new);
    let if_kind = ExprKind::If(Box::new(cond), Box::new(then_expr), else_expr);

    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(if_kind),
    }
}

#[must_use]
pub fn build_if_expr_then_expr_else_expr(
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

    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(if_kind),
    }
}

#[must_use]
pub fn build_if_expr_then_block_else_block(
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

    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(if_kind),
    }
}

#[must_use]
pub fn build_if_expr_then_block(cond: Expr, then_block: Block, span: Span) -> Expr {
    let if_kind = ExprKind::If(Box::new(cond), Box::new(then_block), None);

    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(if_kind),
    }
}

#[must_use]
pub fn build_measure_call(
    expr: Expr,
    name_span: Span,
    operand_span: Span,
    stmt_span: Span,
) -> Expr {
    build_call_with_param(
        "M",
        &["Std", "Intrinsic"],
        expr,
        name_span,
        operand_span,
        stmt_span,
    )
}

#[must_use]
pub fn build_measureeachz_call(
    expr: Expr,
    name_span: Span,
    operand_span: Span,
    stmt_span: Span,
) -> Expr {
    build_call_with_param(
        "MeasureEachZ",
        &["Std", "Measurement"],
        expr,
        name_span,
        operand_span,
        stmt_span,
    )
}

#[must_use]
pub fn build_reset_call(expr: Expr, name_span: Span, operand_span: Span) -> Expr {
    build_global_call_with_one_param("Reset", expr, name_span, operand_span)
}

#[must_use]
pub fn build_reset_all_call(expr: Expr, name_span: Span, operand_span: Span) -> Expr {
    build_global_call_with_one_param("ResetAll", expr, name_span, operand_span)
}

#[must_use]
pub fn build_global_call_with_one_param<S: AsRef<str>>(
    name: S,
    expr: Expr,
    name_span: Span,
    operand_span: Span,
) -> Expr {
    let expr_span = expr.span;
    let ident = Ident {
        id: NodeId::default(),
        span: name_span,
        name: Rc::from(name.as_ref()),
    };
    let callee_expr = Expr {
        id: NodeId::default(),
        span: name_span,
        kind: Box::new(ExprKind::Path(PathKind::Ok(Box::new(Path {
            id: NodeId::default(),
            span: name_span,
            segments: None,
            name: Box::new(ident),
        })))),
    };

    let param_expr_kind = ExprKind::Paren(Box::new(expr));

    let param_expr = Expr {
        kind: Box::new(param_expr_kind),
        span: operand_span,
        ..Default::default()
    };
    let call_kind = ExprKind::Call(Box::new(callee_expr), Box::new(param_expr));
    Expr {
        kind: Box::new(call_kind),
        span: expr_span,
        ..Default::default()
    }
}

#[must_use]
pub fn build_call_expr(callee: Expr, args: Expr, call_span: Span) -> Expr {
    Expr {
        kind: Box::new(ExprKind::Call(Box::new(callee), Box::new(args))),
        span: call_span,
        ..Default::default()
    }
}

#[must_use]
pub fn build_call_args_expr(mut args: Vec<Expr>) -> Expr {
    let kind = if args.len() == 1 {
        ExprKind::Paren(Box::new(args.remove(0)))
    } else {
        ExprKind::Tuple(boxed_list_from_iter(args))
    };
    Expr {
        kind: Box::new(kind),
        ..Default::default()
    }
}

#[must_use]
pub fn build_math_call_no_params(name: &str, span: Span) -> Expr {
    build_call_no_params(name, &["Std", "Math"], span, span)
}

#[must_use]
pub fn build_call_no_params(
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

#[must_use]
pub fn build_call_stmt_no_params(
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

#[must_use]
pub fn build_call_with_param(
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

#[must_use]
pub fn build_call_with_params(
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

#[must_use]
pub fn build_lit_double_expr(value: f64, span: Span) -> Expr {
    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(ExprKind::Lit(Box::new(Lit::Double(value)))),
    }
}

#[must_use]
pub fn build_stmt_semi_from_expr(expr: Expr) -> Stmt {
    Stmt {
        id: NodeId::default(),
        span: expr.span,
        kind: Box::new(StmtKind::Semi(Box::new(expr))),
    }
}

#[must_use]
pub fn build_stmt_semi_from_expr_with_span(expr: Expr, span: Span) -> Stmt {
    Stmt {
        id: NodeId::default(),
        span,
        kind: Box::new(StmtKind::Semi(Box::new(expr))),
    }
}

#[must_use]
pub fn build_wrapped_block_expr(block: Block) -> Expr {
    Expr {
        id: NodeId::default(),
        span: block.span,
        kind: Box::new(ExprKind::Block(Box::new(block))),
    }
}

#[must_use]
pub fn build_block(stmts: Vec<Stmt>, span: Span) -> Block {
    Block {
        id: NodeId::default(),
        span,
        stmts: boxed_list_from_iter(stmts),
    }
}

#[must_use]
pub fn build_expr_wrapped_block_expr(expr: Expr) -> Block {
    let span = expr.span;
    build_block(
        vec![Stmt {
            span,
            kind: Box::new(StmtKind::Expr(Box::new(expr))),
            ..Default::default()
        }],
        span,
    )
}

/// `let name = expr;` or `mutable name = expr;`, with the type left to inference.
#[must_use]
pub fn build_local_decl<S: AsRef<str>>(
    name: S,
    mutability: Mutability,
    expr: Expr,
    name_span: Span,
    decl_span: Span,
) -> Stmt {
    let ident = Ident {
        id: NodeId::default(),
        span: name_span,
        name: name.as_ref().into(),
    };
    let pat = Pat {
        kind: Box::new(PatKind::Bind(Box::new(ident), None)),
        span: name_span,
        ..Default::default()
    };
    Stmt {
        id: NodeId::default(),
        span: decl_span,
        kind: Box::new(StmtKind::Local(mutability, Box::new(pat), Box::new(expr))),
    }
}

#[must_use]
pub fn build_tuple_expr(output_exprs: Vec<Expr>) -> Expr {
    let boxed_exprs = output_exprs.into_iter().map(Box::new).collect::<Vec<_>>();
    let tuple_expr_kind = ExprKind::Tuple(boxed_exprs.into_boxed_slice());
    Expr {
        kind: Box::new(tuple_expr_kind),
        ..Default::default()
    }
}

#[must_use]
pub fn build_implicit_return_stmt(output_expr: Expr) -> Stmt {
    Stmt {
        kind: Box::new(StmtKind::Expr(Box::new(output_expr))),
        ..Default::default()
    }
}

#[must_use]
pub fn build_path_ident_ty<S: AsRef<str>>(name: S) -> Ty {
    let ident = Ident {
        name: Rc::from(name.as_ref()),
        ..Default::default()
    };
    let path = PathKind::Ok(Box::new(Path {
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

#[must_use]
pub fn build_complex_ty_ident() -> Ty {
    let ident = Ident {
        name: Rc::from("Complex"),
        ..Default::default()
    };
    let path = PathKind::Ok(Box::new(Path {
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

#[must_use]
pub fn build_top_level_ns_with_items<S: AsRef<str>>(
    whole_span: Span,
    ns: S,
    items: Vec<Item>,
) -> TopLevelNode {
    TopLevelNode::Namespace(Namespace {
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
        kind: NamespaceKind::Implicit,
    })
}

#[must_use]
pub fn build_operation_with_stmts<S: AsRef<str>>(
    name: S,
    input_pats: Vec<Pat>,
    output_ty: Ty,
    stmts: Vec<Stmt>,
    whole_span: Span,
    add_entry_point: bool,
) -> Item {
    let mut attrs = vec![];
    // If there are no input parameters, add an attribute to mark this
    // as an entry point. We will get a Q# compilation error if we
    // attribute an operation with EntryPoint and it has input parameters.
    if input_pats.is_empty() && add_entry_point {
        attrs.push(Box::new(Attr {
            id: NodeId::default(),
            span: Span::default(),
            name: Box::new(Ident {
                name: Rc::from("EntryPoint"),
                ..Default::default()
            }),
            arg: Box::new(create_unit_expr(Span::default())),
        }));
    }
    let input_pats = input_pats.into_iter().map(Box::new).collect::<Vec<_>>();
    let input = match input_pats.len() {
        1 => Box::new(Pat {
            kind: Box::new(PatKind::Paren(input_pats[0].clone())),
            ..Default::default()
        }),
        _ => Box::new(Pat {
            kind: Box::new(PatKind::Tuple(input_pats.into_boxed_slice())),
            ..Default::default()
        }),
    };

    Item {
        id: NodeId::default(),
        span: whole_span,
        doc: "".into(),
        attrs: attrs.into_boxed_slice(),
        kind: Box::new(ItemKind::Callable(Box::new(CallableDecl {
            id: NodeId::default(),
            span: whole_span,
            kind: CallableKind::Operation,
            name: Box::new(Ident {
                name: Rc::from(name.as_ref()),
                ..Default::default()
            }),
            generics: Box::new([]),
            input,
            output: Box::new(output_ty),
            functors: None,
            body: Box::new(CallableBody::Block(Box::new(build_block(
                stmts, whole_span,
            )))),
        }))),
    }
}

#[must_use]
pub fn build_arg_pat(name: String, span: Span, ty: Ty) -> Pat {
    Pat {
        kind: Box::new(PatKind::Bind(
            Box::new(Ident {
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

#[must_use]
pub fn build_unary_op_expr(op: UnOp, expr: Expr, prefix_span: Span) -> Expr {
    Expr {
        span: prefix_span,
        kind: Box::new(ExprKind::UnOp(op, Box::new(expr))),
        ..Default::default()
    }
}

#[must_use]
pub fn wrap_array_ty_by_dims(dims: impl Into<u32>, mut ty: Ty) -> Ty {
    let dims: u32 = dims.into();
    for _ in 0..dims {
        ty = wrap_ty_in_array(ty);
    }
    ty
}

#[must_use]
pub fn wrap_ty_in_array(ty: Ty) -> Ty {
    let kind = TyKind::Array(Box::new(ty));
    Ty {
        kind: Box::new(kind),
        ..Default::default()
    }
}

#[must_use]
pub fn wrap_expr_in_parens(expr: Expr, span: Span) -> Expr {
    Expr {
        id: NodeId::default(),
        span,
        kind: Box::new(ExprKind::Paren(Box::new(expr))),
    }
}

#[must_use]
pub fn build_while_stmt(expr: Expr, body: Block, stmt_span: Span) -> Stmt {
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

#[must_use]
pub fn build_return_expr(expr: Expr, span: Span) -> Expr {
    Expr {
        kind: Box::new(ExprKind::Return(Box::new(expr))),
        span,
        ..Default::default()
    }
}

#[must_use]
pub fn build_return_unit(span: Span) -> Expr {
    build_return_expr(create_unit_expr(span), span)
}

#[must_use]
pub fn create_unit_expr(span: Span) -> Expr {
    Expr {
        span,
        kind: Box::new(ExprKind::Tuple(Box::new([]))),
        ..Default::default()
    }
}

#[must_use]
pub fn build_break_stmt(span: Span) -> Stmt {
    let expr = Expr {
        kind: Box::new(ExprKind::Break),
        span,
        ..Default::default()
    };

    build_stmt_semi_from_expr_with_span(expr, span)
}

#[must_use]
pub fn build_continue_stmt(span: Span) -> Stmt {
    let expr = Expr {
        kind: Box::new(ExprKind::Continue),
        span,
        ..Default::default()
    };

    build_stmt_semi_from_expr_with_span(expr, span)
}

#[must_use]
pub fn build_index_expr(expr: Expr, index_expr: Expr, span: Span) -> Expr {
    let kind = ExprKind::Index(Box::new(expr), Box::new(index_expr));
    Expr {
        kind: Box::new(kind),
        span,
        ..Default::default()
    }
}

#[must_use]
pub fn build_adj_plus_ctl_functor() -> FunctorExpr {
    let adj = Box::new(FunctorExpr {
        kind: Box::new(FunctorExprKind::Lit(Functor::Adj)),
        id: NodeId::default(),
        span: Span::default(),
    });

    let ctl = Box::new(FunctorExpr {
        kind: Box::new(FunctorExprKind::Lit(Functor::Ctl)),
        id: NodeId::default(),
        span: Span::default(),
    });

    FunctorExpr {
        kind: Box::new(FunctorExprKind::BinOp(SetOp::Union, adj, ctl)),
        id: NodeId::default(),
        span: Span::default(),
    }
}

/// Builds a functor expression for adjoint only.
#[must_use]
pub fn build_adj_functor() -> FunctorExpr {
    FunctorExpr {
        kind: Box::new(FunctorExprKind::Lit(Functor::Adj)),
        id: NodeId::default(),
        span: Span::default(),
    }
}

/// Builds a functor expression for controlled only.
#[must_use]
pub fn build_ctl_functor() -> FunctorExpr {
    FunctorExpr {
        kind: Box::new(FunctorExprKind::Lit(Functor::Ctl)),
        id: NodeId::default(),
        span: Span::default(),
    }
}

/// Builds a functor expression based on the required constraints.
/// Returns None if no functors are required.
#[must_use]
pub fn build_functor_from_constraints(
    requires_adj: bool,
    requires_ctl: bool,
) -> Option<FunctorExpr> {
    match (requires_adj, requires_ctl) {
        (true, true) => Some(build_adj_plus_ctl_functor()),
        (true, false) => Some(build_adj_functor()),
        (false, true) => Some(build_ctl_functor()),
        (false, false) => None,
    }
}

#[must_use]
pub fn build_idents(idents: &[&str]) -> Option<Box<[Ident]>> {
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

#[must_use]
pub fn build_attr<S: AsRef<str>, T: AsRef<str>>(name: S, value: Option<T>, span: Span) -> Attr {
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
                    id: NodeId::default(),
                    span,
                    segments: None,
                    name: Box::new(Ident {
                        span,
                        name: value.as_ref().into(),
                        ..Default::default()
                    }),
                })))),
                id: NodeId::default(),
            }))),
            id: NodeId::default(),
        })
    } else {
        Box::new(Expr {
            span,
            kind: Box::new(ExprKind::Tuple(Box::default())),
            id: NodeId::default(),
        })
    };

    Attr {
        span,
        name,
        arg,
        id: NodeId::default(),
    }
}
