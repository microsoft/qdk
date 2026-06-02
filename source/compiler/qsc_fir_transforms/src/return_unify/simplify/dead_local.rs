// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Dead-local elimination simplifier rule.
//!
//! Drops single-bind `Local` declarations whose bound local has no
//! downstream reader or writer in the block and whose initializer is
//! provably side-effect-free.
//!
//! ```text
//! {
//!     mutable __has_returned : Bool = false;
//!     mutable __ret_val : Int = 0;
//!     let x : Int = 7;
//!     42
//! }
//! ```
//!
//! becomes
//!
//! ```text
//! {
//!     42
//! }
//! ```
//!
//! # Why this rewrite is safe
//!
//! A `let`/`mutable` binding's only observable effect is evaluating its
//! initializer. When the local is referenced nowhere downstream and the
//! initializer is provably side-effect-free (see
//! [`init_is_side_effect_free`]), removing the binding preserves value,
//! evaluation order, and qubit lifetimes. The canonical `__has_returned` /
//! `__ret_val` slot declarations are the motivating case: their
//! initializers are literals and become dead once the catalogue collapses
//! the merge. The same shape arises from user code and from normalize's
//! synthesized default-value initializers.
//!
//! # Scope
//!
//! Fires only on `StmtKind::Local(_, Bind(_), init)`. Tuple-binding
//! patterns are rejected because decomposing them would change observable
//! shape; discard patterns (`let _ = ...`) are left alone since their
//! initializer must keep evaluating for side effects. Mutability is
//! unconstrained: the bar is "no downstream uses AND side-effect-free
//! init".
//!
//! The purity check is conservative — it accepts only the shapes
//! enumerated in [`init_is_side_effect_free`] and otherwise assumes
//! effects. A misclassification can only leave an extra dead binding
//! standing, never drop observable behavior.
//!
//! [`super::local_use_count`] counts closure captures, so a local that
//! escapes through a downstream closure keeps its binding alive.
//!
//! # Ordering
//!
//! Runs after [`super::dead_flag`] so leftover flag-setter assignments are
//! already pruned; a surviving setter would count as a downstream
//! reference and block the rule.

use qsc_fir::{
    assigner::Assigner,
    fir::{
        BlockId, ExprId, ExprKind, LocalVarId, Package, PackageLookup, PatKind, StmtId, StmtKind,
        StringComponent,
    },
};

use super::local_use_count;

/// Apply the dead-local elimination rule to `block_id`.
///
/// Returns `true` when at least one eligible single-bind `Local` was
/// removed.
pub(super) fn apply(package: &mut Package, _assigner: &mut Assigner, block_id: BlockId) -> bool {
    let stmt_ids = package.get_block(block_id).stmts.clone();
    let mut to_remove = Vec::new();

    for (idx, &sid) in stmt_ids.iter().enumerate() {
        let Some((local_id, init_id)) = eligible_local_binding(package, sid) else {
            continue;
        };
        if !init_is_side_effect_free(package, init_id) {
            continue;
        }
        if !local_is_dead_in(package, &stmt_ids, idx, local_id) {
            continue;
        }
        to_remove.push(idx);
    }

    if to_remove.is_empty() {
        return false;
    }

    let block = package.blocks.get_mut(block_id).expect("block not found");
    for &idx in to_remove.iter().rev() {
        block.stmts.remove(idx);
    }
    true
}

/// Returns the bound [`LocalVarId`] and initializer [`ExprId`] of a
/// single-bind `Local` statement.
///
/// Rejects tuple-bind patterns, discard patterns, and non-`Local`
/// statements. Mutability is unconstrained: the rule's safety depends
/// on "no downstream uses" and "side-effect-free init", which holds
/// independently of mutability.
pub(super) fn eligible_local_binding(
    package: &Package,
    stmt_id: StmtId,
) -> Option<(LocalVarId, ExprId)> {
    let StmtKind::Local(_, pat_id, init_id) = package.get_stmt(stmt_id).kind else {
        return None;
    };
    let pat = package.get_pat(pat_id);
    let PatKind::Bind(ident) = &pat.kind else {
        return None;
    };
    Some((ident.id, init_id))
}

/// Returns `true` when `local_id` has no reads or writes in any
/// statement of `stmt_ids` other than the declaration at `decl_idx`.
fn local_is_dead_in(
    package: &Package,
    stmt_ids: &[StmtId],
    decl_idx: usize,
    local_id: LocalVarId,
) -> bool {
    for (idx, &sid) in stmt_ids.iter().enumerate() {
        if idx == decl_idx {
            continue;
        }
        let expr_id = match &package.get_stmt(sid).kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => *e,
            StmtKind::Item(_) => continue,
        };
        if local_use_count(package, expr_id, local_id) > 0 {
            return false;
        }
    }
    true
}

/// Conservatively decide whether `expr_id` is a side-effect-free
/// initializer.
///
/// Recognized side-effect-free shapes are restricted to pure value
/// constructors, pure reads, and pure projections:
///
/// * Literals, holes, variable references, and closure constructions
///   (the closure body is not invoked at the binding site).
/// * Compound constructors whose children are all side-effect-free:
///   `Tuple`, `Array`, `ArrayLit`, `ArrayRepeat`, `Range`, `String`
///   (interpolation parts), `Struct` (copy-update source and field
///   values).
/// * Projections whose receivers are side-effect-free: `Field`,
///   `Index`, `UpdateField`, `UpdateIndex`.
/// * Conditional and block expressions whose subexpressions are
///   side-effect-free: `If` with both arms present, and `Block` that
///   is either empty or has only a trailing `Expr` stmt whose
///   expression is side-effect-free.
///
/// Returns `false` for any other variant, including all `Call`,
/// `Assign*`, `Return`, `Fail`, `While`, `BinOp`, `UnOp`, and any
/// future or unknown variant. The conservative default is deliberate:
/// missing a possibly-pure shape only leaves a dead binding unfolded;
/// misclassifying a side-effecting shape as pure would silently drop
/// observable behavior.
pub(crate) fn init_is_side_effect_free(package: &Package, expr_id: ExprId) -> bool {
    match &package.get_expr(expr_id).kind {
        ExprKind::Lit(_) | ExprKind::Hole | ExprKind::Var(_, _) | ExprKind::Closure(_, _) => true,
        ExprKind::Tuple(items) | ExprKind::Array(items) | ExprKind::ArrayLit(items) => items
            .iter()
            .all(|&id| init_is_side_effect_free(package, id)),
        ExprKind::ArrayRepeat(value, count) | ExprKind::Index(value, count) => {
            init_is_side_effect_free(package, *value) && init_is_side_effect_free(package, *count)
        }
        ExprKind::Field(record, _) => init_is_side_effect_free(package, *record),
        ExprKind::UpdateField(record, _, value) => {
            init_is_side_effect_free(package, *record) && init_is_side_effect_free(package, *value)
        }
        ExprKind::UpdateIndex(arr, idx, value) => {
            init_is_side_effect_free(package, *arr)
                && init_is_side_effect_free(package, *idx)
                && init_is_side_effect_free(package, *value)
        }
        ExprKind::Range(start, step, end) => [start, step, end].iter().all(|opt| match opt {
            Some(id) => init_is_side_effect_free(package, *id),
            None => true,
        }),
        ExprKind::String(parts) => parts.iter().all(|p| match p {
            StringComponent::Lit(_) => true,
            StringComponent::Expr(e) => init_is_side_effect_free(package, *e),
        }),
        ExprKind::Struct(_, copy, fields) => {
            copy.is_none_or(|id| init_is_side_effect_free(package, id))
                && fields
                    .iter()
                    .all(|f| init_is_side_effect_free(package, f.value))
        }
        ExprKind::If(cond, then, Some(else_id)) => {
            init_is_side_effect_free(package, *cond)
                && init_is_side_effect_free(package, *then)
                && init_is_side_effect_free(package, *else_id)
        }
        ExprKind::Block(bid) => {
            let blk = package.get_block(*bid);
            match blk.stmts.as_slice() {
                [] => true,
                [only] => match &package.get_stmt(*only).kind {
                    StmtKind::Expr(tail) => init_is_side_effect_free(package, *tail),
                    _ => false,
                },
                _ => false,
            }
        }
        _ => false,
    }
}
