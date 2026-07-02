// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Administrative-normal-form (ANF) operand lifting for the
//! return-unification Normalize stage.
//!
//! A `Return` can sit behind a statement-carrying `Block`/`If`/`While` that
//! itself feeds an enclosing operator, call, or binding — an *operand
//! position* the statement-level [`super::hoist_in_expr`] rewrite cannot
//! reach. This module lifts each such operand to a fresh spine `let` binding,
//! innermost-first, so the buried `Return` is exposed to a statement boundary
//! where flag lowering can consume it. Earlier sibling operands are pinned to
//! their own temps first so left-to-right evaluation order (and side effects)
//! are preserved.
//!
//! The module provides three cooperating responsibilities:
//!
//! * **Convergence measure** — [`count_operand_position_returns`] counts the
//!   Returns still buried in operand positions, the companion to the
//!   compound-position measure the fixpoint driver also tracks.
//! * **Pre-check** — [`find_unsupported_operand_lifts`] reports every operand
//!   the lift would attempt but cannot lower soundly. A non-defaultable temp
//!   type (such as `Qubit`) is *not* such a case: the lift backs it with a
//!   length-1 array (`operand.ty[]`, always defaultable), so the pre-check is a
//!   defensive guard for any future non-array-defaultable type, letting the
//!   caller emit a graceful rejection rather than panicking in the slot
//!   machinery.
//! * **Lift** — [`anf_lift_in_expr`] performs a single innermost-first operand
//!   lift, rewriting the operand slot in place and returning the spine `let`
//!   bindings to splice before the enclosing statement.
//!
//! ## Operand-slot consistency contract
//!
//! Four exhaustive `ExprKind` matches enumerate the same operand slots in the
//! same left-to-right order with no wildcard arm:
//! [`count_operand_returns_in_expr`], [`anf_lift_in_expr`],
//! [`scan_operand_tree_for_unsupported_lifts`], and [`replace_operand_slot`].
//! Adding a new `ExprKind` variant forces a compile error in each, keeping the
//! measure, the lift, the pre-check, and the write-back in lockstep.

#[cfg(test)]
mod tests;

use qsc_data_structures::span::Span;
use qsc_fir::{
    assigner::Assigner,
    fir::{
        BinOp, BlockId, ExprId, ExprKind, Mutability, Package, PackageId, PackageLookup, StmtId,
        StmtKind, StringComponent,
    },
    ty::Ty,
};

use crate::{
    EMPTY_EXEC_RANGE,
    return_unify::{is_type_defaultable, slot::singleton_array_index_read},
};
use crate::{
    fir_builder::{alloc_local_var, alloc_local_var_expr},
    return_unify::slot::wrap_in_singleton_array,
};

use super::super::detect::contains_return_in_expr;
use super::collect_reachable_blocks;

/// Drive ANF operand lifting to a fixpoint over the reachable sub-tree of
/// `block_id`.
///
/// Each iteration sweeps every block reachable from `block_id` and applies one
/// innermost-first operand lift per statement. The loop owns a single operand
/// temp counter so every minted `__operand_tmp_<n>` within one specialization
/// body draws a distinct display suffix; the counter advances across fixpoint
/// iterations and never resets until the next call.
///
/// Convergence is tracked by [`count_operand_position_returns`], which must
/// strictly decrease across consecutive changed iterations. A front-loaded hard
/// cap derived from the whole-package node count is a sound over-approximation
/// guarding against unbounded looping.
///
/// On divergence or hard-cap exhaustion, pushes
/// [`super::super::Error::FixpointNotReached`] with the `"anf"` label and
/// returns without panicking.
///
/// Returns `true` iff at least one operand lift fired.
pub(in super::super) fn run_to_fixpoint(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    block_id: BlockId,
    errors: &mut Vec<super::super::Error>,
) -> bool {
    let hard_cap = package.exprs.iter().count() + package.stmts.iter().count() + 1;
    let mut prev_measure: Option<usize> = None;
    let mut changed_any = false;
    // Names each minted ANF operand temp `__operand_tmp_<n>` with a counter
    // scoped to this specialization body, so co-resident temps render with
    // distinct display suffixes. The counter advances across fixpoint
    // iterations and never resets until the next specialization.
    let mut operand_temp_counter: u32 = 0;
    for _ in 0..hard_cap {
        let blocks = collect_reachable_blocks(package, block_id);
        let mut changed_this_iter = false;
        for b in blocks {
            if anf_block_once(package, assigner, package_id, b, &mut operand_temp_counter) {
                changed_this_iter = true;
            }
        }
        if !changed_this_iter {
            return changed_any;
        }
        changed_any = true;
        let measure = count_operand_position_returns(package, block_id);
        if matches!(prev_measure, Some(prev) if measure >= prev) {
            errors.push(super::super::Error::FixpointNotReached("anf", block_id));
            return changed_any;
        }
        prev_measure = Some(measure);
    }
    // Hard cap reached without convergence.
    errors.push(super::super::Error::FixpointNotReached("anf", block_id));
    changed_any
}

/// Runs one ANF operand lift over a single block's direct statement list.
///
/// Does not descend into nested blocks — those are visited independently by
/// [`run_to_fixpoint`]. For each statement, attempts at most one innermost-first
/// operand lift on the statement's surface expression; on success the lifted
/// spine `let` bindings are spliced before the reused original statement.
/// `StmtKind::Item` statements carry no surface expression and are kept as-is.
/// Returns `true` iff any statement was rewritten.
fn anf_block_once(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    block_id: BlockId,
    operand_temp_counter: &mut u32,
) -> bool {
    let stmts = package.get_block(block_id).stmts.clone();
    let mut new_stmts: Vec<StmtId> = Vec::with_capacity(stmts.len());
    let mut changed = false;
    for stmt_id in stmts {
        let surface = match &package.get_stmt(stmt_id).kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => *e,
            StmtKind::Item(_) => {
                new_stmts.push(stmt_id);
                continue;
            }
        };
        if let Some(lifted) =
            anf_lift_in_expr(package, assigner, package_id, surface, operand_temp_counter)
        {
            new_stmts.extend(lifted);
            new_stmts.push(stmt_id);
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

/// Count `ExprKind::Return` nodes sitting in *operand* (eagerly-evaluated
/// subexpression) positions across the reachable sub-tree of `block_id`.
///
/// This is the companion measure to [`super::count_compound_position_returns`].
/// The compound counter is blind to Returns buried behind a
/// statement-carrying `Block`/`If`-branch/`While`-body that nonetheless feeds
/// an enclosing operator, call, or binding — exactly the Returns the ANF
/// operand lift removes. Normalize runs two sequential single-purpose
/// fixpoints, each with its own convergence measure: the hoist fixpoint first
/// drives [`super::count_compound_position_returns`] to zero, then the ANF
/// fixpoint drives this operand measure to zero. This measure strictly
/// decreases across consecutive changed ANF iterations because each lift runs
/// innermost-first.
pub(super) fn count_operand_position_returns(
    package: &Package,
    block_id: qsc_fir::fir::BlockId,
) -> usize {
    let blocks = collect_reachable_blocks(package, block_id);
    let mut count = 0usize;
    for b in blocks {
        for &stmt_id in &package.get_block(b).stmts {
            count += count_operand_returns_in_stmt(package, stmt_id);
        }
    }
    count
}

/// Count operand-position Returns reachable from a single statement's surface
/// expression. The surface is entered in statement (`in_operand = false`)
/// mode: a Return sitting directly at the statement boundary is not
/// operand-embedded, but a Return nested inside any eager compound is.
fn count_operand_returns_in_stmt(package: &Package, stmt_id: StmtId) -> usize {
    match &package.get_stmt(stmt_id).kind {
        StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => {
            count_operand_returns_in_expr(package, *e, false)
        }
        StmtKind::Item(_) => 0,
    }
}

/// Sum the operand-position Returns across a block's statements, carrying the
/// sticky `in_operand` flag of the enclosing context. Used only when an
/// operand-position `Block`/`While`-body is descended (`in_operand == true`).
fn count_operand_returns_in_block(
    package: &Package,
    block_id: qsc_fir::fir::BlockId,
    in_operand: bool,
) -> usize {
    package
        .get_block(block_id)
        .stmts
        .iter()
        .map(|&stmt_id| match &package.get_stmt(stmt_id).kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => {
                count_operand_returns_in_expr(package, *e, in_operand)
            }
            StmtKind::Item(_) => 0,
        })
        .sum()
}

/// Sticky-flag recursion counting Returns in operand positions.
///
/// `in_operand` starts `false` at a statement boundary and flips to `true`
/// on entry to any eagerly-evaluated operand slot, staying `true` through
/// every nested construct underneath (hence "sticky"). A `Return` is counted
/// iff it is reached with `in_operand == true`.
///
/// * `Block` is descended only when already in operand mode (a statement-
///   position block's own statement-boundary Returns are not operand-embedded;
///   they are reached independently via [`count_operand_position_returns`]).
/// * An `If` condition is *always* counted in operand mode: it is evaluated
///   unconditionally before either branch, so a `Return` buried there is
///   operand-position regardless of the `If`'s own context (the ANF lift
///   binds such a condition to a spine temp). A `While` condition keeps the
///   current mode — it is re-evaluated each iteration and is never lifted, so
///   its Returns are handled in place by flag lowering, not this measure. Both
///   descend the branches/body only in operand mode.
/// * Short-circuit `and`/`or` always recurse the LHS (unconditional) but the
///   RHS only in operand mode (its evaluation is conditional).
/// * Every other eager compound recurses each child in operand mode.
fn count_operand_returns_in_expr(package: &Package, expr_id: ExprId, in_operand: bool) -> usize {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Return(inner) => {
            usize::from(in_operand) + count_operand_returns_in_expr(package, *inner, true)
        }
        ExprKind::Block(block_id) => {
            if in_operand {
                count_operand_returns_in_block(package, *block_id, in_operand)
            } else {
                0
            }
        }
        ExprKind::If(cond, then, otherwise) => {
            // The condition is evaluated unconditionally before either branch,
            // so a Return buried there is operand-position and the ANF lift
            // binds the condition to a spine temp: always count it in operand
            // mode.
            let mut count = count_operand_returns_in_expr(package, *cond, true);
            if in_operand {
                count += count_operand_returns_in_expr(package, *then, in_operand);
                if let Some(e) = otherwise {
                    count += count_operand_returns_in_expr(package, *e, in_operand);
                }
            }
            count
        }
        ExprKind::While(cond, body) => {
            let mut count = count_operand_returns_in_expr(package, *cond, in_operand);
            if in_operand {
                count += count_operand_returns_in_block(package, *body, in_operand);
            }
            count
        }
        // Short-circuit logical operators: LHS unconditional, RHS conditional.
        ExprKind::BinOp(BinOp::AndL | BinOp::OrL, a, b) => {
            let mut count = count_operand_returns_in_expr(package, *a, in_operand);
            if in_operand {
                count += count_operand_returns_in_expr(package, *b, in_operand);
            }
            count
        }
        // Single-operand eager compounds.
        ExprKind::Fail(e) | ExprKind::Field(e, _) | ExprKind::UnOp(_, e) => {
            count_operand_returns_in_expr(package, *e, true)
        }
        // Assign family: the lvalue place is provably `Var`/`Hole`/`Tuple`-of-those
        // and can never bury a return, so only the value operand is a real value
        // operand. The place is excluded from the operand-slot contract.
        ExprKind::Assign(_, b) | ExprKind::AssignOp(_, _, b) | ExprKind::AssignField(_, _, b) => {
            count_operand_returns_in_expr(package, *b, true)
        }
        // AssignIndex: the place is excluded; the index `b` and value `c` are
        // genuine rvalue operands.
        ExprKind::AssignIndex(_, b, c) => {
            count_operand_returns_in_expr(package, *b, true)
                + count_operand_returns_in_expr(package, *c, true)
        }
        // Two-operand eager compounds.
        ExprKind::ArrayRepeat(a, b)
        | ExprKind::BinOp(_, a, b)
        | ExprKind::Call(a, b)
        | ExprKind::Index(a, b)
        | ExprKind::UpdateField(a, _, b) => {
            count_operand_returns_in_expr(package, *a, true)
                + count_operand_returns_in_expr(package, *b, true)
        }
        // Three-operand eager compounds. The functional `UpdateIndex` receiver
        // `a` is a genuine value operand and stays enumerated.
        ExprKind::UpdateIndex(a, b, c) => {
            count_operand_returns_in_expr(package, *a, true)
                + count_operand_returns_in_expr(package, *b, true)
                + count_operand_returns_in_expr(package, *c, true)
        }
        // N-ary eager compounds.
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => exprs
            .iter()
            .map(|&e| count_operand_returns_in_expr(package, e, true))
            .sum(),
        ExprKind::Range(start, step, end) => [start, step, end]
            .into_iter()
            .flatten()
            .map(|&e| count_operand_returns_in_expr(package, e, true))
            .sum(),
        ExprKind::Struct(_, copy, fields) => {
            let copy_count = copy.map_or(0, |c| count_operand_returns_in_expr(package, c, true));
            let fields_count: usize = fields
                .iter()
                .map(|fa| count_operand_returns_in_expr(package, fa.value, true))
                .sum();
            copy_count + fields_count
        }
        ExprKind::String(components) => components
            .iter()
            .map(|c| match c {
                StringComponent::Expr(e) => count_operand_returns_in_expr(package, *e, true),
                StringComponent::Lit(_) => 0,
            })
            .sum(),
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => 0,
    }
}

/// Find every operand-position lift the ANF pass would attempt but cannot
/// lower soundly, returning each offending operand's type and span for the
/// caller to surface as a graceful rejection diagnostic.
///
/// One candidate shape is rejected, detected by mirroring the operand
/// classification [`anf_lift_in_expr`] uses, so detection matches exactly what
/// the lift would otherwise do: a statement-carrying `Block`/`If`/`While`
/// reached in an operand position whose type has no synthesizable classical
/// default (realistically `Qubit` or a tuple/UDT containing it). The lift binds
/// it into an immutable `let __operand_tmp_<n> = <candidate>;`; once that
/// return-bearing initializer is processed, flag lowering would need a
/// classical default for the temp's type, which does not exist.
///
/// Projected operand parents (`Range`/`Struct`/`String`) are *not* a rejection
/// reason: each of their eager children has a stable in-place write-back slot
/// (mirrored across the operand-slot contract), so a defaultable projected
/// child lifts like any other operand. Only a genuinely non-defaultable child
/// underneath them is reported, by the same defaultability test applied
/// everywhere.
///
/// Scans every block transitively reachable from `block_id`. Each operand
/// candidate is recorded once; its interior is not descended here, since its
/// own blocks are visited independently as their own reachable blocks.
pub(crate) fn find_unsupported_operand_lifts(
    package: &Package,
    package_id: PackageId,
    block_id: qsc_fir::fir::BlockId,
) -> Vec<(String, Span)> {
    let mut rejected = Vec::new();
    for b in collect_reachable_blocks(package, block_id) {
        for &stmt_id in &package.get_block(b).stmts {
            match &package.get_stmt(stmt_id).kind {
                StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => {
                    scan_operand_tree_for_unsupported_lifts(package, package_id, *e, &mut rejected);
                }
                StmtKind::Item(_) => {}
            }
        }
    }
    rejected
}

/// Walk the unconditional operand sites of `expr_id` exactly as
/// [`anf_lift_in_expr`] does, sending each direct operand to
/// [`check_operand_for_unsupported_lift`].
///
/// Every eager operand site is enumerated identically to the lift, including
/// the projected parents `Range`/`Struct`/`String` (each of whose eager
/// children has a stable write-back slot). `Block`/`If`/`While` and the
/// short-circuit `and`/`or` RHS are not unconditional operand sites (conditions
/// and RHS returns are rewritten in place by the condition/short-circuit
/// handling, and statement-carrying constructs are lifted whole by their
/// parent), so they are not descended.
fn scan_operand_tree_for_unsupported_lifts(
    package: &Package,
    package_id: PackageId,
    expr_id: ExprId,
    rejected: &mut Vec<(String, Span)>,
) {
    if !contains_return_in_expr(package, expr_id) {
        return;
    }
    let kind = package.get_expr(expr_id).kind.clone();
    match kind {
        // Short-circuit `and`/`or`: only the LHS evaluates unconditionally.
        ExprKind::BinOp(BinOp::AndL | BinOp::OrL, a, _) => {
            check_operand_for_unsupported_lift(package, package_id, a, rejected);
        }
        // Assign family: the lvalue place is never a value operand, so only the
        // value operand is scanned.
        ExprKind::Assign(_, b) | ExprKind::AssignOp(_, _, b) | ExprKind::AssignField(_, _, b) => {
            check_operand_for_unsupported_lift(package, package_id, b, rejected);
        }
        // AssignIndex: the place is excluded; index `b` and value `c` are
        // genuine operands.
        ExprKind::AssignIndex(_, b, c) => {
            check_operand_for_unsupported_lift(package, package_id, b, rejected);
            check_operand_for_unsupported_lift(package, package_id, c, rejected);
        }
        // Two-operand eager compounds.
        ExprKind::BinOp(_, a, b)
        | ExprKind::Call(a, b)
        | ExprKind::Index(a, b)
        | ExprKind::ArrayRepeat(a, b)
        | ExprKind::UpdateField(a, _, b) => {
            check_operand_for_unsupported_lift(package, package_id, a, rejected);
            check_operand_for_unsupported_lift(package, package_id, b, rejected);
        }
        // Three-operand eager compounds. The functional `UpdateIndex` receiver
        // `a` stays a value operand.
        ExprKind::UpdateIndex(a, b, c) => {
            check_operand_for_unsupported_lift(package, package_id, a, rejected);
            check_operand_for_unsupported_lift(package, package_id, b, rejected);
            check_operand_for_unsupported_lift(package, package_id, c, rejected);
        }
        // N-ary eager compounds.
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            for e in exprs {
                check_operand_for_unsupported_lift(package, package_id, e, rejected);
            }
        }
        // Single-operand eager compounds.
        ExprKind::UnOp(_, e) | ExprKind::Field(e, _) | ExprKind::Fail(e) => {
            check_operand_for_unsupported_lift(package, package_id, e, rejected);
        }
        // Projected kinds: each eager child has a stable write-back slot, so
        // they are scanned like any other operand site (a defaultable child
        // lifts; only a non-defaultable child is reported).
        ExprKind::Range(start, step, end) => {
            for e in [start, step, end].into_iter().flatten() {
                check_operand_for_unsupported_lift(package, package_id, e, rejected);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(c) = copy {
                check_operand_for_unsupported_lift(package, package_id, c, rejected);
            }
            for fa in &fields {
                check_operand_for_unsupported_lift(package, package_id, fa.value, rejected);
            }
        }
        ExprKind::String(components) => {
            for component in &components {
                if let StringComponent::Expr(e) = component {
                    check_operand_for_unsupported_lift(package, package_id, *e, rejected);
                }
            }
        }
        // Leaves and statement-carrying constructs are not operand sites here.
        ExprKind::Block(_)
        | ExprKind::If(_, _, _)
        | ExprKind::While(_, _)
        | ExprKind::Closure(_, _)
        | ExprKind::Hole
        | ExprKind::Lit(_)
        | ExprKind::Return(_)
        | ExprKind::Var(_, _) => {}
    }
}

/// Classify a single operand the same way [`anf_lift_operands`] does: a
/// statement-carrying `Block`/`If`/`While` containing a `Return` is the whole
/// lifted candidate (recorded only when its temp type cannot be lowered, never
/// descended); any other operand is recursed into to find a deeper candidate.
///
/// The lift backs a non-defaultable operand temp (such as `Qubit`) with a
/// length-1 array, so the temp's effective type is `operand.ty[]`. An array
/// type always has the classical default `[]`, so this rejection path is a
/// defensive guard that no surface-expressible operand type reaches today; it
/// remains so a future non-array-defaultable type degrades to a warning rather
/// than panicking in the slot machinery.
fn check_operand_for_unsupported_lift(
    package: &Package,
    package_id: PackageId,
    operand_id: ExprId,
    rejected: &mut Vec<(String, Span)>,
) {
    if is_anf_lift_candidate(package, operand_id) {
        let operand = package.get_expr(operand_id);
        let temp_ty = Ty::Array(Box::new(operand.ty.clone()));
        if !is_type_defaultable(package, package_id, &temp_ty) {
            rejected.push((format!("{}", operand.ty), operand.span));
        }
    } else {
        scan_operand_tree_for_unsupported_lifts(package, package_id, operand_id, rejected);
    }
}

/// Returns `true` when `expr_id` is an operand subexpression the ANF lift
/// should bind to a spine temp: a statement-carrying `Block`/`If`/`While`
/// that *contains* a `Return`.
///
/// This is the pure equivalent of the
/// `contains_return_in_expr(operand) && hoist_in_expr(operand).is_none()`
/// gate. The ANF fixpoint runs only after the hoist fixpoint has driven the
/// compound-position measure to zero, so every compound-position `Return`
/// `hoist_in_expr` could lift has already been hoisted (and any
/// short-circuit / `If`-condition rewrites already applied). The only
/// operands that still *contain* a `Return` with nothing left for
/// `hoist_in_expr` to do are the statement-carrying constructs. Evaluating
/// it as a predicate (rather than re-invoking `hoist_in_expr`) avoids the
/// in-place rewrites that call has as a side effect.
fn is_anf_lift_candidate(package: &Package, expr_id: ExprId) -> bool {
    contains_return_in_expr(package, expr_id)
        && matches!(
            package.get_expr(expr_id).kind,
            ExprKind::Block(_) | ExprKind::If(_, _, _) | ExprKind::While(_, _)
        )
}

/// Drives the ANF operand lift over a single statement's surface expression,
/// lifting exactly one operand subexpression (innermost-first) that contains
/// a `Return` buried behind a statement-carrying construct.
///
/// Recurses into each unconditional operand site — the eager-compound
/// children, the short-circuit **LHS only**, and an `If`'s **condition** —
/// descending into deeper operands before considering the current level so the
/// innermost candidate is lifted first. `Block`/`While` are recursion leaves:
/// each is lifted *whole* (its interior statements / condition / branches are
/// separate blocks the fixpoint driver visits independently, and a `While`
/// condition's returns are handled in place by `hoist_short_circuit` / flag
/// lowering), never descended into here. An `If` is *not* a leaf: its
/// condition is an unconditional operand site (see the `If` arm), while its
/// branches are separate blocks visited independently.
///
/// # Requires
/// - `expr_id` is valid in `package`.
///
/// # Ensures
/// - Returns `Some(stmts)` — the spine `let` bindings to splice *before* the
///   enclosing statement — when one operand was lifted; the operand slot of
///   its parent is rewritten in place to read the new temp.
/// - Returns `None` when no operand-embedded `Return` remains.
///
/// # Mutations
/// - On a lift, rewrites operand slots in place and allocates spine `let`s
///   through `assigner`. Each minted temp draws and advances `temp_counter`
///   so co-resident temps render with distinct `_<n>` display suffixes.
#[allow(clippy::too_many_lines)] // Exhaustive `ExprKind` operand-site dispatch.
pub(super) fn anf_lift_in_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    expr_id: ExprId,
    temp_counter: &mut u32,
) -> Option<Vec<StmtId>> {
    if !contains_return_in_expr(package, expr_id) {
        return None;
    }
    let kind = package.get_expr(expr_id).kind.clone();
    match kind {
        // Short-circuit `and`/`or`: only the LHS evaluates unconditionally.
        // The RHS is conditional and stays on the `hoist_short_circuit`
        // path, so it is never an ANF operand site.
        ExprKind::BinOp(BinOp::AndL | BinOp::OrL, a, _) => {
            anf_lift_operands(package, assigner, package_id, expr_id, &[a], temp_counter)
        }
        // Assign family: the lvalue place is provably `Var`/`Hole`/`Tuple`-of-those
        // and never buries a return, so it is excluded from the operand
        // enumeration; including it would pin the mutable place to a by-value
        // copy and silently lose the write.
        ExprKind::Assign(_, b) | ExprKind::AssignOp(_, _, b) | ExprKind::AssignField(_, _, b) => {
            anf_lift_operands(package, assigner, package_id, expr_id, &[b], temp_counter)
        }
        // AssignIndex: place excluded; index and value are genuine rvalue operands.
        ExprKind::AssignIndex(_, b, c) => anf_lift_operands(
            package,
            assigner,
            package_id,
            expr_id,
            &[b, c],
            temp_counter,
        ),
        // Two-operand eager compounds; the functional `UpdateField` receiver `a`
        // is a genuine value operand and stays liftable.
        ExprKind::BinOp(_, a, b)
        | ExprKind::Call(a, b)
        | ExprKind::Index(a, b)
        | ExprKind::ArrayRepeat(a, b)
        | ExprKind::UpdateField(a, _, b) => anf_lift_operands(
            package,
            assigner,
            package_id,
            expr_id,
            &[a, b],
            temp_counter,
        ),
        // Three-operand eager compounds; the functional `UpdateIndex` receiver
        // `a` stays liftable.
        ExprKind::UpdateIndex(a, b, c) => anf_lift_operands(
            package,
            assigner,
            package_id,
            expr_id,
            &[a, b, c],
            temp_counter,
        ),
        // N-ary eager compounds.
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            anf_lift_operands(package, assigner, package_id, expr_id, &exprs, temp_counter)
        }
        // Single-operand eager compounds.
        ExprKind::UnOp(_, e) | ExprKind::Field(e, _) | ExprKind::Fail(e) => {
            anf_lift_operands(package, assigner, package_id, expr_id, &[e], temp_counter)
        }
        // Optional operands in left-to-right order.
        ExprKind::Range(start, step, end) => {
            let operands: Vec<ExprId> = [start, step, end].into_iter().flatten().collect();
            anf_lift_operands(
                package,
                assigner,
                package_id,
                expr_id,
                &operands,
                temp_counter,
            )
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
            anf_lift_operands(
                package,
                assigner,
                package_id,
                expr_id,
                &operands,
                temp_counter,
            )
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
            anf_lift_operands(
                package,
                assigner,
                package_id,
                expr_id,
                &operands,
                temp_counter,
            )
        }
        // An `If` condition is an unconditional operand site: it is evaluated
        // in full before either branch, so a `Return` buried there fires
        // before the `If` chooses a branch. Treat the condition as the `If`'s
        // sole ANF operand — recurse into it for a deeper candidate, then lift
        // the whole condition when it is a statement-carrying construct. The
        // branches are *not* operands (they evaluate conditionally); they are
        // separate blocks the fixpoint driver visits independently. Lifting the
        // condition to a spine temp also makes the reconstructed `if <temp>
        // { … }` a later statement that the flag guard skips once the
        // condition's `Return` has fired, so an `else` branch is short-circuited
        // too. (A simple `Return`-valued condition is handled earlier in place
        // by `hoist_in_cond`; only statement-carrying conditions reach here.)
        ExprKind::If(cond, _, _) => anf_lift_operands(
            package,
            assigner,
            package_id,
            expr_id,
            &[cond],
            temp_counter,
        ),

        // `Block`/`While` are recursion leaves. A `Block`/`While` in an operand
        // slot is lifted *whole* by its parent (via `is_anf_lift_candidate`),
        // never descended into here. A `While` condition is deliberately not an
        // operand site: it is re-evaluated each iteration, so lifting it once to
        // a spine temp would break per-iteration re-evaluation; its condition
        // Returns are rewritten in place by `hoist_short_circuit` / consumed by
        // flag lowering's `replace_returns_in_condition_expr`. The branches /
        // loop body are separate blocks the fixpoint driver visits
        // independently.
        ExprKind::Block(_)
        | ExprKind::While(_, _)
        | ExprKind::Closure(_, _)
        | ExprKind::Hole
        | ExprKind::Lit(_)
        | ExprKind::Return(_)
        | ExprKind::Var(_, _) => None,
    }
}

/// Lift one operand from `parent_id`'s ordered operand list, innermost-first.
///
/// First recurses into each operand to lift a *deeper* candidate; only if
/// none is found does it lift the first directly-liftable operand at this
/// level. When lifting operand `i`, every earlier operand `0..i` is pinned to
/// its own spine temp (and its slot rewritten to read it) so left-to-right
/// evaluation order is preserved — an earlier operand's side effects must run
/// before the lifted operand's `Return` fires. Later operands stay inline:
/// they are dead on the return path (the reconstructed statement is
/// flag-guarded) and evaluate in original order otherwise.
fn anf_lift_operands(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    parent_id: ExprId,
    operands: &[ExprId],
    temp_counter: &mut u32,
) -> Option<Vec<StmtId>> {
    // Innermost-first: try to lift a deeper operand inside any child first.
    for &op in operands {
        if let Some(stmts) = anf_lift_in_expr(package, assigner, package_id, op, temp_counter) {
            return Some(stmts);
        }
    }
    // No deeper candidate; lift the first liftable operand at this level.
    for (i, &op) in operands.iter().enumerate() {
        if is_anf_lift_candidate(package, op) {
            let mut out: Vec<StmtId> = Vec::with_capacity(i + 1);
            // Pin each earlier operand to preserve evaluation order.
            for &earlier in &operands[..i] {
                let (pin_stmt, _pin_var) = anf_lift_operand(
                    package,
                    assigner,
                    package_id,
                    parent_id,
                    earlier,
                    temp_counter,
                );
                out.push(pin_stmt);
            }
            let (lift_stmt, _lift_var) =
                anf_lift_operand(package, assigner, package_id, parent_id, op, temp_counter);
            out.push(lift_stmt);
            return Some(out);
        }
    }
    None
}

/// Binds an operand subexpression to a fresh immutable
/// `let __operand_tmp_<n> = <operand>;` on the statement spine and rewrites the
/// matching slot of `parent_id` to read the temp.
///
/// Used both for the return-bearing operand lift and for pinning its earlier
/// sibling operands (the mechanics are identical). The operand's own internal
/// `Return` is left intact inside the Local initializer, where downstream
/// flag lowering rewrites it and `guard_stmt_with_flag` short-circuits the
/// reconstructed enclosing statement.
///
/// # Temp backing
/// When the operand's type has a classical default (`is_type_defaultable`), the
/// temp binds the operand directly and the slot reads `Var(__operand_tmp_<n>)`.
/// When it does not (realistically `Qubit`, or a tuple/UDT containing it), the
/// temp is backed by a length-1 array instead: the binding type becomes
/// `operand.ty[]` (which always has the classical default `[]`), the operand
/// value is retyped to yield that array without re-burying its `Return`, and
/// the slot reads the element back through `[0]`. This stores a non-defaultable
/// value behind the universally-defaultable `T[]` representation — the same
/// representation the array-backed return slot uses for a non-defaultable
/// return value — so the operand lift stays sound for element types that have
/// no classical default of their own.
///
/// # Requires
/// - `parent_id` and `operand_id` are valid in `package`.
/// - `operand_id` is a direct child slot of `parent_id`.
///
/// # Ensures
/// - Returns `(local_stmt_id, slot_read_id)`: the new `let` statement and the
///   expression (a `Var`, or an array `[0]` read) now occupying the operand
///   slot.
///
/// Each minted temp is named `__operand_tmp_<n>`, where `<n>` is the current
/// value of `temp_counter`, which is then incremented. The counter is reset
/// once per specialization body so co-resident temps render with distinct
/// display names; this affects only the bind-pat `Ident.name` (and therefore
/// the parseable rendering), never the `LocalVarId` or `Ty`.
///
/// # Mutations
/// - Allocates a fresh `LocalVarId`, `PatId`, `StmtId`, and slot-read `ExprId`
///   (the Bind ident and the `Var(Res::Local(id))` share one
///   `assigner.next_local()`, so `check_local_reference` holds).
/// - Overwrites the operand slot of `parent_id` in place with the slot read.
/// - For the array-backed path, retypes the operand value in place to yield
///   `operand.ty[]`.
/// - Advances `temp_counter` by one.
fn anf_lift_operand(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    parent_id: ExprId,
    operand_id: ExprId,
    temp_counter: &mut u32,
) -> (StmtId, ExprId) {
    let operand_ty = package.get_expr(operand_id).ty.clone();
    let temp_name = format!("{}_{}", super::super::symbols::OPERAND_TEMP, *temp_counter);
    *temp_counter += 1;

    if is_type_defaultable(package, package_id, &operand_ty) {
        // The temp type has a classical default, so a later flag guard can seed
        // the non-return path with it directly: bind the operand value as-is.
        let (local_id, local_stmt_id) = alloc_local_var(
            package,
            assigner,
            &temp_name,
            &operand_ty,
            operand_id,
            Mutability::Immutable,
        );
        let var_expr_id =
            alloc_local_var_expr(package, assigner, local_id, operand_ty, Span::default());
        replace_operand_slot(package, parent_id, operand_id, var_expr_id);
        (local_stmt_id, var_expr_id)
    } else {
        // The temp type has no classical default. Back it with a length-1 array
        // so the value travels through flag lowering in the universally
        // -defaultable `operand.ty[]` representation; retype the operand value
        // to yield `operand.ty[]` and read the element back through `[0]` at the
        // slot.
        let array_ty = Ty::Array(Box::new(operand_ty.clone()));
        debug_assert!(
            is_type_defaultable(package, package_id, &array_ty),
            "array-backed operand temp type must be classically defaultable"
        );
        let init_id = arrayify_operand_value(package, assigner, operand_id, &operand_ty);
        let (local_id, local_stmt_id) = alloc_local_var(
            package,
            assigner,
            &temp_name,
            &array_ty,
            init_id,
            Mutability::Immutable,
        );
        let array_var_id =
            alloc_local_var_expr(package, assigner, local_id, array_ty, Span::default());
        // Reading the element back through `[0]` leaves a compound `Index` in
        // the operand slot rather than a bare variable. This is a deliberate,
        // sound relaxation of strict ANF atomicity: the `[0]` read is pure,
        // total (the temp is a freshly built length-1 array), and side-effect
        // free, so it preserves evaluation order and never re-buries a `Return`.
        let read_id = singleton_array_index_read(package, assigner, array_var_id, &operand_ty);
        replace_operand_slot(package, parent_id, operand_id, read_id);
        (local_stmt_id, read_id)
    }
}

/// Produces the `operand.ty[]`-typed initializer for an array-backed operand
/// temp, leaving any buried `Return` at a statement boundary.
///
/// A return-bearing candidate (`Block`/`If`) must keep its `Return` at a
/// statement boundary so flag lowering can consume it; wrapping the whole
/// construct in `[ <construct> ]` would push the `Return` back into an operand
/// position and re-trigger the lift, diverging. Instead, this retypes the
/// construct's produced value(s) to `operand.ty[]` in place — the buried
/// `Return` stays where it is. A return-free pinned sibling has no such
/// constraint and is simply wrapped as `[ <operand> ]`.
fn arrayify_operand_value(
    package: &mut Package,
    assigner: &mut Assigner,
    operand_id: ExprId,
    elem_ty: &Ty,
) -> ExprId {
    if is_anf_lift_candidate(package, operand_id) {
        arrayify_value_in_place(package, assigner, operand_id, elem_ty);
        operand_id
    } else {
        wrap_in_singleton_array(package, assigner, operand_id, elem_ty)
    }
}

/// Retypes the value produced by `expr_id` from `elem_ty` to `elem_ty[]` in
/// place, recursing through the value-producing positions a return-bearing
/// candidate can take so the buried `Return` is never relocated.
///
/// - `Block`: retypes the block's trailing value and the block expression.
/// - `If`: retypes both branch values and the `If` expression.
/// - `Return`: a divergent tail produces no value; only the surrounding type is
///   adjusted so the enclosing shape is `elem_ty[]`-typed.
/// - any other leaf: a genuine `elem_ty`-valued result, replaced in place with
///   the single-element array `[ <leaf> ]`.
fn arrayify_value_in_place(
    package: &mut Package,
    assigner: &mut Assigner,
    expr_id: ExprId,
    elem_ty: &Ty,
) {
    let array_ty = Ty::Array(Box::new(elem_ty.clone()));
    let kind = package.get_expr(expr_id).kind.clone();
    match kind {
        ExprKind::Block(block_id) => {
            arrayify_block_tail_value(package, assigner, block_id, elem_ty);
            retype_expr(package, expr_id, &array_ty);
        }
        ExprKind::If(_, then_branch, otherwise) => {
            arrayify_value_in_place(package, assigner, then_branch, elem_ty);
            if let Some(else_branch) = otherwise {
                arrayify_value_in_place(package, assigner, else_branch, elem_ty);
            }
            retype_expr(package, expr_id, &array_ty);
        }
        ExprKind::Return(_) => {
            retype_expr(package, expr_id, &array_ty);
        }
        // A `while` loop is always `Unit`-typed, and `Unit` is classically
        // defaultable, so an operand temp standing in for a `while` never takes
        // the array-backed path that calls into this retyping helper. A `while`
        // therefore never reaches here; flag it explicitly so a future change
        // that routes a non-`Unit` `while` value into an array-backed temp is
        // caught rather than silently wrapped as a leaf.
        ExprKind::While(_, _) => {
            unreachable!("while is unit-typed (defaultable) and never array-backed")
        }
        _ => {
            wrap_leaf_value_in_place(package, assigner, expr_id, elem_ty);
        }
    }
}

/// Retypes a block's trailing value (its last `Expr` statement, if any) to
/// `elem_ty[]` and sets the block's own type to match.
fn arrayify_block_tail_value(
    package: &mut Package,
    assigner: &mut Assigner,
    block_id: BlockId,
    elem_ty: &Ty,
) {
    let array_ty = Ty::Array(Box::new(elem_ty.clone()));
    if let Some(&tail_stmt) = package.get_block(block_id).stmts.last()
        && let StmtKind::Expr(tail_expr) = package.get_stmt(tail_stmt).kind
    {
        arrayify_value_in_place(package, assigner, tail_expr, elem_ty);
    }
    package
        .blocks
        .get_mut(block_id)
        .expect("block not found")
        .ty = array_ty;
}

/// Replaces an `elem_ty`-valued leaf `expr_id` in place with the single-element
/// array literal `[ <leaf> ]` of type `elem_ty[]`.
///
/// The original leaf (with all its child slots intact) is moved to a fresh
/// `ExprId`, and `expr_id` is overwritten to be the wrapping array so any slot
/// that already references `expr_id` now reads `elem_ty[]`.
fn wrap_leaf_value_in_place(
    package: &mut Package,
    assigner: &mut Assigner,
    expr_id: ExprId,
    elem_ty: &Ty,
) {
    let array_ty = Ty::Array(Box::new(elem_ty.clone()));
    let inner_id = assigner.next_expr();
    let mut inner = package.get_expr(expr_id).clone();
    inner.id = inner_id;
    package.exprs.insert(inner_id, inner);
    let target = package.exprs.get_mut(expr_id).expect("leaf expr not found");
    target.ty = array_ty;
    target.kind = ExprKind::Array(vec![inner_id]);
    target.exec_graph_range = EMPTY_EXEC_RANGE;
}

/// Overwrites the type of `expr_id` in place, leaving its kind untouched.
fn retype_expr(package: &mut Package, expr_id: ExprId, ty: &Ty) {
    package.exprs.get_mut(expr_id).expect("expr not found").ty = ty.clone();
}

/// Rewrites the single child slot of `parent_id` currently holding `old_id`
/// to `new_id`. Exhaustive over `ExprKind` so a new variant forces review of
/// every operand-slot site; non-operand-bearing kinds are unreachable here
/// because [`anf_lift_in_expr`] only forwards genuine operand parents.
#[allow(clippy::too_many_lines)] // Exhaustive `ExprKind` operand-slot dispatch.
fn replace_operand_slot(package: &mut Package, parent_id: ExprId, old_id: ExprId, new_id: ExprId) {
    match &mut package
        .exprs
        .get_mut(parent_id)
        .expect("parent expr not found")
        .kind
    {
        ExprKind::Fail(e) | ExprKind::Field(e, _) | ExprKind::UnOp(_, e) => {
            debug_assert_eq!(*e, old_id);
            *e = new_id;
        }
        // Assign family: only the value operand `b` is a lift slot; the lvalue
        // place is never enumerated and must never be retargeted here.
        ExprKind::Assign(_, b) | ExprKind::AssignOp(_, _, b) | ExprKind::AssignField(_, _, b) => {
            debug_assert_eq!(*b, old_id);
            *b = new_id;
        }
        ExprKind::ArrayRepeat(a, b)
        | ExprKind::BinOp(_, a, b)
        | ExprKind::Call(a, b)
        | ExprKind::Index(a, b)
        | ExprKind::UpdateField(a, _, b) => {
            if *a == old_id {
                *a = new_id;
            } else if *b == old_id {
                *b = new_id;
            } else {
                unreachable!("operand slot not found in two-operand parent");
            }
        }
        // AssignIndex: only index `b` and value `c` are lift slots; place excluded.
        ExprKind::AssignIndex(_, b, c) => {
            if *b == old_id {
                *b = new_id;
            } else if *c == old_id {
                *c = new_id;
            } else {
                unreachable!("operand slot not found in AssignIndex parent");
            }
        }
        ExprKind::UpdateIndex(a, b, c) => {
            if *a == old_id {
                *a = new_id;
            } else if *b == old_id {
                *b = new_id;
            } else if *c == old_id {
                *c = new_id;
            } else {
                unreachable!("operand slot not found in three-operand parent");
            }
        }
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            let slot = exprs
                .iter_mut()
                .find(|e| **e == old_id)
                .expect("operand slot not found in n-ary parent");
            *slot = new_id;
        }
        ExprKind::Range(start, step, end) => {
            for slot in [start, step, end].into_iter().flatten() {
                if *slot == old_id {
                    *slot = new_id;
                    return;
                }
            }
            unreachable!("operand slot not found in range parent");
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(c) = copy
                && *c == old_id
            {
                *c = new_id;
                return;
            }
            for fa in fields.iter_mut() {
                if fa.value == old_id {
                    fa.value = new_id;
                    return;
                }
            }
            unreachable!("operand slot not found in struct parent");
        }
        ExprKind::String(components) => {
            for component in components.iter_mut() {
                if let StringComponent::Expr(e) = component
                    && *e == old_id
                {
                    *e = new_id;
                    return;
                }
            }
            unreachable!("operand slot not found in string parent");
        }
        // Only the condition is an unconditional ANF operand site.
        ExprKind::If(cond, _, _) | ExprKind::While(cond, _) => {
            debug_assert_eq!(*cond, old_id);
            *cond = new_id;
        }
        ExprKind::Block(_)
        | ExprKind::Closure(_, _)
        | ExprKind::Hole
        | ExprKind::Lit(_)
        | ExprKind::Return(_)
        | ExprKind::Var(_, _) => {
            unreachable!("not a valid ANF operand-slot parent")
        }
    }
}
