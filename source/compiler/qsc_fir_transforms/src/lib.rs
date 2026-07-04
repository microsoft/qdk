// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! FIR-to-FIR transformation passes for the Q# compiler.
//!
//! This crate runs the production FIR rewrite pipeline after FIR lowering and
//! before partial evaluation and codegen. The output is semantically
//! equivalent to the input but lowered into forms that partial evaluation and
//! codegen can consume.
//!
//! # What to know before diving in
//!
//! - **It is one ordered pipeline, not a toolbox of independent passes.**
//!   Everything runs through [`run_pipeline_with_diagnostics`] in a fixed
//!   order: ``monomorphize`` → ``return_unify`` → ``cond_normalize`` →
//!   ``defunctionalize`` → ``udt_erase`` → ``tuple_compare_lower`` →
//!   ``tuple_decompose`` → ``arg_promote`` → (``tuple_decompose`` ⇄
//!   ``arg_promote`` fixed point) → ``normalize_reachable_call_arg_types`` →
//!   ``run_item_dce_and_gc`` (item DCE first, then whole-closure
//!   ``gc_unreachable``) → ``exec_graph_rebuild``, closed by the ``PostAll``
//!   invariant check. Individual passes are *not* sound or invariant-preserving
//!   on their own. A pass deliberately leaves FIR that violates invariants a
//!   later pass relies on (e.g. defunctionalization is cleaned up by
//!   ``udt_erase`` and ``tuple_compare_lower``). Do not reorder, remove, or run
//!   passes in isolation without understanding the chain.
//!
//! - **``tuple_decompose`` ↔ ``arg_promote`` run to a fixed point.** These two passes
//!   iterate until convergence (capped), so
//!   changes to either must preserve the strictly-decreasing measure that
//!   guarantees termination. The idempotent
//!   ``normalize_reachable_call_arg_types`` cleanup runs exactly once after the
//!   loop converges, not per round.
//!
//! - **A `PackageAssigners` pool gives each package its own id space.** Every
//!   package owns an independent id arena, so passes that synthesize FIR nodes
//!   mint fresh IDs from the assigner of the package they are mutating, never
//!   from one global counter. Each package's
//!   [`Assigner`](qsc_fir::assigner::Assigner) is seeded lazily from that
//!   package's own id watermark via
//!   [`Assigner::from_package`](qsc_fir::assigner::Assigner::from_package) the
//!   first time it is touched, and the advanced watermark persists across passes. A
//!   pass always selects the owning package's assigner at a cross-package
//!   boundary, so entry-package ids are never minted into a foreign package's
//!   arena. See the `package_assigners` module. The trailing metadata passes
//!   take no assigner because they only tombstone, delete, or rebuild derived
//!   data and synthesize nothing.
//!
//! - **Synthesized nodes use the ``EMPTY_EXEC_RANGE`` sentinel.** New
//!   [`Expr`](qsc_fir::fir::Expr)/[`Stmt`](qsc_fir::fir::Stmt) nodes get an
//!   empty ``exec_graph_range``; the final ``exec_graph_rebuild`` pass rebuilds
//!   the execution graph from the rewritten FIR.
//!
//! - **Only consume the result when there are no fatal diagnostics.** On a
//!   fatal error the FIR store may be stuck at an intermediate stage that does
//!   not satisfy any invariant boundary. [`PipelineResult`] carries non-fatal
//!   warnings, which do not block successful output.
//!
//! - **Implementation helpers.** Several passes deep-clone FIR subtrees via
//!   `cloner::FirCloner`; others rewrite in place or rebuild derived
//!   structures from scratch. [`invariants`] checks the structural contracts
//!   between stages.

pub(crate) mod cloner;
pub(crate) mod fir_builder;
pub mod invariants;
pub(crate) mod package_assigners;
#[cfg(test)]
pub(crate) mod pretty;
pub mod reachability;

pub(crate) mod arg_promote;
pub(crate) mod cond_normalize;
pub mod defunctionalize;
pub(crate) mod exec_graph_rebuild;
pub(crate) mod gc_unreachable;
pub(crate) mod intrinsic_precheck;
pub(crate) mod item_dce;
pub(crate) mod monomorphize;
pub(crate) mod return_unify;
#[cfg(test)]
mod sample_pipeline_tests;
#[cfg(test)]
mod signature_preserving_tests;
pub(crate) mod tuple_compare_lower;
pub(crate) mod tuple_decompose;
pub(crate) mod tuple_destructuring;
pub(crate) mod udt_erase;

#[cfg(any(test, feature = "testutil"))]
pub mod test_utils;

pub(crate) mod walk_utils;

use miette::Diagnostic;
use qsc_fir::fir::{ExecGraphIdx, ItemKind, PackageId, PackageStore, StoreItemId};
use rustc_hash::FxHashSet;
use thiserror::Error;

use crate::package_assigners::PackageAssigners;

/// Kinds of callable specializations that carry execution graphs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CallableSpecKind {
    /// The default callable body implementation.
    Body,
    /// The adjoint specialization.
    Adj,
    /// The controlled specialization.
    Ctl,
    /// The controlled-adjoint specialization.
    CtlAdj,
    /// A simulatable intrinsic with an explicit body block.
    SimulatableIntrinsic,
}

/// An empty execution graph range for synthesized FIR nodes that do not
/// participate in the execution graph.
pub(crate) const EMPTY_EXEC_RANGE: std::ops::Range<ExecGraphIdx> = std::ops::Range {
    start: ExecGraphIdx::ZERO,
    end: ExecGraphIdx::ZERO,
};

/// Hard-cap on the number of tuple-decompose <-> argument-promotion fixed-point rounds in
/// [`run_pipeline_to_impl`]. Convergence is mathematically guaranteed by a
/// strictly-decreasing measure, so realistic Q# converges in only a few rounds
/// (linear in tuple nesting depth and copy-alias chain length). This cap is a
/// divergence backstop for adversarial or machine-generated input: on
/// exhaustion the loop stops with residual tuples (suboptimal codegen, never a
/// miscompile) and emits [`PipelineError::TupleDecomposeArgPromoteFixpointNotReached`].
const TUPLE_DECOMPOSE_ARG_PROMOTE_FIXPOINT_CAP: usize = 64;

/// Diagnostics produced by the FIR transform pipeline.
///
/// Wraps pass-specific diagnostic types so callers handle a single diagnostic
/// type from [`run_pipeline_with_diagnostics`],
/// [`run_pipeline_to_with_diagnostics`], and other warning-aware result APIs.
#[derive(Clone, Debug, Diagnostic, Error)]
pub enum PipelineError {
    /// A return-unification error or warning (e.g., unsupported return type).
    #[error(transparent)]
    #[diagnostic(transparent)]
    ReturnUnify(#[from] return_unify::Error),

    /// A defunctionalization error (e.g., dynamic callable, convergence failure).
    #[error(transparent)]
    #[diagnostic(transparent)]
    Defunctionalize(#[from] defunctionalize::Error),

    /// An intrinsic callable has an unsupported parameter or return type.
    #[error(transparent)]
    #[diagnostic(transparent)]
    IntrinsicPrecheck(#[from] intrinsic_precheck::Error),

    /// A pinned item requested by a caller was not present in the FIR store.
    #[error("pinned item {0} does not exist")]
    #[diagnostic(code("Qsc.FirTransform.MissingPinnedItem"))]
    MissingPinnedItem(StoreItemId),

    /// A pinned item requested by a caller was present but was not a callable.
    #[error("pinned item {0} is not a callable")]
    #[diagnostic(code("Qsc.FirTransform.PinnedItemNotCallable"))]
    PinnedItemNotCallable(StoreItemId),

    /// The tuple-decompose <-> argument-promotion fixed-point loop did not converge within
    /// its hard cap. Residual tuple locals may remain (suboptimal codegen), but
    /// the emitted FIR is still correct.
    #[error(
        "tuple-decompose/argument-promotion fixed-point loop did not converge within {0} rounds"
    )]
    #[diagnostic(code("Qsc.FirTransform.TupleDecomposeArgPromoteFixpointNotReached"))]
    #[diagnostic(severity(Warning))]
    TupleDecomposeArgPromoteFixpointNotReached(usize),
}

/// Warning-aware result for the FIR transform pipeline.
///
/// Fatal `errors` block FIR consumption for the requested stage. The store may
/// contain intermediate FIR after fatal diagnostics and must not be treated as
/// successful pipeline output. Non-fatal `warnings` preserve diagnostics that
/// were emitted while still allowing the pipeline to reach the requested stage.
#[derive(Clone, Debug, Default)]
pub struct PipelineResult {
    /// Fatal transform diagnostics that prevent consuming the FIR as successful
    /// output for the requested stage.
    pub errors: Vec<PipelineError>,
    /// Non-fatal transform diagnostics emitted while producing successful FIR
    /// output for the requested stage.
    pub warnings: Vec<PipelineError>,
}

impl PipelineResult {
    /// Returns `true` when the pipeline produced consumable output for the
    /// requested stage.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.errors.is_empty()
    }
}

/// How far through the FIR transform schedule to run.
///
/// Intermediate stages are mainly used by tests and internal validation
/// helpers. Production codegen uses `Full`, including a pinned-item path that
/// preserves callable IDs through DCE and exec graph rebuild.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipelineStage {
    /// Run through monomorphization.
    Mono,
    /// Run through return unification.
    ReturnUnify,
    /// Run through defunctionalization.
    Defunc,
    /// Run through UDT erasure.
    UdtErase,
    /// Run through tuple comparison lowering.
    TupleCompLower,
    /// Run through tuple-decompose.
    TupleDecompose,
    /// Run through argument promotion.
    ArgPromote,
    /// Run through the second tuple-decompose pass (scalar-replaces caller-side tuple
    /// locals left field-only by argument promotion).
    TupleDecompose2,
    /// Run through item-level dead code elimination.
    ItemDce,
    /// Run through exec graph rebuild.
    ExecGraphRebuild,
    /// Run the full pipeline.
    Full,
}

/// Runs the FIR transform schedule up to `stage`, threading a
/// [`PackageAssigners`] pool through every pass.
///
/// The pool is constructed once from the input package and passed by mutable
/// reference to each pass so per-package id allocations from earlier stages are
/// observed by later stages. Between major stages the function invokes
/// [`invariants::check`] with the corresponding [`invariants::InvariantLevel`].
///
/// The schedule has several fatal early exits, in order:
///
/// 1. `intrinsic_precheck` (via [`perform_intrinsic_type_validation`]) runs
///    before any structural rewrites and short-circuits with
///    [`PipelineError::IntrinsicPrecheck`] when an intrinsic callable has an
///    unsupported parameter or return type.
/// 2. [`return_unify::unify_returns`] reports fatal diagnostics that abort
///    the schedule before defunctionalization runs.
/// 3. [`defunctionalize::defunctionalize`] reports fatal diagnostics that
///    abort the schedule before UDT erasure runs. Non-fatal defunctionalization
///    warnings are preserved on [`PipelineResult::warnings`] and the schedule
///    continues to the requested stage.
/// 4. Pinned-item validation runs before seeded item DCE and exec graph
///    rebuild. Missing or non-callable pins are fatal diagnostics because
///    pinned items are explicit preservation requests from callers.
///
/// In every fatal case the intermediate FIR intentionally violates downstream
/// invariants, so running later passes would produce misleading failures.
fn run_pipeline_to_impl(
    store: &mut PackageStore,
    package_id: PackageId,
    stage: PipelineStage,
    pinned_items: &[StoreItemId],
) -> PipelineResult {
    assert!(
        store.get(package_id).entry.is_some(),
        "FIR transform pipeline requires a package with an entry expression; \
         library packages should not be passed to the transform pipeline"
    );

    if let Some(result) = perform_intrinsic_type_validation(store, package_id) {
        return result;
    }

    let mut result = PipelineResult::default();

    let mut assigners = PackageAssigners::new(store, package_id);

    monomorphize::monomorphize(store, package_id, &mut assigners);
    invariants::check(store, package_id, invariants::InvariantLevel::PostMono);
    if matches!(stage, PipelineStage::Mono) {
        return result;
    }

    let (ru_errors, skipped) = return_unify::unify_returns(store, package_id, &mut assigners);
    let (ru_warnings, ru_fatal): (Vec<_>, Vec<_>) = ru_errors
        .into_iter()
        .partition(return_unify::Error::is_warning);
    result
        .warnings
        .extend(ru_warnings.into_iter().map(PipelineError::from));
    // Return unification currently emits only warnings: callables it cannot
    // convert are left un-rewritten (their residual `Return` nodes are carried
    // through downstream stages and the invariant checker skips them via
    // `skipped`). This guard is a defensive abort for a future fatal
    // `return_unify::Error` variant, keeping the schedule from advancing past a
    // genuinely unrecoverable callable; today `ru_fatal` is always empty.
    if !ru_fatal.is_empty() {
        result.errors = ru_fatal.into_iter().map(PipelineError::from).collect();
        return result;
    }
    invariants::check_with_skip(
        store,
        package_id,
        invariants::InvariantLevel::PostReturnUnify,
        &skipped,
    );
    if matches!(stage, PipelineStage::ReturnUnify) {
        return result;
    }

    // Hoist side-effecting `if` conditions into single-evaluation `let`
    // bindings before defunctionalization, so its guard reuse references only
    // pure `Var` reads and never re-runs a condition's effects. This preserves
    // the `PostReturnUnify` invariants (it introduces no `Return` nodes), so no
    // additional checkpoint is required here.
    cond_normalize::normalize_conditions(store, package_id, &mut assigners);
    invariants::check_with_skip(
        store,
        package_id,
        invariants::InvariantLevel::PostReturnUnify,
        &skipped,
    );

    let defunc_lowering_done = run_defunc_and_lowering_stages(
        store,
        package_id,
        stage,
        &mut result,
        &mut assigners,
        &skipped,
    );
    if defunc_lowering_done {
        return result;
    }

    if run_arg_promote_stages(
        store,
        package_id,
        stage,
        &mut result,
        &mut assigners,
        &skipped,
    ) {
        return result;
    }

    finalize_pipeline(
        store,
        package_id,
        stage,
        &mut result,
        pinned_items,
        &skipped,
    );
    result
}

/// Runs the defunctionalization and structural lowering stages: defunc, UDT
/// erasure, tuple-comparison lowering, and tuple decomposition, checking the
/// matching invariant after each.
///
/// Returns `true` when the requested `stage` is reached (or a fatal
/// defunctionalization error occurs), signalling the caller to stop and return
/// the accumulated `result`.
fn run_defunc_and_lowering_stages(
    store: &mut PackageStore,
    package_id: PackageId,
    stage: PipelineStage,
    result: &mut PipelineResult,
    assigners: &mut PackageAssigners,
    skipped: &FxHashSet<StoreItemId>,
) -> bool {
    let defunc_diagnostics = defunctionalize::defunctionalize(store, package_id, assigners);
    let (warnings, fatal_errors): (Vec<_>, Vec<_>) = defunc_diagnostics
        .into_iter()
        .partition(defunctionalize::Error::is_warning);
    // Append rather than overwrite so warnings surfaced by return unification
    // (the warn-and-delegate diagnostics for unconvertible early returns) are
    // preserved alongside defunctionalization warnings.
    result
        .warnings
        .extend(warnings.into_iter().map(PipelineError::from));
    if !fatal_errors.is_empty() {
        result.errors = fatal_errors.into_iter().map(PipelineError::from).collect();
        return true;
    }

    invariants::check_with_skip(
        store,
        package_id,
        invariants::InvariantLevel::PostDefunc,
        skipped,
    );
    if matches!(stage, PipelineStage::Defunc) {
        return true;
    }

    udt_erase::erase_udts(store, package_id, assigners);
    invariants::check_with_skip(
        store,
        package_id,
        invariants::InvariantLevel::PostUdtErase,
        skipped,
    );
    if matches!(stage, PipelineStage::UdtErase) {
        return true;
    }

    tuple_compare_lower::lower_tuple_comparisons(store, package_id, assigners);
    invariants::check_with_skip(
        store,
        package_id,
        invariants::InvariantLevel::PostTupleCompLower,
        skipped,
    );
    if matches!(stage, PipelineStage::TupleCompLower) {
        return true;
    }

    tuple_decompose::tuple_decompose(store, package_id, assigners);
    invariants::check_with_skip(
        store,
        package_id,
        invariants::InvariantLevel::PostTupleDecompose,
        skipped,
    );
    matches!(stage, PipelineStage::TupleDecompose)
}

/// Runs the argument-promotion stages: the initial `arg_promote`, the
/// tuple-decompose/arg-promote fixed point, and the one-shot post-loop
/// call-argument-type normalization, checking `PostArgPromote` after each.
///
/// Returns `true` when the requested `stage` is reached, signalling the caller
/// to stop and return the accumulated `result`.
fn run_arg_promote_stages(
    store: &mut PackageStore,
    package_id: PackageId,
    stage: PipelineStage,
    result: &mut PipelineResult,
    assigners: &mut PackageAssigners,
    skipped: &FxHashSet<StoreItemId>,
) -> bool {
    arg_promote::arg_promote(store, package_id, assigners);
    invariants::check_with_skip(
        store,
        package_id,
        invariants::InvariantLevel::PostArgPromote,
        skipped,
    );
    if matches!(stage, PipelineStage::ArgPromote) {
        return true;
    }

    tuple_decompose_arg_promote_fixed_point(store, package_id, result, assigners, skipped);

    // Call-argument-type normalization is idempotent and candidate-neutral, so
    // it is hoisted to run exactly once after the loop converges rather than
    // per round (per-round runs cause `(T,)` wrapping churn that pollutes
    // change detection).
    arg_promote::normalize_reachable_call_arg_types(store, package_id, assigners);
    invariants::check_with_skip(
        store,
        package_id,
        invariants::InvariantLevel::PostArgPromote,
        skipped,
    );
    matches!(stage, PipelineStage::TupleDecompose2)
}

/// Runs the backend stages after all structural transforms: pinned-item
/// validation, item dead-code elimination, execution-graph rebuild, and the
/// final `PostAll` invariant walk.
///
/// Mutates `result` in place; a fatal pinned-item validation error stops the
/// backend early with the errors recorded on `result`.
fn finalize_pipeline(
    store: &mut PackageStore,
    package_id: PackageId,
    stage: PipelineStage,
    result: &mut PipelineResult,
    pinned_items: &[StoreItemId],
    skipped: &FxHashSet<StoreItemId>,
) {
    // Item DCE: remove unreachable callable items and dead type items.
    // Callers may pin items via `pinned_items` to keep them (and their
    // transitive dependencies) alive through DCE and exec-graph-rebuild.
    let pinned_errors = validate_pinned_items(store, pinned_items);
    if !pinned_errors.is_empty() {
        result.errors = pinned_errors;
        return;
    }
    run_item_dce_and_gc(store, package_id, pinned_items);
    invariants::check_with_skip(
        store,
        package_id,
        invariants::InvariantLevel::PostItemDce,
        skipped,
    );
    if matches!(stage, PipelineStage::ItemDce) {
        return;
    }

    // Exec graphs are rebuilt unconditionally for every reachable spec in every
    // reachable package. Earlier passes may have structurally mutated reachable
    // callables in any package, so the rebuild walks the whole reachable closure
    // rather than tracking which specs were mutated. The reachable-spec exec
    // graphs are validated by the `PostAll` invariant walk below.
    exec_graph_rebuild::rebuild_exec_graphs(store, package_id, pinned_items);
    if matches!(stage, PipelineStage::ExecGraphRebuild) {
        return;
    }

    // PostAll uses entry-only reachability. Pinned items (original target kept
    // for fir_to_qir_from_callable) retain pre-transform types and are not checked.
    invariants::check_with_skip(
        store,
        package_id,
        invariants::InvariantLevel::PostAll,
        skipped,
    );
}

/// Fixed-point loop over tuple-decompose and argument promotion. `arg_promote`
/// can leave caller-side tuple locals field-only (tuple-decompose's eligible
/// shape), and tuple-decompose can expose fresh tuple-copy/destructure
/// candidates for `promote_to_fixed_point`. Iterating both until neither
/// changes the FIR fully flattens arbitrarily nested `let`-destructures and
/// tuple-copy aliases: destructure normalization emits direct multi-index leaf
/// projections with no whole-value temporary, and tuple-decompose
/// scalar-replaces the projected locals. Each pass only decomposes local
/// `Bind` patterns or promotes parameters and never violates `PostArgPromote`,
/// so the invariants hold every round.
///
/// A strictly-decreasing measure (total tuple nesting mass plus unresolved
/// copy-alias hops) guarantees convergence in O(nesting-depth +
/// copy-alias-chain-length) rounds. The hard cap is a divergence backstop for
/// adversarial or machine-generated input: on exhaustion the loop stops with
/// residual tuples (suboptimal codegen, never a miscompile) and surfaces a
/// non-fatal warning.
fn tuple_decompose_arg_promote_fixed_point(
    store: &mut PackageStore,
    package_id: PackageId,
    result: &mut PipelineResult,
    assigners: &mut PackageAssigners,
    skip: &FxHashSet<StoreItemId>,
) {
    let mut rounds = 0;
    let mut arg_promote_tmp_counter = 0;
    loop {
        let tuple_decompose_changed =
            tuple_decompose::tuple_decompose(store, package_id, assigners);
        let promote_changed = arg_promote::promote_to_fixed_point(
            store,
            package_id,
            assigners,
            &mut arg_promote_tmp_counter,
        );
        // One PostArgPromote check per round, after both passes have run.
        // tuple-decompose preserves the PostArgPromote invariants (it only
        // scalar-replaces local bindings), and argument promotion runs on its
        // output, so a single check on the round's cumulative result validates
        // both passes.
        invariants::check_with_skip(
            store,
            package_id,
            invariants::InvariantLevel::PostArgPromote,
            skip,
        );
        if !tuple_decompose_changed && !promote_changed {
            break;
        }
        rounds += 1;
        if rounds >= TUPLE_DECOMPOSE_ARG_PROMOTE_FIXPOINT_CAP {
            // This isn't reachable in practice but provides an escape mechanism
            // if somehow an adversarial input is created.
            result
                .warnings
                .push(PipelineError::TupleDecomposeArgPromoteFixpointNotReached(
                    TUPLE_DECOMPOSE_ARG_PROMOTE_FIXPOINT_CAP,
                ));
            break;
        }
    }
}

/// Pre-pass: reject intrinsic callables with tuple or UDT parameter/return types.
fn perform_intrinsic_type_validation(
    store: &mut PackageStore,
    package_id: PackageId,
) -> Option<PipelineResult> {
    let precheck_errors = intrinsic_precheck::validate_intrinsic_types(store, package_id);

    if !precheck_errors.is_empty() {
        return Some(PipelineResult {
            errors: precheck_errors
                .into_iter()
                .map(PipelineError::from)
                .collect(),
            ..Default::default()
        });
    }
    None
}

/// Validates all explicit pinned items before seeded reachability consumes them.
fn validate_pinned_items(store: &PackageStore, pinned_items: &[StoreItemId]) -> Vec<PipelineError> {
    pinned_items
        .iter()
        .filter_map(|item_id| validate_pinned_item(store, *item_id).err())
        .collect()
}

/// Validates that a pinned item exists and refers to a callable item.
fn validate_pinned_item(store: &PackageStore, item_id: StoreItemId) -> Result<(), PipelineError> {
    let Some((_, package)) = store
        .iter()
        .find(|(package_id, _)| *package_id == item_id.package)
    else {
        return Err(PipelineError::MissingPinnedItem(item_id));
    };
    let Some(item) = package.items.get(item_id.item) else {
        return Err(PipelineError::MissingPinnedItem(item_id));
    };
    if !matches!(item.kind, ItemKind::Callable(_)) {
        return Err(PipelineError::PinnedItemNotCallable(item_id));
    }
    Ok(())
}

/// Runs item-level DCE with optional pinned-root expansion, followed by an
/// unconditional GC pass.
///
/// Item DCE runs in two forms: the entry package keeps every entry-reachable
/// callable, while each foreign (library) package keeps only its
/// entry-reachable callables (its public surface is not an entry point for a
/// closed codegen compilation). GC then runs over the entire reachable package
/// closure because upstream rewrite passes leave orphaned arena nodes behind in
/// every transformed package, regardless of whether item DCE removed any items.
///
/// Pinned items are validated by `run_pipeline_to_impl` before this helper is
/// called. They are not invariant-checked; `PostAll` uses entry-only
/// reachability. Pinning is needed when the original target ID is used
/// by `fir_to_qir_from_callable` after defunc rewrites the entry `Call`
/// to reference the specialized callable.
fn run_item_dce_and_gc(
    store: &mut PackageStore,
    package_id: PackageId,
    pinned_items: &[StoreItemId],
) {
    let reachable = if pinned_items.is_empty() {
        reachability::collect_reachable_from_entry(store, package_id)
    } else {
        reachability::collect_reachable_with_seeds(store, package_id, pinned_items)
    };
    let _ = item_dce::eliminate_dead_items(package_id, store.get_mut(package_id), &reachable);

    // Foreign packages: structural passes transformed only their entry-reachable
    // callables, so each foreign package still holds entry-unreachable callables
    // that reference erased UDTs and pre-promotion signatures. RCA and codegen
    // analyze every item in every package, so those stale callables must be
    // removed to keep each foreign package internally consistent with its
    // transformed reachable callables. Pinned callable items and their
    // transitive dependencies are retained in whichever package they live in;
    // only callable items may be pinned.
    let _ =
        item_dce::eliminate_unreachable_foreign_items(store, package_id, &reachable, pinned_items);

    // Node-level GC runs across the whole reachable package closure, not just
    // the entry package. Structural passes rewrite reachable callables in every
    // package and leave orphaned blocks/stmts/exprs/pats behind in those foreign
    // packages. GC never removes items (pinned items kept by item DCE keep their
    // nodes), but it is required: RCA's top-level statement scan
    // (`unanalyzed_stmts`) treats any package statement not reached through an
    // item body as a top-level statement, so orphaned synthesized statements in
    // foreign packages would otherwise be analyzed out of context and panic.
    let gc_packages = reachability::collect_reachable_package_closure(package_id, &reachable);
    for gc_pkg in gc_packages {
        let _ = gc_unreachable::gc_unreachable(store.get_mut(gc_pkg));
    }
}

/// Runs the authoritative FIR optimization schedule up to the requested stage,
/// returning fatal errors and non-fatal warnings separately.
///
/// Production codegen uses this hidden API with [`PipelineStage::Full`] and
/// non-empty `pinned_items` to retain callable IDs that may no longer be
/// entry-reachable after defunctionalization. Intermediate cut points exist so
/// crate tests can reuse the real production ordering without re-implementing
/// it in helper code.
///
/// `pinned_items` must identify existing callable items. Invalid pins are
/// reported as fatal [`PipelineError::MissingPinnedItem`] or
/// [`PipelineError::PinnedItemNotCallable`] diagnostics before seeded item
/// DCE runs.
///
/// Callers may consume the transformed FIR only when [`PipelineResult::errors`]
/// is empty; warnings do not block successful output.
///
/// # Note
///
/// This operates **destructively** on `store`: it specializes, rewrites, and
/// prunes items in place across the entry package and every reachable foreign
/// package. The mutated store is a disposable codegen artifact — pass a fresh
/// `lower_to_fir` store (or an explicit clone) and do **not** reuse it after the
/// transforms. Item DCE and GC leave each package internally consistent only for
/// the entry-rooted reachable closure, not for reuse as a general-purpose
/// package store. Production callers uphold this by re-lowering from HIR per
/// request; it is a caller property, not a contract this function enforces.
///
/// # Panics
///
/// Panics if the package has no entry expression.
pub fn run_pipeline_to_with_diagnostics(
    store: &mut PackageStore,
    package_id: PackageId,
    stage: PipelineStage,
    pinned_items: &[StoreItemId],
) -> PipelineResult {
    run_pipeline_to_impl(store, package_id, stage, pinned_items)
}

/// Runs the full FIR optimization pipeline on the given package, returning
/// fatal errors and non-fatal warnings separately.
///
/// The pipeline applies the following passes in order:
/// - Monomorphization: eliminates generic callables
/// - Return unification: rewrites callable bodies to a single-exit form
/// - Defunctionalization: eliminates callable-valued expressions
/// - UDT erasure: replaces `Ty::Udt` with pure tuple or scalar types
/// - Tuple comparison lowering: rewrites `BinOp(Eq/Neq)` on non-empty tuple
///   operands into element-wise scalar comparisons
/// - tuple-decompose (iterative): decomposes tuple-typed locals into scalars
/// - Argument promotion (iterative): decomposes tuple-typed callable
///   parameters into scalars
/// - GC unreachable: tombstones orphaned arena nodes
/// - Item DCE: removes unreachable items from the item map, then re-runs
///   GC to tombstone orphaned `StmtKind::Item` stmts
/// - Exec graph rebuild: recomputes exec graph ranges after synthesized FIR
///   nodes are introduced
///
/// Invariant checks are inserted between the major structural stages and after
/// the final rebuild to catch structural violations early.
///
/// Warning-only diagnostics do not block successful `PostAll` output. If
/// [`PipelineResult::errors`] is non-empty, the FIR store must not be consumed
/// as successful post-pipeline output.
///
/// # Note
///
/// Like [`run_pipeline_to_with_diagnostics`], this mutates `store` destructively
/// and the store must not be reused after the transforms; see that function for
/// the disposable-store contract.
///
/// # Panics
///
/// Panics if the package has no entry expression.
pub fn run_pipeline_with_diagnostics(
    store: &mut PackageStore,
    package_id: PackageId,
) -> PipelineResult {
    run_pipeline_to_with_diagnostics(store, package_id, PipelineStage::Full, &[])
}

/// Runs the body-only, signature-preserving FIR sub-pipeline on the seeded
/// pinned target bodies.
///
/// This is the counterpart to [`run_pipeline_with_diagnostics`] for the
/// codegen `ReinvokeOriginal` path. After the main `Full` pipeline runs rooted
/// at the entry expression, the pinned `ReinvokeOriginal` target bodies are
/// not entry-reachable, so the main pipeline never return-unifies them and they
/// retain early `return`s inside dynamic (measurement-dependent) branches. This
/// sub-pipeline re-roots `return_unify` → `tuple_compare_lower` →
/// `tuple_decompose` at `seeds` (the pinned target and its transitive callees)
/// so those early returns are rewritten into flag-guarded forward control flow
/// that RCA and partial evaluation accept under Adaptive profiles.
///
/// Unlike the main pipeline this sub-pipeline deliberately does not run
/// `monomorphize`, `defunctionalize`, `udt_erase`, or `arg_promote`: the pinned
/// target's signature (arrow params, and any residual UDT/struct shape the main
/// pipeline already erased consistently) must be preserved so the runtime
/// `ReinvokeOriginal` value still matches it. `arg_promote` already skips the
/// pinned target because it is not entry-reachable; the remaining passes are
/// simply not invoked here. Validation therefore uses the off-axis
/// [`InvariantLevel::PostSignaturePreserving`](invariants::InvariantLevel::PostSignaturePreserving)
/// level, which forbids residual `Return` while allowing the preserved
/// arrow/UDT residue.
///
/// A `PackageAssigners` pool is derived from the current package state. Each
/// package's assigner continues allocation from that package's current id
/// watermark, so seed bodies in foreign packages are rewritten with their own
/// package's assigner without colliding with existing ids.
///
/// Returns warnings/fatal errors with the same partitioning contract as
/// [`run_pipeline_with_diagnostics`]: consume the FIR only when
/// [`PipelineResult::errors`] is empty. A fatal `return_unify` diagnostic (an
/// un-rewritable early return) aborts before the `PostSignaturePreserving`
/// check would fail on the residual `Return`.
///
/// # Note
///
/// Like [`run_pipeline_to_with_diagnostics`], this mutates `store` in place and
/// the store must not be reused after the transforms; see that function for the
/// disposable-store contract.
///
/// # Panics
///
/// Panics if the package has no entry expression (the codegen path guarantees
/// one exists after the main pipeline runs).
pub fn run_signature_preserving_subpipeline(
    store: &mut PackageStore,
    package_id: PackageId,
    seeds: &[StoreItemId],
) -> PipelineResult {
    let mut result = PipelineResult::default();

    let mut assigners = PackageAssigners::new(store, package_id);

    let (ru_errors, skipped) =
        return_unify::unify_returns_with_seeds(store, package_id, &mut assigners, seeds);
    let (ru_warnings, ru_fatal): (Vec<_>, Vec<_>) = ru_errors
        .into_iter()
        .partition(return_unify::Error::is_warning);
    result
        .warnings
        .extend(ru_warnings.into_iter().map(PipelineError::from));
    // A fatal return_unify error means the affected callable was intentionally
    // left un-rewritten; abort before the PostSignaturePreserving check would
    // fail on the residual Return nodes.
    if !ru_fatal.is_empty() {
        result.errors = ru_fatal.into_iter().map(PipelineError::from).collect();
        return result;
    }

    tuple_compare_lower::lower_tuple_comparisons_with_seeds(
        store,
        package_id,
        &mut assigners,
        seeds,
    );
    tuple_decompose::tuple_decompose_with_seeds(store, package_id, &mut assigners, seeds);

    // The transforms above added statements and expressions to the pinned
    // bodies, invalidating the exec graphs the main pipeline rebuilt for them.
    // The codegen `ReinvokeOriginal` partial evaluator evaluates classical
    // sub-expressions through `SpecDecl::exec_graph` (sliced by each expr's
    // `exec_graph_range`), so a stale exec graph makes the new return-unify
    // literals evaluate to the wrong value. Re-root the rebuild at `seeds` so
    // the pinned (non-entry-reachable) bodies get fresh exec graphs.
    exec_graph_rebuild::rebuild_exec_graphs(store, package_id, seeds);

    invariants::check_with_skip_and_seeds(
        store,
        package_id,
        invariants::InvariantLevel::PostSignaturePreserving,
        &skipped,
        seeds,
    );

    result
}
