// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Dead-local elimination simplifier rule.
//!
//! Drops single-bind `Local` declarations from a block when the bound
//! local has no downstream readers or writers in the block and the
//! initializer expression is provably free of side effects.
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
//! Q# `let` and `mutable` bindings have no observable side effect at
//! the binding site beyond reserving a name in the local scope and
//! evaluating the initializer expression. If the bound local is
//! referenced nowhere downstream and the initializer is provably
//! side-effect-free (see [`init_is_side_effect_free`]), removing the
//! binding preserves every observable behavior of the block, including
//! value, evaluation order, and qubit lifetimes.
//!
//! The canonical return-unify slot/flag declarations
//! (`__has_returned` / `__ret_val`) are the original motivating shape:
//! their initializers are always literals, and once the rest of the
//! simplifier catalogue collapses the merge expression the slot/flag
//! bindings become dead. The same dead-binding shape arises from
//! user-authored code and from the normalize pre-pass when it
//! preserves a user-bound name with a synthesized default-value
//! initializer that may go unused after surrounding rules fold the
//! shape it was reserving.
//!
//! # Scope
//!
//! The rule fires only on `StmtKind::Local(_, Bind(_), init)` shapes.
//! Tuple-binding patterns (`let (a, b) = ...`) are rejected because
//! decomposing them would change observable shape, and discard patterns
//! (`let _ = ...`) are not handled — the initializer at a discard
//! site must keep evaluating for its potential side effects, and the
//! rule has nothing to drop. Mutability is unconstrained: the safety
//! bar is "no downstream uses AND side-effect-free init", which holds
//! independently of whether the binding is `let` or `mutable`.
//!
//! The initializer-purity check is conservative: it accepts only the
//! syntactic shapes enumerated in [`init_is_side_effect_free`] and
//! defaults to "may have effects" for anything not listed. A
//! misclassification can only leave an extra dead binding standing;
//! it cannot silently drop observable behavior.
//!
//! [`super::local_use_count`] counts closure captures correctly, so a
//! bound local that escapes through a downstream closure capture keeps
//! the binding alive.
//!
//! # Ordering
//!
//! Runs after [`super::dead_flag`] so any leftover flag-setter
//! assignments have already been pruned. A surviving setter would count
//! as a downstream reference and prevent the rule from firing.

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
