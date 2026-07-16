// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Dead-local elimination simplifier rule.
//!
//! Drops single-bind `Local` declarations whose bound local has no
//! downstream reader or writer in the block and whose initializer is
//! safe to discard.
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
//! initializer can be discarded without changing effects or runtime errors
//! (see [`crate::walk_utils::expr_is_safe_to_discard`]), removing the binding
//! preserves value, evaluation order, and qubit lifetimes. The canonical
//! `__has_returned` /
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
//! unconstrained: the bar is "no downstream uses and safely discardable
//! init".
//!
//! An initializer must be both effect-free and unable to fail before this rule
//! can remove it. Expressions such as array indexing or division may be pure,
//! but they can still fail at runtime, so their bindings stay in place unless a
//! future value-sensitive proof can show the specific expression is safe.
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
        BlockId, ExprId, LocalVarId, Package, PackageId, PackageLookup, PatKind, StmtId, StmtKind,
    },
};

use crate::walk_utils::expr_is_safe_to_discard;

use super::local_use_count;

/// Apply the dead-local elimination rule to `block_id`.
///
/// Returns `true` when at least one eligible single-bind `Local` was
/// removed.
pub(super) fn apply(
    package: &mut Package,
    _assigner: &mut Assigner,
    package_id: PackageId,
    block_id: BlockId,
) -> bool {
    let stmt_ids = package.get_block(block_id).stmts.clone();
    let mut to_remove = Vec::new();

    for (idx, &sid) in stmt_ids.iter().enumerate() {
        let Some((local_id, init_id)) = eligible_local_binding(package, sid) else {
            continue;
        };
        if !expr_is_safe_to_discard(package, package_id, init_id) {
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
