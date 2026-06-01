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
//! This pass implements the "flag-lowering everywhere" design: every
//! return-bearing block is lowered through a uniform mutable-flag and return-slot
//! scaffolding, then simplified by a named rewrite catalogue in [`simplify`].
//! This is the selected semantic baseline: normalize to statement boundaries,
//! lower through the flag/slot model, and recover structured output only via
//! [`simplify::run_to_fixpoint`].
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
//!    Apply semantic flag lowering to eliminate all `Return` nodes by introducing
//!    `__has_returned` and `__ret_val` mutable slots. This corresponds to
//!    LLVM's `UnifyFunctionExitNodes` / `mergereturn` lowering for early
//!    returns: every return path writes the slot and the flag, and a single
//!    merge expression at the tail reads them.
//!
//! 3. **Simplify** ([`simplify::run_to_fixpoint`]):
//!    After semantic flag lowering, run a named rewrite catalogue
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
//! user-reachable error is [`Error::UnsupportedEarlyReturnType`]: flag
//! lowering cannot synthesize a return slot for unresolved or unsupported
//! return types. Defaultable return types use a direct `__ret_val : T` slot;
//! non-defaultable return types with resolvable structure use an array-backed
//! `T[]` slot. Unsupported shapes produce a user-facing diagnostic, and
//! processing continues for remaining callables.
//!
//! # Qubit release interaction
//!
//! Qubit-release handling is intrinsic to `return_unify`; the historical
//! `release_hoist` pre-pass was folded in.

mod continuation;
mod detect;
mod lower;
mod normalize;
mod simplify;
mod slot;
mod symbols;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "slow-proptest-tests"))]
mod semantic_equivalence_tests;

use crate::fir_builder::functored_specs;
use miette::Diagnostic;
use qsc_data_structures::span::Span;
use qsc_fir::{
    assigner::Assigner,
    fir::{
        BlockId, CallableDecl, CallableImpl, ExprId, ExprKind, ItemId, ItemKind, Package,
        PackageId, PackageLookup, PackageStore, Res, StmtId, StmtKind, StoreItemId,
        StringComponent,
    },
    ty::Ty,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use thiserror::Error;

use crate::reachability::collect_reachable_from_entry;
use detect::contains_return_in_block;
use lower::transform_block_with_flags;
use slot::{ArrowDefaultCache, is_type_defaultable, select_return_slot_strategy};

#[cfg(test)]
use lower::{FlagContext, create_flag_trailing_expr, guard_stmt_with_flag};
#[cfg(test)]
use slot::{ReturnSlot, ReturnSlotStrategy, can_use_array_backed_return_slot};

/// Errors that can occur during return unification.
#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    /// Return-slot selection could not prove that either Direct or
    /// `ArrayBacked` lowering is valid for this return type.
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
    pure_tys: RefCell<FxHashMap<StoreItemId, Ty>>,
}

impl UdtPureTyCache {
    /// Creates a cache from precomputed UDT pure types.
    fn new(pure_tys: FxHashMap<StoreItemId, Ty>) -> Self {
        Self {
            pure_tys: RefCell::new(pure_tys),
        }
    }

    /// Gets a cached pure type for a UDT item, if it has already been resolved.
    fn get(&self, item_id: ItemId) -> Option<Ty> {
        self.pure_tys
            .borrow()
            .get(&(item_id.package, item_id.item).into())
            .cloned()
    }

    /// Inserts a resolved pure type into the cache.
    fn insert(&self, item_id: ItemId, pure_ty: Ty) {
        self.pure_tys
            .borrow_mut()
            .insert((item_id.package, item_id.item).into(), pure_ty);
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
/// records their `StoreItemId` identity in `refs`.
fn collect_udt_refs_from_ty(ty: &Ty, refs: &mut FxHashSet<StoreItemId>) {
    match ty {
        Ty::Udt(Res::Item(item_id)) => {
            refs.insert((item_id.package, item_id.item).into());
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
    let mut needed_udts: FxHashSet<StoreItemId> = FxHashSet::default();
    for item_id in reachable {
        let pkg = store.get(item_id.package);
        let item = pkg.get_item(item_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            collect_udt_refs_from_ty(&decl.output, &mut needed_udts);
        }
    }
    let mut cache = FxHashMap::default();
    for store_item_id in &needed_udts {
        let pkg = store.get(store_item_id.package);
        let item = pkg.get_item(store_item_id.item);
        if let ItemKind::Ty(_, udt) = &item.kind {
            cache.insert(*store_item_id, udt.get_pure_ty());
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
///   return value via semantic flag lowering followed by the [`simplify`]
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
/// The user-reachable variant is [`Error::UnsupportedEarlyReturnType`], emitted
/// when flag lowering cannot select a return-slot representation for the
/// callable's return type. Non-defaultable types with resolvable structure use
/// an array-backed slot, including mixed Qubit/callable shapes; unresolved or
/// otherwise unsupported shapes are left unchanged after the diagnostic.
//
// Only entry-reachable callables are unified. Unreachable callables retain
// their `Return` nodes, but this is safe because:
// 1. `check_no_returns` walks the same reachable set returned by
//    [`collect_reachable_from_entry`].
// 2. Downstream passes (defunc, udt_erase, tuple_decompose, arg_promote,
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
            // statement boundary so flag lowering only sees bare returns or
            // returns inside statement-carrying Block/If/While.
            normalize::hoist_returns_to_statement_boundary(
                store.get_mut(package_id),
                assigner,
                package_id,
                block_id,
                &mut errors,
            );

            let return_slot_strategy = {
                let context = UdtResolutionContext::Store(store);
                select_return_slot_strategy(&return_ty, &udt_pure_tys, &context)
            };

            let Some(return_slot_strategy) = return_slot_strategy else {
                errors.push(Error::UnsupportedEarlyReturnType(
                    format!("{return_ty}"),
                    callable.name.span,
                ));
                continue;
            };

            let package = store.get_mut(package_id);
            let slots = transform_block_with_flags(
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
                simplify::run_to_fixpoint(package, assigner, block_id, &mut errors, &slots);
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

const ARRAY_RETURN_SLOT_UNWRITTEN_FAIL_MESSAGE: &str =
    "return_unify array return slot was not written";

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
