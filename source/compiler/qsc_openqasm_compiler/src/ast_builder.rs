// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_ast::ast::{
    self, Attr, Block, CallableBody, CallableDecl, CallableKind, Expr, ExprKind, FieldAssign,
    FunctorExpr, Ident, ImportKind, ImportOrExportDecl, ImportOrExportItem, Item, ItemKind, Lit,
    Mutability, NodeId, Pat, PatKind, Path, PathKind, Stmt, StmtKind, Ty, TyKind,
};
use qsc_ast::builder::{
    boxed_list_from_iter, build_binary_expr, build_block, build_call_no_params,
    build_call_with_param, build_complex_ty_ident, build_ident, build_idents, build_index_expr,
    build_lit_double_expr, build_lit_int_expr, build_local_decl, build_math_call_from_exprs,
    build_path_ident_expr, build_path_ident_ty, build_stmt_semi_from_expr,
    build_stmt_semi_from_expr_with_span, wrap_array_ty_by_dims,
};
use qsc_data_structures::span::Span;

use qdk_openqasm::{semantic::types::Type, stdlib::angle::Angle};

use crate::types::{ArrayDimensions, Complex};

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

    let fields = boxed_list_from_iter([
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
    decl_span: Span,
    name_span: Span,
    expr: Expr,
) -> Stmt
where
    S: AsRef<str>,
{
    let mutability = if is_const {
        Mutability::Immutable
    } else {
        Mutability::Mutable
    };
    build_local_decl(name, mutability, expr, name_span, decl_span)
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

pub(crate) fn map_qsharp_type_to_ast_ty(output_ty: &crate::types::Type, span: Span) -> Ty {
    let mut ty = match output_ty {
        crate::types::Type::Angle => build_angle_ty_ident(),
        crate::types::Type::Result => build_path_ident_ty("Result"),
        crate::types::Type::Qubit => build_path_ident_ty("Qubit"),
        crate::types::Type::BigInt => build_path_ident_ty("BigInt"),
        crate::types::Type::Int => build_path_ident_ty("Int"),
        crate::types::Type::Double => build_path_ident_ty("Double"),
        crate::types::Type::Complex => build_complex_ty_ident(),
        crate::types::Type::Bool => build_path_ident_ty("Bool"),
        crate::types::Type::ResultArray(dims) => build_array_type_name("Result", *dims),
        crate::types::Type::QubitArray(dims) => build_array_type_name("Qubit", *dims),
        crate::types::Type::BigIntArray(dims) => build_array_type_name("BigInt", *dims),
        crate::types::Type::IntArray(dims) => build_array_type_name("Int", *dims),
        crate::types::Type::DoubleArray(dims) => build_array_type_name("Double", *dims),
        crate::types::Type::BoolArray(dims) => build_array_type_name("Bool", *dims),
        crate::types::Type::ComplexArray(dims) => {
            let ty = build_complex_ty_ident();
            wrap_array_ty_by_dims(*dims, ty)
        }
        crate::types::Type::AngleArray(dims) => {
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

fn build_array_type_name<S: AsRef<str>>(name: S, dims: ArrayDimensions) -> Ty {
    let name = name.as_ref();
    let ty = build_path_ident_ty(name);
    wrap_array_ty_by_dims(dims, ty)
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

pub(crate) fn build_barrier_call(span: Span) -> Stmt {
    let expr = build_call_no_params("__quantum__qis__barrier__body", &[], span, span);
    build_stmt_semi_from_expr(expr)
}

pub(crate) fn build_argument_validation_stmts(name: &String, ty: &Type, span: Span) -> Vec<Stmt> {
    assert!(ty.is_array(), "Expected array type");
    assert!(
        !matches!(ty, Type::RankedArrayRef(..)),
        "Unexpected ranked array reference type"
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
    let fail_block = build_block(vec![fail_stmt], span);

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
    attrs: Box<[Box<Attr>]>,
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

    let body = CallableBody::Block(Box::new(
        body.unwrap_or_else(|| build_block(vec![], body_span)),
    ));

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
