// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_ast::{
    ast::{
        BinOp, Expr, ExprKind, FieldAccess, Mutability, NodeId, Package, Pat, PatKind, PathKind,
        Stmt, StmtKind, TernOp, TyKind,
    },
    visit::{Visitor, walk_expr, walk_stmt},
};
use qsc_data_structures::index_map::IndexMap;
use qsc_hir::{
    hir,
    ty::{SizeKind, Ty},
};

use crate::{
    resolve::Res,
    typeck::{Error, ErrorKind, Table},
};

pub fn propagate_array_sizes(
    ast_package: &Package,
    names: &IndexMap<NodeId, Res>,
    table: &mut Table,
) -> Vec<Error> {
    let mut visitor = PropagateSizes {
        names,
        table,
        errors: Vec::new(),
    };
    visitor.visit_package(ast_package);
    visitor.errors
}

struct PropagateSizes<'a> {
    names: &'a IndexMap<NodeId, Res>,
    table: &'a mut Table,
    errors: Vec<Error>,
}

impl PropagateSizes<'_> {
    fn propagate_pat_sizes(&mut self, pat: &Pat, node_id: NodeId) {
        let Some(ref_ty) = self.table.terms.get(node_id) else {
            return;
        };
        if self.table.terms.get(pat.id) != Some(ref_ty) {
            self.propagate_pat_sizes_from_ty(pat, ref_ty.clone());
        }
    }

    fn propagate_pat_sizes_from_ty(&mut self, pat: &Pat, ty: Ty) {
        match (pat.kind.as_ref(), ty) {
            (PatKind::Bind(ident, Some(explicit_ty)), ty) => {
                match (explicit_ty.kind.as_ref(), ty) {
                    (
                        TyKind::Array(_, Some(explicit_size)),
                        Ty::Array(_, SizeKind::Known(size)),
                    ) if *explicit_size > size => {
                        self.errors.push(Error(ErrorKind::ArraySizeMismatch {
                            span: ident.span,
                            expected: *explicit_size,
                            actual: size,
                        }));
                    }
                    _ => {}
                }
            }
            (PatKind::Bind(ident, None), ty) => {
                let Some(mut ident_ty) = self.table.terms.get_mut(ident.id) else {
                    return;
                };
                match (&mut ident_ty, ty) {
                    (Ty::Array(_, size), Ty::Array(_, new_size))
                        if *size == SizeKind::Unknown && new_size != SizeKind::Unknown =>
                    {
                        *size = new_size;
                        let new_ty = ident_ty.clone();
                        self.table.terms.insert(pat.id, new_ty);
                    }
                    _ => {}
                }
            }
            (PatKind::Tuple(pats), Ty::Tuple(tys)) if pats.len() == tys.len() => {
                let mut new_tys = Vec::new();
                for (pat, ty) in pats.iter().zip(tys.iter()) {
                    self.propagate_pat_sizes_from_ty(pat, ty.clone());
                    new_tys.push(
                        self.table
                            .terms
                            .get(pat.id)
                            .expect("type should be present")
                            .clone(),
                    );
                }
                self.table.terms.insert(pat.id, Ty::Tuple(new_tys));
            }
            _ => {}
        }
    }

    fn check_sizes(&mut self, input_ty: &Ty, expr: &Expr) {
        match input_ty {
            Ty::Array(_, SizeKind::Known(explicit_size)) => {
                if let Some(Ty::Array(_, SizeKind::Known(arg_size))) = self.table.terms.get(expr.id)
                    && explicit_size > arg_size
                {
                    self.errors.push(Error(ErrorKind::ArraySizeMismatch {
                        span: expr.span,
                        expected: *explicit_size,
                        actual: *arg_size,
                    }));
                }
            }
            Ty::Tuple(input_tys) => {
                if let ExprKind::Tuple(exprs) = expr.kind.as_ref() {
                    for (input_ty, expr) in input_tys.iter().zip(exprs.iter()) {
                        self.check_sizes(input_ty, expr);
                    }
                } else if let Some(expr_ty) = self.table.terms.get(expr.id)
                    && let Ty::Tuple(expr_tys) = expr_ty
                {
                    for (input_ty, expr_ty) in input_tys.iter().zip(expr_tys.iter()) {
                        if let Ty::Array(_, SizeKind::Known(explicit_size)) = input_ty
                            && let Ty::Array(_, SizeKind::Known(arg_size)) = expr_ty
                            && explicit_size > arg_size
                        {
                            self.errors.push(Error(ErrorKind::ArraySizeMismatch {
                                span: expr.span,
                                expected: *explicit_size,
                                actual: *arg_size,
                            }));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

impl<'a> Visitor<'a> for PropagateSizes<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        walk_stmt(self, stmt);
        match stmt.kind.as_ref() {
            // Immutable local bindings and qubit declarations can propagate and check array sizes from
            // their initializer expressions to their patterns.
            StmtKind::Local(Mutability::Immutable, pat, expr) => {
                self.propagate_pat_sizes(pat, expr.id);
            }
            StmtKind::Qubit(_, pat, init, _) => {
                self.propagate_pat_sizes(pat, init.id);
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_lines)]
    fn visit_expr(&mut self, expr: &'a Expr) {
        walk_expr(self, expr);
        match expr.kind.as_ref() {
            // Addition of two arrays with known sizes results in an array with the sum of their sizes.
            ExprKind::BinOp(BinOp::Add, lhs, rhs) => {
                if let Some(Ty::Array(_, SizeKind::Known(lhs_size))) = self.table.terms.get(lhs.id)
                    && let Some(Ty::Array(_, SizeKind::Known(rhs_size))) =
                        self.table.terms.get(rhs.id)
                {
                    let new_size = lhs_size + rhs_size;
                    if let Some(Ty::Array(_, size)) = self.table.terms.get_mut(expr.id) {
                        *size = SizeKind::Known(new_size);
                    }
                }
            }

            // Verify the types of the call arguments against the function's input type to look for
            // any array size mismatches. Then propagate the output type if the call is not a partial
            // application.
            ExprKind::Call(callee, args) => {
                if let Some(Ty::Arrow(arrow)) = self.table.terms.get(callee.id) {
                    let input_ty = arrow.input.borrow().clone();
                    let output_ty = arrow.output.borrow().clone();
                    if Some(&input_ty) != self.table.terms.get(args.id) {
                        self.check_sizes(&input_ty, args);
                    }
                    if !args_include_hole(args) {
                        self.table.terms.insert(expr.id, output_ty);
                    }
                }
            }

            // Verify the types of field assignments in a struct constructor to look for any array size mismatches.
            ExprKind::Struct(_, _, fields) => {
                let Some(Ty::Udt(_, hir::Res::Item(item_id))) = self.table.terms.get(expr.id)
                else {
                    return;
                };
                let Some(udt) = self.table.udts.get(item_id) else {
                    return;
                };
                let mut field_tys = Vec::new();
                for field in fields {
                    field_tys.push(
                        udt.find_field_by_name(&field.field.name)
                            .map(|f| f.ty.clone()),
                    );
                }
                for (field, field_ty) in fields.iter().zip(field_tys.iter()) {
                    if let Some(field_ty) = field_ty {
                        self.check_sizes(field_ty, &field.value);
                    }
                }
            }

            // Only propagate the known sizes for conditional expressions if both branches have the same size.
            ExprKind::If(_, then_expr, Some(else_expr)) => {
                if let Some(Ty::Array(_, SizeKind::Known(then_size))) =
                    self.table.terms.get(then_expr.id)
                    && let Some(Ty::Array(_, SizeKind::Known(else_size))) =
                        self.table.terms.get(else_expr.id)
                    && then_size == else_size
                {
                    let new_size = *then_size;
                    if let Some(Ty::Array(_, size)) = self.table.terms.get_mut(expr.id) {
                        *size = SizeKind::Known(new_size);
                    }
                }
            }
            ExprKind::TernOp(TernOp::Cond, _, then_expr, else_expr) => {
                if let Some(Ty::Array(_, SizeKind::Known(then_size))) =
                    self.table.terms.get(then_expr.id)
                    && let Some(Ty::Array(_, SizeKind::Known(else_size))) =
                        self.table.terms.get(else_expr.id)
                    && then_size == else_size
                {
                    let new_size = *then_size;
                    if let Some(Ty::Array(_, size)) = self.table.terms.get_mut(expr.id) {
                        *size = SizeKind::Known(new_size);
                    }
                }
            }

            // Propagate the computed types for these expressions to their parent.
            ExprKind::Block(block) | ExprKind::Conjugate(_, block) => {
                if let Some(ty) = self.table.terms.get(block.id) {
                    self.table.terms.insert(expr.id, ty.clone());
                }
            }
            ExprKind::Path(PathKind::Ok(path)) => {
                if let Some(res) = self.names.get(path.id)
                    && let Res::Local(node_id) = res
                    && let Some(ty) = self.table.terms.get(*node_id)
                {
                    // A normal local variable propagates its type to the path expression.
                    self.table.terms.insert(expr.id, ty.clone());
                } else if let Some(segments) = &path.segments
                    && let Some(Ty::Udt(_, hir::Res::Item(item_id))) = self
                        .table
                        .terms
                        .get(segments.last().expect("segments shound have content").id)
                    && let Some(udt) = self.table.udts.get(item_id)
                    && let Some(field) = udt.find_field_by_name(&path.name.name)
                {
                    // A field access propagates the type from the field definition to the path expression
                    // and to the leaf identifier within the path expression.
                    let new_ty = field.ty.clone();
                    self.table.terms.insert(expr.id, new_ty.clone());
                    self.table.terms.insert(path.name.id, new_ty);
                }
            }
            ExprKind::Paren(inner) => {
                if let Some(ty) = self.table.terms.get(inner.id) {
                    self.table.terms.insert(expr.id, ty.clone());
                }
            }
            ExprKind::Tuple(exprs) => {
                let mut tys = Vec::new();
                for expr in exprs {
                    if let Some(ty) = self.table.terms.get(expr.id) {
                        tys.push(ty.clone());
                    } else {
                        return;
                    }
                }
                self.table.terms.insert(expr.id, Ty::Tuple(tys));
            }
            ExprKind::Field(base, FieldAccess::Ok(ident)) => {
                if let Some(Ty::Udt(_, hir::Res::Item(item_id))) = self.table.terms.get(base.id)
                    && let Some(udt) = self.table.udts.get(item_id)
                    && let Some(field) = udt.find_field_by_name(&ident.name)
                {
                    let new_ty = field.ty.clone();
                    self.table.terms.insert(expr.id, new_ty);
                }
            }
            ExprKind::TernOp(TernOp::Update, container, _, _) => {
                if let Some(ty) = self.table.terms.get(container.id) {
                    self.table.terms.insert(expr.id, ty.clone());
                }
            }

            // Explicit returns checked against the expected return type of the callable.
            // TODO: walk callable decl to track current expected return, also verify
            // spec block type after walking the callable body.
            // ExprKind::Return()

            // For all other expressions, we do not need to propagate any type information.
            _ => {}
        }
    }
}

fn args_include_hole(args: &Expr) -> bool {
    match args.kind.as_ref() {
        ExprKind::Tuple(exprs) => exprs.iter().any(|expr| args_include_hole(expr)),
        ExprKind::Paren(inner) => args_include_hole(inner),
        ExprKind::Hole => true,
        _ => false,
    }
}
