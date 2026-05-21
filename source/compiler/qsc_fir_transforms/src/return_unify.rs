// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Return unification pass.
//!
//! Eliminates all `ExprKind::Return` nodes in reachable callable bodies,
//! ensuring every such callable has exactly one exit point — the trailing
//! expression of its top-level block. Unreachable callables are left as-is;
//! see the rationale block above [`unify_returns`] for why this is safe.
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
//! This pass implements the "flag strategy everywhere" design: every
//! return-bearing block is lowered through a uniform mutable-flag
//! scaffolding (`__has_returned : Bool`, `__ret_val : T`), then simplified
//! by a named rewrite catalogue in [`simplify`].
//!
//! The formal basis is Böhm–Jacopini (1966) — every program with
//! arbitrary control flow can be expressed as a structured program using
//! bounded mutable state. Kozen–Tseng (2008) shows the auxiliary state
//! is essential at the propositional level. The algorithmic ancestor is
//! LLVM's `UnifyFunctionExitNodes` followed by `SimplifyCFG`: lower
//! once into a single-exit form, then fold the canonical output shapes
//! back into structured form with named, individually-tested rewrite
//! rules. (FIR is tree-IR; LLVM's pass substitutes an auxiliary boolean +
//! slot for PHI-based unification.)
//!
//! The pass uses a three-phase pipeline per callable block:
//!
//! 1. **Normalize** ([`normalize::hoist_returns_to_statement_boundary`]):
//!    Hoist any `Return` in compound positions (e.g. inside a block-expression
//!    used as a `Call` argument) to its enclosing statement boundary. After
//!    this phase, every `Return` is either a bare `Semi(Return(_))` /
//!    `Expr(Return(_))` or nested inside `If`, `While`, or `Block` statements.
//!
//! 2. **Transform** ([`transform_block_with_flags`]):
//!    Apply the flag strategy to eliminate all `Return` nodes by introducing
//!    `__has_returned` and `__ret_val` mutable slots. This corresponds to
//!    LLVM's `UnifyFunctionExitNodes` / `mergereturn` lowering for early
//!    returns: every return path writes the slot and the flag, and a single
//!    merge expression at the tail reads them.
//!
//! 3. **Simplify** ([`simplify::run_to_fixpoint`]):
//!    After the flag strategy, run a named rewrite catalogue
//!    ([`simplify::guard_clause`], [`simplify::both_branches`],
//!    [`simplify::bare_return`], [`simplify::let_folding`],
//!    [`simplify::dead_flag`], [`simplify::dead_local`]) to fold the
//!    canonical flag-output shapes back into structured form. This is the
//!    structured-IR analog of LLVM's `SimplifyCFG` after `mergereturn`.
//!
//! # Contract
//!
//! The pass runs post-monomorphization and pre-defunctionalization. Output
//! guarantees enforced by [`crate::invariants::InvariantLevel::PostReturnUnify`]:
//!
//! * **No `Return` nodes** in reachable code — checked by
//!   `crate::invariants::check_no_returns`.
//! * **No non-Unit `Semi`-terminated block tails** — checked by
//!   `crate::invariants::check_non_unit_block_tails`.
//! * **`LocalVarId` consistency** — every `LocalVarId` referenced in a
//!   callable body is bound in that body's scope.
//!
//! # Callable-body arity contract
//!
//! RCA depends on the callable-body arity remaining stable across this
//! pass. This pass never introduces or removes top-level callable
//! parameters; it only rewrites block bodies. The flag/slot allocations
//! become `Local` bindings inside the body's outermost block, not
//! parameters of the enclosing callable.
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
//! # Error reporting
//!
//! [`unify_returns`] returns `Vec<Error>` rather than panicking. The known
//! user-reachable errors are [`Error::UnsupportedEarlyReturnType`] and
//! [`Error::UnsupportedMixedQubitArrowReturnType`]: the flag strategy cannot
//! synthesize a return slot for unresolved or unsupported return types.
//! Defaultable return types use a direct `__ret_val : T` slot;
//! non-defaultable non-arrow data return types use an array-backed `T[]` slot.
//! Unsupported shapes produce a user-facing diagnostic, and processing
//! continues for remaining callables.
//!
//! # Qubit release interaction
//!
//! Qubit-release handling is intrinsic to `return_unify`; the historical
//! `release_hoist` pre-pass was folded in.

mod detect;
mod normalize;
mod simplify;
mod symbols;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "slow-proptest-tests"))]
mod semantic_equivalence_tests;

use crate::fir_builder::{
    alloc_assign_expr, alloc_bin_op_expr, alloc_block, alloc_block_expr, alloc_bool_lit,
    alloc_expr, alloc_expr_stmt, alloc_if_expr, alloc_local_var, alloc_local_var_expr,
    alloc_not_expr, alloc_semi_stmt, alloc_unit_expr, functored_specs,
};
use miette::Diagnostic;
use num_bigint::BigInt;
use qsc_data_structures::span::Span;
use qsc_fir::{
    assigner::Assigner,
    fir::{
        BinOp, BlockId, CallableDecl, CallableImpl, Expr, ExprId, ExprKind, Ident, ItemId,
        ItemKind, Lit, LocalItemId, LocalVarId, Mutability, Package, PackageId, PackageLookup,
        PackageStore, Pat, PatKind, Res, Result, StmtId, StmtKind, StoreItemId, StringComponent,
        UnOp,
    },
    ty::{Prim, Ty},
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{cell::RefCell, rc::Rc};
use thiserror::Error;

use crate::{EMPTY_EXEC_RANGE, reachability::collect_reachable_from_entry};

/// Errors that can occur during return unification.
#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    /// Catch-all for non-defaultable, arrow-free shapes that neither Direct
    /// nor `ArrayBacked` can encode. Currently the safety-net path.
    #[error("cannot unify early returns of type `{0}`")]
    #[diagnostic(code("Qsc.ReturnUnify.UnsupportedEarlyReturnType"))]
    #[diagnostic(help(
        "the return type has no classical default and cannot be array-backed; \
         consider restructuring to avoid early returns of this type"
    ))]
    UnsupportedEarlyReturnType(
        String,
        #[label("callable with unsupported return type")] Span,
    ),

    /// Emitted when the simplifier or hoist fixpoint loop fails to reach a
    /// fixpoint within the per-block measure bound. The IR remains semantically
    /// valid, but the partial fold indicates a rule regression.
    #[error("return-unification {phase} did not reach a fixpoint")]
    #[diagnostic(code("Qsc.ReturnUnify.FixpointNotReached"))]
    #[diagnostic(severity(Warning))]
    #[diagnostic(help(
        "this is an internal compiler diagnostic; please file an issue \
         including the source program that triggered it"
    ))]
    FixpointNotReached { phase: &'static str, block: BlockId },

    /// The return type contains both a non-defaultable type (e.g. Qubit)
    /// and an arrow (callable) component. Direct requires a classical default
    /// the type lacks, and `ArrayBacked` is gated against arrow content.
    #[error("cannot unify early returns of type `{ty}`")]
    #[diagnostic(code("Qsc.ReturnUnify.UnsupportedMixedQubitArrowReturnType"))]
    #[diagnostic(help(
        "the return type combines a non-defaultable type (such as Qubit) \
         with a callable type; consider restructuring to return these \
         separately, or refactor the early-return into a single tail \
         expression"
    ))]
    UnsupportedMixedQubitArrowReturnType {
        ty: String,
        #[label("callable with unsupported return-slot shape")]
        span: Span,
    },

    /// A return appears inside a compound expression whose enclosing
    /// expression has a type with no classical default.
    #[error("cannot hoist `return` from a compound position of type `{enclosing_ty}`")]
    #[diagnostic(code("Qsc.ReturnUnify.UnsupportedHoistContext"))]
    #[diagnostic(help(
        "the surrounding expression has a non-defaultable type; \
         move the `return` to a statement boundary, or restructure the \
         expression so it does not contain a `return`"
    ))]
    UnsupportedHoistContext {
        enclosing_ty: String,
        #[label("compound expression with unsupported `return`")]
        span: Span,
    },
}

impl Error {
    /// Returns true if this error is a non-fatal warning that should not
    /// trigger pipeline abort.
    #[must_use]
    pub fn is_warning(&self) -> bool {
        matches!(self, Self::FixpointNotReached { .. })
    }
}

/// Cache of pure structural UDT types used by defaultability and continuation-safety checks.
///
/// The cache is seeded from reachable callable output types, then lazily extended when a
/// continuation local references a UDT that does not appear in those outputs.
#[derive(Default)]
struct UdtPureTyCache {
    pure_tys: RefCell<FxHashMap<(PackageId, LocalItemId), Ty>>,
}

impl UdtPureTyCache {
    /// Creates a cache from precomputed UDT pure types.
    fn new(pure_tys: FxHashMap<(PackageId, LocalItemId), Ty>) -> Self {
        Self {
            pure_tys: RefCell::new(pure_tys),
        }
    }

    /// Gets a cached pure type for a UDT item, if it has already been resolved.
    fn get(&self, item_id: ItemId) -> Option<Ty> {
        self.pure_tys
            .borrow()
            .get(&(item_id.package, item_id.item))
            .cloned()
    }

    /// Inserts a resolved pure type into the cache.
    fn insert(&self, item_id: ItemId, pure_ty: Ty) {
        self.pure_tys
            .borrow_mut()
            .insert((item_id.package, item_id.item), pure_ty);
    }

    /// Resolves a UDT pure type from the package store and caches the result.
    fn resolve_from_store(&self, store: &PackageStore, item_id: ItemId) -> Option<Ty> {
        if let Some(pure_ty) = self.get(item_id) {
            return Some(pure_ty);
        }

        let pkg = store.get(item_id.package);
        let item = pkg.items.get(item_id.item)?;
        let ItemKind::Ty(_, udt) = &item.kind else {
            return None;
        };
        let pure_ty = udt.get_pure_ty();
        self.insert(item_id, pure_ty.clone());
        Some(pure_ty)
    }

    /// Resolves a UDT pure type from the currently borrowed package and caches the result.
    fn resolve_from_package(
        &self,
        package_id: PackageId,
        package: &Package,
        item_id: ItemId,
    ) -> Option<Ty> {
        if let Some(pure_ty) = self.get(item_id) {
            return Some(pure_ty);
        }

        if item_id.package != package_id {
            return None;
        }

        let item = package.items.get(item_id.item)?;
        let ItemKind::Ty(_, udt) = &item.kind else {
            return None;
        };
        let pure_ty = udt.get_pure_ty();
        self.insert(item_id, pure_ty.clone());
        Some(pure_ty)
    }
}

/// Source available for lazy UDT pure-type resolution at a policy check site.
enum UdtResolutionContext<'a> {
    /// Resolve from the package store before the target package is mutably borrowed.
    Store(&'a PackageStore),
    /// Resolve from the package currently being rewritten.
    Package {
        package_id: PackageId,
        package: &'a Package,
    },
}

impl UdtResolutionContext<'_> {
    /// Resolves a UDT pure type through the context's available package access.
    fn resolve_udt_pure_ty(&self, udt_pure_tys: &UdtPureTyCache, item_id: ItemId) -> Option<Ty> {
        match self {
            Self::Store(store) => udt_pure_tys.resolve_from_store(store, item_id),
            Self::Package {
                package_id,
                package,
            } => udt_pure_tys.resolve_from_package(*package_id, package, item_id),
        }
    }
}

/// Recursively collects UDT item references from a type.
///
/// Walks nested tuples, arrays, and arrows to find all `Ty::Udt` variants and
/// records their `(PackageId, LocalItemId)` identity in `refs`.
fn collect_udt_refs_from_ty(ty: &Ty, refs: &mut FxHashSet<(PackageId, LocalItemId)>) {
    match ty {
        Ty::Udt(Res::Item(item_id)) => {
            refs.insert((item_id.package, item_id.item));
        }
        Ty::Array(inner) => collect_udt_refs_from_ty(inner, refs),
        Ty::Tuple(tys) => {
            for t in tys {
                collect_udt_refs_from_ty(t, refs);
            }
        }
        Ty::Arrow(arrow) => {
            collect_udt_refs_from_ty(&arrow.input, refs);
            collect_udt_refs_from_ty(&arrow.output, refs);
        }
        _ => {}
    }
}

/// Builds a UDT pure-type cache scoped to UDTs referenced in reachable callable return types.
///
/// Only resolves `get_pure_ty()` for UDTs that appear in the output types of callables in
/// `reachable`. This avoids scanning all packages × all items when only a fraction of UDTs
/// are actually needed during return unification.
fn build_scoped_udt_pure_ty_cache(
    store: &PackageStore,
    reachable: &FxHashSet<StoreItemId>,
) -> UdtPureTyCache {
    let mut needed_udts: FxHashSet<(PackageId, LocalItemId)> = FxHashSet::default();
    for item_id in reachable {
        let pkg = store.get(item_id.package);
        let item = pkg.get_item(item_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            collect_udt_refs_from_ty(&decl.output, &mut needed_udts);
        }
    }
    let mut cache = FxHashMap::default();
    for (pkg_id, local_id) in &needed_udts {
        let pkg = store.get(*pkg_id);
        let item = pkg.get_item(*local_id);
        if let ItemKind::Ty(_, udt) = &item.kind {
            cache.insert((*pkg_id, *local_id), udt.get_pure_ty());
        }
    }
    UdtPureTyCache::new(cache)
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
///   return value via the flag strategy followed by the [`simplify`]
///   rewrite catalogue.
///
/// # Mutations
/// - Rewrites `CallableDecl` body blocks in `store[package_id]`.
/// - Allocates new FIR nodes through `assigner`.
///
/// # Returns
/// A `Vec<Error>` collecting per-callable diagnostics. An empty vector means
/// every reachable callable in `package_id` was rewritten successfully.
/// Errors are accumulated, not fatal: processing continues for remaining
/// callables after each diagnostic is recorded.
///
/// # Errors
/// The user-reachable variants are [`Error::UnsupportedEarlyReturnType`] and
/// [`Error::UnsupportedMixedQubitArrowReturnType`], emitted when the flag
/// strategy is selected (categories B, C, D) for a callable whose return
/// type has no classical default — for example `Qubit` or any compound type
/// containing `Qubit`. The check is performed up-front by
/// `can_create_classical_default` before the transform runs, so the affected
/// callable is left unchanged.
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
// if that design lands. tagged-union
// defunctionalization would create *new* dispatch items (union type +
// apply function) rather than widening reachability to existing dead
// callables, so the invariant is expected to hold. Re-audit if the
// tagged-union design instead reuses or inlines dead callables.
pub fn unify_returns(
    store: &mut PackageStore,
    package_id: PackageId,
    assigner: &mut Assigner,
) -> Vec<Error> {
    unify_returns_impl(store, package_id, assigner, /* run_simplify */ true)
}

/// Test-only variant of [`unify_returns`] that stops after
/// `transform_block_with_flags` and skips [`simplify::run_to_fixpoint`].
///
/// Per-rule simplify tests use this to capture the pre-simplify FIR
/// shape so they can apply individual rules and snapshot the delta.
#[cfg(test)]
pub(crate) fn unify_returns_without_simplify(
    store: &mut PackageStore,
    package_id: PackageId,
    assigner: &mut Assigner,
) -> Vec<Error> {
    unify_returns_impl(store, package_id, assigner, /* run_simplify */ false)
}

fn unify_returns_impl(
    store: &mut PackageStore,
    package_id: PackageId,
    assigner: &mut Assigner,
    run_simplify: bool,
) -> Vec<Error> {
    let reachable = collect_reachable_from_entry(store, package_id);
    let udt_pure_tys = build_scoped_udt_pure_ty_cache(store, &reachable);
    let mut errors = Vec::new();

    let mut arrow_default_cache = ArrowDefaultCache::default();
    let local_reachable: Vec<_> = reachable
        .iter()
        .filter(|id| id.package == package_id)
        .map(|id| id.item)
        .collect();

    for item_id in local_reachable {
        let callable = {
            let package = store.get(package_id);
            let item = package.get_item(item_id);
            match &item.kind {
                ItemKind::Callable(callable) => callable.clone(),
                _ => continue,
            }
        };
        let return_ty = callable.output.clone();
        let body_blocks = get_callable_body_blocks(&callable);

        // Pre-check: verify the normalize phase can handle all compound-
        // position Returns in this callable's body blocks. If any block
        // contains a Return in a context that requires a non-defaultable
        // type default, emit a diagnostic and skip the entire callable.
        let pre_check_error_count = errors.len();
        for &block_id in &body_blocks {
            if !contains_return_in_block(store.get(package_id), block_id) {
                continue;
            }
            check_normalize_supportable(store.get(package_id), package_id, block_id, &mut errors);
        }
        if errors[pre_check_error_count..]
            .iter()
            .any(|e| !e.is_warning())
        {
            continue;
        }

        for block_id in body_blocks {
            if !contains_return_in_block(store.get(package_id), block_id) {
                continue;
            }

            // Pre-pass: hoist any compound-position Return to its enclosing
            // statement boundary so the strategy pass only sees bare returns
            // or returns inside statement-carrying Block/If/While.
            normalize::hoist_returns_to_statement_boundary(
                store.get_mut(package_id),
                assigner,
                package_id,
                block_id,
                &mut errors,
            );

            // The flag strategy is the only return-unification strategy.
            let return_slot_strategy = {
                let context = UdtResolutionContext::Store(store);
                select_return_slot_strategy(&return_ty, &udt_pure_tys, &context)
            };

            let Some(return_slot_strategy) = return_slot_strategy else {
                // Distinguish: arrow-containing types get a specific
                // diagnostic; pure non-defaultable types get the catch-all.
                let has_arrow = {
                    let context = UdtResolutionContext::Store(store);
                    matches!(
                        arrow_scan_for_ty(
                            &return_ty,
                            &udt_pure_tys,
                            &context,
                            &mut FxHashSet::default(),
                        ),
                        ArrowScan::ContainsArrow
                    )
                };
                if has_arrow {
                    errors.push(Error::UnsupportedMixedQubitArrowReturnType {
                        ty: format!("{return_ty}"),
                        span: callable.name.span,
                    });
                } else {
                    errors.push(Error::UnsupportedEarlyReturnType(
                        format!("{return_ty}"),
                        callable.name.span,
                    ));
                }
                continue;
            };

            let package = store.get_mut(package_id);
            transform_block_with_flags(
                package,
                assigner,
                package_id,
                block_id,
                &return_ty,
                &udt_pure_tys,
                &mut arrow_default_cache,
                return_slot_strategy,
            );
            if run_simplify {
                simplify::run_to_fixpoint(package, assigner, block_id, &mut errors);
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
            for spec in functored_specs(spec_impl) {
                blocks.push(spec.block);
            }
            blocks
        }
        CallableImpl::SimulatableIntrinsic(spec) => vec![spec.block],
    }
}

use detect::{contains_return_in_block, contains_return_in_expr, contains_return_in_stmt};

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

/// Re-syncs an expression's `Ty` from its children after a return-
/// replacement pass may have changed a child's type.
///
/// # If-expression policy
///
/// For `ExprKind::If(cond, then_expr, else_expr)`, the expression type
/// is set to the non-Unit branch type when one branch was rewritten to
/// Unit by return replacement. This preserves the original type for
/// surrounding `Local` bindings. If both branches are non-Unit or both
/// are Unit, the then-branch type wins.
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
/// - `return_slot_strategy` was selected for `return_ty` before the package
///   was mutably borrowed.
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
#[allow(clippy::too_many_arguments)]
fn transform_block_with_flags(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    block_id: BlockId,
    return_ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    arrow_default_cache: &mut ArrowDefaultCache,
    return_slot_strategy: ReturnSlotStrategy,
) {
    // Create __has_returned: Bool = false
    let (has_returned_var_id, has_returned_decl_stmt) =
        create_mutable_bool_var(package, assigner, symbols::HAS_RETURNED, false);

    let (return_slot, ret_val_decl_stmt) = create_return_slot_decl(
        package,
        assigner,
        package_id,
        return_ty,
        udt_pure_tys,
        arrow_default_cache,
        return_slot_strategy,
    );

    let original_stmts = package.get_block(block_id).stmts.clone();
    let mut new_stmts: Vec<StmtId> = Vec::new();

    // Insert flag declarations.
    new_stmts.push(has_returned_decl_stmt);
    new_stmts.push(ret_val_decl_stmt);
    let flag_context = FlagContext {
        package_id,
        has_returned_var_id,
        return_slot,
        return_ty,
        udt_pure_tys,
    };
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

    // Create trailing expression: if __has_returned { __ret_val } else { <original_trailing> }
    let trailing =
        create_flag_trailing_expr_for_slot(package, assigner, &mut new_stmts, &flag_context);

    if let Some(trailing_stmt) = trailing {
        new_stmts.push(trailing_stmt);
    }

    let block = package.blocks.get_mut(block_id).expect("block not found");
    block.stmts = new_stmts;
    block.ty = return_ty.clone();
}

/// Policy for handling the final value-producing statement in a flag-rewritten block.
#[derive(Clone, Copy)]
enum FinalTrailingExprStrategy {
    /// Keep the final trailing expression in place unless it contains rewritten returns.
    Preserve,
    /// Wrap the final trailing expression in a lazy continuation guarded by `__has_returned`.
    Lazy,
}

/// Fallback to use for a value-producing nested expression when an earlier
/// return has already fired and the value is statically unused.
#[derive(Clone, Copy)]
enum ValueFallback {
    /// Synthesize a default value for the nested expression's type.
    Default,
    /// Use `false`; this is used for condition expressions so their branches
    /// do not run after an early return in the condition.
    BoolFalse,
}

/// Output contract for recursively rewriting a statement sequence with an existing flag pair.
#[derive(Clone)]
enum FlagBlockOutput {
    /// The sequence must produce the callable return value.
    ReturnValue {
        final_trailing_expr_strategy: FinalTrailingExprStrategy,
    },
    /// The sequence must preserve a nested expression value that is not the
    /// callable return value (for example a `Bool` condition block).
    Value {
        value_ty: Ty,
        final_trailing_expr_strategy: FinalTrailingExprStrategy,
        fallback: ValueFallback,
    },
    /// The sequence is used only for side effects and has Unit type.
    Unit,
}

impl FlagBlockOutput {
    /// Returns the same output mode, forcing value-producing final tails to be lazy.
    fn lazy(&self) -> Self {
        match self {
            Self::ReturnValue { .. } => Self::ReturnValue {
                final_trailing_expr_strategy: FinalTrailingExprStrategy::Lazy,
            },
            Self::Value {
                value_ty, fallback, ..
            } => Self::Value {
                value_ty: value_ty.clone(),
                final_trailing_expr_strategy: FinalTrailingExprStrategy::Lazy,
                fallback: *fallback,
            },
            Self::Unit => Self::Unit,
        }
    }

    /// Gets the final-tail strategy when the rewritten sequence is value-producing.
    fn final_trailing_expr_strategy(&self) -> Option<FinalTrailingExprStrategy> {
        match self {
            Self::ReturnValue {
                final_trailing_expr_strategy,
            }
            | Self::Value {
                final_trailing_expr_strategy,
                ..
            } => Some(*final_trailing_expr_strategy),
            Self::Unit => None,
        }
    }
}

/// Strategy used for the synthesized return-value slot in flag-based rewrites.
///
/// Selected once per callable by [`select_return_slot_strategy`] before the
/// package is mutably borrowed, and threaded through the rewrite via
/// [`ReturnSlot`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReturnSlotStrategy {
    /// Store the returned value directly in `__ret_val : T`.
    ///
    /// Used when `T` has a classical default. Reads of the slot need no
    /// further wrapping: `__ret_val` already has the right type and the
    /// initial value keeps unreachable false branches well-typed.
    Direct,
    /// Store the returned value as the single element of `__ret_val : T[]`.
    ///
    /// Used when `T` has no classical default but is arrow-free, so the
    /// universal array default `[]` is well-typed. Reads index `[0]` and are
    /// guarded by `__has_returned` (or by a typed [`ExprKind::Fail`] in
    /// statically dead branches).
    ArrayBacked,
}

/// Synthesized return-value slot shared by flag-strategy rewrites.
///
/// Carries both the slot's [`LocalVarId`] and the [`ReturnSlotStrategy`]
/// chosen for it, so downstream helpers like [`create_return_slot_write_expr`]
/// can emit the right shape (`__ret_val = v` vs `__ret_val = [v]`) without
/// re-deriving the policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReturnSlot {
    /// Local id for the synthesized `__ret_val` slot.
    var_id: LocalVarId,
    /// Representation strategy selected for the slot.
    strategy: ReturnSlotStrategy,
}

/// Conservative scan result for arrow-containing return types.
///
/// Used by [`arrow_scan_for_ty`] to decide whether an array-backed return
/// slot is safe. The lattice is [`ArrowScan::ContainsArrow`] >
/// [`ArrowScan::Unknown`] > [`ArrowScan::NoArrow`]; only `NoArrow` enables
/// array-backed mode, so any ambiguity falls back to rejecting the type and
/// emitting an unsupported-return-type diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ArrowScan {
    /// The scanned type is definitely arrow-free.
    NoArrow,
    /// The scanned type contains at least one arrow.
    ContainsArrow,
    /// The scanned type could not be resolved precisely enough.
    Unknown,
}

impl ArrowScan {
    /// Combines two scan results, preserving the most conservative outcome.
    ///
    /// `ContainsArrow` dominates `Unknown`, which dominates `NoArrow`. The
    /// operation is commutative and associative, so it is safe to fold over
    /// children of tuples/arrays/UDTs in any order.
    fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::ContainsArrow, _) | (_, Self::ContainsArrow) => Self::ContainsArrow,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::NoArrow, Self::NoArrow) => Self::NoArrow,
        }
    }
}

/// Shared flag-strategy state threaded through top-level and recursive rewrites.
struct FlagContext<'a> {
    /// Package that owns synthesized defaults and lazy UDT lookups.
    package_id: PackageId,
    /// Local id for the synthesized `__has_returned` flag.
    has_returned_var_id: LocalVarId,
    /// Synthesized `__ret_val` return slot.
    return_slot: ReturnSlot,
    /// Callable return type captured by the flag strategy.
    return_ty: &'a Ty,
    /// Cache used for defaultability and continuation-safety policy checks.
    udt_pure_tys: &'a UdtPureTyCache,
}

const ARRAY_RETURN_SLOT_UNWRITTEN_FAIL_MESSAGE: &str =
    "return_unify array return slot was not written";

/// Allocates the `mutable __ret_val` declaration for the flag strategy.
///
/// The declaration's shape is determined by `strategy`:
///
/// * [`ReturnSlotStrategy::Direct`] synthesizes `mutable __ret_val : T = default(T)`
///   using [`require_classical_default`]. Callable-valued returns may insert
///   a fail-bodied callable as the default; it is never actually invoked because
///   reads of `__ret_val` are always guarded by `__has_returned`.
/// * [`ReturnSlotStrategy::ArrayBacked`] synthesizes `mutable __ret_val : T[] = []`,
///   which is the universal classical default for any array type and lets the
///   strategy support non-defaultable `T` (e.g. `Qubit`, tuples containing
///   `Qubit`, or arrow-free UDTs).
///
/// # Requires
/// - `strategy` was previously selected by [`select_return_slot_strategy`] for
///   `return_ty`. In particular, `Direct` requires `return_ty` to have a
///   classical default.
///
/// # Ensures
/// - Returns a [`ReturnSlot`] handle and the [`StmtId`] of the new `Local`
///   declaration that the caller prepends to the rewritten block.
///
/// # Mutations
/// - Allocates a new local var, init expression, and `Local` statement through
///   `assigner`.
fn create_return_slot_decl(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    return_ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    arrow_default_cache: &mut ArrowDefaultCache,
    strategy: ReturnSlotStrategy,
) -> (ReturnSlot, StmtId) {
    let (slot_ty, init_expr) = match strategy {
        ReturnSlotStrategy::Direct => {
            // For callable-valued direct slots, default synthesis may insert a fail-bodied callable.
            let init_expr = require_classical_default(
                package,
                assigner,
                package_id,
                return_ty,
                udt_pure_tys,
                arrow_default_cache,
                UnsupportedDefaultSite::ReturnSlot,
            );
            (return_ty.clone(), init_expr)
        }
        ReturnSlotStrategy::ArrayBacked => {
            let slot_ty = Ty::Array(Box::new(return_ty.clone()));
            let init_expr = alloc_expr(
                package,
                assigner,
                slot_ty.clone(),
                ExprKind::Array(Vec::new()),
                Span::default(),
            );
            (slot_ty, init_expr)
        }
    };

    let (var_id, stmt_id) = alloc_local_var(
        package,
        assigner,
        symbols::RET_VAL,
        &slot_ty,
        init_expr,
        Mutability::Mutable,
    );
    (ReturnSlot { var_id, strategy }, stmt_id)
}

/// Builds the write expression that stores a returned value into `slot`.
///
/// * [`ReturnSlotStrategy::Direct`] emits `set __ret_val = value`.
/// * [`ReturnSlotStrategy::ArrayBacked`] emits `set __ret_val = [value]`,
///   wrapping the value in a singleton array so the slot's array type is
///   preserved.
///
/// # Mutations
/// - Allocates new FIR nodes through `assigner` for the singleton array
///   wrapper (array-backed mode) and the resulting assignment expression.
fn create_return_slot_write_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    slot: ReturnSlot,
    value_expr: ExprId,
    value_ty: &Ty,
) -> ExprId {
    match slot.strategy {
        ReturnSlotStrategy::Direct => {
            create_assign_expr(package, assigner, slot.var_id, value_expr, value_ty)
        }
        ReturnSlotStrategy::ArrayBacked => {
            let array_ty = Ty::Array(Box::new(value_ty.clone()));
            let singleton = alloc_expr(
                package,
                assigner,
                array_ty.clone(),
                ExprKind::Array(vec![value_expr]),
                Span::default(),
            );
            create_assign_expr(package, assigner, slot.var_id, singleton, &array_ty)
        }
    }
}

/// Builds an expression that reads the returned value out of `slot`.
///
/// * [`ReturnSlotStrategy::Direct`] emits `__ret_val`.
/// * [`ReturnSlotStrategy::ArrayBacked`] emits `__ret_val[0]`. Callers must
///   guard such reads with `__has_returned` because reading index 0 of the
///   empty initial array would fail at runtime. Use
///   [`create_return_slot_read_or_fail_expr`] when the guard is not already
///   in place.
///
/// # Mutations
/// - Allocates new FIR nodes through `assigner` for the var read and (in
///   array-backed mode) the index expression.
fn create_return_slot_read_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    slot: ReturnSlot,
    return_ty: &Ty,
) -> ExprId {
    match slot.strategy {
        ReturnSlotStrategy::Direct => alloc_local_var_expr(
            package,
            assigner,
            slot.var_id,
            return_ty.clone(),
            Span::default(),
        ),
        ReturnSlotStrategy::ArrayBacked => {
            let array_ty = Ty::Array(Box::new(return_ty.clone()));
            let array_expr =
                alloc_local_var_expr(package, assigner, slot.var_id, array_ty, Span::default());
            let zero = alloc_expr(
                package,
                assigner,
                Ty::Prim(Prim::Int),
                ExprKind::Lit(Lit::Int(0)),
                Span::default(),
            );
            alloc_expr(
                package,
                assigner,
                return_ty.clone(),
                ExprKind::Index(array_expr, zero),
                Span::default(),
            )
        }
    }
}

/// Builds a slot read that is safe to use without an enclosing flag guard.
///
/// * [`ReturnSlotStrategy::Direct`] returns the raw read; the initialized
///   default value makes an unguarded read well-typed.
/// * [`ReturnSlotStrategy::ArrayBacked`] returns
///   `if __has_returned { __ret_val[0] } else { fail "..." }`. The empty
///   initial array cannot be indexed, so the else branch fails with a typed
///   `Fail` expression rather than performing an out-of-bounds read.
///
/// Used by lazy continuation helpers that need a value-typed expression in
/// post-return suffixes where the surrounding flag is not yet established.
///
/// # Mutations
/// - Allocates new FIR nodes through `assigner` for the read, the optional
///   typed fail, and the wrapping `if` expression.
fn create_return_slot_read_or_fail_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    flag_context: &FlagContext<'_>,
) -> ExprId {
    match flag_context.return_slot.strategy {
        ReturnSlotStrategy::Direct => create_return_slot_read_expr(
            package,
            assigner,
            flag_context.return_slot,
            flag_context.return_ty,
        ),
        ReturnSlotStrategy::ArrayBacked => {
            let flag = alloc_local_var_expr(
                package,
                assigner,
                flag_context.has_returned_var_id,
                Ty::Prim(Prim::Bool),
                Span::default(),
            );
            let read = create_return_slot_read_expr(
                package,
                assigner,
                flag_context.return_slot,
                flag_context.return_ty,
            );
            let fail = create_typed_fail_expr(
                package,
                assigner,
                flag_context.return_ty,
                ARRAY_RETURN_SLOT_UNWRITTEN_FAIL_MESSAGE,
            );
            alloc_if_expr(
                package,
                assigner,
                flag,
                read,
                Some(fail),
                flag_context.return_ty.clone(),
                Span::default(),
            )
        }
    }
}

/// Builds the fallback expression used when the block has no fallthrough
/// trailing value and no return is known to have fired.
///
/// Reached only from [`create_flag_trailing_expr_for_slot`] in non-Unit return
/// types whose original body had no value-producing trailing expression. In
/// that situation, every code path either returns through the slot or is
/// unreachable, so the else branch of `if __has_returned { ... } else { ... }`
/// is statically dead.
///
/// * [`ReturnSlotStrategy::Direct`] reuses the initialized default value so
///   the dead branch stays well-typed without inserting a `Fail` node.
/// * [`ReturnSlotStrategy::ArrayBacked`] emits a typed `fail "..."` because
///   the slot's empty initial array cannot be indexed.
///
/// # Mutations
/// - Allocates new FIR nodes through `assigner` for the read or typed fail
///   expression.
fn create_return_slot_unwritten_fallback_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    slot: ReturnSlot,
    return_ty: &Ty,
) -> ExprId {
    match slot.strategy {
        ReturnSlotStrategy::Direct => {
            create_return_slot_read_expr(package, assigner, slot, return_ty)
        }
        ReturnSlotStrategy::ArrayBacked => create_typed_fail_expr(
            package,
            assigner,
            return_ty,
            ARRAY_RETURN_SLOT_UNWRITTEN_FAIL_MESSAGE,
        ),
    }
}

/// Builds a `fail "<message>"` expression typed as `ty`.
///
/// `Fail` is bottom-typed in Q# semantics, so it can take any expected type at
/// its use site. This helper packages the string literal and `Fail` node and
/// stamps the requested type onto the result. Used by array-backed return-slot
/// fallbacks where statically dead reads must remain well-typed.
///
/// # Mutations
/// - Allocates the string literal expression and the `Fail` expression
///   through `assigner`.
fn create_typed_fail_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    ty: &Ty,
    message: &str,
) -> ExprId {
    let message_expr = alloc_expr(
        package,
        assigner,
        Ty::Prim(Prim::String),
        ExprKind::String(vec![StringComponent::Lit(Rc::from(message))]),
        Span::default(),
    );
    alloc_expr(
        package,
        assigner,
        ty.clone(),
        ExprKind::Fail(message_expr),
        Span::default(),
    )
}

/// Rewrites a statement sequence using an existing flag pair.
///
/// The helper is shared by top-level flag rewriting, while bodies, nested block
/// expressions, and lazy suffix continuations. It guards statements after the
/// first return-bearing statement, and splits unsafe suffixes into a lazy
/// `if not __has_returned { ... }` continuation.
#[allow(clippy::too_many_lines)]
fn transform_block_stmts_with_flags(
    package: &mut Package,
    assigner: &mut Assigner,
    original_stmts: &[StmtId],
    flag_context: &FlagContext<'_>,
    arrow_default_cache: &mut ArrowDefaultCache,
    output: FlagBlockOutput,
) -> Vec<StmtId> {
    let mut new_stmts: Vec<StmtId> = Vec::new();
    let mut seen_return_bearing_stmt = false;

    for (index, &stmt_id) in original_stmts.iter().enumerate() {
        let has_return_in_while = match &package.get_stmt(stmt_id).kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) => contains_return_in_while_expr(package, *e),
            _ => false,
        };
        let has_return = contains_return_in_stmt(package, stmt_id);
        let is_final_trailing_expr = output.final_trailing_expr_strategy().is_some()
            && index == original_stmts.len() - 1
            && matches!(package.get_stmt(stmt_id).kind, StmtKind::Expr(_));

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
                output.clone(),
            );
            new_stmts.push(lazy_continuation);
            break;
        }

        if seen_return_bearing_stmt && is_final_trailing_expr {
            match output
                .final_trailing_expr_strategy()
                .expect("final trailing strategy should be set for value output")
            {
                FinalTrailingExprStrategy::Lazy => {
                    let lazy_continuation = create_lazy_flag_continuation_stmt(
                        package,
                        assigner,
                        &original_stmts[index..],
                        flag_context,
                        arrow_default_cache,
                        output.clone(),
                    );
                    new_stmts.push(lazy_continuation);
                    break;
                }
                FinalTrailingExprStrategy::Preserve if has_return => {
                    let lazy_continuation = create_lazy_flag_continuation_stmt(
                        package,
                        assigner,
                        &original_stmts[index..],
                        flag_context,
                        arrow_default_cache,
                        output.clone(),
                    );
                    new_stmts.push(lazy_continuation);
                    break;
                }
                FinalTrailingExprStrategy::Preserve => {
                    new_stmts.push(stmt_id);
                    continue;
                }
            }
        }

        if has_return_in_while {
            transform_while_stmt(
                package,
                assigner,
                stmt_id,
                flag_context,
                arrow_default_cache,
            );
            new_stmts.push(stmt_id);
            seen_return_bearing_stmt = true;
        } else if has_return && !seen_return_bearing_stmt {
            replace_returns_with_flags(
                package,
                assigner,
                stmt_id,
                flag_context,
                arrow_default_cache,
            );
            new_stmts.push(stmt_id);
            seen_return_bearing_stmt = true;
        } else if has_return {
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
        } else if seen_return_bearing_stmt {
            let guarded = guard_stmt_with_flag(
                package,
                assigner,
                flag_context,
                stmt_id,
                arrow_default_cache,
            );
            new_stmts.push(guarded);
        } else {
            new_stmts.push(stmt_id);
        }
    }

    new_stmts
}

/// Creates a lazy continuation statement with the shape required by `output`.
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
        output.clone(),
    );
    match output {
        FlagBlockOutput::ReturnValue { .. } | FlagBlockOutput::Value { .. } => {
            alloc_expr_stmt(package, assigner, lazy_continuation, Span::default())
        }
        FlagBlockOutput::Unit => {
            alloc_semi_stmt(package, assigner, lazy_continuation, Span::default())
        }
    }
}

/// Builds a lazy continuation expression for a post-return suffix.
///
/// Value-producing continuations use `__ret_val` as their else branch, while
/// Unit continuations omit the else branch. The suffix is recursively rewritten
/// with lazy final-tail handling so nested returns still update the shared flag
/// pair before control reaches the outer merge.
fn create_lazy_flag_continuation_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    continuation_stmts: &[StmtId],
    flag_context: &FlagContext<'_>,
    arrow_default_cache: &mut ArrowDefaultCache,
    output: FlagBlockOutput,
) -> ExprId {
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
            if !has_value_trailing_stmt(package, &continuation_stmts, flag_context.return_ty) {
                // The trailing expression (if any) doesn't match the return
                // type — typically a Unit-typed variable read left over from
                // `replace_qubit_allocation`. Drop it when it's a pure, side-
                // effect-free Expr so the fallback `__ret_val` read becomes
                // the sole trailing value, enabling `identical_branches` to
                // fold the degenerate merge afterward.
                if let Some(&last_id) = continuation_stmts.last()
                    && let StmtKind::Expr(e) = package.get_stmt(last_id).kind
                    && package.get_expr(e).ty == Ty::UNIT
                    && simplify::init_is_side_effect_free(package, e)
                {
                    continuation_stmts.pop();
                }
                let missing_value =
                    create_return_slot_read_or_fail_expr(package, assigner, flag_context);
                continuation_stmts.push(alloc_expr_stmt(
                    package,
                    assigner,
                    missing_value,
                    Span::default(),
                ));
            }

            let ret_var = create_return_slot_read_expr(
                package,
                assigner,
                flag_context.return_slot,
                flag_context.return_ty,
            );
            (flag_context.return_ty.clone(), Some(ret_var))
        }
        FlagBlockOutput::Value {
            value_ty, fallback, ..
        } => {
            if !has_value_trailing_stmt(package, &continuation_stmts, &value_ty) {
                let missing_value = create_value_fallback_expr(
                    package,
                    assigner,
                    flag_context,
                    arrow_default_cache,
                    &value_ty,
                    fallback,
                );
                continuation_stmts.push(alloc_expr_stmt(
                    package,
                    assigner,
                    missing_value,
                    Span::default(),
                ));
            }

            let else_expr = create_value_fallback_expr(
                package,
                assigner,
                flag_context,
                arrow_default_cache,
                &value_ty,
                fallback,
            );
            (value_ty, Some(else_expr))
        }
        FlagBlockOutput::Unit => (Ty::UNIT, None),
    };
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

/// Creates a well-typed value for a nested expression whose result is
/// unreachable because the shared return flag is already set.
fn create_value_fallback_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    flag_context: &FlagContext<'_>,
    arrow_default_cache: &mut ArrowDefaultCache,
    value_ty: &Ty,
    fallback: ValueFallback,
) -> ExprId {
    match fallback {
        ValueFallback::BoolFalse => alloc_bool_lit(package, assigner, false, Span::default()),
        ValueFallback::Default if value_ty == flag_context.return_ty => {
            create_return_slot_read_expr(
                package,
                assigner,
                flag_context.return_slot,
                flag_context.return_ty,
            )
        }
        ValueFallback::Default => create_default_value(
            package,
            assigner,
            flag_context.package_id,
            value_ty,
            flag_context.udt_pure_tys,
            arrow_default_cache,
        )
        .unwrap_or_else(|| {
            create_typed_fail_expr(
                package,
                assigner,
                value_ty,
                "return_unify nested value is unreachable after early return",
            )
        }),
    }
}

/// Returns true when the statement sequence already ends with a value of `return_ty`.
fn has_value_trailing_stmt(package: &Package, stmts: &[StmtId], return_ty: &Ty) -> bool {
    stmts.last().is_some_and(|&stmt_id| {
        matches!(
            package.get_stmt(stmt_id).kind,
            StmtKind::Expr(expr_id) if package.get_expr(expr_id).ty == *return_ty
        )
    })
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

            // Conjoin !__has_returned with the while condition.
            // LHS must be the flag guard so that AndL short-circuits and
            // skips the original condition once a return has fired.
            let not_flag = create_not_var_expr(package, assigner, flag_context.has_returned_var_id);
            let new_cond = {
                let op = BinOp::AndL;
                let ty: &Ty = &Ty::Prim(Prim::Bool);
                alloc_bin_op_expr(
                    package,
                    assigner,
                    op,
                    not_flag,
                    cond_id,
                    ty.clone(),
                    Span::default(),
                )
            };

            // Replace returns inside the body.
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
                        inner_expr_id,
                        flag_context,
                        arrow_default_cache,
                    );
                }
            }
        }
        ExprKind::If(_, then_id, else_opt) => {
            if contains_return_in_while_expr(package, *then_id) {
                transform_while_in_expr(
                    package,
                    assigner,
                    *then_id,
                    flag_context,
                    arrow_default_cache,
                );
            }
            if let Some(e) = *else_opt
                && contains_return_in_while_expr(package, e)
            {
                transform_while_in_expr(package, assigner, e, flag_context, arrow_default_cache);
            }
        }
        _ => {}
    }
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
        output.clone(),
    );
    let block = package.blocks.get_mut(block_id).expect("block not found");
    block.stmts = new_stmts;
    if matches!(output, FlagBlockOutput::Unit) {
        block.ty = Ty::UNIT;
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
/// - `flag_context` references the flag pair introduced by
///   [`transform_block_with_flags`].
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
            // Build: { __ret_val = val; __has_returned = true; }
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

            let flag_block = {
                let stmts = vec![assign_val_semi, assign_flag_semi];
                let ty: &Ty = &Ty::UNIT;
                alloc_block(package, assigner, stmts, ty.clone(), Span::default())
            };
            let flag_block_expr = {
                let ty: &Ty = &Ty::UNIT;
                alloc_block_expr(package, assigner, flag_block, ty.clone(), Span::default())
            };

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
            let output = if expr.ty == Ty::UNIT {
                FlagBlockOutput::Unit
            } else {
                FlagBlockOutput::Value {
                    value_ty: expr.ty.clone(),
                    final_trailing_expr_strategy: FinalTrailingExprStrategy::Preserve,
                    fallback: ValueFallback::Default,
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
        ExprKind::If(cond_id, then_id, else_opt) => {
            let cond_id = *cond_id;
            let then_id = *then_id;
            let else_id = *else_opt;
            let if_ty = expr.ty.clone();
            let cond_had_return = contains_return_in_expr(package, cond_id);
            if cond_had_return {
                replace_returns_in_value_expr(
                    package,
                    assigner,
                    cond_id,
                    flag_context,
                    arrow_default_cache,
                    &Ty::Prim(Prim::Bool),
                    ValueFallback::BoolFalse,
                );
            }
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
            if cond_had_return {
                guard_if_after_condition_return(
                    package,
                    assigner,
                    expr_id,
                    cond_id,
                    else_id,
                    flag_context,
                    arrow_default_cache,
                    &if_ty,
                );
            }
            resync_expr_ty_from_children(package, expr_id);
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
                // Delegate to `transform_while_in_expr` so the nested
                // while's condition gets conjoined with `not __has_returned`
                // and its body returns are rewritten, matching the
                // top-level while handling. Without this, a nested while
                // whose only exit is the return would loop forever after
                // the return-to-flag rewrite.
                transform_while_in_expr(
                    package,
                    assigner,
                    expr_id,
                    flag_context,
                    arrow_default_cache,
                );
            } else {
                // No returns reachable through this while; structural
                // recursion into the condition is sufficient (the body is
                // return-free so walking it is a no-op).
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

/// Rewrites return-bearing while-condition subexpressions to preserve
/// Bool-typed condition semantics under the flag strategy.
///
/// A condition-side `Return(v)` becomes:
/// `{ __ret_val = v; __has_returned = true; false }`
/// so the loop condition evaluates to false immediately after capturing
/// the return value.
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
        ExprKind::UnOp(UnOp::NotL, inner_id) => {
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

/// Rewrites a `Return(inner_id)` that appears inside a condition expression into
/// a block that records the return and yields `false`.
///
/// Before, evaluating the condition exits the callable directly. After, the
/// enclosing expression tree stays well-typed as `Bool`, but the block stores
/// the return value in `flag_context.return_slot`, sets `__has_returned`, and leaves
/// later guards to skip the rest of the computation.
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

    // Condition contexts still need a boolean value after the return is lowered
    // into side-effecting flag writes.
    let false_lit = alloc_bool_lit(package, assigner, false, Span::default());
    let false_stmt = alloc_expr_stmt(package, assigner, false_lit, Span::default());

    let flag_block = {
        let stmts = vec![assign_val_semi, assign_flag_semi, false_stmt];
        let ty: &Ty = &Ty::Prim(Prim::Bool);
        alloc_block(package, assigner, stmts, ty.clone(), Span::default())
    };
    let flag_block_expr = {
        let ty: &Ty = &Ty::Prim(Prim::Bool);
        alloc_block_expr(package, assigner, flag_block, ty.clone(), Span::default())
    };

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
    flag_context: &FlagContext<'_>,
    stmt_id: StmtId,
    arrow_default_cache: &mut ArrowDefaultCache,
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
            flag_context.package_id,
            &init_ty,
            flag_context.udt_pure_tys,
            arrow_default_cache,
            UnsupportedDefaultSite::GuardedLocalInitializer,
        );

        let not_flag = create_not_var_expr(package, assigner, flag_context.has_returned_var_id);

        let then_trailing = alloc_expr_stmt(package, assigner, init_expr_id, Span::default());
        let then_block = {
            let stmts = vec![then_trailing];
            let ty: &Ty = &init_ty;
            alloc_block(package, assigner, stmts, ty.clone(), Span::default())
        };
        let then_expr = {
            let ty: &Ty = &init_ty;
            alloc_block_expr(package, assigner, then_block, ty.clone(), Span::default())
        };

        let else_trailing = alloc_expr_stmt(package, assigner, default_val, Span::default());
        let else_block = {
            let stmts = vec![else_trailing];
            let ty: &Ty = &init_ty;
            alloc_block(package, assigner, stmts, ty.clone(), Span::default())
        };
        let else_expr = {
            let ty: &Ty = &init_ty;
            alloc_block_expr(package, assigner, else_block, ty.clone(), Span::default())
        };

        let if_expr = {
            let else_expr = Some(else_expr);
            let ty: &Ty = &init_ty;
            alloc_if_expr(
                package,
                assigner,
                not_flag,
                then_expr,
                else_expr,
                ty.clone(),
                Span::default(),
            )
        };

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
    let guard_block = {
        let stmts = vec![stmt_id];
        let ty: &Ty = &Ty::UNIT;
        alloc_block(package, assigner, stmts, ty.clone(), Span::default())
    };
    let guard_block_expr = {
        let ty: &Ty = &Ty::UNIT;
        alloc_block_expr(package, assigner, guard_block, ty.clone(), Span::default())
    };
    let if_expr = {
        let ty: &Ty = &Ty::UNIT;
        alloc_if_expr(
            package,
            assigner,
            not_flag,
            guard_block_expr,
            None,
            ty.clone(),
            Span::default(),
        )
    };
    alloc_semi_stmt(package, assigner, if_expr, Span::default())
}

/// Rewrites returns inside an expression that must still produce `value_ty`
/// when no callable return has fired.
fn replace_returns_in_value_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    expr_id: ExprId,
    flag_context: &FlagContext<'_>,
    arrow_default_cache: &mut ArrowDefaultCache,
    value_ty: &Ty,
    fallback: ValueFallback,
) {
    let expr = package.get_expr(expr_id).clone();
    match &expr.kind {
        ExprKind::Return(inner_id) => {
            let fallback_expr = create_value_fallback_expr(
                package,
                assigner,
                flag_context,
                arrow_default_cache,
                value_ty,
                fallback,
            );
            replace_return_with_flags_and_tail(
                package,
                assigner,
                expr_id,
                *inner_id,
                flag_context,
                fallback_expr,
                value_ty,
            );
        }
        ExprKind::Block(block_id) => {
            let output = FlagBlockOutput::Value {
                value_ty: expr.ty.clone(),
                final_trailing_expr_strategy: FinalTrailingExprStrategy::Lazy,
                fallback,
            };
            replace_returns_in_block(
                package,
                assigner,
                *block_id,
                flag_context,
                arrow_default_cache,
                output,
            );
            resync_expr_ty_from_children(package, expr_id);
        }
        ExprKind::If(cond_id, then_id, else_opt) => {
            let cond_id = *cond_id;
            let then_id = *then_id;
            let else_id = *else_opt;
            if contains_return_in_expr(package, cond_id) {
                replace_returns_in_value_expr(
                    package,
                    assigner,
                    cond_id,
                    flag_context,
                    arrow_default_cache,
                    &Ty::Prim(Prim::Bool),
                    ValueFallback::BoolFalse,
                );
                guard_if_after_condition_return(
                    package,
                    assigner,
                    expr_id,
                    cond_id,
                    else_id,
                    flag_context,
                    arrow_default_cache,
                    &expr.ty,
                );
            }
            replace_returns_in_value_expr(
                package,
                assigner,
                then_id,
                flag_context,
                arrow_default_cache,
                value_ty,
                fallback,
            );
            if let Some(else_id) = else_id {
                replace_returns_in_value_expr(
                    package,
                    assigner,
                    else_id,
                    flag_context,
                    arrow_default_cache,
                    value_ty,
                    fallback,
                );
            }
            resync_expr_ty_from_children(package, expr_id);
        }
        _ => replace_returns_in_expr(
            package,
            assigner,
            expr_id,
            flag_context,
            arrow_default_cache,
        ),
    }
}

fn replace_return_with_flags_and_tail(
    package: &mut Package,
    assigner: &mut Assigner,
    expr_id: ExprId,
    returned_expr_id: ExprId,
    flag_context: &FlagContext<'_>,
    tail_expr_id: ExprId,
    tail_ty: &Ty,
) {
    let span = package.get_expr(expr_id).span;
    let assign_val = create_return_slot_write_expr(
        package,
        assigner,
        flag_context.return_slot,
        returned_expr_id,
        flag_context.return_ty,
    );
    let assign_val_semi = alloc_semi_stmt(package, assigner, assign_val, Span::default());
    let flag_true = alloc_bool_lit(package, assigner, true, Span::default());
    let assign_flag = create_assign_expr(
        package,
        assigner,
        flag_context.has_returned_var_id,
        flag_true,
        &Ty::Prim(Prim::Bool),
    );
    let assign_flag_semi = alloc_semi_stmt(package, assigner, assign_flag, Span::default());
    let tail_stmt = alloc_expr_stmt(package, assigner, tail_expr_id, Span::default());
    let block_id = alloc_block(
        package,
        assigner,
        vec![assign_val_semi, assign_flag_semi, tail_stmt],
        tail_ty.clone(),
        Span::default(),
    );
    let replacement = alloc_block_expr(
        package,
        assigner,
        block_id,
        tail_ty.clone(),
        Span::default(),
    );
    let replacement = package.get_expr(replacement).clone();
    let expr = package.exprs.get_mut(expr_id).expect("expr not found");
    *expr = Expr {
        id: expr_id,
        span,
        ty: replacement.ty,
        kind: replacement.kind,
        exec_graph_range: EMPTY_EXEC_RANGE,
    };
}

fn guard_if_after_condition_return(
    package: &mut Package,
    assigner: &mut Assigner,
    if_expr_id: ExprId,
    cond_id: ExprId,
    else_id: Option<ExprId>,
    flag_context: &FlagContext<'_>,
    arrow_default_cache: &mut ArrowDefaultCache,
    if_ty: &Ty,
) {
    let not_flag = create_not_var_expr(package, assigner, flag_context.has_returned_var_id);
    let guarded_cond = alloc_bin_op_expr(
        package,
        assigner,
        BinOp::AndL,
        not_flag,
        cond_id,
        Ty::Prim(Prim::Bool),
        Span::default(),
    );

    let guarded_else = else_id.map(|else_expr| {
        let flag = alloc_local_var_expr(
            package,
            assigner,
            flag_context.has_returned_var_id,
            Ty::Prim(Prim::Bool),
            Span::default(),
        );
        let fallback = if if_ty == &Ty::UNIT {
            alloc_unit_expr(package, assigner, Span::default())
        } else {
            create_value_fallback_expr(
                package,
                assigner,
                flag_context,
                arrow_default_cache,
                if_ty,
                ValueFallback::Default,
            )
        };
        alloc_if_expr(
            package,
            assigner,
            flag,
            fallback,
            Some(else_expr),
            if_ty.clone(),
            Span::default(),
        )
    });

    if let ExprKind::If(cond, _, else_expr) = &mut package
        .exprs
        .get_mut(if_expr_id)
        .expect("if expr not found")
        .kind
    {
        *cond = guarded_cond;
        *else_expr = guarded_else;
    }
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
/// `__has_returned` condition is tested. This preserves the temporal ordering
/// between trailing-expression flag writes and the final merge.
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
#[cfg(test)]
fn create_flag_trailing_expr(
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
    create_flag_trailing_expr_for_slot(package, assigner, stmts, &flag_context)
}

/// Slot-aware implementation of [`create_flag_trailing_expr`].
///
/// Performs the same trailing-expression rewrite, but reads `__ret_val`
/// through the supplied [`ReturnSlot`] so both direct and array-backed slot
/// representations are supported.
///
/// See [`create_flag_trailing_expr`] for the high-level before/after shape.
/// The slot's strategy controls two construction details:
///
/// * The merge's `then` branch is read via [`create_return_slot_read_expr`],
///   which emits `__ret_val` for direct slots and `__ret_val[0]` for
///   array-backed slots (always reached under `__has_returned`, so the read
///   is safe).
/// * The non-Unit fallback when no trailing value survives is built by
///   [`create_return_slot_unwritten_fallback_expr`], which reuses the
///   direct slot's initialized default or emits a typed `fail` for
///   array-backed slots.
///
/// # Mutations
/// - Pops the original trailing expression statement off `stmts` when one is
///   present, and pushes the `let __trailing_result = ...` binding.
/// - Allocates new FIR nodes through `assigner` for the merge `if` and any
///   supporting reads, defaults, or fail expressions.
fn create_flag_trailing_expr_for_slot(
    package: &mut Package,
    assigner: &mut Assigner,
    stmts: &mut Vec<StmtId>,
    flag_context: &FlagContext<'_>,
) -> Option<StmtId> {
    // Check if the last statement is a value-producing trailing expression
    // for this callable, not just any expression statement. The flag rewrite
    // can turn all-returning non-Unit blocks into Unit expression statements;
    // those must not be rebound as `__trailing_result : T`.
    let trailing_expr = stmts.last().and_then(|&stmt_id| {
        if let StmtKind::Expr(expr_id) = package.get_stmt(stmt_id).kind
            && package.get_expr(expr_id).ty == *flag_context.return_ty
        {
            Some(expr_id)
        } else {
            None
        }
    });

    let flag_var = {
        let ty: &Ty = &Ty::Prim(Prim::Bool);
        alloc_local_var_expr(
            package,
            assigner,
            flag_context.has_returned_var_id,
            ty.clone(),
            Span::default(),
        )
    };
    let ret_var = create_return_slot_read_expr(
        package,
        assigner,
        flag_context.return_slot,
        flag_context.return_ty,
    );

    if let Some(original_trailing) = trailing_expr {
        // Pop the trailing expr and bind it to a local before the flag check.
        // This ensures that any flag assignments inside the trailing expression
        // evaluate before the `__has_returned` condition is tested.
        stmts.pop().expect("stmts should not be empty");

        // let __trailing_result : T = original_trailing;
        let (trailing_var_id, trailing_decl_stmt) = {
            let mutability = Mutability::Immutable;
            alloc_local_var(
                package,
                assigner,
                symbols::TRAILING_RESULT,
                flag_context.return_ty,
                original_trailing,
                mutability,
            )
        };
        stmts.push(trailing_decl_stmt);

        // if __has_returned { __ret_val } else { __trailing_result }
        let trailing_var_expr = alloc_local_var_expr(
            package,
            assigner,
            trailing_var_id,
            flag_context.return_ty.clone(),
            Span::default(),
        );
        let if_expr = {
            let else_expr = Some(trailing_var_expr);
            alloc_if_expr(
                package,
                assigner,
                flag_var,
                ret_var,
                else_expr,
                flag_context.return_ty.clone(),
                Span::default(),
            )
        };
        Some(alloc_expr_stmt(package, assigner, if_expr, Span::default()))
    } else {
        // No fallthrough value survives. Unit returns can keep the previous
        // explicit `()` fallback. For non-Unit returns, direct mode keeps its
        // initialized slot fallback and array-backed mode uses typed fail.
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
        let if_expr = {
            let else_expr = Some(fallback_expr);
            alloc_if_expr(
                package,
                assigner,
                flag_var,
                ret_var,
                else_expr,
                flag_context.return_ty.clone(),
                Span::default(),
            )
        };
        Some(alloc_expr_stmt(package, assigner, if_expr, Span::default()))
    }
}

/// Selects the representation for the flag strategy's synthesized return slot.
///
/// Choices in priority order:
///
/// | Condition                                              | Strategy                          |
/// |--------------------------------------------------------|-----------------------------------|
/// | `ty` has a classical default                           | [`ReturnSlotStrategy::Direct`]    |
/// | `ty` lacks a classical default but is resolvable       | [`ReturnSlotStrategy::ArrayBacked`] |
/// | `ty` has unresolved structure (`ArrowScan::Unknown`)   | `None`                            |
///
/// `None` signals to [`unify_returns`] that this callable cannot be rewritten
/// by the flag strategy and the user must see an unsupported-return-type
/// diagnostic.
///
/// Arrow-containing types are eligible for array-backed mode: the
/// synthesized `fail`-bodied default callable provides a well-typed
/// bottom value for the array read fallback, so arrays of callables
/// are handled correctly.
fn select_return_slot_strategy(
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    context: &UdtResolutionContext<'_>,
) -> Option<ReturnSlotStrategy> {
    if can_create_classical_default(ty, udt_pure_tys, context) {
        Some(ReturnSlotStrategy::Direct)
    } else if can_use_array_backed_return_slot(ty, udt_pure_tys, context) {
        Some(ReturnSlotStrategy::ArrayBacked)
    } else {
        None
    }
}

/// Returns true when a non-defaultable type can use an array-backed return slot.
///
/// The array-backed slot stores `T` values inside `T[]` whose default is `[]`,
/// so any type that doesn't otherwise have a classical default still gets a
/// well-typed initializer. Eligibility requires both:
///
/// 1. `ty` has no classical default (otherwise [`ReturnSlotStrategy::Direct`]
///    is preferred and this helper returns `false` so callers don't redundantly
///    select the array-backed shape).
/// 2. `ty` is resolvable per [`arrow_scan_for_ty`] (not
///    [`ArrowScan::Unknown`]). Arrow-containing types are accepted because
///    the cached `fail`-bodied callable provides a well-typed bottom value
///    for the array read fallback.
fn can_use_array_backed_return_slot(
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    context: &UdtResolutionContext<'_>,
) -> bool {
    !can_create_classical_default(ty, udt_pure_tys, context)
        && matches!(
            arrow_scan_for_ty(ty, udt_pure_tys, context, &mut FxHashSet::default()),
            ArrowScan::NoArrow | ArrowScan::ContainsArrow
        )
}

/// Conservatively scans a type for nested arrows.
///
/// Walks tuples, arrays, and UDTs (via their pure types) looking for
/// [`Ty::Arrow`] leaves. Results follow a three-way lattice ordered
/// [`ArrowScan::ContainsArrow`] > [`ArrowScan::Unknown`] > [`ArrowScan::NoArrow`],
/// combined by [`ArrowScan::combine`] so any unresolved or arrow-bearing
/// sub-type forces the overall scan toward the more conservative result.
///
/// UDT recursion is broken using `visiting_udts`: a recursive cycle returns
/// [`ArrowScan::Unknown`] rather than recursing indefinitely. Unresolved UDTs
/// (missing pure types) also return [`ArrowScan::Unknown`], which causes
/// [`can_use_array_backed_return_slot`] to reject the type so the flag
/// strategy degrades gracefully into an unsupported-return-type diagnostic.
fn arrow_scan_for_ty(
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    context: &UdtResolutionContext<'_>,
    visiting_udts: &mut FxHashSet<(PackageId, LocalItemId)>,
) -> ArrowScan {
    match ty {
        Ty::Arrow(_) => ArrowScan::ContainsArrow,
        Ty::Array(elem_ty) => arrow_scan_for_ty(elem_ty, udt_pure_tys, context, visiting_udts),
        Ty::Tuple(elems) => elems.iter().fold(ArrowScan::NoArrow, |scan, elem_ty| {
            scan.combine(arrow_scan_for_ty(
                elem_ty,
                udt_pure_tys,
                context,
                visiting_udts,
            ))
        }),
        Ty::Udt(Res::Item(item_id)) => {
            let key = (item_id.package, item_id.item);
            if !visiting_udts.insert(key) {
                return ArrowScan::Unknown;
            }

            let scan = context
                .resolve_udt_pure_ty(udt_pure_tys, *item_id)
                .map_or(ArrowScan::Unknown, |pure_ty| {
                    arrow_scan_for_ty(&pure_ty, udt_pure_tys, context, visiting_udts)
                });
            visiting_udts.remove(&key);
            scan
        }
        Ty::Prim(_) => ArrowScan::NoArrow,
        Ty::Infer(_) | Ty::Param(_) | Ty::Err | Ty::Udt(_) => ArrowScan::Unknown,
    }
}

/// Checks whether a guarded local initializer can be synthesized eagerly.
///
/// This uses the policy context for the currently rewritten package so UDTs
/// that appear only in continuation locals can still be resolved lazily.
fn can_create_guarded_local_default(
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    context: &UdtResolutionContext<'_>,
) -> bool {
    can_create_classical_default(ty, udt_pure_tys, context)
}

/// Checks whether `ty` has a classical default in the given UDT resolution context.
fn can_create_classical_default(
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    context: &UdtResolutionContext<'_>,
) -> bool {
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
            .all(|e| can_create_classical_default(e, udt_pure_tys, context)),
        Ty::Udt(Res::Item(item_id)) => context
            .resolve_udt_pure_ty(udt_pure_tys, *item_id)
            .is_some_and(|pure_ty| can_create_classical_default(&pure_ty, udt_pure_tys, context)),
        // Arrow types always have a classical default: the fail-bodied
        // callable synthesized by `synthesize_fail_callable`. The body is
        // `fail "callable init expr"`, so no recursive output-type default
        // is needed. The only exclusion is non-Value functors, which should
        // not appear post-monomorphization.
        Ty::Arrow(arrow) => matches!(arrow.functors, qsc_fir::ty::FunctorSet::Value(_)),
        Ty::Infer(_) | Ty::Param(_) | Ty::Err | Ty::Prim(Prim::Qubit) | Ty::Udt(_) => false,
    }
}

/// Safety classification for keeping a continuation local behind an eager guard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContinuationSafety {
    /// The type can be guarded in place without changing quantum lifetime behavior.
    Safe,
    /// The type contains quantum state and must be moved into a lazy continuation.
    SplitRequired,
    /// The type could not be resolved; split conservatively.
    Unknown,
}

impl ContinuationSafety {
    /// Combines two continuation-safety classifications for compound types.
    fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::SplitRequired, _) | (_, Self::SplitRequired) => Self::SplitRequired,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::Safe, Self::Safe) => Self::Safe,
        }
    }

    /// Returns true when the suffix must be moved into a lazy continuation.
    fn requires_split(self) -> bool {
        !matches!(self, Self::Safe)
    }
}

/// Classify whether a continuation suffix type can be guarded in place.
fn continuation_safety_for_ty(
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    context: &UdtResolutionContext<'_>,
) -> ContinuationSafety {
    match ty {
        Ty::Prim(Prim::Qubit) => ContinuationSafety::SplitRequired,
        Ty::Array(elem_ty) => continuation_safety_for_ty(elem_ty, udt_pure_tys, context),
        Ty::Tuple(elems) => elems
            .iter()
            .fold(ContinuationSafety::Safe, |safety, elem_ty| {
                safety.combine(continuation_safety_for_ty(elem_ty, udt_pure_tys, context))
            }),
        Ty::Udt(Res::Item(item_id)) => context
            .resolve_udt_pure_ty(udt_pure_tys, *item_id)
            .map_or(ContinuationSafety::Unknown, |pure_ty| {
                continuation_safety_for_ty(&pure_ty, udt_pure_tys, context)
            }),
        Ty::Arrow(_) | Ty::Infer(_) | Ty::Param(_) | Ty::Prim(_) | Ty::Udt(_) | Ty::Err => {
            ContinuationSafety::Safe
        }
    }
}

/// Returns true when a type's continuation value requires lazy suffix splitting.
fn continuation_ty_requires_split(
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    context: &UdtResolutionContext<'_>,
) -> bool {
    continuation_safety_for_ty(ty, udt_pure_tys, context).requires_split()
}

/// Returns true when a local statement cannot be guarded eagerly after a return.
///
/// Non-defaultable initializers and quantum-containing local or initializer
/// types are moved into a lazy continuation so they are never evaluated after
/// `__has_returned` is set.
fn local_initializer_requires_split_continuation(
    package: &Package,
    stmt_id: StmtId,
    package_id: PackageId,
    udt_pure_tys: &UdtPureTyCache,
) -> bool {
    if let StmtKind::Local(_, pat_id, init_expr_id) = package.get_stmt(stmt_id).kind {
        let local_ty = &package.get_pat(pat_id).ty;
        let init_ty = &package.get_expr(init_expr_id).ty;
        let context = UdtResolutionContext::Package {
            package_id,
            package,
        };

        !can_create_guarded_local_default(init_ty, udt_pure_tys, &context)
            || continuation_ty_requires_split(local_ty, udt_pure_tys, &context)
            || continuation_ty_requires_split(init_ty, udt_pure_tys, &context)
    } else {
        false
    }
}

/// Scans a statement suffix for locals that require lazy continuation splitting.
fn continuation_suffix_requires_split(
    package: &Package,
    original_stmts: &[StmtId],
    index: usize,
    package_id: PackageId,
    udt_pure_tys: &UdtPureTyCache,
) -> bool {
    original_stmts.get(index..).is_some_and(|suffix| {
        suffix.iter().any(|&stmt_id| {
            local_initializer_requires_split_continuation(
                package,
                stmt_id,
                package_id,
                udt_pure_tys,
            )
        })
    })
}

/// Synthesis site used in unsupported-default contract diagnostics.
#[derive(Clone, Copy, Debug)]
enum UnsupportedDefaultSite {
    /// Default needed for the synthesized `__ret_val` return slot.
    ReturnSlot,
    /// Default needed when guarding a local initializer in place.
    GuardedLocalInitializer,
}

impl UnsupportedDefaultSite {
    /// Human-readable description included in contract-violation panic messages.
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
/// which emits a user-facing unsupported-return-type diagnostic.
/// This panic remains as a safety net for unforeseen cases.
fn require_classical_default(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    arrow_default_cache: &mut ArrowDefaultCache,
    site: UnsupportedDefaultSite,
) -> ExprId {
    create_default_value(
        package,
        assigner,
        package_id,
        ty,
        udt_pure_tys,
        arrow_default_cache,
    )
    .unwrap_or_else(|| {
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
    arrow_default_cache: &mut ArrowDefaultCache,
) -> Option<ExprId> {
    let kind = create_default_value_kind(
        package,
        assigner,
        package_id,
        ty,
        udt_pure_tys,
        arrow_default_cache,
    )?;

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
/// ExprKind::{Lit(..), Tuple([defaults..]), Array([]), Var(Res::Item(fail_callable), []), ...}
/// ```
/// # Requires
/// - `ty` is a type reachable from the callable's return type.
/// - `udt_pure_tys` has been populated from the store.
/// - `package_id` is the id of the package owning `package` — the synthesized
///   fail-bodied callable for arrow types is inserted there and referenced through it.
///
/// # Ensures
/// - Returns `None` when the type has no synthesizable classical default:
///   unresolved types (`Ty::Infer`, `Ty::Param`, `Ty::Err`), qubits
///   (`Prim::Qubit`), and UDTs whose pure-ty cache entry is
///   missing or unresolved.
/// - Returns `Some(kind)` whose zero value matches `ty` structurally.
/// - For `Ty::Arrow`, `Some(Var(Res::Item(fail_item), vec![]))` references a
///   synthesized fail-bodied callable of the same arrow signature; its body
///   is `fail "callable init expr"`. Any later `Call` on the resulting
///   `__ret_val` value resolves to that fail callable — correct behavior
///   because the flag guard ensures reads only occur when an explicit return
///   already overwrote `__ret_val` with the real callable.
///
/// # Mutations
/// - For `Ty::Tuple` and `Ty::Udt` composites, allocates nested default
///   `Expr` nodes through `assigner` via [`create_default_value`].
/// - For `Ty::Arrow`, inserts a new `Item` (callable) into `package.items`
///   via [`ArrowDefaultCache`] and allocates its body nodes.
fn create_default_value_kind(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    arrow_default_cache: &mut ArrowDefaultCache,
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
                    create_default_value(
                        package,
                        assigner,
                        package_id,
                        elem_ty,
                        udt_pure_tys,
                        arrow_default_cache,
                    )
                })
                .collect::<Option<_>>()?;
            Some(ExprKind::Tuple(elem_exprs))
        }
        Ty::Array(_) => Some(ExprKind::Array(Vec::new())),
        Ty::Udt(Res::Item(item_id)) => {
            let pure_ty = udt_pure_tys.resolve_from_package(package_id, package, *item_id)?;
            create_default_value_kind(
                package,
                assigner,
                package_id,
                &pure_ty,
                udt_pure_tys,
                arrow_default_cache,
            )
        }
        Ty::Arrow(arrow) => {
            // After monomorphization, non-Value functors should not appear in
            // reachable return types; surface this as a missing default rather
            // than a panic so the pass bails deterministically.
            let qsc_fir::ty::FunctorSet::Value(functors) = arrow.functors else {
                return None;
            };
            let item_id = arrow_default_cache.get_or_insert(
                package,
                assigner,
                arrow.kind,
                &arrow.input,
                &arrow.output,
                functors,
            );
            Some(ExprKind::Var(
                Res::Item(ItemId {
                    package: package_id,
                    item: item_id,
                }),
                Vec::new(),
            ))
        }
        Ty::Prim(Prim::Range | Prim::RangeFrom | Prim::RangeTo | Prim::RangeFull) => {
            Some(ExprKind::Range(None, None, None))
        }
        // No well-typed classical default: unresolved/placeholder types,
        // qubits and unresolved UDTs.
        Ty::Infer(_) | Ty::Param(_) | Ty::Err | Ty::Prim(Prim::Qubit) | Ty::Udt(_) => None,
    }
}

/// Read-only check whether `ty` has a synthesizable classical default.
///
/// Mirrors the defaultability logic of [`create_default_value_kind`] without
/// allocating any FIR nodes, so it can be called on `&Package`.
///
/// Arrow types are considered defaultable because the [`ArrowDefaultCache`]
/// synthesizes a fail-bodied callable at the actual allocation site.
fn is_type_defaultable(package: &Package, package_id: PackageId, ty: &Ty) -> bool {
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
        ) => true,
        Ty::Tuple(elems) => elems
            .iter()
            .all(|e| is_type_defaultable(package, package_id, e)),
        Ty::Array(_) => true,
        Ty::Arrow(_) => true,
        Ty::Udt(Res::Item(item_id)) => {
            if item_id.package != package_id {
                return false;
            }
            let Some(item) = package.items.get(item_id.item) else {
                return false;
            };
            let ItemKind::Ty(_, udt) = &item.kind else {
                return false;
            };
            is_type_defaultable(package, package_id, &udt.get_pure_ty())
        }
        Ty::Prim(Prim::Qubit) | Ty::Infer(_) | Ty::Param(_) | Ty::Err | Ty::Udt(_) => false,
    }
}

/// Pre-check whether the normalize phase can run without panicking on
/// `block_id`.
///
/// Scans the reachable expression tree for two patterns that would cause
/// the normalize phase to panic when it cannot synthesize a classical
/// default:
///
/// 1. An `If` expression whose condition contains a `Return` and whose
///    type is non-Unit and non-defaultable (would panic in
///    [`normalize::hoist_in_cond`]).
/// 2. A `Local` statement whose initializer contains a `Return` and whose
///    pattern type is non-defaultable (would panic in
///    [`normalize::replace_local_init_with_default_and_emit`]).
///
/// For each found, pushes [`Error::UnsupportedHoistContext`]. The caller
/// skips normalize+transform when any non-warning error is emitted.
fn check_normalize_supportable(
    package: &Package,
    package_id: PackageId,
    block_id: BlockId,
    errors: &mut Vec<Error>,
) {
    let mut seen = FxHashSet::default();
    scan_block_for_unsupported_hoist(package, package_id, block_id, errors, &mut seen);
}

fn scan_block_for_unsupported_hoist(
    package: &Package,
    package_id: PackageId,
    block_id: BlockId,
    errors: &mut Vec<Error>,
    seen: &mut FxHashSet<BlockId>,
) {
    if !seen.insert(block_id) {
        return;
    }
    let block = package.get_block(block_id);
    for &stmt_id in &block.stmts {
        scan_stmt_for_unsupported_hoist(package, package_id, stmt_id, errors, seen);
    }
}

fn scan_stmt_for_unsupported_hoist(
    package: &Package,
    package_id: PackageId,
    stmt_id: StmtId,
    errors: &mut Vec<Error>,
    seen: &mut FxHashSet<BlockId>,
) {
    let stmt = package.get_stmt(stmt_id);
    match &stmt.kind {
        StmtKind::Expr(e) | StmtKind::Semi(e) => {
            scan_expr_for_unsupported_hoist(package, package_id, *e, errors, seen);
        }
        StmtKind::Local(_, pat_id, init_id) => {
            if detect::contains_return_in_expr(package, *init_id) {
                let pat_ty = &package.get_pat(*pat_id).ty;
                if !is_type_defaultable(package, package_id, pat_ty) {
                    errors.push(Error::UnsupportedHoistContext {
                        enclosing_ty: format!("{pat_ty}"),
                        span: package.get_expr(*init_id).span,
                    });
                }
            }
            scan_expr_for_unsupported_hoist(package, package_id, *init_id, errors, seen);
        }
        StmtKind::Item(_) => {}
    }
}

fn scan_expr_for_unsupported_hoist(
    package: &Package,
    package_id: PackageId,
    expr_id: ExprId,
    errors: &mut Vec<Error>,
    seen: &mut FxHashSet<BlockId>,
) {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::If(cond, then_id, else_id) => {
            if detect::contains_return_in_expr(package, *cond)
                && expr.ty != Ty::UNIT
                && !is_type_defaultable(package, package_id, &expr.ty)
            {
                errors.push(Error::UnsupportedHoistContext {
                    enclosing_ty: format!("{}", expr.ty),
                    span: expr.span,
                });
            }
            scan_expr_for_unsupported_hoist(package, package_id, *cond, errors, seen);
            scan_expr_for_unsupported_hoist(package, package_id, *then_id, errors, seen);
            if let Some(else_id) = else_id {
                scan_expr_for_unsupported_hoist(package, package_id, *else_id, errors, seen);
            }
        }
        ExprKind::Block(block_id) => {
            scan_block_for_unsupported_hoist(package, package_id, *block_id, errors, seen);
        }
        ExprKind::While(cond, body) => {
            scan_expr_for_unsupported_hoist(package, package_id, *cond, errors, seen);
            scan_block_for_unsupported_hoist(package, package_id, *body, errors, seen);
        }
        ExprKind::Return(inner) => {
            scan_expr_for_unsupported_hoist(package, package_id, *inner, errors, seen);
        }
        ExprKind::Fail(e) | ExprKind::Field(e, _) | ExprKind::UnOp(_, e) => {
            scan_expr_for_unsupported_hoist(package, package_id, *e, errors, seen);
        }
        ExprKind::ArrayRepeat(a, b)
        | ExprKind::Assign(a, b)
        | ExprKind::AssignOp(_, a, b)
        | ExprKind::BinOp(_, a, b)
        | ExprKind::Call(a, b)
        | ExprKind::Index(a, b)
        | ExprKind::AssignField(a, _, b)
        | ExprKind::UpdateField(a, _, b) => {
            scan_expr_for_unsupported_hoist(package, package_id, *a, errors, seen);
            scan_expr_for_unsupported_hoist(package, package_id, *b, errors, seen);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            scan_expr_for_unsupported_hoist(package, package_id, *a, errors, seen);
            scan_expr_for_unsupported_hoist(package, package_id, *b, errors, seen);
            scan_expr_for_unsupported_hoist(package, package_id, *c, errors, seen);
        }
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            for &e in exprs {
                scan_expr_for_unsupported_hoist(package, package_id, e, errors, seen);
            }
        }
        ExprKind::Range(start, step, end) => {
            for e in [start, step, end].into_iter().flatten() {
                scan_expr_for_unsupported_hoist(package, package_id, *e, errors, seen);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(c) = copy {
                scan_expr_for_unsupported_hoist(package, package_id, *c, errors, seen);
            }
            for fa in fields {
                scan_expr_for_unsupported_hoist(package, package_id, fa.value, errors, seen);
            }
        }
        ExprKind::String(components) => {
            for c in components {
                if let StringComponent::Expr(e) = c {
                    scan_expr_for_unsupported_hoist(package, package_id, *e, errors, seen);
                }
            }
        }
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
    }
}

/// Cache key for synthesized fail-bodied callables.
///
/// `Ty` does not implement `Hash`, so the key uses a `String` representation
/// of the arrow signature built from `Display` of the input and output types.
/// Functors MUST be part of the key — an `is Adj` arrow and an `is Adj+Ctl`
/// arrow are distinct types.
type ArrowDefaultKey = (
    qsc_fir::fir::CallableKind,
    String,
    qsc_fir::ty::FunctorSetValue,
);

/// Caches fail-bodied callables synthesized for arrow-typed default values.
///
/// Each unique arrow signature (kind × input × output × functors) maps to
/// a single synthesized callable whose body is `fail "callable init expr"`.
/// Re-using the same item across multiple default-value sites avoids
/// inflating the package item table with duplicates.
#[derive(Default)]
pub(crate) struct ArrowDefaultCache {
    items: FxHashMap<ArrowDefaultKey, LocalItemId>,
}

impl ArrowDefaultCache {
    fn get_or_insert(
        &mut self,
        package: &mut Package,
        assigner: &mut Assigner,
        kind: qsc_fir::fir::CallableKind,
        input_ty: &Ty,
        output_ty: &Ty,
        functors: qsc_fir::ty::FunctorSetValue,
    ) -> LocalItemId {
        let key = (kind, format!("{input_ty} -> {output_ty}"), functors);
        if let Some(&id) = self.items.get(&key) {
            return id;
        }
        let new_id =
            synthesize_fail_callable(package, assigner, kind, input_ty, output_ty, functors);
        self.items.insert(key, new_id);
        new_id
    }
}

/// Synthesize a fail-bodied callable matching an arrow signature.
///
/// The callable's body is `fail "callable init expr"` — it exists only to
/// give the compiler a well-typed default value for arrow-typed return slots.
/// The slot is never read before being overwritten, so the fail is
/// unreachable at runtime. Later
fn synthesize_fail_callable(
    package: &mut Package,
    assigner: &mut Assigner,
    kind: qsc_fir::fir::CallableKind,
    input_ty: &Ty,
    output_ty: &Ty,
    functors: qsc_fir::ty::FunctorSetValue,
) -> LocalItemId {
    // Build `fail "callable init expr"`
    let msg_expr_id = alloc_expr(
        package,
        assigner,
        Ty::Prim(Prim::String),
        ExprKind::String(vec![StringComponent::Lit("callable init expr".into())]),
        Span::default(),
    );
    let fail_expr_id = alloc_expr(
        package,
        assigner,
        output_ty.clone(),
        ExprKind::Fail(msg_expr_id),
        Span::default(),
    );
    let trailing_stmt = alloc_expr_stmt(package, assigner, fail_expr_id, Span::default());
    let body_block = alloc_block(
        package,
        assigner,
        vec![trailing_stmt],
        output_ty.clone(),
        Span::default(),
    );

    // Input pattern: a typed Discard matching the arrow's input type.
    let input_pat_id = assigner.next_pat();
    package.pats.insert(
        input_pat_id,
        Pat {
            id: input_pat_id,
            span: Span::default(),
            ty: input_ty.clone(),
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

    let new_item_id = assigner.next_item();
    let callable_name: Rc<str> = Rc::from(format!("__return_unify_fail_{new_item_id}"));
    let decl = CallableDecl {
        id: assigner.next_node(),
        span: Span::default(),
        kind,
        name: Ident {
            id: LocalVarId::from(0_u32),
            span: Span::default(),
            name: callable_name,
        },
        generics: Vec::new(),
        input: input_pat_id,
        output: output_ty.clone(),
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

    new_item_id
}

/// Create `not Var(__has_returned)`.
fn create_not_var_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    var_id: LocalVarId,
) -> ExprId {
    let var = {
        let ty: &Ty = &Ty::Prim(Prim::Bool);
        alloc_local_var_expr(package, assigner, var_id, ty.clone(), Span::default())
    };
    alloc_not_expr(package, assigner, var, Span::default())
}

/// Create `Assign(Var(var_id), value)`.
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

/// Create a mutable boolean variable declaration: `mutable name = value`.
/// Returns `(LocalVarId, StmtId)`.
fn create_mutable_bool_var(
    package: &mut Package,
    assigner: &mut Assigner,
    name: &str,
    value: bool,
) -> (LocalVarId, StmtId) {
    let init_expr = alloc_bool_lit(package, assigner, value, Span::default());
    {
        let ty: &Ty = &Ty::Prim(Prim::Bool);
        {
            let mutability = Mutability::Mutable;
            alloc_local_var(package, assigner, name, ty, init_expr, mutability)
        }
    }
}
