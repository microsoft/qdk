// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_ast::{
    ast::{
        BinOp, Block, CallableBody, CallableDecl, Expr, ExprKind, FieldAccess, NodeId, Package,
        Pat, PatKind, PathKind, SpecBody, Stmt, StmtKind, TernOp, TyKind,
    },
    visit::{Visitor, walk_block, walk_callable_decl, walk_expr, walk_stmt},
};
use qsc_data_structures::{index_map::IndexMap, span::Span};
use qsc_hir::{
    hir,
    ty::{SizeKind, Ty},
};
use rustc_hash::FxHashSet;

use crate::{
    resolve::Res,
    typeck::{Error, ErrorKind, Table, convert},
};

pub fn propagate_array_sizes(
    ast_package: &Package,
    names: &IndexMap<NodeId, Res>,
    table: &mut Table,
) -> Vec<Error> {
    let mut reassigned_vars = GatherReassignedVars {
        names,
        reassinged: FxHashSet::default(),
    };
    reassigned_vars.visit_package(ast_package);
    let mut visitor = PropagateSizes {
        names,
        table,
        reassigned: reassigned_vars.reassinged,
        curr_decl_output: None,
        errors: Vec::new(),
    };
    visitor.visit_package(ast_package);
    visitor.errors
}

struct GatherReassignedVars<'a> {
    names: &'a IndexMap<NodeId, Res>,
    reassinged: FxHashSet<NodeId>,
}

impl<'a> GatherReassignedVars<'a> {
    fn mark_reassigned(&mut self, lhs_expr: &'a Expr) {
        match lhs_expr.kind.as_ref() {
            ExprKind::Path(PathKind::Ok(path)) => {
                if let Some(res) = self.names.get(path.id)
                    && let Res::Local(node_id) = res
                {
                    self.reassinged.insert(*node_id);
                }
            }
            ExprKind::Tuple(exprs) => {
                for expr in exprs {
                    self.mark_reassigned(expr);
                }
            }
            _ => {}
        }
    }
}

impl<'a> Visitor<'a> for GatherReassignedVars<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr.kind.as_ref() {
            ExprKind::Assign(lhs, _) | ExprKind::AssignOp(_, lhs, _) => self.mark_reassigned(lhs),
            _ => {}
        }
        walk_expr(self, expr);
    }
}

struct PropagateSizes<'a> {
    names: &'a IndexMap<NodeId, Res>,
    table: &'a mut Table,
    reassigned: FxHashSet<NodeId>,
    curr_decl_output: Option<Ty>,
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
            (PatKind::Bind(ident, None), ty) if !self.reassigned.contains(&ident.id) => {
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
                            .expect("pat type should be present")
                            .clone(),
                    );
                }
                self.table.terms.insert(pat.id, Ty::Tuple(new_tys));
            }
            _ => {}
        }
    }

    fn check_expr_sizes(&mut self, expected_ty: &Ty, expr: &Expr) {
        match expected_ty {
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
            Ty::Tuple(expected_tys) => {
                if let ExprKind::Tuple(exprs) = expr.kind.as_ref() {
                    for (input_ty, expr) in expected_tys.iter().zip(exprs.iter()) {
                        self.check_expr_sizes(input_ty, expr);
                    }
                } else if let Some(expr_ty) = self.table.terms.get(expr.id)
                    && let Ty::Tuple(expr_tys) = expr_ty
                {
                    self.errors
                        .extend(check_ty_sizes(expected_tys, expr_tys, expr.span));
                }
            }
            _ => {}
        }
    }
}

impl<'a> Visitor<'a> for PropagateSizes<'a> {
    fn visit_callable_decl(&mut self, decl: &'a CallableDecl) {
        let output_ty =
            convert::ty_from_ast(self.names, decl.output.as_ref(), &mut Default::default()).0;
        self.curr_decl_output = Some(output_ty);
        walk_callable_decl(self, decl);
        let Some(expected_ty) = self.curr_decl_output.take() else {
            // If we don't know the expected type, we can't use the logic below to validate it.
            return;
        };

        // Check the computed type of the callable implementation against the declaration.
        if let Some(computed_ty) = match decl.body.as_ref() {
            CallableBody::Block(block) => self.table.terms.get(block.id),
            CallableBody::Specs(specs) => {
                // All specs must have the same type, so just check the first one. Callables with multiple explicit
                // specializations must be of type Unit, so it doesn't matter which one we check.
                if let Some(SpecBody::Impl(_, block)) = specs.first().map(|spec| spec.body.clone())
                {
                    self.table.terms.get(block.id)
                } else {
                    None
                }
            }
        } {
            let computed_ty = computed_ty.clone();
            if computed_ty != expected_ty {
                match (expected_ty, computed_ty) {
                    (
                        Ty::Array(_, SizeKind::Known(expected_size)),
                        Ty::Array(_, SizeKind::Known(computed_size)),
                    ) if expected_size > computed_size => {
                        self.errors.push(Error(ErrorKind::ArraySizeMismatch {
                            span: decl.output.span,
                            expected: expected_size,
                            actual: computed_size,
                        }));
                    }
                    (Ty::Tuple(expected_tys), Ty::Tuple(computed_tys)) => {
                        self.errors.extend(check_ty_sizes(
                            &expected_tys,
                            &computed_tys,
                            decl.output.span,
                        ));
                    }
                    _ => {}
                }
            }
        }
    }

    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        walk_stmt(self, stmt);
        match stmt.kind.as_ref() {
            // Local bindings and qubit declarations can propagate and check array sizes from
            // their initializer expressions to their patterns. Only local bindings that are not
            // reassigned are processed, as only those can be guaranteed to have size remain constant
            // during execution.
            StmtKind::Local(_, pat, expr) => {
                self.propagate_pat_sizes(pat, expr.id);
            }
            StmtKind::Qubit(_, pat, init, _) => {
                self.propagate_pat_sizes(pat, init.id);
            }

            // Propagate the type of expression-statements into the statement itself.
            StmtKind::Expr(expr) => {
                if let Some(expr_ty) = self.table.terms.get(expr.id).cloned()
                    && let Some(stmt_ty) = self.table.terms.get_mut(stmt.id)
                {
                    propagate_ty_sizes(&expr_ty, stmt_ty);
                }
            }
            _ => {}
        }
    }

    fn visit_block(&mut self, block: &'a Block) {
        walk_block(self, block);
        // If the last statement in the block is an expression, propagate its type to the block itself.
        if let Some(last_stmt) = block.stmts.last()
            && let StmtKind::Expr(expr) = last_stmt.kind.as_ref()
            && let Some(expr_ty) = self.table.terms.get(expr.id).cloned()
            && let Some(block_ty) = self.table.terms.get_mut(block.id)
        {
            propagate_ty_sizes(&expr_ty, block_ty);
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
                let args = if let ExprKind::Paren(inner_args) = args.kind.as_ref() {
                    inner_args
                } else {
                    args
                };
                if let Some(Ty::Arrow(arrow)) = self.table.terms.get(callee.id) {
                    let input_ty = arrow.input.borrow().clone();
                    let output_ty = arrow.output.borrow().clone();
                    if Some(&input_ty) != self.table.terms.get(args.id) {
                        self.check_expr_sizes(&input_ty, args);
                    }
                    if !args_include_hole(args)
                        && let Some(expr_ty) = self.table.terms.get_mut(expr.id)
                    {
                        propagate_ty_sizes(&output_ty, expr_ty);
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
                        self.check_expr_sizes(field_ty, &field.value);
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
                if let Some(ty) = self.table.terms.get(block.id).cloned()
                    && let Some(expr_ty) = self.table.terms.get_mut(expr.id)
                {
                    propagate_ty_sizes(&ty, expr_ty);
                }
            }
            ExprKind::Path(PathKind::Ok(path)) => {
                if let Some(res) = self.names.get(path.id)
                    && let Res::Local(node_id) = res
                    && let Some(ty) = self.table.terms.get(*node_id).cloned()
                    && let Some(expr_ty) = self.table.terms.get_mut(expr.id)
                {
                    // A normal local variable propagates its type to the path expression.
                    propagate_ty_sizes(&ty, expr_ty);
                } else if let Some(segments) = &path.segments
                    && let Some(Ty::Udt(_, hir::Res::Item(item_id))) = self
                        .table
                        .terms
                        .get(segments.last().expect("segments shound have content").id)
                    && let Some(udt) = self.table.udts.get(item_id)
                    && let Some(field) = udt.find_field_by_name(&path.name.name).cloned()
                {
                    // A field access propagates the type from the field definition to the path expression
                    // and to the leaf identifier within the path expression.
                    if let Some(expr_ty) = self.table.terms.get_mut(expr.id) {
                        propagate_ty_sizes(&field.ty, expr_ty);
                    }
                    if let Some(name_ty) = self.table.terms.get_mut(path.name.id) {
                        propagate_ty_sizes(&field.ty, name_ty);
                    }
                }
            }
            ExprKind::Paren(inner) => {
                if let Some(ty) = self.table.terms.get(inner.id).cloned()
                    && let Some(expr_ty) = self.table.terms.get_mut(expr.id)
                {
                    propagate_ty_sizes(&ty, expr_ty);
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
                if let Some(expr_ty) = self.table.terms.get_mut(expr.id) {
                    propagate_ty_sizes(&Ty::Tuple(tys), expr_ty);
                }
            }
            ExprKind::Field(base, FieldAccess::Ok(ident)) => {
                if let Some(Ty::Udt(_, hir::Res::Item(item_id))) = self.table.terms.get(base.id)
                    && let Some(udt) = self.table.udts.get(item_id)
                    && let Some(field) = udt.find_field_by_name(&ident.name).cloned()
                    && let Some(expr_ty) = self.table.terms.get_mut(expr.id)
                {
                    propagate_ty_sizes(&field.ty, expr_ty);
                }
            }
            ExprKind::TernOp(TernOp::Update, container, _, _) => {
                if let Some(ty) = self.table.terms.get(container.id).cloned()
                    && let Some(expr_ty) = self.table.terms.get_mut(expr.id)
                {
                    propagate_ty_sizes(&ty, expr_ty);
                }
            }

            // Explicit returns checked against the expected return type of the callable.
            ExprKind::Return(expr) => {
                if let Some(output_ty) = &self.curr_decl_output {
                    let expected_ty = output_ty.clone();
                    self.check_expr_sizes(&expected_ty, expr);
                }
            }

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

fn check_ty_sizes(expected_tys: &[Ty], actual_tys: &[Ty], span: Span) -> Vec<Error> {
    let mut errors = Vec::new();
    for (expected_ty, actual_ty) in expected_tys.iter().zip(actual_tys.iter()) {
        if let Ty::Array(_, SizeKind::Known(expected_size)) = expected_ty
            && let Ty::Array(_, SizeKind::Known(actual_size)) = actual_ty
            && expected_size > actual_size
        {
            errors.push(Error(ErrorKind::ArraySizeMismatch {
                span,
                expected: *expected_size,
                actual: *actual_size,
            }));
        } else if let Ty::Tuple(expected_tys) = expected_ty
            && let Ty::Tuple(actual_tys) = actual_ty
        {
            errors.extend(check_ty_sizes(expected_tys, actual_tys, span));
        }
    }
    errors
}

fn propagate_ty_sizes(source: &Ty, target: &mut Ty) {
    match (source, target) {
        (Ty::Array(_, SizeKind::Known(source_size)), Ty::Array(_, target_size))
            if *target_size == SizeKind::Unknown =>
        {
            *target_size = SizeKind::Known(*source_size);
        }
        (Ty::Tuple(source_tys), Ty::Tuple(target_tys)) if source_tys.len() == target_tys.len() => {
            for (source_ty, target_ty) in source_tys.iter().zip(target_tys.iter_mut()) {
                propagate_ty_sizes(source_ty, target_ty);
            }
        }
        _ => {}
    }
}
