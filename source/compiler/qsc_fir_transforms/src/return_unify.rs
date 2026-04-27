// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Return unification pass.
//!
//! Eliminates all `ExprKind::Return` nodes from callable bodies, ensuring
//! every callable has exactly one exit point — the trailing expression of its
//! top-level block.
//!
//! Establishes [`crate::invariants::InvariantLevel::PostReturnUnify`]
//! (additionally) on top of [`crate::invariants::InvariantLevel::PostMono`]:
//! no `ExprKind::Return` remains in reachable code.
//!
//! # Pipeline position
//!
//! This pass runs after monomorphization (types are concrete) and before
//! defunctionalization. Synthesized expressions use `EMPTY_EXEC_RANGE`; the
//! [`crate::exec_graph_rebuild`] pass rebuilds correct exec graphs afterward.
//! See [`crate::run_pipeline_to_impl`] for the full ordering.
//!
//! # Architecture
//!
//! The pass uses a four-phase pipeline per callable block:
//!
//! 1. **Normalize** ([`normalize::hoist_returns_to_statement_boundary`]):
//!    Hoist any `Return` in compound positions (e.g. inside a block-expression
//!    used as a `Call` argument) to its enclosing statement boundary. After
//!    this phase, every `Return` is either a bare `Semi(Return(_))` /
//!    `Expr(Return(_))` or nested inside `If`, `While`, or `Block` statements.
//!
//! 2. **Dispatch** ([`should_use_flag_strategy`]):
//!    Classify the block into one of the dispatch categories below and select
//!    the appropriate transform strategy.
//!
//! 3. **Transform** ([`transform_block_if_else`] or
//!    [`transform_block_with_flags`]):
//!    Apply the selected strategy to eliminate all `Return` nodes.
//!
//! 4. **Simplify** ([`simplify_flag_patterns`]):
//!    After the flag strategy, fold trivial identity patterns such as
//!    `if __has_returned { v } else { v }` → `v`. This is the structured-IR
//!    analog of LLVM's `SimplifyCFG` after `mergereturn`.
//!
//! ## Strategies
//!
//! 1. **If-else lifting** (primary, [`transform_block_if_else`]):
//!    Restructures blocks containing returns into nested if-else expressions.
//!    Handles guard clauses and branching returns without introducing mutable
//!    state. Selected for category-A shapes.
//!
//! 2. **Flag-based transform** (fallback, [`transform_block_with_flags`]):
//!    Introduces `__has_returned` and `__ret_val` mutable locals to handle
//!    returns inside while loops, leaky nested-if patterns, and block-init
//!    returns. Selected for category-B, -C, and -D shapes.
//!
//! # Input patterns
//!
//! - `Return(value)` appearing inside conditional or loop blocks.
//!
//! # Rewrites
//!
//! Flag-based rewrite of a return inside a while loop:
//!
//! ```text
//! // Before
//! mutable r = 0;
//! while cond {
//!     if done { return r; }
//!     r += 1;
//! }
//!
//! // After
//! mutable __has_returned = false;
//! mutable __ret_val = 0;
//! mutable r = 0;
//! while not __has_returned and cond {
//!     if done {
//!         __ret_val = r;
//!         __has_returned = true;
//!     } else {
//!         r += 1;
//!     }
//! }
//! if __has_returned { __ret_val } else { () }
//! ```
//!
//! # Dispatch policy
//!
//! The function [`should_use_flag_strategy`] is the single dispatch point
//! that decides, per callable block, whether to use the structured if-else
//! lifting strategy or fall back to the flag-based transform.
//!
//! Fallback detection is driven by [`contains_return_in_while`] and
//! [`contains_leaky_early_return`], with nested statement/expression scans
//! delegated to [`crate::return_unify::detect::contains_return_in_expr`] and
//! its block-level companion.
//!
//! The dispatch categories recognized today are:
//!
//! * **Category A — guard clauses and pure `if`/`else` nests.** Returns
//!   appear only on conditional branches outside `while` bodies and do not
//!   hit the leaky nested-if shape; the structured strategy lifts them into
//!   nested if-else expressions.
//! * **Category B — returns inside while loops.** Any `Return` reachable in a
//!   `while` body causes [`should_use_flag_strategy`] to select the flag-based
//!   fallback.
//! * **Category C — leaky nested-if early returns.** A `Return` under an
//!   if-without-else chain at depth >= 2 causes
//!   [`should_use_flag_strategy`] to select the flag-based fallback.
//! * **Category D — returns inside block-expression Local initializers.**
//!   A `Return` inside a `Block` expression used as a `Local` initializer
//!   causes [`should_use_flag_strategy`] to select the flag-based fallback
//!   (detected by [`contains_return_in_block_init`]).
//!   Non-block `Local`-initializer `MayReturn` shapes stay on the structured
//!   path and are rewritten by [`transform_local_init`] via
//!   [`decompose_returning_init`].
//!
//! The flag-strategy fallback is modeled on the LLVM lowering pattern for
//! early returns (cf. LLVM `UnifyFunctionExitNodes` / `mergereturn`): a
//! synthesized `__has_returned` slot plus a merge block guard the remainder
//! of the loop body, preserving the semantics of the original early exit
//! when category-B, -C, or -D shape makes the structured lowering unsound.
//!
//! # Invariant contracts
//!
//! After this pass completes, the following invariants hold:
//!
//! * **No `Return` nodes** — checked by
//!   `crate::invariants::check_no_returns`. Every `ExprKind::Return` in
//!   reachable code must be eliminated; any surviving `Return` triggers a
//!   hard assertion failure.
//! * **Non-Unit block tails** — checked by
//!   `crate::invariants::check_non_unit_block_tails`. Every block whose
//!   type is not `Unit` must end with a `StmtKind::Expr` (not `Semi`),
//!   ensuring downstream code generation sees a value-producing tail.
//!
//! These invariants are verified at
//! [`crate::invariants::InvariantLevel::PostReturnUnify`] by the pipeline
//! runner after this pass returns.
//!
//! # Error reporting
//!
//! [`unify_returns`] returns `Vec<Error>` rather than panicking. The known
//! user-reachable error is [`Error::UnsupportedLoopReturnType`]: the flag
//! strategy requires a classical default for `__ret_val`, but types like
//! `Qubit` have no classical default. This is caught by
//! [`can_create_classical_default`] before entering the transform, producing
//! a user-facing diagnostic. Processing continues for remaining callables.
//!
//! # Qubit release interaction
//!
//! This pass does not classify or hoist release calls. Release operations are
//! treated as ordinary side effects, and correctness comes from preserving
//! control-flow reachability while eliminating `Return` nodes:
//!
//! - In structured `if` rewrites, continuation statements (including releases)
//!   are moved only to fallthrough paths.
//! - For `if` where both branches always return, trailing continuation is dead
//!   and removed.
//! - For `Local` initializer `MayReturn` shapes, the pass decomposes the init
//!   into an outer guard statement ([`decompose_returning_init`]) so the
//!   continuation executes only on fallthrough.
//!
//! This avoids dedicated release-shape analysis while still preventing
//! path-duplication bugs such as double release.
//!
//! Extension: to add a new category, widen [`should_use_flag_strategy`]
//! and extend the test matrix under `return_unify/normalize/tests.rs`.

mod detect;
mod normalize;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "slow-proptest-tests"))]
mod semantic_equivalence_tests;

use miette::Diagnostic;
use num_bigint::BigInt;
use qsc_data_structures::span::Span;
use qsc_fir::{
    assigner::Assigner,
    fir::{
        BinOp, Block, BlockId, CallableDecl, CallableImpl, Expr, ExprId, ExprKind, Ident, ItemKind,
        Lit, LocalItemId, LocalVarId, Mutability, Package, PackageId, PackageLookup, PackageStore,
        Pat, PatId, PatKind, Res, Result, Stmt, StmtId, StmtKind, UnOp,
    },
    ty::{Prim, Ty},
};
use rustc_hash::FxHashMap;
use std::rc::Rc;
use thiserror::Error;

use crate::{EMPTY_EXEC_RANGE, reachability::collect_reachable_from_entry};

/// Errors that can occur during return unification.
#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    /// The flag-based return unification strategy requires a classical default
    /// value for the return type to initialize `__ret_val`. Types such as
    /// `Qubit` have no classical default and cannot be handled.
    #[error("cannot unify returns of type `{0}` inside a loop")]
    #[diagnostic(code("Qsc.ReturnUnify.UnsupportedLoopReturnType"))]
    #[diagnostic(help(
        "the return type has no classical default value; \
         consider restructuring to avoid returning this type from inside a loop"
    ))]
    UnsupportedLoopReturnType(
        String,
        #[label("callable with unsupported return pattern")] Span,
    ),
}

type UdtPureTyCache = FxHashMap<(PackageId, LocalItemId), Ty>;

/// Builds a UDT identifier → pure representation type cache.
///
/// Keys are `(PackageId, LocalItemId)` for every UDT in the store and values
/// are the callable-free representation type produced by
/// [`qsc_fir::ty::Udt::get_pure_ty`], used when synthesizing default values
/// for the flag-based transform.
fn build_udt_pure_ty_cache(store: &PackageStore) -> UdtPureTyCache {
    let mut cache = FxHashMap::default();
    for (pkg_id, package) in store {
        for (item_id, item) in &package.items {
            if let ItemKind::Ty(_, udt) = &item.kind {
                cache.insert((pkg_id, item_id), udt.get_pure_ty());
            }
        }
    }
    cache
}

/// Eliminate all `ExprKind::Return` nodes from reachable callable bodies.
///
/// # Before
/// ```text
/// callable body { ...; return v; ...; trailing }
/// ```
/// # After
/// ```text
/// callable body { ...; ...; new_trailing }   // no ExprKind::Return remains
/// ```
/// # Requires
/// - `package_id` is present in `store`.
/// - Monomorphization has run (types are concrete).
///
/// # Ensures
/// - Establishes [`crate::invariants::InvariantLevel::PostReturnUnify`] on
///   top of `PostMono`: no `ExprKind::Return` in reachable bodies.
/// - Each rewritten body's trailing expression produces the callable's
///   return value via if-else lifting or the flag-based transform.
///
/// # Mutations
/// - Rewrites `CallableDecl` body blocks in `store[package_id]`.
/// - Allocates new FIR nodes through `assigner`.
//
// Only entry-reachable callables are unified. Unreachable callables retain
// their `Return` nodes, but this is safe because:
// 1. `check_no_returns` walks the same reachable set returned by
//    [`collect_reachable_from_entry`].
// 2. Downstream passes (defunc, udt_erase, sroa, arg_promote,
//    exec_graph_rebuild) recompute reachability via the same walker and
//    never re-reach a callable that was unreachable here. Defunc's
//    specialization creates new clone items rather than widening
//    reachability to existing-but-dead items.
// 3. A future pass that violates this (for example, inlines a dead call or
//    rewires a dead callable into the call graph) must re-invoke
//    `unify_returns` on newly reachable items before `check_no_returns`
//    runs.
//
// Re-audit trigger: the defunc "tagged-union" future work noted at
// source/compiler/qsc_fir_transforms/src/defunctionalize.rs:42-45 could
// change the reachability story above; this rationale must be re-validated
// if that design lands. Assessment (2026-04): tagged-union
// defunctionalization would create *new* dispatch items (union type +
// apply function) rather than widening reachability to existing dead
// callables, so the invariant is expected to hold. Re-audit if the
// tagged-union design instead reuses or inlines dead callables.
pub fn unify_returns(
    store: &mut PackageStore,
    package_id: PackageId,
    assigner: &mut Assigner,
) -> Vec<Error> {
    let reachable = collect_reachable_from_entry(store, package_id);
    let udt_pure_tys = build_udt_pure_ty_cache(store);
    let mut errors = Vec::new();

    let local_reachable: Vec<_> = reachable
        .iter()
        .filter(|id| id.package == package_id)
        .map(|id| id.item)
        .collect();

    let package = store.get_mut(package_id);

    for item_id in local_reachable {
        let callable = {
            let item = package.get_item(item_id);
            match &item.kind {
                ItemKind::Callable(callable) => callable.clone(),
                _ => continue,
            }
        };
        let return_ty = callable.output.clone();
        for block_id in get_callable_body_blocks(&callable) {
            if contains_return_in_block(package, block_id) {
                // Pre-pass: hoist any compound-position Return to its
                // enclosing statement boundary so the strategy pass only sees
                // bare returns or returns inside statement-carrying Block/If/While.
                normalize::hoist_returns_to_statement_boundary(
                    package, assigner, package_id, block_id,
                );
                if should_use_flag_strategy(package, block_id) {
                    // The flag strategy requires a classical default for
                    // `__ret_val`. Check before entering the transform so
                    // unsupported types (e.g. Qubit) produce a user-facing
                    // diagnostic instead of panicking.
                    if !can_create_classical_default(&return_ty, &udt_pure_tys) {
                        errors.push(Error::UnsupportedLoopReturnType(
                            format!("{return_ty}"),
                            callable.name.span,
                        ));
                        continue;
                    }
                    transform_block_with_flags(
                        package,
                        assigner,
                        package_id,
                        block_id,
                        &return_ty,
                        &udt_pure_tys,
                    );
                    simplify_flag_patterns(package, block_id);
                } else {
                    transform_block_if_else(package, assigner, block_id, &return_ty);
                }
            }
        }
    }

    errors
}

/// Extract every explicit body block from a callable declaration.
///
/// # Before
/// ```text
/// CallableDecl { implementation: Spec { body, adj?, ctl?, ctl_adj? } }
/// ```
/// # After
/// ```text
/// [body.block, adj.block?, ctl.block?, ctl_adj.block?]
/// ```
/// # Requires
/// - `callable` has been lowered to FIR.
///
/// # Ensures
/// - Returns an empty `Vec` for `CallableImpl::Intrinsic`.
/// - Includes only specializations with an explicit body block.
///
/// # Mutations
/// - None (read-only).
fn get_callable_body_blocks(callable: &CallableDecl) -> Vec<BlockId> {
    // Exhaustive match over CallableImpl. Adding a variant fails to compile
    // here; extend the match rather than adding a wildcard.
    match &callable.implementation {
        CallableImpl::Intrinsic => Vec::new(),
        CallableImpl::Spec(spec_impl) => {
            let mut blocks = vec![spec_impl.body.block];
            for spec in [&spec_impl.adj, &spec_impl.ctl, &spec_impl.ctl_adj]
                .into_iter()
                .flatten()
            {
                blocks.push(spec.block);
            }
            blocks
        }
        CallableImpl::SimulatableIntrinsic(spec) => vec![spec.block],
    }
}

use detect::{contains_return_in_block, contains_return_in_expr, contains_return_in_stmt};

/// Returns true if any `Return` node is inside a while loop body.
fn contains_return_in_while(package: &Package, block_id: BlockId) -> bool {
    let block = package.get_block(block_id);
    block
        .stmts
        .iter()
        .any(|&stmt_id| contains_return_in_while_stmt(package, stmt_id))
}

/// Returns true if any `Return` node is reachable through a while-condition
/// expression (at any nesting depth).
fn contains_return_in_while_condition(package: &Package, block_id: BlockId) -> bool {
    let block = package.get_block(block_id);
    block
        .stmts
        .iter()
        .any(|&stmt_id| contains_return_in_while_condition_stmt(package, stmt_id))
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

fn contains_return_in_while_condition_stmt(package: &Package, stmt_id: StmtId) -> bool {
    let stmt = package.get_stmt(stmt_id);
    match &stmt.kind {
        StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) | StmtKind::Local(_, _, expr_id) => {
            contains_return_in_while_condition_expr(package, *expr_id)
        }
        StmtKind::Item(_) => false,
    }
}

fn contains_return_in_while_expr(package: &Package, expr_id: ExprId) -> bool {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::While(_, body_id) => contains_return_in_block(package, *body_id),
        ExprKind::Block(block_id) => contains_return_in_while(package, *block_id),
        ExprKind::If(_, then_id, else_opt) => {
            contains_return_in_while_expr(package, *then_id)
                || else_opt.is_some_and(|e| contains_return_in_while_expr(package, e))
        }
        _ => false,
    }
}

fn contains_return_in_while_condition_expr(package: &Package, expr_id: ExprId) -> bool {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::While(cond_id, body_id) => {
            contains_return_in_expr(package, *cond_id)
                || contains_return_in_while_condition(package, *body_id)
        }
        ExprKind::Block(block_id) => contains_return_in_while_condition(package, *block_id),
        ExprKind::If(cond_id, then_id, else_opt) => {
            contains_return_in_while_condition_expr(package, *cond_id)
                || contains_return_in_while_condition_expr(package, *then_id)
                || else_opt.is_some_and(|e| contains_return_in_while_condition_expr(package, e))
        }
        _ => false,
    }
}

/// Returns true when the flag-based strategy is required for `block_id`.
///
/// The if-else lifting strategy cannot correctly handle either:
/// 1. Returns nested inside while loops (already detected by
///    [`contains_return_in_while`]), or
/// 2. Returns reachable through while-condition expressions (detected by
///    [`contains_return_in_while_condition`]), or
/// 3. "Leaky" early returns inside an if-without-else nested at depth >= 2,
///    where lifting would synthesize an empty-else continuation whose type
///    does not match the non-Unit return type (detected by
///    [`contains_leaky_early_return`]).
/// 4. Returns inside a `Block` expression used as a `Local` initializer,
///    where the structured strategy's `strip_returns_from_block` would
///    consume the return at the block level instead of propagating it to
///    the enclosing callable (detected by
///    [`contains_return_in_block_init`]).
fn should_use_flag_strategy(package: &Package, block_id: BlockId) -> bool {
    contains_return_in_while(package, block_id)
        || contains_return_in_while_condition(package, block_id)
        || contains_leaky_early_return(package, block_id)
        || contains_return_in_block_init(package, block_id)
}

/// Returns true when any `Local` statement in the block has an initializer
/// that is a `Block` expression containing a `Return` inside a nested
/// control-flow construct (`If`, `While`, or inner `Block`), or an `If`
/// expression containing a `Return` inside a `While` loop. Bare returns
/// at the init block's direct statement level are handled correctly by the
/// structured strategy's `transform_local_init` + `apply_bare_return`, so
/// they are excluded. For `If`-expression initializers,
/// `strip_returns_from_expr` handles returns directly in branches and
/// nested if-else chains; only returns inside `While` loops within the
/// branches require the flag strategy.
fn contains_return_in_block_init(package: &Package, block_id: BlockId) -> bool {
    let block = package.get_block(block_id);
    block.stmts.iter().any(|&stmt_id| {
        let stmt = package.get_stmt(stmt_id);
        if let StmtKind::Local(_, _, init_id) = &stmt.kind {
            let init_expr = package.get_expr(*init_id);
            match &init_expr.kind {
                ExprKind::Block(inner_block_id) => {
                    return block_has_nested_return(package, *inner_block_id);
                }
                ExprKind::If(_, then_id, else_opt) => {
                    return contains_return_in_while_expr(package, *then_id)
                        || else_opt.is_some_and(|e| contains_return_in_while_expr(package, e));
                }
                _ => {}
            }
        }
        false
    })
}

/// Returns true when any statement in the block contains a `Return` inside
/// a nested construct (`If`, `While`, inner `Block`) rather than as a bare
/// `Semi(Return(_))` / `Expr(Return(_))`.
fn block_has_nested_return(package: &Package, block_id: BlockId) -> bool {
    let block = package.get_block(block_id);
    block.stmts.iter().any(|&stmt_id| {
        let stmt = package.get_stmt(stmt_id);
        let expr_id = match &stmt.kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => *e,
            StmtKind::Item(_) => return false,
        };
        let expr = package.get_expr(expr_id);
        match &expr.kind {
            ExprKind::If(_, then_id, else_opt) => {
                contains_return_in_expr(package, *then_id)
                    || else_opt.is_some_and(|e| contains_return_in_expr(package, e))
            }
            ExprKind::While(cond, body) => {
                contains_return_in_expr(package, *cond) || contains_return_in_block(package, *body)
            }
            ExprKind::Block(bid) => contains_return_in_block(package, *bid),
            _ => false,
        }
    })
}

/// Returns true if a `Return` appears inside an if-without-else nested at
/// depth >= 2 within the block. Such returns cannot be lifted via the
/// if-else strategy because the synthesized empty-else continuation block
/// would be typed `Unit`, conflicting with a non-Unit callable return type.
fn contains_leaky_early_return(package: &Package, block_id: BlockId) -> bool {
    let block = package.get_block(block_id);
    block
        .stmts
        .iter()
        .any(|&stmt_id| leaky_early_return_in_stmt(package, stmt_id, 0))
}

fn leaky_early_return_in_stmt(package: &Package, stmt_id: StmtId, if_no_else_depth: u32) -> bool {
    let stmt = package.get_stmt(stmt_id);
    match &stmt.kind {
        StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) | StmtKind::Local(_, _, expr_id) => {
            leaky_early_return_in_expr(package, *expr_id, if_no_else_depth)
        }
        StmtKind::Item(_) => false,
    }
}

fn leaky_early_return_in_expr(package: &Package, expr_id: ExprId, if_no_else_depth: u32) -> bool {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Return(_) => if_no_else_depth >= 2,
        ExprKind::If(_, then_id, None) => {
            leaky_early_return_in_expr(package, *then_id, if_no_else_depth + 1)
        }
        ExprKind::If(_, then_id, Some(else_id)) => {
            leaky_early_return_in_expr(package, *then_id, if_no_else_depth)
                || leaky_early_return_in_expr(package, *else_id, if_no_else_depth)
        }
        ExprKind::Block(bid) => {
            let inner = package.get_block(*bid);
            inner
                .stmts
                .iter()
                .any(|&s| leaky_early_return_in_stmt(package, s, if_no_else_depth))
        }
        // A while whose body transitively contains a return is already
        // covered by `contains_return_in_while`; re-report here so any such
        // shape triggers the flag strategy through this predicate too. A
        // return-free while (e.g., residual structural while after hoist
        // moves a return out of a Local initializer) must stay on the
        // structured path, otherwise the flag strategy would preserve the
        // now-dead while and its references to deleted bindings.
        ExprKind::While(_, body) => contains_return_in_block(package, *body),
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReturnFlow {
    FallsThrough,
    MayReturn,
    AlwaysReturns,
}

impl ReturnFlow {
    fn sequence_with(self, next: Self) -> Self {
        match (self, next) {
            (Self::AlwaysReturns, _) => Self::AlwaysReturns,
            (Self::FallsThrough, flow) => flow,
            (Self::MayReturn, Self::AlwaysReturns) => Self::AlwaysReturns,
            (Self::MayReturn, _) => Self::MayReturn,
        }
    }

    fn from_if_branches(then_flow: Self, else_flow: Option<Self>) -> Self {
        match else_flow {
            Some(else_flow) => match (then_flow, else_flow) {
                (Self::AlwaysReturns, Self::AlwaysReturns) => Self::AlwaysReturns,
                (Self::FallsThrough, Self::FallsThrough) => Self::FallsThrough,
                _ => Self::MayReturn,
            },
            None if then_flow == Self::FallsThrough => Self::FallsThrough,
            None => Self::MayReturn,
        }
    }
}

fn block_return_flow(package: &Package, block_id: BlockId) -> ReturnFlow {
    let mut flow = ReturnFlow::FallsThrough;
    for &stmt_id in &package.get_block(block_id).stmts {
        flow = flow.sequence_with(stmt_return_flow(package, stmt_id));
        if flow == ReturnFlow::AlwaysReturns {
            return flow;
        }
    }
    flow
}

fn stmt_return_flow(package: &Package, stmt_id: StmtId) -> ReturnFlow {
    let stmt = package.get_stmt(stmt_id);
    match &stmt.kind {
        StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) | StmtKind::Local(_, _, expr_id) => {
            expr_return_flow(package, *expr_id)
        }
        StmtKind::Item(_) => ReturnFlow::FallsThrough,
    }
}

fn sequence_expr_flows(
    package: &Package,
    expr_ids: impl IntoIterator<Item = ExprId>,
) -> ReturnFlow {
    let mut flow = ReturnFlow::FallsThrough;
    for expr_id in expr_ids {
        flow = flow.sequence_with(expr_return_flow(package, expr_id));
        if flow == ReturnFlow::AlwaysReturns {
            return flow;
        }
    }
    flow
}

fn expr_return_flow(package: &Package, expr_id: ExprId) -> ReturnFlow {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Return(_) => ReturnFlow::AlwaysReturns,
        ExprKind::Block(block_id) => block_return_flow(package, *block_id),
        ExprKind::If(cond_id, then_id, else_opt) => {
            let cond_flow = expr_return_flow(package, *cond_id);
            let branch_flow = ReturnFlow::from_if_branches(
                expr_return_flow(package, *then_id),
                else_opt.map(|else_id| expr_return_flow(package, else_id)),
            );
            cond_flow.sequence_with(branch_flow)
        }
        ExprKind::While(cond_id, body_id) => match expr_return_flow(package, *cond_id) {
            ReturnFlow::AlwaysReturns => ReturnFlow::AlwaysReturns,
            ReturnFlow::MayReturn => ReturnFlow::MayReturn,
            ReturnFlow::FallsThrough => match block_return_flow(package, *body_id) {
                ReturnFlow::FallsThrough => ReturnFlow::FallsThrough,
                ReturnFlow::MayReturn | ReturnFlow::AlwaysReturns => ReturnFlow::MayReturn,
            },
        },
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            sequence_expr_flows(package, exprs.iter().copied())
        }
        ExprKind::ArrayRepeat(a_id, b_id)
        | ExprKind::Assign(a_id, b_id)
        | ExprKind::AssignOp(_, a_id, b_id)
        | ExprKind::BinOp(_, a_id, b_id)
        | ExprKind::Call(a_id, b_id)
        | ExprKind::Index(a_id, b_id)
        | ExprKind::AssignField(a_id, _, b_id)
        | ExprKind::UpdateField(a_id, _, b_id) => sequence_expr_flows(package, [*a_id, *b_id]),
        ExprKind::AssignIndex(a_id, b_id, c_id) | ExprKind::UpdateIndex(a_id, b_id, c_id) => {
            sequence_expr_flows(package, [*a_id, *b_id, *c_id])
        }
        ExprKind::Fail(inner_id) | ExprKind::Field(inner_id, _) | ExprKind::UnOp(_, inner_id) => {
            expr_return_flow(package, *inner_id)
        }
        ExprKind::Range(start, step, end) => {
            let expr_ids = [start, step, end].into_iter().flatten().copied();
            sequence_expr_flows(package, expr_ids)
        }
        ExprKind::Struct(_, copy, fields) => {
            let copy_flow = copy.map_or(ReturnFlow::FallsThrough, |copy_id| {
                expr_return_flow(package, copy_id)
            });
            let field_flow = sequence_expr_flows(package, fields.iter().map(|field| field.value));
            copy_flow.sequence_with(field_flow)
        }
        ExprKind::String(components) => {
            let expr_ids = components.iter().filter_map(|component| match component {
                qsc_fir::fir::StringComponent::Expr(expr_id) => Some(*expr_id),
                qsc_fir::fir::StringComponent::Lit(_) => None,
            });
            sequence_expr_flows(package, expr_ids)
        }
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {
            ReturnFlow::FallsThrough
        }
    }
}

/// Classification of a statement that contains a Return.
enum ReturnClass {
    /// The statement is `Expr(Return(inner))` or `Semi(Return(inner))`.
    BareReturn(ExprId),
    /// An if where only the then-branch contains a Return.
    IfThenReturn {
        cond: ExprId,
        then_expr: ExprId,
        else_opt: Option<ExprId>,
    },
    /// An if where both branches contain a Return.
    IfBothReturn {
        cond: ExprId,
        then_expr: ExprId,
        else_expr: ExprId,
    },
    /// An if where only the else-branch contains a Return. Normalized to
    /// `IfThenReturn` with negated condition at dispatch so downstream
    /// transform code only handles the then-return shape.
    IfElseReturn {
        cond: ExprId,
        then_expr: ExprId,
        else_expr: ExprId,
    },
    /// A nested block expression that needs recursive descent.
    NestedBlock(BlockId),
    /// A Local binding whose init expression contains Returns.
    /// The returns must be stripped from the init and types updated.
    LocalInit(PatId, ExprId),
    /// No return found.
    None,
}

/// Classify a statement's relationship to `ExprKind::Return`.
///
/// # Before
/// ```text
/// StmtKind::{Expr, Semi}(If/Block/Return/...) | StmtKind::Local(.., init)
/// ```
/// # After
/// ```text
/// ReturnClass::{BareReturn, IfThenReturn, IfBothReturn, IfElseReturn,
///               NestedBlock, LocalInit, None}
/// ```
/// # Requires
/// - `kind` is the kind of a statement whose expressions are valid in `package`.
///
/// # Ensures
/// - Returns the most specific shape matching the statement's surface expression.
/// - Returns `ReturnClass::None` for `StmtKind::Item` and return-free initializers.
///
/// # Mutations
/// - None (read-only).
///
/// # Notes
///
/// `StmtKind::Semi(e)` and `StmtKind::Expr(e)` both collapse to
/// `ReturnClass::BareReturn(*inner)` using the same `inner` expression. This
/// mapping is lossy on purpose: [`apply_bare_return`] discards the source
/// `StmtKind` and synthesizes a fresh `StmtKind::Expr(inner)` trailing
/// statement, then overwrites `block.ty` with the inner expression's type.
/// Downstream callers therefore must not depend on the original `Semi` vs
/// `Expr` kind being preserved for a bare-return statement.
fn classify_return_stmt(package: &Package, kind: &StmtKind) -> ReturnClass {
    let expr_id = match kind {
        StmtKind::Expr(id) | StmtKind::Semi(id) => *id,
        StmtKind::Local(_, pat_id, init_id) => {
            return if contains_return_in_expr(package, *init_id) {
                ReturnClass::LocalInit(*pat_id, *init_id)
            } else {
                ReturnClass::None
            };
        }
        StmtKind::Item(_) => return ReturnClass::None,
    };

    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Return(inner) => ReturnClass::BareReturn(*inner),
        ExprKind::If(cond, then_expr, else_opt) => {
            let then_has = contains_return_in_expr(package, *then_expr);
            let else_has = else_opt.is_some_and(|e| contains_return_in_expr(package, e));
            match (then_has, else_has) {
                (true, true) => ReturnClass::IfBothReturn {
                    cond: *cond,
                    then_expr: *then_expr,
                    else_expr: else_opt.expect("else branch must exist when it contains a return"),
                },
                (true, false) => ReturnClass::IfThenReturn {
                    cond: *cond,
                    then_expr: *then_expr,
                    else_opt: *else_opt,
                },
                (false, true) => ReturnClass::IfElseReturn {
                    cond: *cond,
                    then_expr: *then_expr,
                    else_expr: else_opt.expect("else branch must exist when it contains a return"),
                },
                (false, false) => ReturnClass::None,
            }
        }
        ExprKind::Block(block_id) => {
            if contains_return_in_block(package, *block_id) {
                ReturnClass::NestedBlock(*block_id)
            } else {
                ReturnClass::None
            }
        }
        _ => ReturnClass::None,
    }
}

/// Rewrite the first return-containing statement in `block_id` into return-free flow.
///
/// Finds the first statement that still contains an `ExprKind::Return`,
/// classifies it via [`classify_return_stmt`], and dispatches to the
/// matching per-shape rewriter:
///
/// | Classification      | Dispatched helper                                 |
/// |---------------------|---------------------------------------------------|
/// | `BareReturn`        | [`transform_bare_return`]                         |
/// | `IfThenReturn`      | [`apply_if_then_return`]                          |
/// | `IfBothReturn`      | [`apply_if_both_return`]                          |
/// | `IfElseReturn`      | [`transform_if_else_return`] (normalizing rewrite)|
/// | `NestedBlock`       | [`transform_nested_block`]                        |
/// | `LocalInit`         | [`transform_local_init`]                          |
/// | `None`              | no-op                                             |
///
/// # Before
/// ```text
/// { stmts_before; if cond { return v; } stmts_after }   // IfThenReturn shape
/// ```
/// # After
/// ```text
/// { stmts_before; if cond { v } else { stmts_after } }
/// ```
/// # Requires
/// - `block_id` is valid in `package`.
/// - The normalization pre-pass has run, so Returns only appear at statement
///   boundaries or inside `Block`/`If`/`While` expressions.
///
/// # Ensures
/// - Returns `true` iff the block was rewritten.
/// - When `true`, the first return-containing statement has been replaced
///   by return-free control flow; recursion may rewrite nested blocks.
///
/// # Mutations
/// - Rewrites `Block.stmts` and `Block.ty` for `block_id` and reachable
///   sub-blocks via the dispatched helper.
/// - Allocates new FIR nodes through `assigner`.
#[allow(clippy::too_many_lines)]
fn transform_block_if_else(
    package: &mut Package,
    assigner: &mut Assigner,
    block_id: BlockId,
    return_ty: &Ty,
) -> bool {
    let stmts = package.get_block(block_id).stmts.clone();
    let first_return_idx = stmts
        .iter()
        .position(|&sid| contains_return_in_stmt(package, sid));
    let Some(idx) = first_return_idx else {
        return false;
    };

    let stmt_kind = package.get_stmt(stmts[idx]).kind.clone();
    match classify_return_stmt(package, &stmt_kind) {
        ReturnClass::BareReturn(inner_expr_id) => {
            transform_bare_return(package, assigner, block_id, idx, inner_expr_id)
        }
        ReturnClass::IfThenReturn {
            cond,
            then_expr,
            else_opt,
        } => {
            apply_if_then_return(
                package, assigner, block_id, idx, cond, then_expr, else_opt, return_ty,
            );
            true
        }
        ReturnClass::IfBothReturn {
            cond,
            then_expr,
            else_expr,
        } => {
            apply_if_both_return(
                package, assigner, block_id, idx, cond, then_expr, else_expr, return_ty,
            );
            true
        }
        ReturnClass::IfElseReturn {
            cond,
            then_expr,
            else_expr,
        } => {
            transform_if_else_return(
                package, assigner, block_id, return_ty, idx, cond, then_expr, else_expr,
            );
            true
        }
        ReturnClass::NestedBlock(inner_block_id) => transform_nested_block(
            package,
            assigner,
            block_id,
            return_ty,
            &stmts,
            idx,
            inner_block_id,
        ),
        ReturnClass::LocalInit(pat_id, init_expr_id) => transform_local_init(
            package,
            assigner,
            block_id,
            return_ty,
            idx,
            pat_id,
            init_expr_id,
        ),
        ReturnClass::None => false,
    }
}

/// Normalize an `IfElseReturn` (return only in the else branch) to the
/// `IfThenReturn` shape by negating the condition and swapping branches,
/// then delegate to [`apply_if_then_return`].
///
/// ```text
/// // Before
/// if cond { then_expr } else { return v; }
/// stmts_after;
///
/// // After (equivalent to apply_if_then_return on the negated shape)
/// if not cond { v } else { then_expr; stmts_after; }
/// ```
#[allow(clippy::too_many_arguments)]
fn transform_if_else_return(
    package: &mut Package,
    assigner: &mut Assigner,
    block_id: BlockId,
    return_ty: &Ty,
    idx: usize,
    cond: ExprId,
    then_expr: ExprId,
    else_expr: ExprId,
) {
    // Convert to IfThenReturn by negating the condition and swapping branches.
    let neg_cond = create_not_expr(package, assigner, cond);
    apply_if_then_return(
        package,
        assigner,
        block_id,
        idx,
        neg_cond,
        else_expr,
        Some(then_expr),
        return_ty,
    );
}

/// Rewrite a `let` binding whose initializer contains `Return` nodes.
///
/// When the init always returns, the let and all continuation are dead code —
/// strip returns and replace the block with just the init expression.
///
/// When the init may return (some paths return, others produce values),
/// decompose the init into a guard statement at the outer block level so the
/// return is visible to `transform_block_if_else`. This avoids stripping
/// returns in-place (which would leave side effects from the return path
/// reachable on the fallthrough path).
///
/// ```text
/// // Before
/// {
///     let x = if cond { return v; } else { u };
///     continuation;
/// }
///
/// // After decomposition (MayReturn case)
/// {
///     if cond { return v; }   // guard — return preserved
///     let x = u;              // fallthrough value only
///     continuation;
/// }
/// // Then transform_block_if_else handles the guard normally.
/// ```
fn transform_local_init(
    package: &mut Package,
    assigner: &mut Assigner,
    block_id: BlockId,
    return_ty: &Ty,
    idx: usize,
    pat_id: PatId,
    init_expr_id: ExprId,
) -> bool {
    let init_flow = expr_return_flow(package, init_expr_id);

    if init_flow == ReturnFlow::AlwaysReturns {
        // Everything after this let is dead code.
        strip_returns_from_expr(package, assigner, init_expr_id, return_ty);
        let new_init_ty = package.get_expr(init_expr_id).ty.clone();
        let new_stmt_id = create_expr_stmt(package, assigner, init_expr_id);
        let block = package.blocks.get_mut(block_id).expect("block not found");
        block.stmts.truncate(idx);
        block.stmts.push(new_stmt_id);
        block.ty = new_init_ty;
        return true;
    }

    // MayReturn: try to decompose the returning init into a guard statement
    // at the outer block level so the continuation only runs on fallthrough.
    if init_flow == ReturnFlow::MayReturn
        && decompose_returning_init(
            package,
            assigner,
            block_id,
            return_ty,
            idx,
            pat_id,
            init_expr_id,
        )
    {
        return true;
    }

    // FallsThrough or failed decomposition: strip returns and retype.
    strip_returns_from_expr(package, assigner, init_expr_id, return_ty);
    let new_init_ty = package.get_expr(init_expr_id).ty.clone();

    // Update the pattern's type to match the stripped init.
    let local_var_id = match &package.get_pat(pat_id).kind {
        PatKind::Bind(ident) => Some(ident.id),
        _ => None,
    };
    let pat = package.pats.get_mut(pat_id).expect("pat not found");
    pat.ty = new_init_ty.clone();

    // Update all Var references to this local in the block.
    if let Some(var_id) = local_var_id {
        let block_stmts = package.get_block(block_id).stmts.clone();
        for &stmt_id in &block_stmts {
            update_local_var_type(package, stmt_id, var_id, &new_init_ty);
        }
    }

    // Re-analyze this block after stripping so any newly exposed
    // returns or nested wrappers are normalized, then synchronize the
    // block type with its new trailing expression.
    let _ = transform_block_if_else(package, assigner, block_id, return_ty);
    sync_block_type_to_trailing_expr(package, block_id);
    true
}

/// Decompose a `MayReturn` init expression into a guard statement at the
/// outer block level, preserving the return so `transform_block_if_else`
/// can handle it with proper continuation threading.
///
/// Handles `if cond { RETURN_BRANCH } else { VALUE_BRANCH }` patterns
/// (and the inverse) by extracting the return-bearing branch into a
/// preceding guard statement and replacing the init with just the value.
///
/// Returns `true` if decomposition succeeded and the block was restructured.
#[allow(clippy::too_many_arguments)]
fn decompose_returning_init(
    package: &mut Package,
    assigner: &mut Assigner,
    block_id: BlockId,
    return_ty: &Ty,
    idx: usize,
    pat_id: PatId,
    init_expr_id: ExprId,
) -> bool {
    let init_kind = package.get_expr(init_expr_id).kind.clone();

    match init_kind {
        ExprKind::If(cond_id, then_id, Some(else_id)) => {
            let then_flow = expr_return_flow(package, then_id);
            let else_flow = expr_return_flow(package, else_id);

            match (then_flow, else_flow) {
                (ReturnFlow::AlwaysReturns | ReturnFlow::MayReturn, ReturnFlow::FallsThrough) => {
                    // Then branch returns, else is the fallthrough value.
                    // Insert: if cond { then_branch } as guard (return preserved!)
                    // Replace init with: else value
                    extract_guard_and_replace_init(
                        package,
                        assigner,
                        block_id,
                        return_ty,
                        idx,
                        pat_id,
                        init_expr_id,
                        cond_id,
                        then_id,
                        else_id,
                    );
                    true
                }
                (ReturnFlow::FallsThrough, ReturnFlow::AlwaysReturns | ReturnFlow::MayReturn) => {
                    // Else branch returns, then is the fallthrough value.
                    // Negate condition and swap.
                    let neg_cond = create_not_expr(package, assigner, cond_id);
                    extract_guard_and_replace_init(
                        package,
                        assigner,
                        block_id,
                        return_ty,
                        idx,
                        pat_id,
                        init_expr_id,
                        neg_cond,
                        else_id,
                        then_id,
                    );
                    true
                }
                _ => false,
            }
        }
        ExprKind::Block(inner_block_id) => {
            // Unwrap block and try to decompose the trailing expression.
            let inner_stmts = package.get_block(inner_block_id).stmts.clone();
            let Some(&tail_stmt_id) = inner_stmts.last() else {
                return false;
            };
            let tail_stmt = package.get_stmt(tail_stmt_id);
            let tail_expr_id = match &tail_stmt.kind {
                StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) => *expr_id,
                _ => return false,
            };
            // If the block has prefix statements before the tail, we can't
            // simply decompose — the prefix needs to stay in scope.
            if inner_stmts.len() > 1 {
                return false;
            }
            decompose_returning_init(
                package,
                assigner,
                block_id,
                return_ty,
                idx,
                pat_id,
                tail_expr_id,
            )
        }
        _ => false,
    }
}

/// Extract a return-bearing branch from an if-expression init into a guard
/// statement, replacing the init with the fallthrough value.
///
/// ```text
/// // Before
/// let x = if cond { return v; } else { u };
/// continuation;
///
/// // After
/// if cond { return v; }   // guard stmt inserted before the let
/// let x = u;              // init replaced with fallthrough value
/// continuation;
/// ```
#[allow(clippy::too_many_arguments)]
fn extract_guard_and_replace_init(
    package: &mut Package,
    assigner: &mut Assigner,
    block_id: BlockId,
    return_ty: &Ty,
    idx: usize,
    pat_id: PatId,
    init_expr_id: ExprId,
    cond_id: ExprId,
    return_branch_id: ExprId,
    value_branch_id: ExprId,
) {
    // Create the guard: if cond { return_branch } (no else — it's a guard)
    let guard_if = create_if_expr(
        package,
        assigner,
        cond_id,
        return_branch_id,
        None,
        &Ty::UNIT,
    );
    let guard_stmt = create_semi_stmt(package, assigner, guard_if);

    // Replace the init expression with the fallthrough value.
    let value_expr = package.get_expr(value_branch_id).clone();
    let init = package
        .exprs
        .get_mut(init_expr_id)
        .expect("init expr not found");
    init.kind = value_expr.kind;
    init.ty = value_expr.ty.clone();
    init.exec_graph_range = EMPTY_EXEC_RANGE;

    // Retype the pattern to match the new init type.
    let value_ty = value_expr.ty;
    let pat = package.pats.get_mut(pat_id).expect("pat not found");
    pat.ty = value_ty.clone();

    // Update all Var references to this local.
    if let PatKind::Bind(ident) = &package.get_pat(pat_id).kind {
        let var_id = ident.id;
        let block_stmts = package.get_block(block_id).stmts.clone();
        for &stmt_id in &block_stmts {
            update_local_var_type(package, stmt_id, var_id, &value_ty);
        }
    }

    // Insert the guard statement before the let binding.
    let block = package.blocks.get_mut(block_id).expect("block not found");
    block.stmts.insert(idx, guard_stmt);

    // Re-analyze the block — transform_block_if_else will find the guard
    // and move the continuation (including the let + subsequent stmts) into
    // the else branch, which is the correct control flow.
    let _ = transform_block_if_else(package, assigner, block_id, return_ty);
    sync_block_type_to_trailing_expr(package, block_id);
}

/// Recursively rewrite an inner block wrapped in a statement-position `Block` expression.
///
/// After the inner block is rewritten, this helper:
/// 1. Retypes the wrapper `Block` expression to match the inner block's new type.
/// 2. Promotes a trailing `Semi` wrapper to `Expr` so the inner value flows
///    out as the enclosing block's trailing expression.
///
/// # Before
/// ```text
/// { stmts_before; { stmts_inner; if cond { return v; } } }
/// ```
/// # After
/// ```text
/// {
///     stmts_before;
///     { stmts_inner; if cond { v } else { () } }
/// }
/// ```
/// # Requires
/// - `inner_block_id` is the body of the `Block` expression at `stmts[idx]`.
/// - `stmts` is the current statement list of `block_id`.
/// - The normalization pre-pass has run.
///
/// # Ensures
/// - Returns `true` iff inner or outer rewriting made progress.
/// - When the inner transform cannot proceed, returns `false` without
///   recursing to avoid infinite recursion.
/// - Block types remain consistent with their trailing expressions
///   (via `sync_block_type_to_trailing_expr` when needed).
///
/// # Mutations
/// - Rewrites the wrapper `Expr.ty` at `stmts[idx]`.
/// - May rewrite `Block.stmts`, `Block.ty`, and statement kinds for both
///   inner and outer blocks.
/// - Allocates new FIR nodes through `assigner`.
#[allow(clippy::too_many_arguments)]
fn transform_nested_block(
    package: &mut Package,
    assigner: &mut Assigner,
    block_id: BlockId,
    return_ty: &Ty,
    stmts: &[StmtId],
    idx: usize,
    inner_block_id: BlockId,
) -> bool {
    // Before transforming the inner block, check whether its first
    // return-containing statement is unconditional (BareReturn or
    // IfBothReturn). When the return is unconditional, ALL code paths
    // through the inner block end in a return, so statements after
    // this nested-block wrapper in the outer block are dead code.
    let is_unconditional_return =
        block_return_flow(package, inner_block_id) == ReturnFlow::AlwaysReturns;

    let inner_changed = transform_block_if_else(package, assigner, inner_block_id, return_ty);

    // If the inner block couldn't be transformed (e.g. the return is
    // inside a While that must be handled by the flag-based path),
    // stop to avoid infinite recursion.
    if !inner_changed {
        return false;
    }

    // Update the Block expression's type to match the inner block's new type.
    let new_inner_ty = package.get_block(inner_block_id).ty.clone();
    let wrapper_expr_id = match &package.get_stmt(stmts[idx]).kind {
        StmtKind::Expr(e) | StmtKind::Semi(e) => *e,
        _ => unreachable!("NestedBlock must be Expr or Semi"),
    };
    let e = package
        .exprs
        .get_mut(wrapper_expr_id)
        .expect("expr not found");
    e.ty = new_inner_ty.clone();

    // When the inner block's return was unconditional and there are
    // statements after this one in the outer block, all subsequent
    // statements are dead code. Truncate them and promote the block
    // expression to the outer block's trailing expression.
    if is_unconditional_return && idx < stmts.len() - 1 {
        apply_bare_return(package, assigner, block_id, idx, wrapper_expr_id);
        return true;
    }

    // If this is the last statement and is Semi, promote to Expr so the
    // value flows through as the block's trailing expression.
    if idx == stmts.len() - 1 {
        let stmt = package.stmts.get_mut(stmts[idx]).expect("stmt not found");
        if matches!(stmt.kind, StmtKind::Semi(_)) {
            stmt.kind = StmtKind::Expr(wrapper_expr_id);
        }
        let block = package.blocks.get_mut(block_id).expect("block not found");
        block.ty = new_inner_ty;
    } else {
        sync_block_type_to_trailing_expr(package, block_id);
    }

    // Re-analyze this block after inner transform.
    let outer_changed = transform_block_if_else(package, assigner, block_id, return_ty);
    inner_changed || outer_changed
}

/// Rewrite a bare-return statement into a trailing expression.
///
/// Runs [`apply_bare_return`] to drop post-return statements and install
/// the return value as the block's trailing expression.
///
/// ```text
/// // Before
/// {
///     stmts_before;
///     return v;
///     stmts_dead;
/// }
///
/// // After
/// {
///     stmts_before;
///     v
/// }
/// ```
fn transform_bare_return(
    package: &mut Package,
    assigner: &mut Assigner,
    block_id: BlockId,
    idx: usize,
    inner_expr_id: ExprId,
) -> bool {
    apply_bare_return(package, assigner, block_id, idx, inner_expr_id);
    true
}

/// Synchronize a block's type with the type of its trailing expression.
///
/// # Before
/// ```text
/// Block { stmts: [..., Expr(e: T)], ty: U }   // U may have drifted from T
/// ```
/// # After
/// ```text
/// Block { stmts: [..., Expr(e: T)], ty: T }
/// ```
/// # Requires
/// - `block_id` is valid in `package`.
///
/// # Ensures
/// - `Block.ty == trailing_expr.ty` after return if the block ends in a
///   `StmtKind::Expr`.
/// - No-op when the block is empty or ends in a non-expression statement.
///
/// # Mutations
/// - Writes `Block.ty` for `block_id` in place.
fn sync_block_type_to_trailing_expr(package: &mut Package, block_id: BlockId) {
    let Some(&stmt_id) = package.get_block(block_id).stmts.last() else {
        return;
    };

    let StmtKind::Expr(expr_id) = package.get_stmt(stmt_id).kind else {
        return;
    };

    let trailing_ty = package.get_expr(expr_id).ty.clone();
    let block = package.blocks.get_mut(block_id).expect("block not found");
    block.ty = trailing_ty;
}

/// Strongly sync a block's type to its tail: the trailing `Expr`'s type
/// when present, otherwise `Unit`. Used by the flag-strategy's Return
/// replacement so nested blocks whose trailing `Return(v)` expression was
/// typed to the callable return type get their type refreshed to `Unit`
/// once the Return has been replaced with a Unit flag-assignment block.
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

/// Replace a block's statements at and after `idx` with a single trailing
/// expression carrying the returned value.
///
/// ```text
/// // Before (stmt at idx is Expr(Return(inner)) or Semi(Return(inner)))
/// {
///     stmts[..idx];
///     return inner;
///     stmts[idx+1..];
/// }
///
/// // After
/// {
///     stmts[..idx];
///     inner
/// }
/// ```
///
/// Also updates the block's type to the type of `inner`.
fn apply_bare_return(
    package: &mut Package,
    assigner: &mut Assigner,
    block_id: BlockId,
    idx: usize,
    inner_expr_id: ExprId,
) {
    let inner_ty = package.get_expr(inner_expr_id).ty.clone();

    // Create a new trailing-expression statement for the inner value.
    let new_stmt_id = create_expr_stmt(package, assigner, inner_expr_id);

    let block = package.blocks.get_mut(block_id).expect("block not found");
    block.stmts.truncate(idx);
    block.stmts.push(new_stmt_id);
    block.ty = inner_ty;
}

/// Rewrite an `if cond { return v; }` guard-clause into a return-free
/// `if cond { v } else { /* continuation */ }` trailing expression.
///
/// Statements after the `if` become the new else branch (recursively
/// rewritten via [`transform_block_if_else`]). When the original `if`
/// already had an else, it is preserved as a leading `Semi` statement
/// inside that new else block.
///
/// ```text
/// // Before
/// {
///     stmts_before;
///     if cond { return v; } else { side_effect; }
///     rest;
/// }
///
/// // After
/// {
///     stmts_before;
///     if cond { v } else { side_effect; rest; }
/// }
/// ```
#[allow(clippy::too_many_arguments)]
fn apply_if_then_return(
    package: &mut Package,
    assigner: &mut Assigner,
    block_id: BlockId,
    idx: usize,
    cond: ExprId,
    then_expr: ExprId,
    else_opt: Option<ExprId>,
    return_ty: &Ty,
) {
    // Strip returns from the then branch.
    strip_returns_from_expr(package, assigner, then_expr, return_ty);

    // Collect remaining statements after the if.
    let remaining_stmts: Vec<StmtId> = package.get_block(block_id).stmts[idx + 1..].to_vec();

    let then_flow = expr_return_flow(package, then_expr);

    if then_flow == ReturnFlow::AlwaysReturns {
        let new_else_expr_id = create_fallthrough_continuation_expr(
            package,
            assigner,
            else_opt,
            remaining_stmts,
            return_ty,
        );
        let new_if_expr_id = create_if_expr(
            package,
            assigner,
            cond,
            then_expr,
            Some(new_else_expr_id),
            return_ty,
        );
        let new_tail = create_expr_stmt(package, assigner, new_if_expr_id);

        let block = package.blocks.get_mut(block_id).expect("block not found");
        block.stmts.truncate(idx);
        block.stmts.push(new_tail);
        block.ty = return_ty.clone();
        return;
    }

    if let Some(else_expr_id) = else_opt.filter(|_| remaining_stmts.is_empty()) {
        // No remaining statements; keep the existing else as-is but strip returns.
        strip_returns_from_expr(package, assigner, else_expr_id, return_ty);

        // Create the new if expression using the existing else.
        let new_if_expr_id = create_if_expr(
            package,
            assigner,
            cond,
            then_expr,
            Some(else_expr_id),
            return_ty,
        );
        let new_tail = create_expr_stmt(package, assigner, new_if_expr_id);

        let block = package.blocks.get_mut(block_id).expect("block not found");
        block.stmts.truncate(idx);
        block.stmts.push(new_tail);
        block.ty = return_ty.clone();
        return;
    }

    // Build a new block from the remaining statements (plus any existing else content).
    let new_else_block_id = if let Some(else_expr_id) = else_opt {
        // Prepend the existing else as a Semi statement in the new continuation block.
        let else_semi = create_semi_stmt(package, assigner, else_expr_id);
        let mut new_stmts = vec![else_semi];
        new_stmts.extend(remaining_stmts);
        create_block(package, assigner, new_stmts, return_ty)
    } else {
        if remaining_stmts.is_empty() {
            // Invariant: `should_use_flag_strategy` routes non-Unit leaky
            // early-return shapes to the flag strategy, so this empty-else
            // synthesis is only reachable for Unit-typed returns.
            assert!(
                *return_ty == Ty::UNIT,
                "apply_if_then_return reached empty-else for non-Unit return type — \
                 should have been routed through transform_block_with_flags"
            );
        }
        create_block(package, assigner, remaining_stmts, return_ty)
    };

    // Recursively transform the new else block (it may contain more returns).
    transform_block_if_else(package, assigner, new_else_block_id, return_ty);

    // Create new else expression wrapping the block.
    let new_else_expr_id = create_block_expr(package, assigner, new_else_block_id, return_ty);

    // Create the new if expression.
    let new_if_expr_id = create_if_expr(
        package,
        assigner,
        cond,
        then_expr,
        Some(new_else_expr_id),
        return_ty,
    );
    let new_tail = create_expr_stmt(package, assigner, new_if_expr_id);

    let block = package.blocks.get_mut(block_id).expect("block not found");
    block.stmts.truncate(idx);
    block.stmts.push(new_tail);
    block.ty = return_ty.clone();
}

/// Rewrite an `if cond { return a; } else { return b; }` into a return-free
/// value-producing tail. If release statements follow the if, capture the
/// selected value before the releases and leave the captured value as the
/// block's trailing expression.
///
/// ```text
/// // Before
/// {
///     stmts_before;
///     if cond { return a; } else { return b; }
///     release_call();
///     stmts_dead;
/// }
///
/// // After
/// {
///     stmts_before;
///     let __return_unify_result = if cond { a } else { b };
///     release_call();
///     __return_unify_result
/// }
/// ```
#[allow(clippy::too_many_arguments)]
fn apply_if_both_return(
    package: &mut Package,
    assigner: &mut Assigner,
    block_id: BlockId,
    idx: usize,
    cond: ExprId,
    then_expr: ExprId,
    else_expr: ExprId,
    return_ty: &Ty,
) {
    strip_returns_from_expr(package, assigner, then_expr, return_ty);
    strip_returns_from_expr(package, assigner, else_expr, return_ty);

    let new_if_expr_id = create_if_expr(
        package,
        assigner,
        cond,
        then_expr,
        Some(else_expr),
        return_ty,
    );

    // Both branches contain returns, so any statements after the if are dead.
    let new_tail = create_expr_stmt(package, assigner, new_if_expr_id);

    let block = package.blocks.get_mut(block_id).expect("block not found");
    block.stmts.truncate(idx);
    block.stmts.push(new_tail);
    block.ty = return_ty.clone();
}

/// Strip `ExprKind::Return` nodes from an expression tree in place.
///
/// Lifts returned values to take the place of the `Return` wrapper, and
/// retypes enclosing `Block` and `If` expressions so the lifted value's
/// type propagates outward (`()` → `return_ty`).
///
/// # Before
/// ```text
/// return v              // ExprKind::Return(v)  : ()
/// { stmts; return v; }  // ExprKind::Block      : ()
/// ```
/// # After
/// ```text
/// v                     // v.kind               : T
/// { stmts; v }          // ExprKind::Block      : T
/// ```
/// # Requires
/// - `expr_id` is valid in `package`.
/// - `return_ty` is the enclosing callable's return type.
///
/// # Ensures
/// - Every `ExprKind::Return` reachable through `Block`/`If`/compound
///   descent is replaced with the inner value.
/// - `Block` and `If` expression types are refreshed to propagate the
///   lifted value's type.
///
/// # Mutations
/// - Rewrites `Expr` nodes in place via `package.exprs.get_mut`.
/// - Rewrites nested `Block` contents via [`strip_returns_from_block`].
/// - Allocates new FIR nodes through `assigner` where required.
#[allow(clippy::too_many_lines)]
fn strip_returns_from_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    expr_id: ExprId,
    return_ty: &Ty,
) {
    let expr = package.get_expr(expr_id).clone();
    match &expr.kind {
        ExprKind::Return(inner) => {
            let inner_expr = package.get_expr(*inner).clone();
            let e = package.exprs.get_mut(expr_id).expect("expr not found");
            *e = Expr {
                id: expr_id,
                span: expr.span,
                ty: inner_expr.ty.clone(),
                kind: inner_expr.kind.clone(),
                exec_graph_range: EMPTY_EXEC_RANGE,
            };
            // Recursively strip in case the inner also has returns.
            strip_returns_from_expr(package, assigner, expr_id, return_ty);
        }
        ExprKind::Block(block_id) => {
            let bid = *block_id;
            strip_returns_from_block(package, assigner, bid, return_ty);
            // Update the Block expression's type to match the block's new type.
            let new_block_ty = package.get_block(bid).ty.clone();
            let e = package.exprs.get_mut(expr_id).expect("expr not found");
            e.ty = new_block_ty;
        }
        ExprKind::If(_, then_expr, else_opt) => {
            let then_id = *then_expr;
            let else_id = *else_opt;
            strip_returns_from_expr(package, assigner, then_id, return_ty);
            if let Some(e) = else_id {
                strip_returns_from_expr(package, assigner, e, return_ty);
            }
            // Update the If expression's type to match the return type.
            let e = package.exprs.get_mut(expr_id).expect("expr not found");
            e.ty = return_ty.clone();
        }
        // Compound-expression descent. Sub-expressions are visited so any
        // `Return` nested through these kinds after normalization is still
        // stripped defensively. Types of these kinds are not refreshed because
        // valid normalized FIR should not leave return-bearing values here.
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            let ids: Vec<ExprId> = exprs.clone();
            for e in ids {
                strip_returns_from_expr(package, assigner, e, return_ty);
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
            strip_returns_from_expr(package, assigner, a_id, return_ty);
            strip_returns_from_expr(package, assigner, b_id, return_ty);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            let (a_id, b_id, c_id) = (*a, *b, *c);
            strip_returns_from_expr(package, assigner, a_id, return_ty);
            strip_returns_from_expr(package, assigner, b_id, return_ty);
            strip_returns_from_expr(package, assigner, c_id, return_ty);
        }
        ExprKind::Fail(e) | ExprKind::Field(e, _) | ExprKind::UnOp(_, e) => {
            let sub = *e;
            strip_returns_from_expr(package, assigner, sub, return_ty);
        }
        ExprKind::Range(start, step, end) => {
            let ids: Vec<ExprId> = [start, step, end].into_iter().flatten().copied().collect();
            for e in ids {
                strip_returns_from_expr(package, assigner, e, return_ty);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            let copy_id = *copy;
            let field_ids: Vec<ExprId> = fields.iter().map(|fa| fa.value).collect();
            if let Some(c) = copy_id {
                strip_returns_from_expr(package, assigner, c, return_ty);
            }
            for e in field_ids {
                strip_returns_from_expr(package, assigner, e, return_ty);
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
                strip_returns_from_expr(package, assigner, e, return_ty);
            }
        }
        ExprKind::While(cond, body) => {
            let (cond_id, body_id) = (*cond, *body);
            strip_returns_from_expr(package, assigner, cond_id, return_ty);
            // Walk every statement-level expression inside the while body.
            let stmts = package.get_block(body_id).stmts.clone();
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
                    strip_returns_from_expr(package, assigner, e, return_ty);
                }
            }
        }
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
    }
}

/// Strip returns from a block by transforming it with if-else lifting,
/// using the function's return type rather than the block's own type.
fn strip_returns_from_block(
    package: &mut Package,
    assigner: &mut Assigner,
    block_id: BlockId,
    return_ty: &Ty,
) {
    transform_block_if_else(package, assigner, block_id, return_ty);
}

/// Retype every `Var(Local(var_id))` expression reachable from a statement
/// to `new_ty`.
///
/// Used after [`transform_local_init`] strips returns from a `let`
/// initializer, to keep reads of the bound local type-consistent with the
/// newly-lifted init type.
///
/// ```text
/// // Before (init type lifted from () to T after strip_returns_from_expr)
/// let x : () = { ... };  // x reads typed ()
///
/// // After
/// let x : T = { ... };   // every Var(x) retyped to T
/// ```
fn update_local_var_type(package: &mut Package, stmt_id: StmtId, var_id: LocalVarId, new_ty: &Ty) {
    let expr_ids: Vec<ExprId> = {
        let stmt = package.get_stmt(stmt_id);
        match &stmt.kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => vec![*e],
            StmtKind::Item(_) => vec![],
        }
    };
    for expr_id in expr_ids {
        update_local_var_type_in_expr(package, expr_id, var_id, new_ty);
    }
}

/// Recursively retype every `Var(Local(var_id))` read inside an expression tree.
///
/// # Before
/// ```text
/// Var(Local(var_id)) : OldTy   // anywhere in the subtree
/// ```
/// # After
/// ```text
/// Var(Local(var_id)) : NewTy
/// ```
/// # Requires
/// - `expr_id` is valid in `package`.
/// - `var_id` is the binding whose referencing `Var`s must be retyped.
///
/// # Ensures
/// - Every `Var(Local(var_id))` reachable through `Block`/`If`/compound
///   descent has its `Expr.ty` set to `new_ty`.
/// - Does not touch `Var`s resolving to other locals or non-local `Res`.
///
/// # Mutations
/// - Writes `Expr.ty` in place for each matching `Var` node.
fn update_local_var_type_in_expr(
    package: &mut Package,
    expr_id: ExprId,
    var_id: LocalVarId,
    new_ty: &Ty,
) {
    let kind = package.get_expr(expr_id).kind.clone();
    match &kind {
        ExprKind::Var(Res::Local(id), _) if *id == var_id => {
            let e = package.exprs.get_mut(expr_id).expect("expr not found");
            e.ty = new_ty.clone();
        }
        ExprKind::Block(block_id) => {
            let stmts = package.get_block(*block_id).stmts.clone();
            for stmt_id in stmts {
                update_local_var_type(package, stmt_id, var_id, new_ty);
            }
        }
        ExprKind::If(_, then_id, else_opt) => {
            update_local_var_type_in_expr(package, *then_id, var_id, new_ty);
            if let Some(e) = *else_opt {
                update_local_var_type_in_expr(package, e, var_id, new_ty);
            }
        }
        // Exhaustive descent through every compound `ExprKind`. Closes G3:
        // a retype request must reach every `Var(Local(var_id))` read no
        // matter how deeply nested it is, not only those inside Block/If.
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            let ids: Vec<ExprId> = exprs.clone();
            for e in ids {
                update_local_var_type_in_expr(package, e, var_id, new_ty);
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
            update_local_var_type_in_expr(package, a_id, var_id, new_ty);
            update_local_var_type_in_expr(package, b_id, var_id, new_ty);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            let (a_id, b_id, c_id) = (*a, *b, *c);
            update_local_var_type_in_expr(package, a_id, var_id, new_ty);
            update_local_var_type_in_expr(package, b_id, var_id, new_ty);
            update_local_var_type_in_expr(package, c_id, var_id, new_ty);
        }
        ExprKind::Fail(e) | ExprKind::Field(e, _) | ExprKind::Return(e) | ExprKind::UnOp(_, e) => {
            let sub = *e;
            update_local_var_type_in_expr(package, sub, var_id, new_ty);
        }
        ExprKind::Range(start, step, end) => {
            let ids: Vec<ExprId> = [start, step, end].into_iter().flatten().copied().collect();
            for e in ids {
                update_local_var_type_in_expr(package, e, var_id, new_ty);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            let copy_id = *copy;
            let field_ids: Vec<ExprId> = fields.iter().map(|fa| fa.value).collect();
            if let Some(c) = copy_id {
                update_local_var_type_in_expr(package, c, var_id, new_ty);
            }
            for e in field_ids {
                update_local_var_type_in_expr(package, e, var_id, new_ty);
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
                update_local_var_type_in_expr(package, e, var_id, new_ty);
            }
        }
        ExprKind::While(cond, body) => {
            let (cond_id, body_id) = (*cond, *body);
            update_local_var_type_in_expr(package, cond_id, var_id, new_ty);
            let stmts = package.get_block(body_id).stmts.clone();
            for stmt_id in stmts {
                update_local_var_type(package, stmt_id, var_id, new_ty);
            }
        }
        ExprKind::Var(_, _) | ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) => {}
    }
}

/// Rewrite a block containing returns inside while loops using the flag-based strategy.
///
/// Introduces two mutable locals at the top of the block:
/// * `__has_returned : Bool = false` — set when a return fires.
/// * `__ret_val : T = default(T)` — holds the returned value (never read
///   unless `__has_returned` is `true`).
///
/// Each while loop containing a return has its condition conjoined with
/// `not __has_returned` (see [`transform_while_stmt`]), and returns inside
/// its body are rewritten by [`replace_returns_with_flags`]. Statements
/// after the first return-bearing while are wrapped by
/// [`guard_stmt_with_flag`], including release calls. A trailing
/// `if __has_returned { __ret_val }
/// else { original_trailing }` is appended by [`create_flag_trailing_expr`].
/// A final call to [`transform_block_if_else`] mops up any non-while
/// returns that remain.
///
/// # Before
/// ```text
/// {
///     mutable r = 0;
///     while cond {
///         if done { return r; }
///         r += 1;
///     }
///     r
/// }
/// ```
/// # After
/// ```text
/// {
///     mutable __has_returned = false;
///     mutable __ret_val = 0;
///     mutable r = 0;
///     while not __has_returned and cond {
///         if done { __ret_val = r; __has_returned = true; }
///         else   { r += 1; }
///     }
///     if __has_returned { __ret_val } else { r }
/// }
/// ```
/// # Requires
/// - `block_id` is valid in `package`.
/// - `return_ty` has a synthesizable classical default (see
///   [`create_default_value_kind`]); otherwise this triggers the
///   unsupported-default internal contract panic.
///
/// # Ensures
/// - While loops exit promptly once `__has_returned` is set.
/// - Post-return continuation statements execute only when no return has fired.
/// - The block's trailing expression produces the return value.
///
/// # Mutations
/// - Prepends `__has_returned` / `__ret_val` `Local` statements to `block.stmts`.
/// - Rewrites statements carrying returns (while loops, guarded reads, trailing expr).
/// - Allocates new FIR nodes through `assigner`.
#[allow(clippy::too_many_lines)]
fn transform_block_with_flags(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    block_id: BlockId,
    return_ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
) {
    // Create __has_returned: Bool = false
    let (has_returned_var_id, has_returned_decl_stmt) =
        create_mutable_bool_var(package, assigner, "__has_returned", false);

    // Create __ret_val: T = default(T).
    //
    // For callable-valued return types, `create_default_value` synthesizes
    // a nop callable item of the matching signature and returns a
    // `Var(Res::Item(..))` reference to it; any later `Call(Var(__ret_val), .)`
    // then resolves to that nop (its body returns the output type's default).
    // The nop is never actually invoked because `__has_returned` guards
    // every read of `__ret_val`, but it keeps the flag-fallback well-typed.
    let default_val = require_classical_default(
        package,
        assigner,
        package_id,
        return_ty,
        udt_pure_tys,
        UnsupportedDefaultSite::ReturnSlot,
    );
    let (ret_val_var_id, ret_val_decl_stmt) =
        create_mutable_var(package, assigner, "__ret_val", return_ty, default_val);

    let original_stmts = package.get_block(block_id).stmts.clone();
    let mut new_stmts: Vec<StmtId> = Vec::new();

    // Insert flag declarations.
    new_stmts.push(has_returned_decl_stmt);
    new_stmts.push(ret_val_decl_stmt);

    let mut seen_return_bearing_stmt = false;

    for (index, &stmt_id) in original_stmts.iter().enumerate() {
        let has_return_in_while = match &package.get_stmt(stmt_id).kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) => contains_return_in_while_expr(package, *e),
            _ => false,
        };
        let has_return = contains_return_in_stmt(package, stmt_id);
        let is_final_trailing_expr = index == original_stmts.len() - 1
            && matches!(package.get_stmt(stmt_id).kind, StmtKind::Expr(_));

        if has_return_in_while {
            // Transform the while loop (conjoins `not __has_returned` onto
            // the condition and rewrites Returns in its body via the flag
            // slot).
            transform_while_stmt(
                package,
                assigner,
                package_id,
                stmt_id,
                has_returned_var_id,
                ret_val_var_id,
                udt_pure_tys,
            );
            new_stmts.push(stmt_id);
            seen_return_bearing_stmt = true;
        } else if has_return && !seen_return_bearing_stmt {
            // First return-bearing non-while statement. The flag is known
            // to be `false` on entry so no guard is needed here; rewriting
            // the returns in place to flag assignments is sufficient.
            replace_returns_with_flags(
                package,
                assigner,
                package_id,
                stmt_id,
                has_returned_var_id,
                ret_val_var_id,
                udt_pure_tys,
            );
            new_stmts.push(stmt_id);
            seen_return_bearing_stmt = true;
        } else if has_return {
            // Subsequent return-bearing statement after another
            // return-bearing statement has already fired. Rewrite returns,
            // then guard the whole statement so it is skipped when the
            // earlier return already set `__has_returned`.
            replace_returns_with_flags(
                package,
                assigner,
                package_id,
                stmt_id,
                has_returned_var_id,
                ret_val_var_id,
                udt_pure_tys,
            );
            let guarded = guard_stmt_with_flag(
                package,
                assigner,
                package_id,
                stmt_id,
                has_returned_var_id,
                udt_pure_tys,
            );
            new_stmts.push(guarded);
        } else if seen_return_bearing_stmt && is_final_trailing_expr {
            // Preserve the original trailing value so the final flag check
            // can return it from the else branch instead of discarding it
            // as a guarded semicolon statement.
            new_stmts.push(stmt_id);
        } else if seen_return_bearing_stmt {
            // Guard continuation statements that follow a return-bearing
            // statement so they are skipped once the flag is set. Release
            // calls are ordinary side effects here; no-hoist raw wrappers
            // keep path-local releases on the returning paths.
            let guarded = guard_stmt_with_flag(
                package,
                assigner,
                package_id,
                stmt_id,
                has_returned_var_id,
                udt_pure_tys,
            );
            new_stmts.push(guarded);
        } else {
            new_stmts.push(stmt_id);
        }
    }

    // Create trailing expression: if __has_returned { __ret_val } else { <original_trailing> }
    let trailing = create_flag_trailing_expr(
        package,
        assigner,
        &mut new_stmts,
        has_returned_var_id,
        ret_val_var_id,
        return_ty,
    );

    if let Some(trailing_stmt) = trailing {
        new_stmts.push(trailing_stmt);
    }

    let block = package.blocks.get_mut(block_id).expect("block not found");
    block.stmts = new_stmts;
    block.ty = return_ty.clone();

    // Apply if-else lifting to handle any remaining non-while returns.
    transform_block_if_else(package, assigner, block_id, return_ty);
}

/// Post-transform simplification pass that folds trivial flag patterns.
///
/// After the flag-based transform, the output can contain redundant
/// if-expressions whose branches are structurally identical:
///
/// ```text
/// if __has_returned { x } else { x }   →   x
/// ```
///
/// This pass walks the block's statements and trailing expression, folding
/// such identity patterns. Only clearly safe, semantics-preserving folds
/// are applied. This is the structured-IR analog of LLVM's `SimplifyCFG`
/// running after `mergereturn`.
fn simplify_flag_patterns(package: &mut Package, block_id: BlockId) {
    let stmts = package.get_block(block_id).stmts.clone();
    for &stmt_id in &stmts {
        simplify_flag_patterns_in_stmt(package, stmt_id);
    }
}

/// Simplify flag patterns within a single statement.
fn simplify_flag_patterns_in_stmt(package: &mut Package, stmt_id: StmtId) {
    let expr_id = match &package.get_stmt(stmt_id).kind {
        StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => *e,
        StmtKind::Item(_) => return,
    };
    if let Some(replacement) = try_fold_identical_branches(package, expr_id) {
        let stmt = package.stmts.get_mut(stmt_id).expect("stmt not found");
        match &mut stmt.kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => {
                *e = replacement;
            }
            StmtKind::Item(_) => {}
        }
    }
}

/// If `expr_id` is an `If(cond, then_expr, Some(else_expr))` where the
/// then and else branches are structurally identical, return the branch
/// expression id to replace the if with. Returns `None` otherwise.
fn try_fold_identical_branches(package: &Package, expr_id: ExprId) -> Option<ExprId> {
    let expr = package.get_expr(expr_id);
    let ExprKind::If(_, then_id, Some(else_id)) = &expr.kind else {
        return None;
    };
    if exprs_structurally_equal(package, *then_id, *else_id) {
        Some(*then_id)
    } else {
        None
    }
}

/// Recursively compare two expression trees for structural equality.
///
/// Two expressions are structurally equal when their `ExprKind` variants
/// match and all recursive children are structurally equal. Span and
/// exec-graph metadata are ignored; only the semantic shape matters.
///
/// This is intentionally conservative: any unknown or complex pattern
/// returns `false` to avoid incorrect folding.
fn exprs_structurally_equal(package: &Package, a: ExprId, b: ExprId) -> bool {
    if a == b {
        return true;
    }
    let ea = package.get_expr(a);
    let eb = package.get_expr(b);
    if ea.ty != eb.ty {
        return false;
    }
    match (&ea.kind, &eb.kind) {
        (ExprKind::Var(res_a, args_a), ExprKind::Var(res_b, args_b)) => {
            res_a == res_b && args_a == args_b
        }
        (ExprKind::Lit(lit_a), ExprKind::Lit(lit_b)) => lit_a == lit_b,
        (ExprKind::Tuple(elems_a), ExprKind::Tuple(elems_b)) => {
            elems_a.len() == elems_b.len()
                && elems_a
                    .iter()
                    .zip(elems_b.iter())
                    .all(|(&a, &b)| exprs_structurally_equal(package, a, b))
        }
        (ExprKind::Block(bid_a), ExprKind::Block(bid_b)) => {
            blocks_structurally_equal(package, *bid_a, *bid_b)
        }
        (ExprKind::UnOp(op_a, operand_a), ExprKind::UnOp(op_b, operand_b)) => {
            op_a == op_b && exprs_structurally_equal(package, *operand_a, *operand_b)
        }
        (ExprKind::BinOp(op_a, l_a, r_a), ExprKind::BinOp(op_b, l_b, r_b)) => {
            op_a == op_b
                && exprs_structurally_equal(package, *l_a, *l_b)
                && exprs_structurally_equal(package, *r_a, *r_b)
        }
        (ExprKind::If(c_a, t_a, e_a), ExprKind::If(c_b, t_b, e_b)) => {
            exprs_structurally_equal(package, *c_a, *c_b)
                && exprs_structurally_equal(package, *t_a, *t_b)
                && match (e_a, e_b) {
                    (Some(ea), Some(eb)) => exprs_structurally_equal(package, *ea, *eb),
                    (None, None) => true,
                    _ => false,
                }
        }
        (ExprKind::Array(a_elems), ExprKind::Array(b_elems))
        | (ExprKind::ArrayLit(a_elems), ExprKind::ArrayLit(b_elems)) => {
            a_elems.len() == b_elems.len()
                && a_elems
                    .iter()
                    .zip(b_elems.iter())
                    .all(|(&a, &b)| exprs_structurally_equal(package, a, b))
        }
        // Conservative: anything else is considered non-equal.
        _ => false,
    }
}

/// Recursively compare two blocks for structural equality.
fn blocks_structurally_equal(package: &Package, a: BlockId, b: BlockId) -> bool {
    if a == b {
        return true;
    }
    let ba = package.get_block(a);
    let bb = package.get_block(b);
    if ba.ty != bb.ty || ba.stmts.len() != bb.stmts.len() {
        return false;
    }
    ba.stmts
        .iter()
        .zip(bb.stmts.iter())
        .all(|(&sa, &sb)| stmts_structurally_equal(package, sa, sb))
}

/// Recursively compare two statements for structural equality.
fn stmts_structurally_equal(package: &Package, a: StmtId, b: StmtId) -> bool {
    if a == b {
        return true;
    }
    let sa = package.get_stmt(a);
    let sb = package.get_stmt(b);
    match (&sa.kind, &sb.kind) {
        (StmtKind::Expr(ea), StmtKind::Expr(eb)) | (StmtKind::Semi(ea), StmtKind::Semi(eb)) => {
            exprs_structurally_equal(package, *ea, *eb)
        }
        (StmtKind::Local(m_a, p_a, e_a), StmtKind::Local(m_b, p_b, e_b)) => {
            m_a == m_b && p_a == p_b && exprs_structurally_equal(package, *e_a, *e_b)
        }
        _ => false,
    }
}

/// Rewrite a while-loop statement under the flag-based transform.
///
/// Delegates to [`transform_while_in_expr`] on the statement's inner
/// expression; descends through `Block` and `If` wrappers so for-loop
/// desugarings (which wrap the while in a block) are handled.
///
/// ```text
/// // Before
/// while cond { body }
///
/// // After
/// while not __has_returned and cond { body' }
/// // where body' has all `return v` replaced by
/// //   { __ret_val = v; __has_returned = true; }
/// ```
#[allow(clippy::too_many_arguments)]
fn transform_while_stmt(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    stmt_id: StmtId,
    has_returned_var_id: LocalVarId,
    ret_val_var_id: LocalVarId,
    udt_pure_tys: &UdtPureTyCache,
) {
    let expr_id = match &package.get_stmt(stmt_id).kind {
        StmtKind::Expr(e) | StmtKind::Semi(e) => *e,
        _ => return,
    };

    transform_while_in_expr(
        package,
        assigner,
        package_id,
        expr_id,
        has_returned_var_id,
        ret_val_var_id,
        udt_pure_tys,
    );
}

/// Walk an expression tree, locate every `ExprKind::While` that transitively
/// contains a return, and rewrite it for the flag-based transform.
///
/// For each such `While`:
/// * Conjoins `not __has_returned` onto the loop condition.
/// * Calls [`replace_returns_in_block`] to rewrite `return v` inside the
///   body as flag-assignment blocks.
///
/// Descends into `Block` and `If` wrappers so nested structures (including
/// for-loop desugarings) are handled.
///
/// ```text
/// // Before
/// while cond { ...; if guard { return v; }; ... }
///
/// // After
/// while not __has_returned and cond {
///     ...;
///     if guard {
///         __ret_val = v;
///         __has_returned = true;
///     };
///     ...
/// }
/// ```
#[allow(clippy::too_many_arguments)]
fn transform_while_in_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    expr_id: ExprId,
    has_returned_var_id: LocalVarId,
    ret_val_var_id: LocalVarId,
    udt_pure_tys: &UdtPureTyCache,
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
                    package_id,
                    cond_id,
                    has_returned_var_id,
                    ret_val_var_id,
                    udt_pure_tys,
                );
            }

            // Conjoin !__has_returned with the while condition.
            // LHS must be the flag guard so that AndL short-circuits and
            // skips the original condition once a return has fired.
            let not_flag = create_not_var_expr(package, assigner, has_returned_var_id);
            let new_cond = create_bin_op_expr(
                package,
                assigner,
                BinOp::AndL,
                not_flag,
                cond_id,
                &Ty::Prim(Prim::Bool),
            );

            // Replace returns inside the body.
            if contains_return_in_block(package, body_block_id) {
                replace_returns_in_block(
                    package,
                    assigner,
                    package_id,
                    body_block_id,
                    has_returned_var_id,
                    ret_val_var_id,
                    udt_pure_tys,
                );
            }

            // Update the while expression.
            let e = package.exprs.get_mut(expr_id).expect("expr not found");
            *e = Expr {
                id: expr_id,
                span: expr.span,
                ty: expr.ty.clone(),
                kind: ExprKind::While(new_cond, body_block_id),
                exec_graph_range: EMPTY_EXEC_RANGE,
            };
        }
        ExprKind::Block(block_id) => {
            let stmts = package.get_block(*block_id).stmts.clone();
            for &stmt_id in &stmts {
                let inner_expr_id = match &package.get_stmt(stmt_id).kind {
                    StmtKind::Expr(e) | StmtKind::Semi(e) => *e,
                    _ => continue,
                };
                if contains_return_in_while_expr(package, inner_expr_id) {
                    transform_while_in_expr(
                        package,
                        assigner,
                        package_id,
                        inner_expr_id,
                        has_returned_var_id,
                        ret_val_var_id,
                        udt_pure_tys,
                    );
                }
            }
        }
        ExprKind::If(_, then_id, else_opt) => {
            if contains_return_in_while_expr(package, *then_id) {
                transform_while_in_expr(
                    package,
                    assigner,
                    package_id,
                    *then_id,
                    has_returned_var_id,
                    ret_val_var_id,
                    udt_pure_tys,
                );
            }
            if let Some(e) = *else_opt
                && contains_return_in_while_expr(package, e)
            {
                transform_while_in_expr(
                    package,
                    assigner,
                    package_id,
                    e,
                    has_returned_var_id,
                    ret_val_var_id,
                    udt_pure_tys,
                );
            }
        }
        _ => {}
    }
}

fn create_fallthrough_continuation_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    else_opt: Option<ExprId>,
    continuation_stmts: Vec<StmtId>,
    return_ty: &Ty,
) -> ExprId {
    if let Some(else_expr_id) = else_opt {
        strip_returns_from_expr(package, assigner, else_expr_id, return_ty);
        if continuation_stmts.is_empty() {
            return else_expr_id;
        }

        let else_semi = create_semi_stmt(package, assigner, else_expr_id);
        let mut new_stmts = Vec::with_capacity(continuation_stmts.len() + 1);
        new_stmts.push(else_semi);
        new_stmts.extend(continuation_stmts);
        let block_id = create_block(package, assigner, new_stmts, return_ty);
        transform_block_if_else(package, assigner, block_id, return_ty);
        return create_block_expr(package, assigner, block_id, return_ty);
    }

    if continuation_stmts.is_empty() {
        assert!(
            *return_ty == Ty::UNIT,
            "fallthrough continuation is empty for non-Unit return type"
        );
    }

    let block_id = create_block(package, assigner, continuation_stmts, return_ty);
    transform_block_if_else(package, assigner, block_id, return_ty);
    create_block_expr(package, assigner, block_id, return_ty)
}

/// Walk every statement in a block and rewrite `Return(val)` subexpressions
/// into `{ __ret_val = val; __has_returned = true; }` via
/// [`replace_returns_with_flags`].
///
/// After replacement, statements following the first return-bearing
/// statement in the same block are wrapped in `if not __has_returned { … }`
/// guards so they are skipped once the flag fires within the current
/// iteration or scope.
///
/// ```text
/// // Before
/// { if g { return v; }; stmt2 }
///
/// // After
/// { if g { { __ret_val = v; __has_returned = true; } };
///   if not __has_returned { stmt2 }; }
/// ```
#[allow(clippy::too_many_arguments)]
fn replace_returns_in_block(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    block_id: BlockId,
    has_returned_var_id: LocalVarId,
    ret_val_var_id: LocalVarId,
    udt_pure_tys: &UdtPureTyCache,
) {
    let stmts = package.get_block(block_id).stmts.clone();

    // Identify the first statement carrying a return *before* any
    // replacement so the index is stable.
    let first_return_idx = stmts
        .iter()
        .position(|&sid| contains_return_in_stmt(package, sid));

    // Replace returns in every statement.
    for &stmt_id in &stmts {
        replace_returns_with_flags(
            package,
            assigner,
            package_id,
            stmt_id,
            has_returned_var_id,
            ret_val_var_id,
            udt_pure_tys,
        );
    }

    // Guard subsequent statements so they are skipped once the flag is set.
    if let Some(first_idx) = first_return_idx
        && first_idx + 1 < stmts.len()
    {
        let last_idx = stmts.len() - 1;
        let is_last_trailing_expr =
            matches!(package.get_stmt(stmts[last_idx]).kind, StmtKind::Expr(_));

        let mut new_stmts: Vec<StmtId> = stmts[..=first_idx].to_vec();
        for (i, &stmt_id) in stmts[first_idx + 1..].iter().enumerate() {
            let actual_idx = first_idx + 1 + i;
            // Preserve the trailing expression without guarding — its
            // value is only consumed when `__has_returned` is false
            // (the flag-trailing expression handles the true case).
            if actual_idx == last_idx && is_last_trailing_expr {
                new_stmts.push(stmt_id);
            } else {
                let guarded = guard_stmt_with_flag(
                    package,
                    assigner,
                    package_id,
                    stmt_id,
                    has_returned_var_id,
                    udt_pure_tys,
                );
                new_stmts.push(guarded);
            }
        }
        let block = package.blocks.get_mut(block_id).expect("block not found");
        block.stmts = new_stmts;
    }
}

/// Rewrite `Return(val)` subexpressions in a single statement's expression
/// tree to the flag-assignment pair.
///
/// ```text
/// // Before
/// Expr(if cond { return v; })
///
/// // After
/// Expr(if cond { { __ret_val = v; __has_returned = true; } })
/// ```
#[allow(clippy::too_many_arguments)]
fn replace_returns_with_flags(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    stmt_id: StmtId,
    has_returned_var_id: LocalVarId,
    ret_val_var_id: LocalVarId,
    udt_pure_tys: &UdtPureTyCache,
) {
    let expr_id = match &package.get_stmt(stmt_id).kind {
        StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => *e,
        StmtKind::Item(_) => return,
    };
    replace_returns_in_expr(
        package,
        assigner,
        package_id,
        expr_id,
        has_returned_var_id,
        ret_val_var_id,
        udt_pure_tys,
    );

    // Sync Pat type for Local bindings whose initializer type may have
    // changed after return replacement (e.g. a Block wrapping an If whose
    // else branch was replaced with a Unit-typed flag-assignment block).
    if let StmtKind::Local(_, pat_id, init_id) = &package.get_stmt(stmt_id).kind {
        let pat_id = *pat_id;
        let init_id = *init_id;
        let init_ty = package.get_expr(init_id).ty.clone();
        let pat = package.pats.get_mut(pat_id).expect("pat not found");
        pat.ty = init_ty;
    }
}

/// Rewrite `Return(val)` nodes inside an expression tree to Unit-typed flag-assignment blocks.
///
/// Each `Return(val)` expression is replaced in place with:
///
/// ```text
/// { __ret_val = val; __has_returned = true; } : ()
/// ```
///
/// # Before
/// ```text
/// if cond { return v; }
/// ```
/// # After
/// ```text
/// if cond { { __ret_val = v; __has_returned = true; } }
/// ```
/// # Requires
/// - `expr_id` is valid in `package`.
/// - `has_returned_var_id` and `ret_val_var_id` reference the flag pair
///   introduced by [`transform_block_with_flags`].
///
/// # Ensures
/// - Every `ExprKind::Return` reachable through `Block`/`If`/compound
///   descent is replaced with the flag-assignment block.
/// - The outer expression's type becomes Unit at each replacement site;
///   callers guarantee the enclosing loop exits on the next condition check.
///
/// # Mutations
/// - Rewrites `Expr` nodes in place at each Return replacement site.
/// - Recurses into nested blocks via [`replace_returns_in_block`].
/// - Allocates new FIR nodes through `assigner`.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)]
fn replace_returns_in_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    expr_id: ExprId,
    has_returned_var_id: LocalVarId,
    ret_val_var_id: LocalVarId,
    udt_pure_tys: &UdtPureTyCache,
) {
    let expr = package.get_expr(expr_id).clone();
    match &expr.kind {
        ExprKind::Return(inner) => {
            let inner_id = *inner;
            let inner_ty = package.get_expr(inner_id).ty.clone();
            // Build: { __ret_val = val; __has_returned = true; }
            let assign_val =
                create_assign_expr(package, assigner, ret_val_var_id, inner_id, &inner_ty);
            let assign_val_semi = create_semi_stmt(package, assigner, assign_val);

            let true_lit = create_bool_lit(package, assigner, true);
            let assign_flag = create_assign_expr(
                package,
                assigner,
                has_returned_var_id,
                true_lit,
                &Ty::Prim(Prim::Bool),
            );
            let assign_flag_semi = create_semi_stmt(package, assigner, assign_flag);

            let flag_block = create_block(
                package,
                assigner,
                vec![assign_val_semi, assign_flag_semi],
                &Ty::UNIT,
            );
            let flag_block_expr = create_block_expr(package, assigner, flag_block, &Ty::UNIT);

            // Replace the Return expression in-place with the block expression.
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
            replace_returns_in_block(
                package,
                assigner,
                package_id,
                bid,
                has_returned_var_id,
                ret_val_var_id,
                udt_pure_tys,
            );
            // Nested blocks that previously contained a trailing `Return`
            // expression may have been typed to the callable return type.
            // After replacement the Return is a Unit block, so sync the
            // block's type to its trailing expression (Unit when the block
            // has no trailing Expr stmt). Also refresh the enclosing
            // expression's type since `Block` expressions carry the
            // block's type on the `Expr` node.
            sync_block_type_to_stmt_or_unit(package, bid);
            let new_block_ty = package.get_block(bid).ty.clone();
            let e = package.exprs.get_mut(expr_id).expect("expr not found");
            e.ty = new_block_ty;
        }
        ExprKind::If(_, then_id, else_opt) => {
            let then_id = *then_id;
            let else_id = *else_opt;
            replace_returns_in_expr(
                package,
                assigner,
                package_id,
                then_id,
                has_returned_var_id,
                ret_val_var_id,
                udt_pure_tys,
            );
            if let Some(e) = else_id {
                replace_returns_in_expr(
                    package,
                    assigner,
                    package_id,
                    e,
                    has_returned_var_id,
                    ret_val_var_id,
                    udt_pure_tys,
                );
            }
            // Update the If expression type to reflect branch type changes.
            // After return replacement, a branch containing Return is
            // replaced with a Unit-typed flag-assignment block. Derive the
            // If type from branch types: prefer the non-Unit branch type so
            // the surrounding Local binding keeps its original type.
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
        // Audit: Only `Block` and `If` arms above require
        // post-recursion type synchronization (their enclosing `Expr`
        // carries the inner expression's type, which shifts to `Unit`
        // when a trailing `Return` is replaced). All remaining arms
        // below are defensive — `Return` cannot legitimately nest in
        // these positions in valid normalized FIR. Recursive replacement here
        // keeps the pass robust by construction.
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            let ids: Vec<ExprId> = exprs.clone();
            for e in ids {
                replace_returns_in_expr(
                    package,
                    assigner,
                    package_id,
                    e,
                    has_returned_var_id,
                    ret_val_var_id,
                    udt_pure_tys,
                );
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
            replace_returns_in_expr(
                package,
                assigner,
                package_id,
                a_id,
                has_returned_var_id,
                ret_val_var_id,
                udt_pure_tys,
            );
            replace_returns_in_expr(
                package,
                assigner,
                package_id,
                b_id,
                has_returned_var_id,
                ret_val_var_id,
                udt_pure_tys,
            );
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            let (a_id, b_id, c_id) = (*a, *b, *c);
            replace_returns_in_expr(
                package,
                assigner,
                package_id,
                a_id,
                has_returned_var_id,
                ret_val_var_id,
                udt_pure_tys,
            );
            replace_returns_in_expr(
                package,
                assigner,
                package_id,
                b_id,
                has_returned_var_id,
                ret_val_var_id,
                udt_pure_tys,
            );
            replace_returns_in_expr(
                package,
                assigner,
                package_id,
                c_id,
                has_returned_var_id,
                ret_val_var_id,
                udt_pure_tys,
            );
        }
        ExprKind::Fail(e) | ExprKind::Field(e, _) | ExprKind::UnOp(_, e) => {
            let sub = *e;
            replace_returns_in_expr(
                package,
                assigner,
                package_id,
                sub,
                has_returned_var_id,
                ret_val_var_id,
                udt_pure_tys,
            );
        }
        ExprKind::Range(start, step, end) => {
            let ids: Vec<ExprId> = [start, step, end].into_iter().flatten().copied().collect();
            for e in ids {
                replace_returns_in_expr(
                    package,
                    assigner,
                    package_id,
                    e,
                    has_returned_var_id,
                    ret_val_var_id,
                    udt_pure_tys,
                );
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            let copy_id = *copy;
            let field_ids: Vec<ExprId> = fields.iter().map(|fa| fa.value).collect();
            if let Some(c) = copy_id {
                replace_returns_in_expr(
                    package,
                    assigner,
                    package_id,
                    c,
                    has_returned_var_id,
                    ret_val_var_id,
                    udt_pure_tys,
                );
            }
            for e in field_ids {
                replace_returns_in_expr(
                    package,
                    assigner,
                    package_id,
                    e,
                    has_returned_var_id,
                    ret_val_var_id,
                    udt_pure_tys,
                );
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
                replace_returns_in_expr(
                    package,
                    assigner,
                    package_id,
                    e,
                    has_returned_var_id,
                    ret_val_var_id,
                    udt_pure_tys,
                );
            }
        }
        ExprKind::While(cond, body) => {
            let (cond_id, body_id) = (*cond, *body);
            if contains_return_in_block(package, body_id)
                || contains_return_in_expr(package, cond_id)
            {
                // Delegate to `transform_while_in_expr` so the nested
                // while's condition gets conjoined with `not __has_returned`
                // and its body returns are rewritten, matching the
                // top-level while handling. Without this, a nested while
                // whose only exit is the return would loop forever after
                // the return-to-flag rewrite.
                transform_while_in_expr(
                    package,
                    assigner,
                    package_id,
                    expr_id,
                    has_returned_var_id,
                    ret_val_var_id,
                    udt_pure_tys,
                );
            } else {
                // No returns reachable through this while; structural
                // recursion into the condition is sufficient (the body is
                // return-free so walking it is a no-op).
                replace_returns_in_expr(
                    package,
                    assigner,
                    package_id,
                    cond_id,
                    has_returned_var_id,
                    ret_val_var_id,
                    udt_pure_tys,
                );
            }
        }
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
    }
}

/// Rewrites return-bearing while-condition subexpressions to preserve
/// Bool-typed condition semantics under the flag strategy.
///
/// A condition-side `Return(v)` becomes:
/// `{ __ret_val = v; __has_returned = true; false }`
/// so the loop condition evaluates to false immediately after capturing
/// the return value.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)]
fn replace_returns_in_condition_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    expr_id: ExprId,
    has_returned_var_id: LocalVarId,
    ret_val_var_id: LocalVarId,
    udt_pure_tys: &UdtPureTyCache,
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
                has_returned_var_id,
                ret_val_var_id,
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
                            package_id,
                            e,
                            has_returned_var_id,
                            ret_val_var_id,
                            udt_pure_tys,
                        );
                    } else {
                        replace_returns_in_expr(
                            package,
                            assigner,
                            package_id,
                            e,
                            has_returned_var_id,
                            ret_val_var_id,
                            udt_pure_tys,
                        );
                    }
                }
            }

            sync_block_type_to_stmt_or_unit(package, bid);
            let new_block_ty = package.get_block(bid).ty.clone();
            let e = package.exprs.get_mut(expr_id).expect("expr not found");
            e.ty = new_block_ty;
        }
        ExprKind::If(cond_id, then_id, else_opt) => {
            replace_returns_in_condition_expr(
                package,
                assigner,
                package_id,
                *cond_id,
                has_returned_var_id,
                ret_val_var_id,
                udt_pure_tys,
            );
            replace_returns_in_condition_expr(
                package,
                assigner,
                package_id,
                *then_id,
                has_returned_var_id,
                ret_val_var_id,
                udt_pure_tys,
            );
            if let Some(e) = else_opt {
                replace_returns_in_condition_expr(
                    package,
                    assigner,
                    package_id,
                    *e,
                    has_returned_var_id,
                    ret_val_var_id,
                    udt_pure_tys,
                );
            }
        }
        ExprKind::BinOp(BinOp::AndL | BinOp::OrL, lhs, rhs) => {
            replace_returns_in_condition_expr(
                package,
                assigner,
                package_id,
                *lhs,
                has_returned_var_id,
                ret_val_var_id,
                udt_pure_tys,
            );
            replace_returns_in_condition_expr(
                package,
                assigner,
                package_id,
                *rhs,
                has_returned_var_id,
                ret_val_var_id,
                udt_pure_tys,
            );
        }
        ExprKind::UnOp(UnOp::NotL, inner_id) => {
            replace_returns_in_condition_expr(
                package,
                assigner,
                package_id,
                *inner_id,
                has_returned_var_id,
                ret_val_var_id,
                udt_pure_tys,
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
    has_returned_var_id: LocalVarId,
    ret_val_var_id: LocalVarId,
) {
    let inner_ty = package.get_expr(inner_id).ty.clone();
    let assign_val = create_assign_expr(package, assigner, ret_val_var_id, inner_id, &inner_ty);
    let assign_val_semi = create_semi_stmt(package, assigner, assign_val);

    let true_lit = create_bool_lit(package, assigner, true);
    let assign_flag = create_assign_expr(
        package,
        assigner,
        has_returned_var_id,
        true_lit,
        &Ty::Prim(Prim::Bool),
    );
    let assign_flag_semi = create_semi_stmt(package, assigner, assign_flag);

    let false_lit = create_bool_lit(package, assigner, false);
    let false_stmt = create_expr_stmt(package, assigner, false_lit);

    let flag_block = create_block(
        package,
        assigner,
        vec![assign_val_semi, assign_flag_semi, false_stmt],
        &Ty::Prim(Prim::Bool),
    );
    let flag_block_expr = create_block_expr(package, assigner, flag_block, &Ty::Prim(Prim::Bool));

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

/// Wrap a statement so it is skipped when `__has_returned` is already set.
///
/// # Before
/// ```text
/// stmt;
/// ```
/// # After (Semi / Item / Unit-typed Expr statements)
/// ```text
/// if not __has_returned { stmt };
/// ```
/// # After (Local statements)
/// ```text
/// // `let x : T = init;` becomes:
/// let x : T = if not __has_returned { init } else { default(T) };
/// ```
/// For `Local` statements, wrapping the whole statement in an `if` block
/// would scope the binding away from subsequent statements that reference
/// it (a real bug if an earlier rewrite lifts a trailing value into a
/// `let @generated_ident_N = ...` that is then referenced from the
/// block's final `if __has_returned { __ret_val } else { @generated_ident_N }`
/// expression). Instead, the initializer is rewritten to a conditional
/// expression and the `Local` statement itself stays at the outer scope,
/// preserving the binding's visibility.
///
/// # Requires
/// - `stmt_id` is valid in `package`.
/// - `has_returned_var_id` is the flag introduced by
///   [`transform_block_with_flags`].
/// - For `Local` statements, the initializer's type has a classical
///   default reachable through [`create_default_value`].
///
/// # Ensures
/// - For non-`Local` statements, returns a new `Semi` statement whose
///   expression guards execution of `stmt_id` on `not __has_returned`.
/// - For `Local` statements, mutates the statement's initializer in place
///   and returns the original `stmt_id` unchanged.
/// - The original statement's effects execute only when no prior
///   flag-based return has fired.
///
/// # Mutations
/// - Allocates the guard block, `Var`/`Not` expressions, `If` expression,
///   and wrapping `Semi` statement through `assigner`.
/// - For `Local` statements, rewrites `package.stmts[stmt_id].kind` in
///   place to reference a new guarded-initializer `ExprId`.
fn guard_stmt_with_flag(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    stmt_id: StmtId,
    has_returned_var_id: LocalVarId,
    udt_pure_tys: &UdtPureTyCache,
) -> StmtId {
    // `Local` statements require special handling: wrapping the whole
    // statement in `if not __has_returned { let x = init; }` would hide
    // `x` from subsequent statements that reference it. Instead, rewrite
    // the initializer to `if not __has_returned { init } else { default }`
    // and leave the `Local` at the outer scope.
    if let StmtKind::Local(mutability, pat_id, init_expr_id) = package.get_stmt(stmt_id).kind {
        let init_ty = package.get_expr(init_expr_id).ty.clone();
        let default_val = require_classical_default(
            package,
            assigner,
            package_id,
            &init_ty,
            udt_pure_tys,
            UnsupportedDefaultSite::GuardedLocalInitializer,
        );

        let not_flag = create_not_var_expr(package, assigner, has_returned_var_id);

        let then_trailing = create_expr_stmt(package, assigner, init_expr_id);
        let then_block = create_block(package, assigner, vec![then_trailing], &init_ty);
        let then_expr = create_block_expr(package, assigner, then_block, &init_ty);

        let else_trailing = create_expr_stmt(package, assigner, default_val);
        let else_block = create_block(package, assigner, vec![else_trailing], &init_ty);
        let else_expr = create_block_expr(package, assigner, else_block, &init_ty);

        let if_expr = create_if_expr(
            package,
            assigner,
            not_flag,
            then_expr,
            Some(else_expr),
            &init_ty,
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
    let not_flag = create_not_var_expr(package, assigner, has_returned_var_id);
    let guard_block = create_block(package, assigner, vec![stmt_id], &Ty::UNIT);
    let guard_block_expr = create_block_expr(package, assigner, guard_block, &Ty::UNIT);
    let if_expr = create_if_expr(
        package,
        assigner,
        not_flag,
        guard_block_expr,
        None,
        &Ty::UNIT,
    );
    create_semi_stmt(package, assigner, if_expr)
}

/// Synthesize the trailing expression that finalizes the flag-based
/// transform, using `__has_returned` to select between the captured return
/// value and the block's original trailing value.
///
/// When the last statement in `stmts` is a trailing expression (`Expr`,
/// not `Semi`), it is popped and reused as the else branch. Otherwise the
/// else branch is `()` (the return type must be Unit in that case).
///
/// The trailing expression is first bound to a local variable
/// (`__trailing_result`) before the flag check, ensuring that any flag
/// assignments inside the trailing expression evaluate before the
/// `__has_returned` condition is tested. This prevents the temporal ordering
/// temporal ordering violation.
///
/// ```text
/// // stmts ends in: ...; original_trailing
/// // Result appended:
/// let __trailing_result : T = original_trailing;
/// if __has_returned { __ret_val } else { __trailing_result }
///
/// // stmts ends in: ...; side_effect;
/// // Result appended:
/// if __has_returned { __ret_val } else { () }
/// ```
fn create_flag_trailing_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    stmts: &mut Vec<StmtId>,
    has_returned_var_id: LocalVarId,
    ret_val_var_id: LocalVarId,
    return_ty: &Ty,
) -> Option<StmtId> {
    // Check if the last statement is a trailing expression (Expr, not Semi).
    let last_is_trailing = stmts
        .last()
        .is_some_and(|&sid| matches!(package.get_stmt(sid).kind, StmtKind::Expr(_)));

    let flag_var = create_var_expr(
        package,
        assigner,
        has_returned_var_id,
        &Ty::Prim(Prim::Bool),
    );
    let ret_var = create_var_expr(package, assigner, ret_val_var_id, return_ty);

    if last_is_trailing {
        // Pop the trailing expr and bind it to a local before the flag check.
        // This ensures that any flag assignments inside the trailing expression
        // evaluate before the `__has_returned` condition is tested.
        let last_stmt = stmts.pop().expect("stmts should not be empty");
        let original_trailing = match &package.get_stmt(last_stmt).kind {
            StmtKind::Expr(e) => *e,
            _ => unreachable!(),
        };

        // let __trailing_result : T = original_trailing;
        let (trailing_var_id, trailing_decl_stmt) = create_immutable_var(
            package,
            assigner,
            "__trailing_result",
            return_ty,
            original_trailing,
        );
        stmts.push(trailing_decl_stmt);

        // if __has_returned { __ret_val } else { __trailing_result }
        let trailing_var_expr = create_var_expr(package, assigner, trailing_var_id, return_ty);
        let if_expr = create_if_expr(
            package,
            assigner,
            flag_var,
            ret_var,
            Some(trailing_var_expr),
            return_ty,
        );
        Some(create_expr_stmt(package, assigner, if_expr))
    } else {
        // No trailing expression — return type should be Unit.
        debug_assert_eq!(
            return_ty,
            &Ty::UNIT,
            "create_flag_trailing_expr fallback requires Unit return type"
        );
        let unit_expr = create_unit_expr(package, assigner);
        let if_expr = create_if_expr(
            package,
            assigner,
            flag_var,
            ret_var,
            Some(unit_expr),
            return_ty,
        );
        Some(create_expr_stmt(package, assigner, if_expr))
    }
}

/// Check whether a type has a synthesizable classical default value without
/// allocating any FIR nodes.
///
/// Returns `true` for types that [`create_default_value`] would succeed on,
/// `false` for types (like `Qubit`) that have no classical default. Used by
/// [`unify_returns`] to emit a user-facing error before entering the flag
/// strategy, avoiding a panic in [`require_classical_default`].
fn can_create_classical_default(ty: &Ty, udt_pure_tys: &UdtPureTyCache) -> bool {
    match ty {
        Ty::Prim(
            Prim::Bool
            | Prim::Int
            | Prim::BigInt
            | Prim::Double
            | Prim::Pauli
            | Prim::Result
            | Prim::String
            | Prim::Range
            | Prim::RangeFrom
            | Prim::RangeTo
            | Prim::RangeFull,
        )
        | Ty::Array(_) => true,
        Ty::Tuple(elems) => elems
            .iter()
            .all(|e| can_create_classical_default(e, udt_pure_tys)),
        Ty::Udt(Res::Item(item_id)) => udt_pure_tys
            .get(&(item_id.package, item_id.item))
            .is_some_and(|pure_ty| can_create_classical_default(pure_ty, udt_pure_tys)),
        Ty::Arrow(arrow) => {
            can_create_classical_default(&arrow.output, udt_pure_tys)
                && matches!(arrow.functors, qsc_fir::ty::FunctorSet::Value(_))
        }
        Ty::Infer(_) | Ty::Param(_) | Ty::Err | Ty::Prim(Prim::Qubit) | Ty::Udt(_) => false,
    }
}

#[derive(Clone, Copy, Debug)]
enum UnsupportedDefaultSite {
    ReturnSlot,
    GuardedLocalInitializer,
}

impl UnsupportedDefaultSite {
    fn description(self) -> &'static str {
        match self {
            Self::ReturnSlot => "flag-strategy return-slot (__ret_val) initialization",
            Self::GuardedLocalInitializer => "flag-strategy guarded Local initializer",
        }
    }
}

/// Enforces the unsupported-default policy for flag-strategy synthesis sites.
///
/// The `create_default_value*` helpers intentionally return `Option` so callers
/// can decide policy. For return unification's flag strategy, missing defaults
/// are an internal compiler-contract violation and must fail loudly with a
/// stable, site-specific panic message.
///
/// **Note:** the known user-reachable case (Qubit-return-in-loop) is now
/// caught earlier by [`can_create_classical_default`] in [`unify_returns`],
/// which emits a user-facing [`Error::UnsupportedLoopReturnType`] diagnostic.
/// This panic remains as a safety net for unforeseen cases.
fn require_classical_default(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    site: UnsupportedDefaultSite,
) -> ExprId {
    create_default_value(package, assigner, package_id, ty, udt_pure_tys).unwrap_or_else(|| {
        panic!(
            "return_unify unsupported-default contract violation: {} requires a classical default, but `{ty}` has none",
            site.description(),
        )
    })
}

/// Create a default value expression for a type, used to initialize `__ret_val`.
///
/// The value is never observed: any read of `__ret_val` is guarded by
/// `__has_returned`, which becomes `true` only after an explicit return has
/// written `__ret_val`. Only the type must match.
///
/// # Before
/// ```text
/// (no expression)
/// ```
/// # After
/// ```text
/// Expr { ty, kind: default(ty) }   // e.g. Lit(Int(0)), Tuple(()), Array([])
/// ```
/// # Requires
/// - `ty` has a synthesizable classical default (see
///   [`create_default_value_kind`]); otherwise this returns `None`.
///
/// # Ensures
/// - Returns `Some(expr_id)` whose `Expr.ty == ty.clone()`.
/// - Returns `None` when no classical default exists (caller surfaces as a
///   deterministic diagnostic rather than emitting malformed FIR).
///
/// # Mutations
/// - Allocates one fresh `Expr` through `assigner` when `Some`.
fn create_default_value(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
) -> Option<ExprId> {
    let kind = create_default_value_kind(package, assigner, package_id, ty, udt_pure_tys)?;

    let expr_id = assigner.next_expr();
    package.exprs.insert(
        expr_id,
        Expr {
            id: expr_id,
            span: Span::default(),
            ty: ty.clone(),
            kind,
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    Some(expr_id)
}

/// Build a well-typed FIR `ExprKind` for the zero value of `ty`.
///
/// # Before
/// ```text
/// Ty::{Prim, Tuple, Array, Udt(Res::Item(..)), Arrow(..)}
/// ```
/// # After
/// ```text
/// ExprKind::{Lit(..), Tuple([defaults..]), Array([]), Var(Res::Item(nop), []), ...}
/// ```
/// # Requires
/// - `ty` is a type reachable from the callable's return type.
/// - `udt_pure_tys` has been populated from the store.
/// - `package_id` is the id of the package owning `package` — the synthesized
///   nop callable for arrow types is inserted there and referenced through it.
///
/// # Ensures
/// - Returns `None` when the type has no synthesizable classical default:
///   unresolved types (`Ty::Infer`, `Ty::Param`, `Ty::Err`), qubits
///   (`Prim::Qubit`), UDTs whose pure-ty cache entry is
///   missing or unresolved, and arrow types whose output type itself has no
///   default.
/// - Returns `Some(kind)` whose zero value matches `ty` structurally.
/// - For `Ty::Arrow`, `Some(Var(Res::Item(nop_item), vec![]))` references a
///   newly synthesized nop callable of the same arrow signature; the nop's
///   body returns the output type's default. Any later `Call` on the
///   resulting `__ret_val` value resolves to that nop — correct behavior
///   because the flag guard ensures reads only occur when an explicit return
///   already overwrote `__ret_val` with the real callable.
///
/// # Mutations
/// - For `Ty::Tuple` and `Ty::Udt` composites, allocates nested default
///   `Expr` nodes through `assigner` via [`create_default_value`].
/// - For `Ty::Arrow`, inserts a new `Item` (callable) into `package.items`
///   and allocates its body `Pat`, `Block`, and trailing `Expr` / `Stmt`.
fn create_default_value_kind(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
) -> Option<ExprKind> {
    match ty {
        Ty::Prim(Prim::Bool) => Some(ExprKind::Lit(Lit::Bool(false))),
        Ty::Prim(Prim::Int) => Some(ExprKind::Lit(Lit::Int(0))),
        Ty::Prim(Prim::BigInt) => Some(ExprKind::Lit(Lit::BigInt(BigInt::from(0)))),
        Ty::Prim(Prim::Double) => Some(ExprKind::Lit(Lit::Double(0.0))),
        Ty::Prim(Prim::Pauli) => Some(ExprKind::Lit(Lit::Pauli(qsc_fir::fir::Pauli::I))),
        Ty::Prim(Prim::Result) => Some(ExprKind::Lit(Lit::Result(Result::Zero))),
        Ty::Prim(Prim::String) => Some(ExprKind::String(Vec::new())),
        Ty::Tuple(elems) if elems.is_empty() => Some(ExprKind::Tuple(Vec::new())),
        Ty::Tuple(elems) => {
            let elem_exprs: Vec<ExprId> = elems
                .iter()
                .map(|elem_ty| {
                    create_default_value(package, assigner, package_id, elem_ty, udt_pure_tys)
                })
                .collect::<Option<_>>()?;
            Some(ExprKind::Tuple(elem_exprs))
        }
        Ty::Array(_) => Some(ExprKind::Array(Vec::new())),
        Ty::Udt(Res::Item(item_id)) => {
            let pure_ty = udt_pure_tys.get(&(item_id.package, item_id.item))?.clone();
            create_default_value_kind(package, assigner, package_id, &pure_ty, udt_pure_tys)
        }
        Ty::Arrow(arrow) => {
            create_nop_callable_var(package, assigner, package_id, arrow, udt_pure_tys)
        }
        Ty::Prim(Prim::Range | Prim::RangeFrom | Prim::RangeTo | Prim::RangeFull) => {
            Some(ExprKind::Range(None, None, None))
        }
        // No well-typed classical default: unresolved/placeholder types,
        // qubits and unresolved UDTs.
        Ty::Infer(_) | Ty::Param(_) | Ty::Err | Ty::Prim(Prim::Qubit) | Ty::Udt(_) => None,
    }
}

/// Synthesize a nop callable matching `arrow`'s signature, insert it into
/// `package`, and return a `Var(Res::Item(..))` expression referring to it.
///
/// The synthesized callable's body is a single-statement block whose
/// trailing expression is the default of the arrow's output type. The input
/// pattern is a typed `Discard`. If the output type itself has no classical
/// default the synthesis is abandoned and `None` is propagated.
///
/// # Ensures
/// - Returns `Some(Var(Res::Item(ItemId { package: package_id, item: new_item_id }), vec![]))`.
/// - Inserts exactly one new `Item` of kind `Callable` into `package.items`.
/// - The new callable's arrow scheme (input/output/kind/functors) matches
///   `arrow`.
fn create_nop_callable_var(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    arrow: &qsc_fir::ty::Arrow,
    udt_pure_tys: &UdtPureTyCache,
) -> Option<ExprKind> {
    // Build the nop body's default-of-output trailing expression.
    let output_default =
        create_default_value(package, assigner, package_id, &arrow.output, udt_pure_tys)?;
    let trailing_stmt = create_expr_stmt(package, assigner, output_default);
    let body_block = create_block(package, assigner, vec![trailing_stmt], &arrow.output);

    // Input pattern: a typed Discard matching the arrow's input type.
    let input_pat_id = assigner.next_pat();
    package.pats.insert(
        input_pat_id,
        Pat {
            id: input_pat_id,
            span: Span::default(),
            ty: *arrow.input.clone(),
            kind: PatKind::Discard,
        },
    );

    let body_spec = qsc_fir::fir::SpecDecl {
        id: assigner.next_node(),
        span: Span::default(),
        block: body_block,
        input: None,
        exec_graph: qsc_fir::fir::ExecGraph::default(),
    };
    let body_impl = qsc_fir::fir::SpecImpl {
        body: body_spec,
        adj: None,
        ctl: None,
        ctl_adj: None,
    };

    // After monomorphization, non-Value functors should not appear in
    // reachable return types; surface this as a missing default rather
    // than a panic so the pass bails deterministically.
    let qsc_fir::ty::FunctorSet::Value(functors) = arrow.functors else {
        return None;
    };

    let new_item_id = assigner.next_item();
    let callable_name: Rc<str> = Rc::from(format!("__return_unify_nop_{new_item_id}"));
    let decl = CallableDecl {
        id: assigner.next_node(),
        span: Span::default(),
        kind: arrow.kind,
        name: Ident {
            id: LocalVarId::from(0_u32),
            span: Span::default(),
            name: callable_name,
        },
        generics: Vec::new(),
        input: input_pat_id,
        output: *arrow.output.clone(),
        functors,
        implementation: CallableImpl::Spec(body_impl),
        attrs: Vec::new(),
    };

    let item = qsc_fir::fir::Item {
        id: new_item_id,
        span: Span::default(),
        parent: None,
        doc: Rc::from(""),
        attrs: Vec::new(),
        visibility: qsc_fir::fir::Visibility::Internal,
        kind: ItemKind::Callable(Box::new(decl)),
    };
    package.items.insert(new_item_id, item);

    Some(ExprKind::Var(
        Res::Item(qsc_fir::fir::ItemId {
            package: package_id,
            item: new_item_id,
        }),
        Vec::new(),
    ))
}

/// Create a new `Block` and insert it into the package.
fn create_block(
    package: &mut Package,
    assigner: &mut Assigner,
    stmts: Vec<StmtId>,
    ty: &Ty,
) -> BlockId {
    let block_id = assigner.next_block();
    package.blocks.insert(
        block_id,
        Block {
            id: block_id,
            span: Span::default(),
            ty: ty.clone(),
            stmts,
        },
    );
    block_id
}

/// Create an `Expr` statement (trailing expression, no semicolon).
fn create_expr_stmt(package: &mut Package, assigner: &mut Assigner, expr_id: ExprId) -> StmtId {
    let stmt_id = assigner.next_stmt();
    package.stmts.insert(
        stmt_id,
        Stmt {
            id: stmt_id,
            span: Span::default(),
            kind: StmtKind::Expr(expr_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    stmt_id
}

/// Create a `Semi` statement (expression with trailing semicolon).
fn create_semi_stmt(package: &mut Package, assigner: &mut Assigner, expr_id: ExprId) -> StmtId {
    let stmt_id = assigner.next_stmt();
    package.stmts.insert(
        stmt_id,
        Stmt {
            id: stmt_id,
            span: Span::default(),
            kind: StmtKind::Semi(expr_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    stmt_id
}

/// Create a `Block` expression.
fn create_block_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    block_id: BlockId,
    ty: &Ty,
) -> ExprId {
    let expr_id = assigner.next_expr();
    package.exprs.insert(
        expr_id,
        Expr {
            id: expr_id,
            span: Span::default(),
            ty: ty.clone(),
            kind: ExprKind::Block(block_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    expr_id
}

/// Create an `If` expression.
fn create_if_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    cond: ExprId,
    then_expr: ExprId,
    else_expr: Option<ExprId>,
    ty: &Ty,
) -> ExprId {
    let expr_id = assigner.next_expr();
    package.exprs.insert(
        expr_id,
        Expr {
            id: expr_id,
            span: Span::default(),
            ty: ty.clone(),
            kind: ExprKind::If(cond, then_expr, else_expr),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    expr_id
}

/// Create a `UnOp::NotL` expression.
fn create_not_expr(package: &mut Package, assigner: &mut Assigner, operand: ExprId) -> ExprId {
    let expr_id = assigner.next_expr();
    package.exprs.insert(
        expr_id,
        Expr {
            id: expr_id,
            span: Span::default(),
            ty: Ty::Prim(Prim::Bool),
            kind: ExprKind::UnOp(UnOp::NotL, operand),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    expr_id
}

/// Create a `BinOp` expression.
fn create_bin_op_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    op: BinOp,
    lhs: ExprId,
    rhs: ExprId,
    ty: &Ty,
) -> ExprId {
    let expr_id = assigner.next_expr();
    package.exprs.insert(
        expr_id,
        Expr {
            id: expr_id,
            span: Span::default(),
            ty: ty.clone(),
            kind: ExprKind::BinOp(op, lhs, rhs),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    expr_id
}

/// Create a `Var(Res::Local(var_id))` expression.
fn create_var_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    var_id: LocalVarId,
    ty: &Ty,
) -> ExprId {
    let expr_id = assigner.next_expr();
    package.exprs.insert(
        expr_id,
        Expr {
            id: expr_id,
            span: Span::default(),
            ty: ty.clone(),
            kind: ExprKind::Var(Res::Local(var_id), Vec::new()),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    expr_id
}

/// Create `not Var(__has_returned)`.
fn create_not_var_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    var_id: LocalVarId,
) -> ExprId {
    let var = create_var_expr(package, assigner, var_id, &Ty::Prim(Prim::Bool));
    create_not_expr(package, assigner, var)
}

/// Create `Assign(Var(var_id), value)`.
fn create_assign_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    var_id: LocalVarId,
    value: ExprId,
    ty: &Ty,
) -> ExprId {
    let var_expr = create_var_expr(package, assigner, var_id, ty);
    let expr_id = assigner.next_expr();
    package.exprs.insert(
        expr_id,
        Expr {
            id: expr_id,
            span: Span::default(),
            ty: Ty::UNIT,
            kind: ExprKind::Assign(var_expr, value),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    expr_id
}

/// Create a boolean literal expression.
fn create_bool_lit(package: &mut Package, assigner: &mut Assigner, value: bool) -> ExprId {
    let expr_id = assigner.next_expr();
    package.exprs.insert(
        expr_id,
        Expr {
            id: expr_id,
            span: Span::default(),
            ty: Ty::Prim(Prim::Bool),
            kind: ExprKind::Lit(Lit::Bool(value)),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    expr_id
}

/// Create a Unit `()` expression.
fn create_unit_expr(package: &mut Package, assigner: &mut Assigner) -> ExprId {
    let expr_id = assigner.next_expr();
    package.exprs.insert(
        expr_id,
        Expr {
            id: expr_id,
            span: Span::default(),
            ty: Ty::UNIT,
            kind: ExprKind::Tuple(Vec::new()),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    expr_id
}

/// Create a mutable boolean variable declaration: `mutable name = value`.
/// Returns `(LocalVarId, StmtId)`.
fn create_mutable_bool_var(
    package: &mut Package,
    assigner: &mut Assigner,
    name: &str,
    value: bool,
) -> (LocalVarId, StmtId) {
    let init_expr = create_bool_lit(package, assigner, value);
    create_mutable_var(package, assigner, name, &Ty::Prim(Prim::Bool), init_expr)
}

/// Create a mutable variable declaration: `mutable name: ty = init_expr`.
/// Returns `(LocalVarId, StmtId)`.
fn create_mutable_var(
    package: &mut Package,
    assigner: &mut Assigner,
    name: &str,
    ty: &Ty,
    init_expr: ExprId,
) -> (LocalVarId, StmtId) {
    create_local_var(package, assigner, name, ty, init_expr, Mutability::Mutable)
}

/// Create an immutable variable declaration: `let name: ty = init_expr`.
/// Returns `(LocalVarId, StmtId)`.
fn create_immutable_var(
    package: &mut Package,
    assigner: &mut Assigner,
    name: &str,
    ty: &Ty,
    init_expr: ExprId,
) -> (LocalVarId, StmtId) {
    create_local_var(
        package,
        assigner,
        name,
        ty,
        init_expr,
        Mutability::Immutable,
    )
}

/// Create a local variable declaration with the given mutability.
/// Returns `(LocalVarId, StmtId)`.
fn create_local_var(
    package: &mut Package,
    assigner: &mut Assigner,
    name: &str,
    ty: &Ty,
    init_expr: ExprId,
    mutability: Mutability,
) -> (LocalVarId, StmtId) {
    let local_var_id = assigner.next_local();
    let pat_id = assigner.next_pat();
    package.pats.insert(
        pat_id,
        Pat {
            id: pat_id,
            span: Span::default(),
            ty: ty.clone(),
            kind: PatKind::Bind(Ident {
                id: local_var_id,
                span: Span::default(),
                name: Rc::from(name),
            }),
        },
    );

    let stmt_id = assigner.next_stmt();
    package.stmts.insert(
        stmt_id,
        Stmt {
            id: stmt_id,
            span: Span::default(),
            kind: StmtKind::Local(mutability, pat_id, init_expr),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    (local_var_id, stmt_id)
}
