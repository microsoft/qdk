// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Hoist-returns pre-pass for the return-unification pass.
//!
//! Rewrites every callable-body block so that any `ExprKind::Return`
//! surviving in a compound (non-statement-carrying) position is lifted to a
//! bare `return v;` statement at the enclosing statement boundary. After
//! this pass, `Return` only appears as:
//!
//! * a `StmtKind::Semi`/`StmtKind::Expr` whose expression is `ExprKind::Return(_)`,
//! * the trailing expression of a block reached through `ExprKind::Block`,
//! * a branch of `ExprKind::If`, or
//! * the body of `ExprKind::While`.
//!
//! The downstream strategy pass (`transform_block_if_else` /
//! `transform_block_with_flags`) consumes that statement-level shape.
//!
//! ## Match exhaustiveness
//!
//! [`hoist_in_expr`] is an exhaustive match over every `ExprKind` variant
//! — no wildcard arm — so introducing a new variant forces a compile error
//! here and at [`super::detect::contains_return_in_expr`].
//!
//! ## Short-circuit special cases
//!
//! The logical `and` / `or` operators evaluate their right-hand side
//! conditionally. A Return in the RHS is handled by rewriting the `BinOp`
//! in place to an equivalent `if` that the strategy pass consumes:
//!
//! ```text
//! a and (return v)  →  if a { return v } else { false }
//! a or  (return v)  →  if a { true } else { return v }
//! ```
//!
//! A Return in the LHS evaluates unconditionally and is hoisted without a
//! guard.
//!
//! ## If / While condition returns
//!
//! A Return in the *condition* of an `If` or `While` fires before either
//! branch / the loop body ever runs.
//!
//! * For `If`, the hoist rewrites the expression in place to a `Block`
//!   whose statements are the hoisted condition (ending in
//!   `Semi(Return(v))`) plus a trailing default value of the original `If`
//!   type, preserving the enclosing block-tail invariant.
//! * For `While`, the hoist lifts condition returns directly to statement
//!   boundary (same as other compounds) so downstream rewriting preserves
//!   callable-level early-exit semantics.

#[cfg(test)]
mod tests;

#[cfg(test)]
mod shape_tests;

use qsc_fir::{
    assigner::Assigner,
    fir::{
        BinOp, Expr, ExprId, ExprKind, Ident, Mutability, Package, PackageId, PackageLookup, Pat,
        PatId, PatKind, Res, Stmt, StmtId, StmtKind, StringComponent,
    },
    ty::Ty,
};

use crate::{
    EMPTY_EXEC_RANGE,
    fir_builder::{alloc_block, alloc_bool_lit, alloc_expr_stmt, alloc_semi_stmt},
};
use qsc_data_structures::span::Span;
use std::rc::Rc;

use super::detect::contains_return_in_expr;

/// Iteration bound for the fixpoint loop — each pass removes at least one
/// compound-position `Return`, so the total expression count dominates.
fn fixpoint_bound(package: &Package) -> usize {
    package.exprs.iter().count() + package.stmts.iter().count() + 1
}

/// Hoist every compound-position `Return` to its enclosing statement boundary.
///
/// Runs to fixpoint across `block_id` and all transitively reachable
/// sub-blocks.
///
/// # Before
/// ```text
/// { let x = f(return v); rest }
/// ```
/// # After
/// ```text
/// { let _ = f; return v; }   // compound-position Return lifted to Semi
/// ```
/// # Requires
/// - `block_id` is a valid block in `package`.
///
/// # Ensures
/// - Returns `true` iff any statement was rewritten.
/// - No `ExprKind::Return` remains in a compound (non-statement-carrying)
///   position; surviving Returns sit only at statement boundaries or inside
///   `Block`/`If`/`While` (which the strategy pass handles).
/// - Panics when the fixpoint bound is exceeded.
///
/// # Mutations
/// - Rewrites `Block.stmts` for each reachable block that hoists a Return.
/// - Allocates fresh statements and expressions through `assigner`.
pub(super) fn hoist_returns_to_statement_boundary(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    block_id: qsc_fir::fir::BlockId,
) -> bool {
    let bound = fixpoint_bound(package);
    let mut changed_any = false;
    for _ in 0..bound {
        let blocks = collect_reachable_blocks(package, block_id);
        let mut changed_this_iter = false;
        for b in blocks {
            if hoist_block_once(package, assigner, package_id, b) {
                changed_this_iter = true;
            }
        }
        if !changed_this_iter {
            return changed_any;
        }
        changed_any = true;
    }
    panic!("hoist_returns_to_statement_boundary exceeded fixpoint bound");
}

/// Collects every block transitively reachable from `root` without crossing
/// a closure boundary. The root itself is always included first.
fn collect_reachable_blocks(
    package: &Package,
    root: qsc_fir::fir::BlockId,
) -> Vec<qsc_fir::fir::BlockId> {
    let mut out = Vec::new();
    let mut seen = rustc_hash::FxHashSet::default();
    visit_block_for_collect(package, root, &mut out, &mut seen);
    out
}

fn visit_block_for_collect(
    package: &Package,
    block_id: qsc_fir::fir::BlockId,
    out: &mut Vec<qsc_fir::fir::BlockId>,
    seen: &mut rustc_hash::FxHashSet<qsc_fir::fir::BlockId>,
) {
    if !seen.insert(block_id) {
        return;
    }
    out.push(block_id);
    let stmts = package.get_block(block_id).stmts.clone();
    for stmt_id in stmts {
        let stmt_kind = package.get_stmt(stmt_id).kind.clone();
        match stmt_kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => {
                visit_expr_for_collect(package, e, out, seen);
            }
            StmtKind::Item(_) => {}
        }
    }
}

fn visit_expr_for_collect(
    package: &Package,
    expr_id: ExprId,
    out: &mut Vec<qsc_fir::fir::BlockId>,
    seen: &mut rustc_hash::FxHashSet<qsc_fir::fir::BlockId>,
) {
    let kind = package.get_expr(expr_id).kind.clone();
    match kind {
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            for e in exprs {
                visit_expr_for_collect(package, e, out, seen);
            }
        }
        ExprKind::ArrayRepeat(a, b)
        | ExprKind::Assign(a, b)
        | ExprKind::AssignOp(_, a, b)
        | ExprKind::BinOp(_, a, b)
        | ExprKind::Call(a, b)
        | ExprKind::Index(a, b)
        | ExprKind::AssignField(a, _, b)
        | ExprKind::UpdateField(a, _, b) => {
            visit_expr_for_collect(package, a, out, seen);
            visit_expr_for_collect(package, b, out, seen);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            visit_expr_for_collect(package, a, out, seen);
            visit_expr_for_collect(package, b, out, seen);
            visit_expr_for_collect(package, c, out, seen);
        }
        ExprKind::Block(b) => visit_block_for_collect(package, b, out, seen),
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
        ExprKind::Fail(e) | ExprKind::Field(e, _) | ExprKind::Return(e) | ExprKind::UnOp(_, e) => {
            visit_expr_for_collect(package, e, out, seen);
        }
        ExprKind::If(cond, body, otherwise) => {
            visit_expr_for_collect(package, cond, out, seen);
            visit_expr_for_collect(package, body, out, seen);
            if let Some(e) = otherwise {
                visit_expr_for_collect(package, e, out, seen);
            }
        }
        ExprKind::Range(start, step, end) => {
            for e in [start, step, end].into_iter().flatten() {
                visit_expr_for_collect(package, e, out, seen);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(c) = copy {
                visit_expr_for_collect(package, c, out, seen);
            }
            for fa in fields {
                visit_expr_for_collect(package, fa.value, out, seen);
            }
        }
        ExprKind::String(components) => {
            for component in components {
                if let StringComponent::Expr(e) = component {
                    visit_expr_for_collect(package, e, out, seen);
                }
            }
        }
        ExprKind::While(cond, block) => {
            visit_expr_for_collect(package, cond, out, seen);
            visit_block_for_collect(package, block, out, seen);
        }
    }
}

/// Runs one hoist pass over a single block's direct statement list.
///
/// Does not descend into nested blocks — those are visited independently by
/// the fixpoint driver.
fn hoist_block_once(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    block_id: qsc_fir::fir::BlockId,
) -> bool {
    let stmts = package.get_block(block_id).stmts.clone();
    let mut new_stmts: Vec<StmtId> = Vec::with_capacity(stmts.len());
    let mut changed = false;
    for stmt_id in stmts {
        if let Some(replacement) = hoist_stmt(package, assigner, package_id, stmt_id) {
            new_stmts.extend(replacement);
            changed = true;
        } else {
            new_stmts.push(stmt_id);
        }
    }
    if changed {
        let block = package.blocks.get_mut(block_id).expect("block not found");
        block.stmts = new_stmts;
    }
    changed
}

/// Attempts to hoist any compound-position `Return` reachable from the
/// statement's surface expression. Returns `Some(replacement_stmts)` if the
/// statement must be replaced, where the last entry is the bare `return v;`.
fn hoist_stmt(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    stmt_id: StmtId,
) -> Option<Vec<StmtId>> {
    let (surface, is_bare_return_form) = match &package.get_stmt(stmt_id).kind {
        StmtKind::Expr(e) | StmtKind::Semi(e) => {
            let is_return = matches!(package.get_expr(*e).kind, ExprKind::Return(_));
            (*e, is_return)
        }
        StmtKind::Local(_, _, e) => (*e, false),
        StmtKind::Item(_) => return None,
    };

    // When the statement is already `Semi(Return(v))` / `Expr(Return(v))`,
    // the Return is at the statement boundary. Recurse into `inner` rather
    // than `surface`: any hoistable Return inside `inner` fires before the
    // outer Return evaluates, so its emitted statements (which already end
    // in a bare `return ...;`) supersede the outer return entirely.
    //
    // If `inner` is a statement-carrying construct (`Block`/`If`/`While`)
    // whose internal Returns sit at statement boundaries, `hoist_in_expr`
    // returns `None` even though `inner` still contains Returns. The
    // strategy pass cannot consume Returns sitting under a Return wrapper,
    // so pin `inner` to a fresh `let __ret_hoist = inner;` binding and
    // return the bound value. The strategy pass then rewrites the Local's
    // initializer through its `LocalInit` handling, and the trailing
    // `Semi(Return(Var))` is canonical.
    //
    // If `inner` has no Returns at all, the statement is already canonical
    // — returning `Some` with a fresh Semi(Return(inner)) wrapping the same
    // expression would let the fixpoint re-replace the statement forever.
    if is_bare_return_form {
        let ExprKind::Return(inner) = package.get_expr(surface).kind else {
            unreachable!()
        };
        if let Some(stmts) = hoist_in_expr(package, assigner, package_id, inner) {
            return Some(stmts);
        }
        if !contains_return_in_expr(package, inner) {
            return None;
        }
        return Some(bind_inner_and_return(package, assigner, surface, inner));
    }

    hoist_in_expr(package, assigner, package_id, surface)
}

/// Hoist any compound-position `Return` out of `expr_id`.
///
/// # Before
/// ```text
/// f(a, return v, c)
/// ```
/// # After
/// ```text
/// [let _ = a; return v;]   // caller splices into enclosing block.stmts
/// ```
/// # Requires
/// - `expr_id` is valid in `package`.
///
/// # Ensures
/// - Returns `Some(stmts)` ending in `Semi(Return(..))` when a Return was lifted.
/// - Returns `None` when the subtree is return-free or the only Returns sit
///   behind a statement-carrying construct (`Block`, `If`, `While`) which the
///   downstream strategy pass handles.
/// - Preserves left-to-right evaluation order of earlier operands via
///   discard-`let` bindings; operands after the hoist point are dropped
///   because their results are dead.
/// - Short-circuit `and`/`or` RHS Returns are guarded with an `if`; LHS
///   Returns are unconditional.
///
/// # Mutations
/// - Allocates new statements and expressions through `assigner`.
/// - Does not rewrite `expr_id`'s own node in place.
#[allow(clippy::match_same_arms)] // Statement-carrying vs leaf arms kept distinct for clarity.
fn hoist_in_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    expr_id: ExprId,
) -> Option<Vec<StmtId>> {
    if !contains_return_in_expr(package, expr_id) {
        return None;
    }
    let kind = package.get_expr(expr_id).kind.clone();
    match kind {
        ExprKind::Return(inner) => {
            // Degenerate `return (return x)`: inner Return fires first.
            if let Some(inner_stmts) = hoist_in_expr(package, assigner, package_id, inner) {
                return Some(inner_stmts);
            }
            // Re-use the existing Return expression as a Semi statement.
            let stmt = alloc_semi_stmt(package, assigner, expr_id, Span::default());
            Some(vec![stmt])
        }

        // Statement-carrying Block: leave to the strategy pass.
        ExprKind::Block(_) => None,

        // If: the strategy pass handles Return in branches, but we must
        // hoist any Return sitting in the *condition* slot because a
        // condition-Return fires before either branch evaluates. Rewrite
        // the whole If in place to a `Block` expression whose statements
        // run the hoist and whose trailing expression supplies a default of
        // the original type so the enclosing block's tail invariant is
        // preserved.
        ExprKind::If(cond, _, _) => hoist_in_cond(package, assigner, package_id, expr_id, cond),
        // While: lift condition returns directly to statement boundary.
        // Rewriting While-in-place to `Block` can hide callable-level
        // early-exit semantics when the While is in statement position.
        ExprKind::While(cond, _) => hoist_in_expr(package, assigner, package_id, cond),

        // Leaves: no sub-expression can hold a Return.
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => None,

        // Short-circuit logical operators: rewrite `a and/or b` in place to
        // an equivalent `if` when the RHS (short-circuited operand) holds
        // the Return, so the Return ends up in a branch of an If that the
        // strategy pass consumes while the BinOp's `Bool` type is preserved.
        ExprKind::BinOp(BinOp::AndL, a, b) => {
            hoist_short_circuit(package, assigner, package_id, expr_id, a, b, true)
        }
        ExprKind::BinOp(BinOp::OrL, a, b) => {
            hoist_short_circuit(package, assigner, package_id, expr_id, a, b, false)
        }

        // Two-operand compounds evaluated left-to-right.
        ExprKind::BinOp(_, a, b)
        | ExprKind::Call(a, b)
        | ExprKind::Index(a, b)
        | ExprKind::ArrayRepeat(a, b)
        | ExprKind::Assign(a, b)
        | ExprKind::AssignOp(_, a, b)
        | ExprKind::AssignField(a, _, b)
        | ExprKind::UpdateField(a, _, b) => hoist_n_ary(package, assigner, package_id, &[a, b]),

        // Three-operand compounds evaluated left-to-right.
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            hoist_n_ary(package, assigner, package_id, &[a, b, c])
        }

        // N-ary compounds.
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            hoist_n_ary(package, assigner, package_id, &exprs)
        }

        // Single-operand compounds — the operand's result is dead after a
        // Return fires, so forward its hoist result directly.
        ExprKind::UnOp(_, e) | ExprKind::Field(e, _) | ExprKind::Fail(e) => {
            hoist_in_expr(package, assigner, package_id, e)
        }

        // Optional operands in left-to-right order.
        ExprKind::Range(start, step, end) => {
            let operands: Vec<ExprId> = [start, step, end].into_iter().flatten().collect();
            hoist_n_ary(package, assigner, package_id, &operands)
        }

        // `copy` (if present) evaluates before field values, in source order.
        ExprKind::Struct(_, copy, fields) => {
            let mut operands: Vec<ExprId> = Vec::with_capacity(fields.len() + 1);
            if let Some(c) = copy {
                operands.push(c);
            }
            for fa in &fields {
                operands.push(fa.value);
            }
            hoist_n_ary(package, assigner, package_id, &operands)
        }

        // Interpolated string components in source order.
        ExprKind::String(components) => {
            let operands: Vec<ExprId> = components
                .into_iter()
                .filter_map(|c| match c {
                    StringComponent::Expr(e) => Some(e),
                    StringComponent::Lit(_) => None,
                })
                .collect();
            hoist_n_ary(package, assigner, package_id, &operands)
        }
    }
}

/// Hoists a compound with operands evaluated strictly left-to-right.
///
/// Finds the first operand whose subtree contains a hoistable `Return`.
/// Every earlier operand is bound to a discard-pattern `let` so its
/// side-effects execute in original source order; operands after the hoist
/// point are dead and dropped.
fn hoist_n_ary(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    operands: &[ExprId],
) -> Option<Vec<StmtId>> {
    for (i, &op) in operands.iter().enumerate() {
        if let Some(op_stmts) = hoist_in_expr(package, assigner, package_id, op) {
            let mut out: Vec<StmtId> = Vec::with_capacity(i + op_stmts.len());
            for &pre in &operands[..i] {
                out.push(create_discard_let_stmt(package, assigner, pre));
            }
            out.extend(op_stmts);
            return Some(out);
        }
    }
    None
}

/// Handles `and`/`or` short-circuit `BinOp`s.
///
/// * LHS Return is unconditional — lifted with no guard.
/// * RHS Return short-circuits: `and` fires only when LHS is `true`,
///   `or` fires only when LHS is `false`. We preserve the `BinOp`'s `Bool`
///   type and semantics by rewriting in place:
///
///   ```text
///   a and b  →  if a { b } else { false }
///   a or  b  →  if a { true } else { b }
///   ```
///
///   The Return now sits in a branch of an `If`, which the strategy pass
///   consumes, so the hoist itself does not need to emit statements.
fn hoist_short_circuit(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    expr_id: ExprId,
    a: ExprId,
    b: ExprId,
    is_and: bool,
) -> Option<Vec<StmtId>> {
    // LHS always evaluates — an LHS Return is unconditional.
    if let Some(stmts_a) = hoist_in_expr(package, assigner, package_id, a) {
        return Some(stmts_a);
    }
    // LHS is clean; any hoistable Return must sit in the RHS.
    if !contains_return_in_expr(package, b) {
        return None;
    }
    let lit_expr = {
        let value = !is_and;
        alloc_bool_lit(package, assigner, value, Span::default())
    };
    let (then_id, else_id) = if is_and { (b, lit_expr) } else { (lit_expr, b) };
    let expr = package.exprs.get_mut(expr_id).expect("expr not found");
    expr.kind = ExprKind::If(a, then_id, Some(else_id));
    None
}

/// Handler for `If` condition returns. If the condition expression holds a
/// `Return`, rewrites the surrounding expression in place to a `Block`
/// expression whose statements execute the hoisted return and whose
/// trailing expression provides a default value of the original expression's
/// type so the enclosing block's tail invariant is preserved.
///
/// The branches / loop body are deliberately dropped: if the condition
/// `Return` fires, control transfers out of the callable before any of
/// them ever evaluates.
fn hoist_in_cond(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    expr_id: ExprId,
    cond: ExprId,
) -> Option<Vec<StmtId>> {
    let stmts = hoist_in_expr(package, assigner, package_id, cond)?;
    let orig_ty = package.get_expr(expr_id).ty.clone();
    let mut block_stmts = stmts;
    if orig_ty != Ty::UNIT {
        let default = super::create_default_value(
            package,
            assigner,
            package_id,
            &orig_ty,
            &rustc_hash::FxHashMap::default(),
        )
        .unwrap_or_else(|| {
            panic!("return_unify: unsupported return type in hoisted condition: {orig_ty:?}")
        });
        block_stmts.push(alloc_expr_stmt(package, assigner, default, Span::default()));
    }
    let block_id = {
        let ty: &Ty = &orig_ty;
        alloc_block(package, assigner, block_stmts, ty.clone(), Span::default())
    };
    let expr = package.exprs.get_mut(expr_id).expect("expr not found");
    expr.kind = ExprKind::Block(block_id);
    // `expr.ty` already matches `orig_ty`; leave it as-is.
    None
}

/// Creates `let _ = expr_id;` — a discard-pattern `Local` whose sole
/// purpose is to preserve the operand's evaluation-order side-effects when
/// a later operand hoists a `Return` that discards the overall compound.
fn create_discard_let_stmt(
    package: &mut Package,
    assigner: &mut Assigner,
    expr_id: ExprId,
) -> StmtId {
    let ty = package.get_expr(expr_id).ty.clone();
    let pat_id: PatId = assigner.next_pat();
    package.pats.insert(
        pat_id,
        Pat {
            id: pat_id,
            span: Span::default(),
            ty,
            kind: PatKind::Discard,
        },
    );
    let stmt_id = assigner.next_stmt();
    package.stmts.insert(
        stmt_id,
        Stmt {
            id: stmt_id,
            span: Span::default(),
            kind: StmtKind::Local(Mutability::Immutable, pat_id, expr_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    stmt_id
}

/// Pins a statement-carrying `inner` (Block/If/While with internal Returns)
/// to a fresh immutable `let __ret_hoist = inner;` binding and rewrites
/// `return_expr` to `Return(Var(__ret_hoist))`, yielding a two-statement
/// replacement for the original `Semi(Return(inner))`.
///
/// # Why
/// The strategy pass cannot rewrite Returns that sit under a `Return`
/// wrapper (its classifier peeks at the top-level stmt expression kind and
/// stops). Binding `inner` to a Local instead exposes those Returns through
/// the `LocalInit` path, which the strategy pass does know how to rewrite.
///
/// # Mutations
/// - Allocates a fresh `LocalVarId`, `PatId`, `StmtId`, and a `Var` `ExprId`.
/// - Mutates `return_expr`'s kind in place from `Return(inner)` to
///   `Return(Var(var_id))`.
///
/// # Returns
/// Two statements, in order: the new `Local(__ret_hoist := inner)` and
/// a fresh `Semi(Return(Var))` reusing `return_expr`.
fn bind_inner_and_return(
    package: &mut Package,
    assigner: &mut Assigner,
    return_expr: ExprId,
    inner: ExprId,
) -> Vec<StmtId> {
    let inner_ty = package.get_expr(inner).ty.clone();
    let local_var_id = assigner.next_local();
    let pat_id = assigner.next_pat();
    package.pats.insert(
        pat_id,
        Pat {
            id: pat_id,
            span: Span::default(),
            ty: inner_ty.clone(),
            kind: PatKind::Bind(Ident {
                id: local_var_id,
                span: Span::default(),
                name: Rc::from("__ret_hoist"),
            }),
        },
    );
    let local_stmt_id = assigner.next_stmt();
    package.stmts.insert(
        local_stmt_id,
        Stmt {
            id: local_stmt_id,
            span: Span::default(),
            kind: StmtKind::Local(Mutability::Immutable, pat_id, inner),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    let var_expr_id = assigner.next_expr();
    package.exprs.insert(
        var_expr_id,
        Expr {
            id: var_expr_id,
            span: Span::default(),
            ty: inner_ty,
            kind: ExprKind::Var(Res::Local(local_var_id), Vec::new()),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    // Rewrite the existing Return expression in place so it now wraps the
    // Var, then wrap it in a fresh Semi statement.
    let ret = package
        .exprs
        .get_mut(return_expr)
        .expect("return expr not found");
    ret.kind = ExprKind::Return(var_expr_id);
    let return_stmt_id = alloc_semi_stmt(package, assigner, return_expr, Span::default());

    vec![local_stmt_id, return_stmt_id]
}
