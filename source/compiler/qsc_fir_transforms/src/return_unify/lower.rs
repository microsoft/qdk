// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Flag/slot lowering for return unification.

use crate::{
    EMPTY_EXEC_RANGE,
    fir_builder::{
        alloc_assign_expr, alloc_bin_op_expr, alloc_block, alloc_block_expr, alloc_bool_lit,
        alloc_expr_stmt, alloc_if_expr, alloc_local_var, alloc_local_var_expr, alloc_not_expr,
        alloc_semi_stmt, alloc_unit_expr,
    },
    walk_utils::{expr_is_side_effect_free, for_each_expr},
};
use qsc_data_structures::span::Span;
use qsc_fir::{
    assigner::Assigner,
    fir::{
        BinOp, BlockId, Expr, ExprId, ExprKind, LocalVarId, Mutability, Package, PackageId,
        PackageLookup, StmtId, StmtKind,
    },
    ty::{Prim, Ty},
};

use super::{
    UdtPureTyCache,
    continuation::continuation_suffix_requires_split,
    detect::{contains_return_in_block, contains_return_in_expr, contains_return_in_stmt},
    slot::{
        ArrowDefaultCache, ReturnSlot, ReturnSlotStrategy, UnsupportedDefaultSite,
        create_return_slot_decl, create_return_slot_read_expr,
        create_return_slot_read_or_fail_expr, create_return_slot_unwritten_fallback_expr,
        create_return_slot_write_expr, require_classical_default,
    },
    symbols,
};

fn contains_return_in_while_expr(package: &Package, expr_id: ExprId) -> bool {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::While(_, body_id) => contains_return_in_block(package, *body_id),
        ExprKind::Block(block_id) => {
            let block = package.get_block(*block_id);
            block
                .stmts
                .iter()
                .any(|&stmt_id| contains_return_in_while_stmt(package, stmt_id))
        }
        ExprKind::If(_, then_id, else_opt) => {
            contains_return_in_while_expr(package, *then_id)
                || else_opt.is_some_and(|e| contains_return_in_while_expr(package, e))
        }
        _ => false,
    }
}

fn contains_return_in_while_stmt(package: &Package, stmt_id: StmtId) -> bool {
    let stmt = package.get_stmt(stmt_id);
    match &stmt.kind {
        StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) => {
            contains_return_in_while_expr(package, *expr_id)
        }
        _ => false,
    }
}

fn sync_block_type_to_stmt_or_unit(package: &mut Package, block_id: BlockId) {
    let trailing_ty = match package.get_block(block_id).stmts.last() {
        Some(&stmt_id) => match package.get_stmt(stmt_id).kind {
            StmtKind::Expr(expr_id) => package.get_expr(expr_id).ty.clone(),
            _ => Ty::UNIT,
        },
        None => Ty::UNIT,
    };
    let block = package.blocks.get_mut(block_id).expect("block not found");
    block.ty = trailing_ty;
}

fn resync_expr_ty_from_children(package: &mut Package, expr_id: ExprId) {
    let kind = package.get_expr(expr_id).kind.clone();
    match &kind {
        ExprKind::Block(block_id) => {
            let bid = *block_id;
            sync_block_type_to_stmt_or_unit(package, bid);
            let block_ty = package.get_block(bid).ty.clone();
            let e = package.exprs.get_mut(expr_id).expect("expr not found");
            e.ty = block_ty;
        }
        ExprKind::If(_, then_expr_id, else_expr_id) => {
            let then_id = *then_expr_id;
            let else_id = *else_expr_id;
            let then_ty = package.get_expr(then_id).ty.clone();
            let new_ty = if let Some(else_id) = else_id {
                let else_ty = package.get_expr(else_id).ty.clone();
                if then_ty == Ty::UNIT {
                    else_ty
                } else {
                    then_ty
                }
            } else {
                then_ty
            };
            let e = package.exprs.get_mut(expr_id).expect("expr not found");
            e.ty = new_ty;
        }
        _ => {}
    }
}

/// Synthesized `LocalVarId`s minted by [`transform_block_with_flags`] that
/// the simplify catalogue recovers by identity rather than by synthesized
/// name.
///
/// The `__has_returned` flag id is carried separately because it is not
/// part of [`ReturnSlot`]. `trailing_result` is `Some` only when a
/// `__trailing_result` binding was emitted, i.e. the block had a trailing
/// value to merge.
#[derive(Clone, Copy, Debug)]
pub(super) struct SynthSlots {
    pub(super) has_returned: LocalVarId,
    pub(super) return_slot: ReturnSlot,
    pub(super) trailing_result: Option<LocalVarId>,
}

/// Transform phase of return unification: rewrites `block_id` so every
/// `Return` is lowered into a write to a `__has_returned` flag plus a return
/// slot, leaving the block with a single exit through its trailing expression.
///
/// Declares the flag and slot locals at the block head, threads them through
/// the statement rewrite (every subsequent statement runs guarded by the flag,
/// standing in for LLVM's PHI merge), and reads the slot back in a synthesized
/// trailing expression. Returns the [`SynthSlots`] handles so the simplify
/// phase can fold the canonical flag/slot shapes back into structured control
/// flow.
///
/// # Transformation
///
/// ```text
/// // before: two exits — the early return and the trailing value
/// {
///     if cond { return x; }
///     y
/// }
/// // after: one exit — declare flag + slot, guard the suffix, read the slot
/// {
///     mutable __has_returned = false;
///     mutable __return_slot = <default>;
///     if cond { set __return_slot = x; set __has_returned = true; }
///     if not __has_returned { set __return_slot = y; }
///     __return_slot
/// }
/// ```
#[allow(clippy::too_many_arguments)]
pub(super) fn transform_block_with_flags(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    block_id: BlockId,
    return_ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    arrow_default_cache: &mut ArrowDefaultCache,
    return_slot_strategy: ReturnSlotStrategy,
) -> SynthSlots {
    // Declare the `__has_returned` flag (starts `false`) at the block head. It
    // records whether an early return has fired.
    let (has_returned_var_id, has_returned_decl_stmt) =
        create_mutable_bool_var(package, assigner, symbols::HAS_RETURNED, false);

    // Declare the return slot that will hold the block's eventual return value,
    // seeded with a default appropriate for `return_ty`.
    let (return_slot, ret_val_decl_stmt) = create_return_slot_decl(
        package,
        assigner,
        package_id,
        return_ty,
        udt_pure_tys,
        arrow_default_cache,
        return_slot_strategy,
    );

    // Snapshot the original statements, then start the new body with the two
    // fresh declarations before any rewritten statement.
    let original_stmts = package.get_block(block_id).stmts.clone();
    let mut new_stmts: Vec<StmtId> = Vec::new();

    new_stmts.push(has_returned_decl_stmt);
    new_stmts.push(ret_val_decl_stmt);
    // Bundle the flag/slot handles so the per-statement rewrite can reference
    // them without passing each one separately.
    let flag_context = FlagContext {
        package_id,
        has_returned_var_id,
        return_slot,
        return_ty,
        udt_pure_tys,
    };
    // Rewrite the body statements: returns become flag/slot writes and every
    // statement after the first return is guarded behind the flag. The trailing
    // value is handled lazily so it, too, is skipped once a return has fired.
    new_stmts.extend(transform_block_stmts_with_flags(
        package,
        assigner,
        &original_stmts,
        &flag_context,
        arrow_default_cache,
        FlagBlockOutput::ReturnValue {
            final_trailing_expr_strategy: FinalTrailingExprStrategy::Lazy,
        },
    ));

    // Append the synthesized trailing expression that reads the slot back out,
    // giving the block its single, unified exit value.
    let (trailing, trailing_result) =
        create_flag_trailing_expr_for_slot(package, assigner, &mut new_stmts, &flag_context);

    if let Some(trailing_stmt) = trailing {
        new_stmts.push(trailing_stmt);
    }

    // Swap in the rewritten statement list and keep the block's type as the
    // declared return type.
    let block = package.blocks.get_mut(block_id).expect("block not found");
    block.stmts = new_stmts;
    block.ty = return_ty.clone();

    // Hand back the synthesized local ids so the simplify phase can recognize
    // and fold these canonical flag/slot shapes.
    SynthSlots {
        has_returned: has_returned_var_id,
        return_slot,
        trailing_result,
    }
}

#[derive(Clone, Copy)]
enum FinalTrailingExprStrategy {
    Preserve,
    Lazy,
}

#[derive(Clone, Copy)]
enum FlagBlockOutput {
    ReturnValue {
        final_trailing_expr_strategy: FinalTrailingExprStrategy,
    },
    Unit,
}

impl FlagBlockOutput {
    fn lazy(self) -> Self {
        match self {
            Self::ReturnValue { .. } => Self::ReturnValue {
                final_trailing_expr_strategy: FinalTrailingExprStrategy::Lazy,
            },
            Self::Unit => Self::Unit,
        }
    }

    fn final_trailing_expr_strategy(self) -> Option<FinalTrailingExprStrategy> {
        match self {
            Self::ReturnValue {
                final_trailing_expr_strategy,
            } => Some(final_trailing_expr_strategy),
            Self::Unit => None,
        }
    }
}

pub(super) struct FlagContext<'a> {
    pub(super) package_id: PackageId,
    pub(super) has_returned_var_id: LocalVarId,
    pub(super) return_slot: ReturnSlot,
    pub(super) return_ty: &'a Ty,
    pub(super) udt_pure_tys: &'a UdtPureTyCache,
}

/// Threads the `__has_returned` flag through a sequence of statements so that,
/// once an early `return` has fired, no later statement in the block runs.
///
/// This is the per-statement workhorse behind [`transform_block_with_flags`].
/// It walks the statements in order while remembering whether a return-bearing
/// statement has already been seen. The first return-bearing statement has its
/// `return`s rewritten into flag/slot writes; every statement after it is
/// wrapped in `if not __has_returned { … }` so it becomes a no-op on the paths
/// where the early return already ran. This guarding stands in for the PHI
/// merge a backend like LLVM would insert at the join point.
///
/// Two cases break out of the straight-line guarding and instead emit a single
/// *lazy continuation* — the entire remaining suffix nested inside one
/// `if not __has_returned { … }` block:
/// - When [`continuation_suffix_requires_split`] reports that the suffix can't
///   be guarded statement-by-statement (for example because a later binding is
///   read by the trailing value and must stay in one scope).
/// - When the final trailing expression is reached under
///   [`FinalTrailingExprStrategy::Lazy`], or under `Preserve` but the trailing
///   expression itself still contains a `return`.
///
/// Under `FinalTrailingExprStrategy::Preserve` with no nested return, the
/// trailing expression is left untouched so a block that already produces its
/// value can keep it verbatim.
///
/// # Transformation
///
/// Given a block body like:
///
/// ```text
/// foo();
/// if cond { return x; }
/// bar();
/// baz()
/// ```
///
/// the returns become flag/slot writes and each following statement is guarded:
///
/// ```text
/// foo();
/// if cond { set __return_slot = x; set __has_returned = true; }
/// if not __has_returned { bar(); }
/// if not __has_returned { baz() } else { __return_slot }
/// ```
fn transform_block_stmts_with_flags(
    package: &mut Package,
    assigner: &mut Assigner,
    original_stmts: &[StmtId],
    flag_context: &FlagContext<'_>,
    arrow_default_cache: &mut ArrowDefaultCache,
    output: FlagBlockOutput,
) -> Vec<StmtId> {
    // The rewritten statement list we build up, and a running flag that flips
    // to `true` the moment we pass a statement that can perform an early return.
    let mut new_stmts: Vec<StmtId> = Vec::new();
    let mut seen_return_bearing_stmt = false;

    for (index, &stmt_id) in original_stmts.iter().enumerate() {
        // Classify this statement before deciding how to handle it:
        // - does it hold a `while` loop that itself contains a `return`?
        // - does it contain a `return` anywhere?
        // - is it the block's final trailing expression (the block's value)?
        let has_return_in_while = match &package.get_stmt(stmt_id).kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) => contains_return_in_while_expr(package, *e),
            _ => false,
        };
        let has_return = contains_return_in_stmt(package, stmt_id);
        let is_final_trailing_expr = output.final_trailing_expr_strategy().is_some()
            && index == original_stmts.len() - 1
            && matches!(package.get_stmt(stmt_id).kind, StmtKind::Expr(_));

        // We are already past an early return, and the rest of the block can't
        // be guarded one statement at a time (e.g. a later binding is needed by
        // the trailing value). Emit the whole remaining suffix as a single lazy
        // `if not __has_returned { … }` block and stop.
        if seen_return_bearing_stmt
            && continuation_suffix_requires_split(
                package,
                original_stmts,
                index,
                flag_context.package_id,
                flag_context.udt_pure_tys,
            )
        {
            let lazy_continuation = create_lazy_flag_continuation_stmt(
                package,
                assigner,
                &original_stmts[index..],
                flag_context,
                arrow_default_cache,
                output,
            );
            new_stmts.push(lazy_continuation);
            break;
        }

        // We are past an early return and this is the block's trailing value.
        // How we finish depends on the caller's strategy for the trailing expr.
        if seen_return_bearing_stmt && is_final_trailing_expr {
            match output
                .final_trailing_expr_strategy()
                .expect("final trailing strategy should be set for value output")
            {
                // Lazy: wrap the trailing expression (and anything left) in one
                // guarded block that falls back to the return slot value.
                FinalTrailingExprStrategy::Lazy => {
                    let lazy_continuation = create_lazy_flag_continuation_stmt(
                        package,
                        assigner,
                        &original_stmts[index..],
                        flag_context,
                        arrow_default_cache,
                        output,
                    );
                    new_stmts.push(lazy_continuation);
                    break;
                }
                // Preserve, but the trailing expression still has its own
                // `return` inside it: it can't be kept verbatim, so guard it
                // lazily like the Lazy case.
                FinalTrailingExprStrategy::Preserve if has_return => {
                    let lazy_continuation = create_lazy_flag_continuation_stmt(
                        package,
                        assigner,
                        &original_stmts[index..],
                        flag_context,
                        arrow_default_cache,
                        output,
                    );
                    new_stmts.push(lazy_continuation);
                    break;
                }
                // Preserve with no nested return: the block already produces its
                // value here, so keep the trailing expression exactly as-is.
                FinalTrailingExprStrategy::Preserve => {
                    new_stmts.push(stmt_id);
                    continue;
                }
            }
        }

        // The common path: rewrite this one statement (replace its returns with
        // flag writes and/or guard it behind the flag as needed) and record
        // whether we have now passed a return-bearing statement.
        seen_return_bearing_stmt = transform_and_push_flag_stmt(
            package,
            assigner,
            stmt_id,
            flag_context,
            arrow_default_cache,
            &mut new_stmts,
            has_return_in_while,
            has_return,
            seen_return_bearing_stmt,
        );
    }

    new_stmts
}

/// Rewrites a single statement for the flag-threaded block and appends it to
/// `new_stmts`, returning the updated `seen_return_bearing_stmt` state.
///
/// A `while` bearing a return is rewritten in place; the first return-bearing
/// statement has its returns replaced with flag writes; later statements are
/// guarded by the flag (standing in for LLVM's PHI merge). A statement that is
/// neither return-bearing nor after a return is passed through unchanged.
#[allow(clippy::too_many_arguments)]
fn transform_and_push_flag_stmt(
    package: &mut Package,
    assigner: &mut Assigner,
    stmt_id: StmtId,
    flag_context: &FlagContext<'_>,
    arrow_default_cache: &mut ArrowDefaultCache,
    new_stmts: &mut Vec<StmtId>,
    has_return_in_while: bool,
    has_return: bool,
    seen_return_bearing_stmt: bool,
) -> bool {
    if has_return_in_while {
        // A `while` that can early-return is rewritten in place (flag-guarded
        // loop condition + return replacement in its body) and kept as-is.
        transform_while_stmt(
            package,
            assigner,
            stmt_id,
            flag_context,
            arrow_default_cache,
        );
        new_stmts.push(stmt_id);
        true
    } else if has_return && !seen_return_bearing_stmt {
        // The first return-bearing statement: turn its `return`s into flag/slot
        // writes. It runs unconditionally (nothing before it could have
        // returned), so no flag guard is needed.
        replace_returns_with_flags(
            package,
            assigner,
            stmt_id,
            flag_context,
            arrow_default_cache,
        );
        new_stmts.push(stmt_id);
        true
    } else if has_return {
        // A later return-bearing statement: replace its returns with flag
        // writes AND guard the whole statement, since an earlier return may
        // already have fired.
        replace_returns_with_flags(
            package,
            assigner,
            stmt_id,
            flag_context,
            arrow_default_cache,
        );
        let guarded = guard_stmt_with_flag(
            package,
            assigner,
            flag_context,
            stmt_id,
            arrow_default_cache,
        );
        new_stmts.push(guarded);
        seen_return_bearing_stmt
    } else if seen_return_bearing_stmt {
        // No return of its own, but it sits after one, so guard it behind the
        // flag so it is skipped on the early-return path.
        let guarded = guard_stmt_with_flag(
            package,
            assigner,
            flag_context,
            stmt_id,
            arrow_default_cache,
        );
        new_stmts.push(guarded);
        seen_return_bearing_stmt
    } else {
        // Nothing special: no return here and none before it, so keep it
        // untouched.
        new_stmts.push(stmt_id);
        seen_return_bearing_stmt
    }
}

/// Builds a lazy-continuation *statement* wrapping the remaining suffix in one
/// flag-guarded block (see [`create_lazy_flag_continuation_expr`]), choosing an
/// expression- or semicolon-statement to match the block's value/unit output.
fn create_lazy_flag_continuation_stmt(
    package: &mut Package,
    assigner: &mut Assigner,
    continuation_stmts: &[StmtId],
    flag_context: &FlagContext<'_>,
    arrow_default_cache: &mut ArrowDefaultCache,
    output: FlagBlockOutput,
) -> StmtId {
    let lazy_continuation = create_lazy_flag_continuation_expr(
        package,
        assigner,
        continuation_stmts,
        flag_context,
        arrow_default_cache,
        output,
    );
    match output {
        // A value-producing block ends in an expression statement (no trailing
        // semicolon) so the guarded `if` becomes the block's value.
        FlagBlockOutput::ReturnValue { .. } => {
            alloc_expr_stmt(package, assigner, lazy_continuation, Span::default())
        }
        // A unit block ends in a semicolon statement.
        FlagBlockOutput::Unit => {
            alloc_semi_stmt(package, assigner, lazy_continuation, Span::default())
        }
    }
}

/// Builds the lazy-continuation *expression*: the entire remaining suffix of a
/// block nested inside `if not __has_returned { <suffix> } else { <slot> }`.
///
/// Rather than guard each remaining statement individually, everything after an
/// early return is bundled into one guarded block. On the not-yet-returned path
/// the suffix runs; on the already-returned path control falls to the `else`,
/// which reads the return slot so the block still produces the return value.
///
/// # Transformation
///
/// ```text
/// // suffix after an early return, value-producing block:
/// if not __has_returned {
///     <transformed suffix statements...>
///     <trailing value>       // slot-read-or-fail appended if the suffix
///                            // produced no value of the return type
/// } else {
///     __return_slot          // already returned: yield the stored value
/// }
/// ```
fn create_lazy_flag_continuation_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    continuation_stmts: &[StmtId],
    flag_context: &FlagContext<'_>,
    arrow_default_cache: &mut ArrowDefaultCache,
    output: FlagBlockOutput,
) -> ExprId {
    // Recursively flag-thread the suffix statements. `output.lazy()` keeps the
    // nested block on the lazy trailing-expression strategy too.
    let mut continuation_stmts = transform_block_stmts_with_flags(
        package,
        assigner,
        continuation_stmts,
        flag_context,
        arrow_default_cache,
        output.lazy(),
    );
    let (continuation_ty, else_expr) = match output {
        FlagBlockOutput::ReturnValue { .. } => {
            // The guarded block must yield a value of the return type. If the
            // transformed suffix doesn't already end in one, make it.
            if !has_value_trailing_stmt(package, &continuation_stmts, flag_context.return_ty) {
                // Drop a trailing side-effect-free unit statement first so it
                // doesn't sit awkwardly before the value we're about to append.
                if let Some(&last_id) = continuation_stmts.last()
                    && let StmtKind::Expr(e) = package.get_stmt(last_id).kind
                    && package.get_expr(e).ty == Ty::UNIT
                    && expr_is_side_effect_free(package, e)
                {
                    continuation_stmts.pop();
                }
                // Append an expression that reads the slot, or fails at runtime
                // if this path is reached without a value having been set.
                let missing_value = create_return_slot_read_or_fail_expr(
                    package,
                    assigner,
                    flag_context.has_returned_var_id,
                    flag_context.return_slot,
                    flag_context.return_ty,
                );
                continuation_stmts.push(alloc_expr_stmt(
                    package,
                    assigner,
                    missing_value,
                    Span::default(),
                ));
            }

            // The `else` branch (already-returned path) simply reads the slot.
            let ret_var = create_return_slot_read_expr(
                package,
                assigner,
                flag_context.return_slot,
                flag_context.return_ty,
            );
            (flag_context.return_ty.clone(), Some(ret_var))
        }
        // A unit block needs no value and no `else` branch.
        FlagBlockOutput::Unit => (Ty::UNIT, None),
    };
    // Wrap the suffix statements in a block expression of the chosen type.
    let continuation_block = alloc_block(
        package,
        assigner,
        continuation_stmts,
        continuation_ty.clone(),
        Span::default(),
    );
    let continuation_expr = alloc_block_expr(
        package,
        assigner,
        continuation_block,
        continuation_ty.clone(),
        Span::default(),
    );
    // Guard on `not __has_returned` so the suffix runs only when no early return
    // has fired; otherwise control takes the `else` (slot read) path.
    let not_flag = create_not_var_expr(package, assigner, flag_context.has_returned_var_id);

    alloc_if_expr(
        package,
        assigner,
        not_flag,
        continuation_expr,
        else_expr,
        continuation_ty,
        Span::default(),
    )
}

/// Reports whether the last statement is an expression statement whose type is
/// the block's return type — i.e. the block already ends in a return value.
fn has_value_trailing_stmt(package: &Package, stmts: &[StmtId], return_ty: &Ty) -> bool {
    stmts.last().is_some_and(|&stmt_id| {
        matches!(
            package.get_stmt(stmt_id).kind,
            StmtKind::Expr(expr_id) if package.get_expr(expr_id).ty == *return_ty
        )
    })
}

/// Entry point for rewriting a statement that holds a `while`-with-return:
/// unwraps the statement's expression and hands it to [`transform_while_in_expr`].
fn transform_while_stmt(
    package: &mut Package,
    assigner: &mut Assigner,
    stmt_id: StmtId,
    flag_context: &FlagContext<'_>,
    arrow_default_cache: &mut ArrowDefaultCache,
) {
    let expr_id = match &package.get_stmt(stmt_id).kind {
        StmtKind::Expr(e) | StmtKind::Semi(e) => *e,
        _ => return,
    };

    transform_while_in_expr(
        package,
        assigner,
        expr_id,
        flag_context,
        arrow_default_cache,
    );
}

/// Lowers `while`-with-return loops nested anywhere inside `expr_id`.
///
/// The enclosing statement was routed here by
/// [`transform_block_stmts_with_flags`] because it contains at least one
/// `while`-with-return (detected by [`contains_return_in_while_expr`]). This
/// walker must reach *every* such loop in the statement — including ones in
/// operand position (e.g. a `Call` argument) or inside a `Local` binding's
/// initializer — so no raw `Return` survives to trip `check_no_returns`.
///
/// The `While` arm performs the loop rewrite (flag-guarded condition + return
/// replacement in the body). Nested *statement sequences* — a `Block` and the
/// branch blocks of an `If` — are delegated to the canonical
/// [`replace_returns_in_expr`]/[`replace_returns_in_block`] family so that any
/// statement following a return-bearing statement inside them is wrapped in
/// `if not __has_returned { … }` (via [`transform_block_stmts_with_flags`]).
/// This keeps the guarding of nested blocks identical to spine blocks: once a
/// buried `return` (or a `while`-with-return, or an ANF-lifted return-bearing
/// `Local` initializer) has fired, no later statement in the same block runs.
/// The `If` *condition* is not a statement sequence, so a `while`-with-return
/// there is still handled additively via [`transform_while_in_child`].
///
/// Every other child-bearing `ExprKind` recurses into its children via
/// [`transform_while_in_child`], which guards each descent with
/// [`expr_contains_while_with_return`] so it only enters subtrees that
/// actually contain a `while`-with-return, keeping that part of the rewrite
/// additive: subtrees with no such loop are left byte-identical.
#[allow(clippy::too_many_lines)]
fn transform_while_in_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    expr_id: ExprId,
    flag_context: &FlagContext<'_>,
    arrow_default_cache: &mut ArrowDefaultCache,
) {
    let expr = package.get_expr(expr_id).clone();
    match &expr.kind {
        ExprKind::While(cond_id, body_block_id) => {
            let cond_id = *cond_id;
            let body_block_id = *body_block_id;

            if contains_return_in_expr(package, cond_id) {
                replace_returns_in_condition_expr(
                    package,
                    assigner,
                    cond_id,
                    flag_context,
                    arrow_default_cache,
                );
            }

            let not_flag = create_not_var_expr(package, assigner, flag_context.has_returned_var_id);
            let new_cond = alloc_bin_op_expr(
                package,
                assigner,
                BinOp::AndL,
                not_flag,
                cond_id,
                Ty::Prim(Prim::Bool),
                Span::default(),
            );

            if contains_return_in_block(package, body_block_id) {
                replace_returns_in_block(
                    package,
                    assigner,
                    body_block_id,
                    flag_context,
                    arrow_default_cache,
                    FlagBlockOutput::Unit,
                );
            }

            let e = package.exprs.get_mut(expr_id).expect("expr not found");
            *e = Expr {
                id: expr_id,
                span: expr.span,
                ty: expr.ty.clone(),
                kind: ExprKind::While(new_cond, body_block_id),
                exec_graph_range: EMPTY_EXEC_RANGE,
            };
        }
        ExprKind::Block(_) => {
            // Delegate the block to the canonical return-replacement family so
            // its statements receive the same `if not __has_returned { … }`
            // guarding as a spine block: `replace_returns_in_expr`'s `Block`
            // arm chooses the output strategy from the block type and routes
            // through `transform_block_stmts_with_flags`, which re-enters
            // `transform_while_stmt` for any `while`-with-return statement.
            replace_returns_in_expr(
                package,
                assigner,
                expr_id,
                flag_context,
                arrow_default_cache,
            );
        }
        ExprKind::If(cond_id, then_id, else_opt) => {
            let cond_id = *cond_id;
            let then_id = *then_id;
            let else_opt = *else_opt;
            // A condition *can* itself be a statement sequence (a `Block`, or a
            // short-circuit chain). Normalize's ANF lift hoists any
            // return-bearing `if` condition to a spine `let` temp before flag
            // lowering runs, so by here a `while`-with-return is not expected in
            // the condition; `transform_while_in_child` is a guarded, defensive
            // descent (a no-op unless the condition still contains one). The
            // branch blocks *are* statement sequences, so route them through
            // `replace_returns_in_expr` to get full post-return statement
            // guarding.
            transform_while_in_child(
                package,
                assigner,
                cond_id,
                flag_context,
                arrow_default_cache,
            );
            replace_returns_in_expr(
                package,
                assigner,
                then_id,
                flag_context,
                arrow_default_cache,
            );
            if let Some(e) = else_opt {
                replace_returns_in_expr(package, assigner, e, flag_context, arrow_default_cache);
            }
            resync_expr_ty_from_children(package, expr_id);
        }
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            let ids: Vec<ExprId> = exprs.clone();
            for e in ids {
                transform_while_in_child(package, assigner, e, flag_context, arrow_default_cache);
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
            let (a_id, b_id) = (*a, *b);
            transform_while_in_child(package, assigner, a_id, flag_context, arrow_default_cache);
            transform_while_in_child(package, assigner, b_id, flag_context, arrow_default_cache);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            let (a_id, b_id, c_id) = (*a, *b, *c);
            transform_while_in_child(package, assigner, a_id, flag_context, arrow_default_cache);
            transform_while_in_child(package, assigner, b_id, flag_context, arrow_default_cache);
            transform_while_in_child(package, assigner, c_id, flag_context, arrow_default_cache);
        }
        ExprKind::Fail(e) | ExprKind::Field(e, _) | ExprKind::UnOp(_, e) | ExprKind::Return(e) => {
            let sub = *e;
            transform_while_in_child(package, assigner, sub, flag_context, arrow_default_cache);
        }
        ExprKind::Range(start, step, end) => {
            let ids: Vec<ExprId> = [start, step, end].into_iter().flatten().copied().collect();
            for e in ids {
                transform_while_in_child(package, assigner, e, flag_context, arrow_default_cache);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            let copy_id = *copy;
            let field_ids: Vec<ExprId> = fields.iter().map(|fa| fa.value).collect();
            if let Some(c) = copy_id {
                transform_while_in_child(package, assigner, c, flag_context, arrow_default_cache);
            }
            for e in field_ids {
                transform_while_in_child(package, assigner, e, flag_context, arrow_default_cache);
            }
        }
        ExprKind::String(components) => {
            let ids: Vec<ExprId> = components
                .iter()
                .filter_map(|c| match c {
                    qsc_fir::fir::StringComponent::Expr(e) => Some(*e),
                    qsc_fir::fir::StringComponent::Lit(_) => None,
                })
                .collect();
            for e in ids {
                transform_while_in_child(package, assigner, e, flag_context, arrow_default_cache);
            }
        }
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
    }
}

/// Guarded recursion helper for [`transform_while_in_expr`]: descends into
/// `child_id` only when it contains a `while`-with-return, so subtrees with
/// no such loop are never visited or mutated (preserving prior behavior).
fn transform_while_in_child(
    package: &mut Package,
    assigner: &mut Assigner,
    child_id: ExprId,
    flag_context: &FlagContext<'_>,
    arrow_default_cache: &mut ArrowDefaultCache,
) {
    if expr_contains_while_with_return(package, child_id) {
        transform_while_in_expr(
            package,
            assigner,
            child_id,
            flag_context,
            arrow_default_cache,
        );
    }
}

/// Exhaustive predicate: does `expr_id`'s subtree contain a `while` loop whose
/// condition or body holds a `Return`?
///
/// Unlike the narrow [`contains_return_in_while_expr`] (which only looks
/// through `While`/`Block`/`If` spines and feeds the dispatcher's routing
/// flag), this walks every child via [`for_each_expr`] — reaching loops in
/// operand position and inside `Local` initializers. It is used solely as the
/// recursion guard for [`transform_while_in_child`] so the exhaustive
/// transform descends into exactly those subtrees, leaving the dispatcher's
/// detection (and thus existing routing/snapshots) unchanged. Closure bodies
/// are not traversed, matching the transform's scope.
fn expr_contains_while_with_return(package: &Package, expr_id: ExprId) -> bool {
    let mut found = false;
    for_each_expr(package, expr_id, &mut |_id, expr| {
        if let ExprKind::While(cond_id, body_id) = &expr.kind
            && (contains_return_in_block(package, *body_id)
                || contains_return_in_expr(package, *cond_id))
        {
            found = true;
        }
    });
    found
}

fn replace_returns_in_block(
    package: &mut Package,
    assigner: &mut Assigner,
    block_id: BlockId,
    flag_context: &FlagContext<'_>,
    arrow_default_cache: &mut ArrowDefaultCache,
    output: FlagBlockOutput,
) {
    let stmts = package.get_block(block_id).stmts.clone();
    let new_stmts = transform_block_stmts_with_flags(
        package,
        assigner,
        &stmts,
        flag_context,
        arrow_default_cache,
        output,
    );
    let block = package.blocks.get_mut(block_id).expect("block not found");
    block.stmts = new_stmts;
    if matches!(output, FlagBlockOutput::Unit) {
        block.ty = Ty::UNIT;
    }
}

fn replace_returns_with_flags(
    package: &mut Package,
    assigner: &mut Assigner,
    stmt_id: StmtId,
    flag_context: &FlagContext<'_>,
    arrow_default_cache: &mut ArrowDefaultCache,
) {
    let expr_id = match &package.get_stmt(stmt_id).kind {
        StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => *e,
        StmtKind::Item(_) => return,
    };
    replace_returns_in_expr(
        package,
        assigner,
        expr_id,
        flag_context,
        arrow_default_cache,
    );

    if let StmtKind::Local(_, pat_id, init_id) = &package.get_stmt(stmt_id).kind {
        let pat_id = *pat_id;
        let init_id = *init_id;
        let init_ty = package.get_expr(init_id).ty.clone();
        let pat = package.pats.get_mut(pat_id).expect("pat not found");
        pat.ty = init_ty;
    }
}

#[allow(clippy::too_many_lines)]
fn replace_returns_in_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    expr_id: ExprId,
    flag_context: &FlagContext<'_>,
    arrow_default_cache: &mut ArrowDefaultCache,
) {
    let expr = package.get_expr(expr_id).clone();
    match &expr.kind {
        ExprKind::Return(inner) => {
            let inner_id = *inner;
            let inner_ty = package.get_expr(inner_id).ty.clone();
            let assign_val = create_return_slot_write_expr(
                package,
                assigner,
                flag_context.return_slot,
                inner_id,
                &inner_ty,
            );
            let assign_val_semi = alloc_semi_stmt(package, assigner, assign_val, Span::default());

            let true_lit = alloc_bool_lit(package, assigner, true, Span::default());
            let assign_flag = create_assign_expr(
                package,
                assigner,
                flag_context.has_returned_var_id,
                true_lit,
                &Ty::Prim(Prim::Bool),
            );
            let assign_flag_semi = alloc_semi_stmt(package, assigner, assign_flag, Span::default());

            let flag_block = alloc_block(
                package,
                assigner,
                vec![assign_val_semi, assign_flag_semi],
                Ty::UNIT,
                Span::default(),
            );
            let flag_block_expr =
                alloc_block_expr(package, assigner, flag_block, Ty::UNIT, Span::default());

            let replacement = package.get_expr(flag_block_expr).clone();
            let e = package.exprs.get_mut(expr_id).expect("expr not found");
            *e = Expr {
                id: expr_id,
                span: expr.span,
                ty: replacement.ty,
                kind: replacement.kind,
                exec_graph_range: EMPTY_EXEC_RANGE,
            };
        }
        ExprKind::Block(block_id) => {
            let bid = *block_id;
            let output = if expr.ty == Ty::UNIT {
                FlagBlockOutput::Unit
            } else {
                FlagBlockOutput::ReturnValue {
                    final_trailing_expr_strategy: FinalTrailingExprStrategy::Preserve,
                }
            };
            replace_returns_in_block(
                package,
                assigner,
                bid,
                flag_context,
                arrow_default_cache,
                output,
            );
            resync_expr_ty_from_children(package, expr_id);
        }
        ExprKind::If(_, then_id, else_opt) => {
            let then_id = *then_id;
            let else_id = *else_opt;
            replace_returns_in_expr(
                package,
                assigner,
                then_id,
                flag_context,
                arrow_default_cache,
            );
            if let Some(e) = else_id {
                replace_returns_in_expr(package, assigner, e, flag_context, arrow_default_cache);
            }
            resync_expr_ty_from_children(package, expr_id);
        }
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            let ids: Vec<ExprId> = exprs.clone();
            for e in ids {
                replace_returns_in_expr(package, assigner, e, flag_context, arrow_default_cache);
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
            let (a_id, b_id) = (*a, *b);
            replace_returns_in_expr(package, assigner, a_id, flag_context, arrow_default_cache);
            replace_returns_in_expr(package, assigner, b_id, flag_context, arrow_default_cache);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            let (a_id, b_id, c_id) = (*a, *b, *c);
            replace_returns_in_expr(package, assigner, a_id, flag_context, arrow_default_cache);
            replace_returns_in_expr(package, assigner, b_id, flag_context, arrow_default_cache);
            replace_returns_in_expr(package, assigner, c_id, flag_context, arrow_default_cache);
        }
        ExprKind::Fail(e) | ExprKind::Field(e, _) | ExprKind::UnOp(_, e) => {
            let sub = *e;
            replace_returns_in_expr(package, assigner, sub, flag_context, arrow_default_cache);
        }
        ExprKind::Range(start, step, end) => {
            let ids: Vec<ExprId> = [start, step, end].into_iter().flatten().copied().collect();
            for e in ids {
                replace_returns_in_expr(package, assigner, e, flag_context, arrow_default_cache);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            let copy_id = *copy;
            let field_ids: Vec<ExprId> = fields.iter().map(|fa| fa.value).collect();
            if let Some(c) = copy_id {
                replace_returns_in_expr(package, assigner, c, flag_context, arrow_default_cache);
            }
            for e in field_ids {
                replace_returns_in_expr(package, assigner, e, flag_context, arrow_default_cache);
            }
        }
        ExprKind::String(components) => {
            let ids: Vec<ExprId> = components
                .iter()
                .filter_map(|c| match c {
                    qsc_fir::fir::StringComponent::Expr(e) => Some(*e),
                    qsc_fir::fir::StringComponent::Lit(_) => None,
                })
                .collect();
            for e in ids {
                replace_returns_in_expr(package, assigner, e, flag_context, arrow_default_cache);
            }
        }
        ExprKind::While(cond, body) => {
            let (cond_id, body_id) = (*cond, *body);
            if contains_return_in_block(package, body_id)
                || contains_return_in_expr(package, cond_id)
            {
                transform_while_in_expr(
                    package,
                    assigner,
                    expr_id,
                    flag_context,
                    arrow_default_cache,
                );
            } else {
                replace_returns_in_expr(
                    package,
                    assigner,
                    cond_id,
                    flag_context,
                    arrow_default_cache,
                );
            }
        }
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
    }
}

#[allow(clippy::too_many_lines)]
fn replace_returns_in_condition_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    expr_id: ExprId,
    flag_context: &FlagContext<'_>,
    arrow_default_cache: &mut ArrowDefaultCache,
) {
    let expr = package.get_expr(expr_id).clone();
    match &expr.kind {
        ExprKind::Return(inner_id) => {
            replace_condition_return_with_flags(
                package,
                assigner,
                expr_id,
                expr.span,
                *inner_id,
                flag_context,
            );
        }
        ExprKind::Block(block_id) => {
            let bid = *block_id;
            let stmts = package.get_block(bid).stmts.clone();
            let last_stmt = stmts.last().copied();

            for stmt_id in stmts {
                let expr_ids: Vec<ExprId> = {
                    let stmt = package.get_stmt(stmt_id);
                    match &stmt.kind {
                        StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => {
                            vec![*e]
                        }
                        StmtKind::Item(_) => vec![],
                    }
                };

                for e in expr_ids {
                    if Some(stmt_id) == last_stmt
                        && matches!(package.get_stmt(stmt_id).kind, StmtKind::Expr(_))
                    {
                        replace_returns_in_condition_expr(
                            package,
                            assigner,
                            e,
                            flag_context,
                            arrow_default_cache,
                        );
                    } else {
                        replace_returns_in_expr(
                            package,
                            assigner,
                            e,
                            flag_context,
                            arrow_default_cache,
                        );
                    }
                }
            }

            resync_expr_ty_from_children(package, expr_id);
        }
        ExprKind::If(cond_id, then_id, else_opt) => {
            replace_returns_in_condition_expr(
                package,
                assigner,
                *cond_id,
                flag_context,
                arrow_default_cache,
            );
            replace_returns_in_condition_expr(
                package,
                assigner,
                *then_id,
                flag_context,
                arrow_default_cache,
            );
            if let Some(e) = else_opt {
                replace_returns_in_condition_expr(
                    package,
                    assigner,
                    *e,
                    flag_context,
                    arrow_default_cache,
                );
            }
        }
        ExprKind::BinOp(BinOp::AndL | BinOp::OrL, lhs, rhs) => {
            replace_returns_in_condition_expr(
                package,
                assigner,
                *lhs,
                flag_context,
                arrow_default_cache,
            );
            replace_returns_in_condition_expr(
                package,
                assigner,
                *rhs,
                flag_context,
                arrow_default_cache,
            );
        }
        ExprKind::UnOp(qsc_fir::fir::UnOp::NotL, inner_id) => {
            replace_returns_in_condition_expr(
                package,
                assigner,
                *inner_id,
                flag_context,
                arrow_default_cache,
            );
        }
        _ => {
            assert!(
                !contains_return_in_expr(package, expr_id),
                "unexpected return-bearing while-condition shape after normalize"
            );
        }
    }
}

fn replace_condition_return_with_flags(
    package: &mut Package,
    assigner: &mut Assigner,
    return_expr_id: ExprId,
    span: Span,
    inner_id: ExprId,
    flag_context: &FlagContext<'_>,
) {
    let inner_ty = package.get_expr(inner_id).ty.clone();
    let assign_val = create_return_slot_write_expr(
        package,
        assigner,
        flag_context.return_slot,
        inner_id,
        &inner_ty,
    );
    let assign_val_semi = alloc_semi_stmt(package, assigner, assign_val, Span::default());

    let true_lit = alloc_bool_lit(package, assigner, true, Span::default());
    let assign_flag = create_assign_expr(
        package,
        assigner,
        flag_context.has_returned_var_id,
        true_lit,
        &Ty::Prim(Prim::Bool),
    );
    let assign_flag_semi = alloc_semi_stmt(package, assigner, assign_flag, Span::default());

    let false_lit = alloc_bool_lit(package, assigner, false, Span::default());
    let false_stmt = alloc_expr_stmt(package, assigner, false_lit, Span::default());

    let flag_block = alloc_block(
        package,
        assigner,
        vec![assign_val_semi, assign_flag_semi, false_stmt],
        Ty::Prim(Prim::Bool),
        Span::default(),
    );
    let flag_block_expr = alloc_block_expr(
        package,
        assigner,
        flag_block,
        Ty::Prim(Prim::Bool),
        Span::default(),
    );

    let replacement = package.get_expr(flag_block_expr).clone();
    let e = package
        .exprs
        .get_mut(return_expr_id)
        .expect("expr not found");
    *e = Expr {
        id: return_expr_id,
        span,
        ty: replacement.ty,
        kind: replacement.kind,
        exec_graph_range: EMPTY_EXEC_RANGE,
    };
}

pub(super) fn guard_stmt_with_flag(
    package: &mut Package,
    assigner: &mut Assigner,
    flag_context: &FlagContext<'_>,
    stmt_id: StmtId,
    arrow_default_cache: &mut ArrowDefaultCache,
) -> StmtId {
    if let StmtKind::Local(mutability, pat_id, init_expr_id) = package.get_stmt(stmt_id).kind {
        let init_ty = package.get_expr(init_expr_id).ty.clone();
        let default_val = require_classical_default(
            package,
            assigner,
            flag_context.package_id,
            &init_ty,
            flag_context.udt_pure_tys,
            arrow_default_cache,
            UnsupportedDefaultSite::GuardedLocalInitializer,
        );

        let not_flag = create_not_var_expr(package, assigner, flag_context.has_returned_var_id);

        let then_trailing = alloc_expr_stmt(package, assigner, init_expr_id, Span::default());
        let then_block = alloc_block(
            package,
            assigner,
            vec![then_trailing],
            init_ty.clone(),
            Span::default(),
        );
        let then_expr = alloc_block_expr(
            package,
            assigner,
            then_block,
            init_ty.clone(),
            Span::default(),
        );

        let else_trailing = alloc_expr_stmt(package, assigner, default_val, Span::default());
        let else_block = alloc_block(
            package,
            assigner,
            vec![else_trailing],
            init_ty.clone(),
            Span::default(),
        );
        let else_expr = alloc_block_expr(
            package,
            assigner,
            else_block,
            init_ty.clone(),
            Span::default(),
        );

        let if_expr = alloc_if_expr(
            package,
            assigner,
            not_flag,
            then_expr,
            Some(else_expr),
            init_ty,
            Span::default(),
        );

        let stmt = package.stmts.get_mut(stmt_id).expect("stmt not found");
        stmt.kind = StmtKind::Local(mutability, pat_id, if_expr);
        return stmt_id;
    }

    assert!(
        match &package.get_stmt(stmt_id).kind {
            StmtKind::Semi(_) | StmtKind::Item(_) => true,
            StmtKind::Expr(e) => package.get_expr(*e).ty == Ty::UNIT,
            StmtKind::Local(_, _, _) => unreachable!("Local handled above"),
        },
        "guard_stmt_with_flag requires Unit-typed inner stmt"
    );
    let not_flag = create_not_var_expr(package, assigner, flag_context.has_returned_var_id);
    let guard_block = alloc_block(package, assigner, vec![stmt_id], Ty::UNIT, Span::default());
    let guard_block_expr =
        alloc_block_expr(package, assigner, guard_block, Ty::UNIT, Span::default());
    let if_expr = alloc_if_expr(
        package,
        assigner,
        not_flag,
        guard_block_expr,
        None,
        Ty::UNIT,
        Span::default(),
    );
    alloc_semi_stmt(package, assigner, if_expr, Span::default())
}

#[cfg(test)]
pub(super) fn create_flag_trailing_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    stmts: &mut Vec<StmtId>,
    has_returned_var_id: LocalVarId,
    ret_val_var_id: LocalVarId,
    return_ty: &Ty,
) -> Option<StmtId> {
    let udt_pure_tys = UdtPureTyCache::default();
    let flag_context = FlagContext {
        package_id: PackageId::CORE,
        has_returned_var_id,
        return_slot: ReturnSlot {
            var_id: ret_val_var_id,
            strategy: ReturnSlotStrategy::Direct,
        },
        return_ty,
        udt_pure_tys: &udt_pure_tys,
    };
    create_flag_trailing_expr_for_slot(package, assigner, stmts, &flag_context).0
}

fn create_flag_trailing_expr_for_slot(
    package: &mut Package,
    assigner: &mut Assigner,
    stmts: &mut Vec<StmtId>,
    flag_context: &FlagContext<'_>,
) -> (Option<StmtId>, Option<LocalVarId>) {
    let trailing_expr = stmts.last().and_then(|&stmt_id| {
        if let StmtKind::Expr(expr_id) = package.get_stmt(stmt_id).kind
            && package.get_expr(expr_id).ty == *flag_context.return_ty
        {
            Some(expr_id)
        } else {
            None
        }
    });

    let flag_var = alloc_local_var_expr(
        package,
        assigner,
        flag_context.has_returned_var_id,
        Ty::Prim(Prim::Bool),
        Span::default(),
    );
    let ret_var = create_return_slot_read_expr(
        package,
        assigner,
        flag_context.return_slot,
        flag_context.return_ty,
    );

    if let Some(original_trailing) = trailing_expr {
        stmts.pop().expect("stmts should not be empty");

        let (trailing_var_id, trailing_decl_stmt) = alloc_local_var(
            package,
            assigner,
            symbols::TRAILING_RESULT,
            flag_context.return_ty,
            original_trailing,
            Mutability::Immutable,
        );
        stmts.push(trailing_decl_stmt);

        let trailing_var_expr = alloc_local_var_expr(
            package,
            assigner,
            trailing_var_id,
            flag_context.return_ty.clone(),
            Span::default(),
        );
        let if_expr = alloc_if_expr(
            package,
            assigner,
            flag_var,
            ret_var,
            Some(trailing_var_expr),
            flag_context.return_ty.clone(),
            Span::default(),
        );
        (
            Some(alloc_expr_stmt(package, assigner, if_expr, Span::default())),
            Some(trailing_var_id),
        )
    } else {
        let fallback_expr = if flag_context.return_ty == &Ty::UNIT {
            alloc_unit_expr(package, assigner, Span::default())
        } else {
            create_return_slot_unwritten_fallback_expr(
                package,
                assigner,
                flag_context.return_slot,
                flag_context.return_ty,
            )
        };
        let if_expr = alloc_if_expr(
            package,
            assigner,
            flag_var,
            ret_var,
            Some(fallback_expr),
            flag_context.return_ty.clone(),
            Span::default(),
        );
        (
            Some(alloc_expr_stmt(package, assigner, if_expr, Span::default())),
            None,
        )
    }
}

fn create_not_var_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    var_id: LocalVarId,
) -> ExprId {
    let var = alloc_local_var_expr(
        package,
        assigner,
        var_id,
        Ty::Prim(Prim::Bool),
        Span::default(),
    );
    alloc_not_expr(package, assigner, var, Span::default())
}

fn create_assign_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    var_id: LocalVarId,
    value: ExprId,
    ty: &Ty,
) -> ExprId {
    let var_expr = alloc_local_var_expr(package, assigner, var_id, ty.clone(), Span::default());
    alloc_assign_expr(package, assigner, var_expr, value, Span::default())
}

fn create_mutable_bool_var(
    package: &mut Package,
    assigner: &mut Assigner,
    name: &str,
    value: bool,
) -> (LocalVarId, StmtId) {
    let init_expr = alloc_bool_lit(package, assigner, value, Span::default());
    alloc_local_var(
        package,
        assigner,
        name,
        &Ty::Prim(Prim::Bool),
        init_expr,
        Mutability::Mutable,
    )
}
