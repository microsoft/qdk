// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Return unification pass — runs after monomorphization, before
//! defunctionalization.
//!
//! Eliminates every `ExprKind::Return` in reachable callable bodies so each
//! callable has a single exit point: the trailing expression of its top-level
//! block. Unreachable callables are left untouched (see the rationale above
//! [`unify_returns`]).
//!
//! # What to know before diving in
//!
//! - **Establishes [`crate::invariants::InvariantLevel::PostReturnUnify`]:**
//!   no `Return` nodes and no non-Unit `Semi`-terminated block tails in
//!   reachable code, with consistent `LocalVarId` binding.
//! - **"Flag-lowering everywhere" design.** Because FIR is a tree IR, returns
//!   are lowered into a `__has_returned` boolean flag plus a `__ret_val` slot
//!   (standing in for LLVM's PHI nodes), then structure is recovered by named,
//!   individually
//!   tested rewrite rules. Three phases per block: **Normalize** first hoists
//!   compound-position returns to statement boundaries
//!   ([`normalize::hoist_returns_to_statement_boundary`]), then lifts any
//!   returns still buried in operand positions into spine `let` temps via a
//!   standalone ANF fixpoint ([`normalize::run_anf_to_fixpoint`]); **Transform**
//!   ([`transform_block_with_flags`]) eliminates returns via the flag/slot;
//!   **Simplify** ([`simplify::run_to_fixpoint`]) folds the canonical shapes
//!   back into structured form.
//! - **Callable arity is preserved.** RCA depends on it: flag/slot allocations
//!   are body-local `Local` bindings, never new top-level parameters.
//! - **Error handling, not panics.** Returns `Vec<Error>`; the user-reachable
//!   case is [`Error::UnsupportedEarlyReturnType`] (no return slot can be
//!   synthesized for unsupported types — defaultable types use a `T` slot,
//!   resolvable non-defaultable types use a `T[]` slot). Processing continues
//!   for the remaining callables.
//! - **Qubit release is folded in.**
//! - Synthesized expressions use `EMPTY_EXEC_RANGE`;
//!   [`crate::exec_graph_rebuild`] repairs exec graphs later.

mod continuation;
mod detect;
mod lower;
mod normalize;
mod simplify;
mod slot;
pub(crate) mod symbols;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "slow-proptest-tests"))]
mod semantic_equivalence_tests;

use crate::fir_builder::functored_specs;
use crate::package_assigners::PackageAssigners;
use miette::Diagnostic;
use qsc_data_structures::span::Span;
use qsc_fir::{
    assigner::Assigner,
    fir::{
        BlockId, CallableDecl, CallableImpl, ExprKind, ItemId, ItemKind, LocalItemId, Package,
        PackageId, PackageLookup, PackageStore, Res, StmtKind, StoreItemId,
    },
    ty::Ty,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use thiserror::Error;

use crate::reachability::{collect_reachable_from_entry, collect_reachable_with_seeds};
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
    #[diagnostic(severity(Warning))]
    #[diagnostic(help(
        "the return type has no classical default and cannot be array-backed; \
         consider restructuring to avoid early returns of this type"
    ))]
    UnsupportedEarlyReturnType(
        String,
        #[label("callable with unsupported return type")] Span,
    ),

    /// Emitted when one of the return-unification fixpoint loops — the
    /// compound-position hoist, the ANF operand-lift (label `"anf"`), or the
    /// post-transform simplifier — fails to reach a fixpoint within its
    /// per-block measure bound. The label in field 0 (`"hoist"`, `"anf"`, or
    /// `"simplify"`) identifies which loop did not converge. The IR remains
    /// semantically valid, but the partial fold indicates a rule regression.
    #[error("return-unification {0} did not reach a fixpoint")]
    #[diagnostic(code("Qsc.ReturnUnify.FixpointNotReached"))]
    #[diagnostic(severity(Warning))]
    #[diagnostic(help(
        "this is an internal compiler diagnostic; please file an issue \
         including the source program that triggered it"
    ))]
    FixpointNotReached(&'static str, BlockId),

    /// A return appears inside a compound expression whose enclosing
    /// expression has a type with no classical default.
    #[error("cannot hoist `return` from a compound position of type `{0}`")]
    #[diagnostic(code("Qsc.ReturnUnify.UnsupportedHoistContext"))]
    #[diagnostic(severity(Warning))]
    #[diagnostic(help(
        "the surrounding expression has a non-defaultable type; \
         move the `return` to a statement boundary, or restructure the \
         expression so it does not contain a `return`"
    ))]
    UnsupportedHoistContext(
        String,
        #[label("compound expression with unsupported `return`")] Span,
    ),
}

impl Error {
    /// Returns true if this error is a non-fatal warning that should not
    /// trigger pipeline abort.
    #[must_use]
    pub fn is_warning(&self) -> bool {
        matches!(
            self,
            Self::FixpointNotReached { .. }
                | Self::UnsupportedEarlyReturnType { .. }
                | Self::UnsupportedHoistContext { .. }
        )
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
/// A `Vec<Error>` collecting per-callable diagnostics, paired with the set of
/// [`StoreItemId`]s of callables that were deliberately left un-rewritten
/// (their bodies still contain a residual `Return`). An empty diagnostic
/// vector means every reachable callable was rewritten
/// successfully. Diagnostics are accumulated, not fatal: processing continues
/// for remaining callables after each one is recorded. The unconvertible
/// patterns surface as warnings, and the skipped-callable set lets later
/// invariant checks bypass exactly the residual-`Return` checks on those
/// bodies.
///
/// # Errors
/// The user-reachable variant is [`Error::UnsupportedEarlyReturnType`], emitted
/// when flag lowering cannot select a return-slot representation for the
/// callable's return type. Non-defaultable types with resolvable structure use
/// an array-backed slot, including mixed Qubit/callable shapes; unresolved or
/// otherwise unsupported shapes are left unchanged after the diagnostic.
//
// Every callable reachable from the entry expression is unified across the
// whole reachable package closure. Synthesized flag/slot locals are minted into
// each callable's owning package so foreign-package id arenas stay
// collision-free. The shared arrow-default cache ([`slot::ArrowDefaultCache`])
// is keyed by `PackageId` so a default synthesized for one package is never
// handed back for another.
//
// Unreachable callables retain their `Return` nodes, which is safe because:
// 1. `check_no_returns` walks the same reachable set from
//    [`collect_reachable_from_entry`].
// 2. Downstream passes recompute reachability via the same walker and never
//    re-reach a callable that was unreachable here. Defunc's specialization
//    creates new clone items rather than widening reachability to
//    existing-but-dead items.
// 3. A future pass that inlines a dead call or rewires a dead callable into
//    the call graph must re-invoke `unify_returns` on the newly reachable
//    items before `check_no_returns` runs.
pub fn unify_returns(
    store: &mut PackageStore,
    package_id: PackageId,
    assigners: &mut PackageAssigners,
) -> (Vec<Error>, FxHashSet<StoreItemId>) {
    unify_returns_impl_cross_package(
        store,
        package_id,
        assigners,
        &[],
        /* run_simplify */ true,
    )
}

/// Seed-rooted variant of [`unify_returns`] for the signature-preserving
/// sub-pipeline.
///
/// In addition to entry-reachable callables, the `seeds` roots (pinned
/// `ReinvokeOriginal` target bodies and their transitive callees) are
/// transformed. Each reachable callable, including seeds that live in foreign
/// packages, is processed in its owning package via [`PackageAssigners`].
/// Entry-reachable callables already return-unified contain no `ExprKind::Return`,
/// so re-walking them is a no-op; only the seeded bodies still carry returns.
pub fn unify_returns_with_seeds(
    store: &mut PackageStore,
    package_id: PackageId,
    assigners: &mut PackageAssigners,
    seeds: &[StoreItemId],
) -> (Vec<Error>, FxHashSet<StoreItemId>) {
    unify_returns_impl_cross_package(
        store, package_id, assigners, seeds, /* run_simplify */ true,
    )
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
    unify_returns_impl_single(
        store,
        package_id,
        assigner,
        &[],
        /* run_simplify */ false,
    )
    .0
}

/// Cross-package driver for [`unify_returns`].
///
/// Roots reachability once at the entry package, which already spans the whole
/// reachable package closure, and processes every reachable callable in its
/// owning package so each callable's synthesized flag/slot locals are minted
/// into that package's id arena via [`PackageAssigners::get_mut`]. The shared
/// [`ArrowDefaultCache`] is keyed by `PackageId` so reusing it across the loop
/// never returns a package-local id synthesized for a different package.
fn unify_returns_impl_cross_package(
    store: &mut PackageStore,
    package_id: PackageId,
    assigners: &mut PackageAssigners,
    seeds: &[StoreItemId],
    run_simplify: bool,
) -> (Vec<Error>, FxHashSet<StoreItemId>) {
    let reachable = if seeds.is_empty() {
        collect_reachable_from_entry(store, package_id)
    } else {
        collect_reachable_with_seeds(store, package_id, seeds)
    };
    let udt_pure_tys = build_scoped_udt_pure_ty_cache(store, &reachable);
    let mut errors = Vec::new();
    // Callables deliberately left un-rewritten: a residual `Return` survives in
    // each of these bodies. The set is surfaced so the invariant checker can
    // bypass exactly the post-return-unification checks a residual `Return`
    // would otherwise violate, while every other invariant still runs on them.
    let mut skipped = FxHashSet::default();
    let mut arrow_default_cache = ArrowDefaultCache::default();

    let reachable_callables: Vec<StoreItemId> = reachable.iter().copied().collect();
    for store_id in reachable_callables {
        let owning_pkg = store_id.package;
        let item_id = store_id.item;
        let assigner = assigners.get_mut(store, owning_pkg);
        process_callable_returns(
            store,
            owning_pkg,
            assigner,
            item_id,
            &udt_pure_tys,
            &mut arrow_default_cache,
            run_simplify,
            &mut errors,
            &mut skipped,
        );
    }

    (errors, skipped)
}

/// Single-package, seed-rooted driver used by the test-only
/// [`unify_returns_without_simplify`].
///
/// Processes only the reachable callables that live in `package_id`, using the
/// caller-supplied single [`Assigner`].
#[cfg(test)]
fn unify_returns_impl_single(
    store: &mut PackageStore,
    package_id: PackageId,
    assigner: &mut Assigner,
    seeds: &[StoreItemId],
    run_simplify: bool,
) -> (Vec<Error>, FxHashSet<StoreItemId>) {
    let reachable = collect_reachable_with_seeds(store, package_id, seeds);
    let udt_pure_tys = build_scoped_udt_pure_ty_cache(store, &reachable);
    let mut errors = Vec::new();
    let mut skipped = FxHashSet::default();
    let mut arrow_default_cache = ArrowDefaultCache::default();

    let local_reachable: Vec<_> = reachable
        .iter()
        .filter(|id| id.package == package_id)
        .map(|id| id.item)
        .collect();

    for item_id in local_reachable {
        process_callable_returns(
            store,
            package_id,
            assigner,
            item_id,
            &udt_pure_tys,
            &mut arrow_default_cache,
            run_simplify,
            &mut errors,
            &mut skipped,
        );
    }

    (errors, skipped)
}

/// Return-unifies a single callable `item_id` that lives in `owning_pkg`,
/// minting synthesized nodes through `owning_pkg`'s `assigner`.
///
/// Shared by the cross-package and single-package drivers. A callable left
/// un-rewritten (its body keeps a residual `Return`) is recorded in `skipped`
/// keyed by its full [`StoreItemId`] so the invariant checker bypasses exactly
/// the residual-`Return` checks on that callable in its owning package.
#[allow(clippy::too_many_arguments)]
fn process_callable_returns(
    store: &mut PackageStore,
    owning_pkg: PackageId,
    assigner: &mut Assigner,
    item_id: LocalItemId,
    udt_pure_tys: &UdtPureTyCache,
    arrow_default_cache: &mut ArrowDefaultCache,
    run_simplify: bool,
    errors: &mut Vec<Error>,
    skipped: &mut FxHashSet<StoreItemId>,
) {
    let callable = {
        let package = store.get(owning_pkg);
        let Some(item) = package.items.get(item_id) else {
            return;
        };
        match &item.kind {
            ItemKind::Callable(callable) => callable.clone(),
            ItemKind::Ty(..) => return,
        }
    };
    let return_ty = callable.output.clone();
    let body_blocks = get_callable_body_blocks(&callable);

    // Pre-check: skip the whole callable if any body block holds a
    // compound-position Return whose context needs a non-defaultable
    // default, which would otherwise panic in normalize. This pre-check
    // runs before any mutation, so a callable left un-rewritten here keeps
    // its body byte-for-byte the monomorphized FIR.
    let pre_check_diag_count = errors.len();
    for &block_id in &body_blocks {
        if !contains_return_in_block(store.get(owning_pkg), block_id) {
            continue;
        }
        check_normalize_supportable(store.get(owning_pkg), owning_pkg, block_id, errors);
    }
    if errors.len() > pre_check_diag_count {
        skipped.insert(StoreItemId {
            package: owning_pkg,
            item: item_id,
        });
        return;
    }

    // Return-slot selection depends only on the callable's return type, so
    // it is decided once, before any block is mutated. When no slot
    // representation exists for a return-bearing callable, leave it
    // un-rewritten rather than partially hoisting and then bailing out.
    let return_slot_strategy = {
        let context = UdtResolutionContext::Store(store);
        select_return_slot_strategy(&return_ty, udt_pure_tys, &context)
    };
    let Some(return_slot_strategy) = return_slot_strategy else {
        let has_return = body_blocks
            .iter()
            .any(|&block_id| contains_return_in_block(store.get(owning_pkg), block_id));
        if has_return {
            errors.push(Error::UnsupportedEarlyReturnType(
                format!("{return_ty}"),
                callable.name.span,
            ));
            skipped.insert(StoreItemId {
                package: owning_pkg,
                item: item_id,
            });
        }
        return;
    };

    for block_id in body_blocks {
        if !contains_return_in_block(store.get(owning_pkg), block_id) {
            continue;
        }

        // Pre-pass: hoist any compound-position Return to its enclosing
        // statement boundary so flag lowering only sees bare returns or
        // returns inside statement-carrying Block/If/While.
        normalize::hoist_returns_to_statement_boundary(
            store.get_mut(owning_pkg),
            assigner,
            owning_pkg,
            block_id,
            errors,
        );

        // Normalize operand-buried returns: lift each `Return` sitting in an
        // eagerly-evaluated operand position to a spine `let` temp so it
        // reaches a statement boundary the flag lowering can consume. Runs
        // after the compound-position hoist fixpoint, which mints the
        // operand-lift candidates this phase removes.
        normalize::run_anf_to_fixpoint(
            store.get_mut(owning_pkg),
            assigner,
            owning_pkg,
            block_id,
            errors,
        );

        let package = store.get_mut(owning_pkg);
        let slots = transform_block_with_flags(
            package,
            assigner,
            owning_pkg,
            block_id,
            &return_ty,
            udt_pure_tys,
            arrow_default_cache,
            return_slot_strategy,
        );
        if run_simplify {
            simplify::run_to_fixpoint(package, assigner, block_id, errors, &slots);
        }
    }
}

/// Extract every explicit body block from a callable declaration.
///
/// Returns the body block plus any adj/ctl/ctl-adj specialization blocks.
/// Intrinsics have no explicit body block, so the result is empty.
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
/// Scans the reachable expression tree for patterns that would cause
/// the normalize phase to panic when it cannot synthesize a classical
/// default:
///
/// 1. An `If` expression whose condition contains a `Return` and whose
///    type is non-Unit and non-defaultable (would panic in
///    `normalize::hoist_in_cond`).
/// 2. A `Local` statement whose initializer contains a `Return` and whose
///    pattern type is non-defaultable (would panic in
///    `normalize::replace_local_init_with_default_and_emit`).
/// 3. An operand-position `Return` whose ANF lift would bind a
///    non-defaultable spine temp (see
///    `normalize::find_unsupported_operand_lifts`).
///
/// For each found, pushes [`Error::UnsupportedHoistContext`]. The caller
/// skips normalize+transform when any non-warning error is emitted.
fn check_normalize_supportable(
    package: &Package,
    package_id: PackageId,
    block_id: BlockId,
    errors: &mut Vec<Error>,
) {
    // Single pre-order walk over the block. The shared walker visits every
    // sub-expression (including those nested in local initializers) and treats
    // `Closure` as a leaf, so closure bodies are scanned independently. During
    // the walk we both run the `If`-expression check and collect nested block
    // ids for the statement-level `Local` check below.
    let mut block_ids = vec![block_id];
    crate::walk_utils::for_each_expr_in_block(package, block_id, &mut |_id, expr| {
        match &expr.kind {
            // An `If` whose condition contains a `return` and whose type is
            // non-Unit and non-defaultable cannot be hoisted (would panic in
            // `normalize::hoist_in_cond`).
            ExprKind::If(cond, _, _)
                if detect::contains_return_in_expr(package, *cond)
                    && expr.ty != Ty::UNIT
                    && !is_type_defaultable(package, package_id, &expr.ty) =>
            {
                errors.push(Error::UnsupportedHoistContext(
                    format!("{}", expr.ty),
                    expr.span,
                ));
            }
            ExprKind::Block(bid) | ExprKind::While(_, bid) => block_ids.push(*bid),
            _ => {}
        }
    });

    // A `Local` whose initializer contains a `return` and whose pattern type is
    // non-defaultable cannot be hoisted (would panic in
    // `normalize::replace_local_init_with_default_and_emit`). This is a
    // statement-level check, so iterate the root block plus every nested block.
    for &bid in &block_ids {
        for &stmt_id in &package.get_block(bid).stmts {
            if let StmtKind::Local(_, pat_id, init_id) = &package.get_stmt(stmt_id).kind
                && detect::contains_return_in_expr(package, *init_id)
            {
                let pat_ty = &package.get_pat(*pat_id).ty;
                if !is_type_defaultable(package, package_id, pat_ty) {
                    errors.push(Error::UnsupportedHoistContext(
                        format!("{pat_ty}"),
                        package.get_expr(*init_id).span,
                    ));
                }
            }
        }
    }

    // An operand-position `return` is lifted to a spine temp by the ANF pass.
    // Reject the one case that temp lift cannot lower: a temp whose type is
    // non-defaultable (no classical default to seed on the non-return path).
    // Projected parents (`Range`/`Struct`/`String`) lift like any other
    // operand because each eager child has a stable write-back slot. Reporting
    // here leaves the callable unchanged instead of panicking during normalize.
    for (ty, span) in normalize::find_unsupported_operand_lifts(package, package_id, block_id) {
        errors.push(Error::UnsupportedHoistContext(ty, span));
    }
}
