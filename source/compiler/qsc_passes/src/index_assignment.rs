// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use std::mem::take;

use qsc_data_structures::span::Span;
use qsc_hir::{
    assigner::Assigner,
    hir::{BinOp, Block, Expr, ExprKind, Mutability, Stmt, StmtKind},
    mut_visit::{MutVisitor, walk_expr},
    ty::{Prim, Ty},
};

use crate::common::{IdentTemplate, gen_ident};

pub(crate) struct ConvertToWSlash<'a> {
    pub(crate) assigner: &'a mut Assigner,
}

impl ConvertToWSlash<'_> {
    /// Creates local binding statements for each index expression and collects identifier templates.
    ///
    /// # Parameters
    /// - `index_exprs`: A vector of mutable references to boxed index expressions (from LHS of assignment).
    ///
    /// # Returns
    /// A tuple containing:
    /// - A vector of statements binding each index to a local identifier.
    /// - A vector of identifier templates for each index.
    fn create_index_bindings(
        &mut self,
        index_exprs: Vec<&mut Box<Expr>>,
    ) -> (Vec<Stmt>, Vec<IdentTemplate>) {
        let mut stmts = vec![];
        let mut ids = vec![];
        for index in index_exprs.into_iter().rev() {
            let id = gen_ident(self.assigner, "index", index.ty.clone(), index.span);

            // Create binding statement
            let stmt = id.gen_id_init(Mutability::Immutable, *take(index), self.assigner);

            stmts.push(stmt);
            ids.push(id);
        }

        (stmts, ids)
    }

    /// Creates a target expression for either the outermost `w/=` or an inner `w/` operation.
    ///
    /// # Parameters
    /// - `nested_level`: The index of the current nesting level (0 is outermost).
    /// - `index_ids`: Slice of identifier templates for all indices.
    /// - `base_array`: The base array expression being updated.
    ///
    /// # Returns
    /// An `Expr` representing the target array or subarray for the assignment.
    fn create_target_expr(
        &mut self,
        nested_level: usize,
        index_ids: &[IdentTemplate],
        base_array: &Expr,
    ) -> Expr {
        if nested_level == 0 {
            // For the outermost `w/=`, use the base array reference
            let mut array_expr = base_array.clone();
            array_expr.id = self.assigner.next_node();
            array_expr
        } else {
            // For inner `w/`, use the array indexing
            self.create_indexed_expr(nested_level, index_ids, base_array)
        }
    }

    /// Creates an indexed expression for inner `w/` operations.
    ///
    /// # Parameters
    /// - `nested_level`: The index of the current nesting level (0 is outermost).
    /// - `index_ids`: Slice of identifier templates for all indices.
    /// - `base_array`: The base array expression being indexed.
    ///
    /// # Returns
    /// An `Expr` representing the nested indexed array expression.
    fn create_indexed_expr(
        &mut self,
        nested_level: usize,
        index_ids: &[IdentTemplate],
        base_array: &Expr,
    ) -> Expr {
        let mut array_expr = base_array.clone();
        array_expr.id = self.assigner.next_node();
        let mut index_type = array_expr.ty.clone();

        for target_index in &index_ids[..nested_level] {
            // Update the array type based on the index type
            index_type = calculate_indexed_type(index_type, target_index);

            array_expr = Expr {
                id: self.assigner.next_node(),
                span: Span::default(),
                ty: index_type.clone(),
                kind: ExprKind::Index(
                    Box::new(array_expr),
                    Box::new(target_index.gen_local_ref(self.assigner)),
                ),
            };
        }

        array_expr
    }

    /// Creates a `w/` or `w/=` expression.
    ///
    /// # Parameters
    /// - `is_w_slash_eq`: Whether this is the outermost `w/=` (true) or an inner `w/` (false).
    /// - `target_expr`: The array or subarray being updated.
    /// - `index_id`: The identifier template for the current index.
    /// - `value_expr`: The expression to assign.
    /// - `expr_ty`: The type of the overall expression.
    ///
    /// # Returns
    /// An `Expr` representing the update or assignment.
    fn create_update_expr(
        &mut self,
        is_w_slash_eq: bool,
        target_expr: Expr,
        index_id: &IdentTemplate,
        value_expr: Expr,
        expr_ty: &Ty,
    ) -> Expr {
        Expr {
            id: self.assigner.next_node(),
            span: Span::default(),
            ty: if is_w_slash_eq {
                expr_ty.clone()
            } else {
                target_expr.ty.clone()
            },
            kind: if is_w_slash_eq {
                // Outermost `w/=`
                ExprKind::AssignIndex(
                    Box::new(target_expr),
                    Box::new(index_id.gen_local_ref(self.assigner)),
                    Box::new(value_expr),
                )
            } else {
                // Inner `w/`
                ExprKind::UpdateIndex(
                    Box::new(target_expr),
                    Box::new(index_id.gen_local_ref(self.assigner)),
                    Box::new(value_expr),
                )
            },
        }
    }

    /// Creates a block expression containing index bindings and the final `w/` expression.
    ///
    /// # Parameters
    /// - `stmts`: Statements binding each index to a local identifier.
    /// - `assign_expr`: The final `w/=` expression.
    /// - `span`: The span for the block expression.
    /// - `ty`: The type of the block expression.
    ///
    /// # Returns
    /// An `Expr` representing the block with all bindings and the update.
    fn create_block_expr(
        &mut self,
        stmts: Vec<Stmt>,
        assign_expr: Expr,
        span: Span,
        ty: &Ty,
    ) -> Expr {
        // Add the final `w/=` expression as a statement
        let mut all_stmts = stmts;
        all_stmts.push(Stmt {
            id: self.assigner.next_node(),
            span: Span::default(),
            kind: StmtKind::Expr(assign_expr),
        });

        // Wrap all statements in a block expression
        Expr {
            id: self.assigner.next_node(),
            span,
            ty: ty.clone(),
            kind: ExprKind::Block(Block {
                id: self.assigner.next_node(),
                span,
                ty: ty.clone(),
                stmts: all_stmts,
            }),
        }
    }
}

impl MutVisitor for ConvertToWSlash<'_> {
    #[allow(clippy::too_many_lines)]
    fn visit_expr(&mut self, expr: &mut Expr) {
        walk_expr(self, expr);

        if let Some((op, lhs, rhs)) = is_assign_or_assign_op(&mut expr.kind) {
            let mut index_exprs = vec![];
            let mut current = lhs;
            let mut curr_kind = &mut current.kind;

            // Traverse the LHS and collect mutable references to index expressions
            while let ExprKind::Index(array, index) = curr_kind {
                index_exprs.push(index);
                current = array;
                curr_kind = &mut current.kind;
            }

            // If the LHS is not an array index, return early
            if index_exprs.is_empty() || !matches!(curr_kind, ExprKind::Var(_, _)) {
                return;
            }

            // Can't `.clone()` current here, so make a clone manually
            let base_array = Box::new(Expr {
                id: current.id,
                span: current.span,
                ty: current.ty.clone(),
                kind: curr_kind.clone(),
            });

            // Create local binding statements for each index and collect index templates
            let (stmts, index_ids) = self.create_index_bindings(index_exprs);

            // Build the "a <op> b" part
            if let Some(op) = op {
                let target_expr = self.create_target_expr(index_ids.len(), &index_ids, &base_array);
                let bin_expr = Expr {
                    id: self.assigner.next_node(),
                    span: expr.span,
                    ty: expr.ty.clone(),
                    kind: ExprKind::BinOp(*op, Box::new(target_expr), take(rhs)),
                };
                **rhs = bin_expr;
            }

            // Construct the nested `w/` expressions
            let mut update_expr = *take(rhs); // start with the ultimate value and expand out from there
            for (i, id) in index_ids.iter().enumerate().rev() {
                let target_expr = self.create_target_expr(i, &index_ids, &base_array);
                update_expr =
                    self.create_update_expr(i == 0, target_expr, id, update_expr, &expr.ty);
            }

            // Create and replace with the block expression
            *expr = self.create_block_expr(stmts, update_expr, expr.span, &expr.ty);
        }
    }
}

/// Calculates the type resulting from indexing into an array.
///
/// # Parameters
/// - `index_type`: The type of the array being indexed.
/// - `target_index`: The identifier template for the index expression.
///
/// # Returns
/// The resulting type after applying the index. Returns `Ty::Err` if the `index_type` is
/// not an array type or `target_index` is not an integer or range type.
fn calculate_indexed_type(index_type: Ty, target_index: &IdentTemplate) -> Ty {
    if matches!(index_type, Ty::Array(_)) {
        // Check the target_index's type
        match &target_index.ty {
            // If indexing with Range, preserve the array type
            Ty::Prim(Prim::Range) => index_type,
            // For Int, extract the element type
            Ty::Prim(Prim::Int) => {
                if let Ty::Array(element_ty) = index_type {
                    *element_ty
                } else {
                    // If the type is not an array, return an error
                    unreachable!("Already checked for array type above");
                }
            }
            // For any other type, return an error
            _ => Ty::Err,
        }
    } else {
        Ty::Err
    }
}

/// Checks if the given expression kind is an assignment or compound assignment,
/// returning the operator (if any), the left-hand side, and the right-hand side as mutable references.
///
/// # Parameters
/// - `expr_kind`: The expression kind to check.
///
/// # Returns
/// - `Some((op, lhs, rhs))` if `expr_kind` is an assignment (`Assign`) or compound assignment (`AssignOp`),
///   where `op` is `None` for `Assign` and `Some(BinOp)` for `AssignOp`.
/// - `None` if `expr_kind` is not an assignment or compound assignment.
fn is_assign_or_assign_op(
    expr_kind: &mut ExprKind,
) -> Option<(Option<&mut BinOp>, &mut Box<Expr>, &mut Box<Expr>)> {
    match expr_kind {
        ExprKind::Assign(lhs, rhs) => Some((None, lhs, rhs)),
        ExprKind::AssignOp(op, lhs, rhs) => Some((Some(op), lhs, rhs)),
        _ => None,
    }
}
