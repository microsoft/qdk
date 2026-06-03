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
//!   order: ``monomorphize`` → ``return_unify`` → ``defunctionalize`` → ``udt_erase`` →
//!   ``tuple_compare_lower`` → ``tuple_decompose`` → ``arg_promote`` → ``gc_unreachable`` →
//!   ``item_dce`` → ``exec_graph_rebuild``. Individual passes are *not* sound or
//!   invariant-preserving on their own. A pass deliberately leaves FIR that
//!   violates invariants a later pass relies on (e.g. defunctionalization is
//!   cleaned up by ``udt_erase`` and ``tuple_compare_lower``). Do not reorder, remove,
//!   or run passes in isolation without understanding the chain.
//!
//! - **``tuple_decompose`` ↔ ``arg_promote`` run to a fixed point.** These two passes
//!   iterate until convergence (capped; see the hard-cap constant below), so
//!   changes to either must preserve the strictly-decreasing measure that
//!   guarantees termination.
//!
//! - **One [`Assigner`] is threaded through the whole pipeline.** Passes that
//!   synthesize FIR nodes allocate fresh IDs from this single shared counter so
//!   IDs never collide across stages. Never construct a new [`Assigner`]
//!   mid-pipeline. The trailing metadata passes (``gc_unreachable``, ``item_dce``,
//!   ``exec_graph_rebuild``) don't take it because they only tombstone, delete,
//!   or rebuild derived data and synthesize nothing.
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
//!   [`cloner::FirCloner`]; others rewrite in place or rebuild derived
//!   structures from scratch. [`invariants`] checks the structural contracts
//!   between stages.

pub(crate) mod cloner;
pub(crate) mod fir_builder;
pub mod invariants;
#[cfg(test)]
pub(crate) mod pretty;
pub mod reachability;

pub(crate) mod arg_promote;
pub mod defunctionalize;
pub(crate) mod exec_graph_rebuild;
pub(crate) mod gc_unreachable;
pub(crate) mod intrinsic_precheck;
pub(crate) mod item_dce;
pub(crate) mod monomorphize;
pub(crate) mod return_unify;
pub(crate) mod tuple_compare_lower;
pub(crate) mod tuple_decompose;
pub(crate) mod udt_erase;

#[cfg(any(test, feature = "testutil"))]
pub mod test_utils;

pub(crate) mod walk_utils;

use miette::Diagnostic;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{ExecGraphIdx, ItemKind, PackageId, PackageStore, StoreItemId};
use thiserror::Error;

/// Identifies a specific callable specialization within a package store item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CallableSpecId {
    /// The callable item that owns the specialization.
    pub(crate) callable: StoreItemId,
    /// The specialization kind on the callable.
    pub(crate) kind: CallableSpecKind,
}

impl CallableSpecId {
    /// Creates a callable specialization identifier.
    #[must_use]
    pub(crate) fn new(callable: StoreItemId, kind: CallableSpecKind) -> Self {
        Self { callable, kind }
    }
}

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
    /// Run through unreachable-node garbage collection.
    Gc,
    /// Run through item-level dead code elimination.
    ItemDce,
    /// Run through exec graph rebuild.
    ExecGraphRebuild,
    /// Run the full pipeline.
    Full,
}

/// Runs the FIR transform schedule up to `stage`, threading a single
/// [`Assigner`] through every pass.
///
/// The [`Assigner`] is constructed once from the input package and passed by
/// mutable reference to each pass so ID allocations from earlier stages are
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
#[allow(clippy::too_many_lines)]
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

    let mut assigner = Assigner::from_package(store.get(package_id));

    monomorphize::monomorphize(store, package_id, &mut assigner);
    invariants::check(store, package_id, invariants::InvariantLevel::PostMono);
    if matches!(stage, PipelineStage::Mono) {
        return result;
    }

    let ru_errors = return_unify::unify_returns(store, package_id, &mut assigner);
    let (ru_warnings, ru_fatal): (Vec<_>, Vec<_>) = ru_errors
        .into_iter()
        .partition(return_unify::Error::is_warning);
    result
        .warnings
        .extend(ru_warnings.into_iter().map(PipelineError::from));
    // If any non-warning errors were emitted, the affected callable(s) were
    // intentionally left un-rewritten. Abort before check_no_returns would
    // fail on the residual Return nodes.
    if !ru_fatal.is_empty() {
        result.errors = ru_fatal.into_iter().map(PipelineError::from).collect();
        return result;
    }
    invariants::check(
        store,
        package_id,
        invariants::InvariantLevel::PostReturnUnify,
    );
    if matches!(stage, PipelineStage::ReturnUnify) {
        return result;
    }

    let defunc_diagnostics = defunctionalize::defunctionalize(store, package_id, &mut assigner);
    let (warnings, fatal_errors): (Vec<_>, Vec<_>) = defunc_diagnostics
        .into_iter()
        .partition(defunctionalize::Error::is_warning);
    result.warnings = warnings.into_iter().map(PipelineError::from).collect();
    if !fatal_errors.is_empty() {
        result.errors = fatal_errors.into_iter().map(PipelineError::from).collect();
        return result;
    }

    invariants::check(store, package_id, invariants::InvariantLevel::PostDefunc);
    if matches!(stage, PipelineStage::Defunc) {
        return result;
    }

    let structurally_mutated_specs = udt_erase::erase_udts(store, package_id, &mut assigner);
    invariants::check(store, package_id, invariants::InvariantLevel::PostUdtErase);
    if matches!(stage, PipelineStage::UdtErase) {
        return result;
    }

    tuple_compare_lower::lower_tuple_comparisons(store, package_id, &mut assigner);
    invariants::check(
        store,
        package_id,
        invariants::InvariantLevel::PostTupleCompLower,
    );
    if matches!(stage, PipelineStage::TupleCompLower) {
        return result;
    }

    tuple_decompose::tuple_decompose(store, package_id, &mut assigner);
    invariants::check(
        store,
        package_id,
        invariants::InvariantLevel::PostTupleDecompose,
    );
    if matches!(stage, PipelineStage::TupleDecompose) {
        return result;
    }

    arg_promote::arg_promote(store, package_id, &mut assigner);
    invariants::check(
        store,
        package_id,
        invariants::InvariantLevel::PostArgPromote,
    );
    if matches!(stage, PipelineStage::ArgPromote) {
        return result;
    }

    tuple_decompose_arg_promote_fixed_point(store, package_id, &mut result, &mut assigner);

    // Call-argument-type normalization is idempotent and candidate-neutral, so
    // it is hoisted to run exactly once after the loop converges rather than
    // per round (per-round runs cause `(T,)` wrapping churn that pollutes
    // change detection).
    arg_promote::normalize_reachable_call_arg_types(store, package_id, &mut assigner);
    invariants::check(
        store,
        package_id,
        invariants::InvariantLevel::PostArgPromote,
    );
    if matches!(stage, PipelineStage::TupleDecompose2) {
        return result;
    }

    gc_unreachable::gc_unreachable(store.get_mut(package_id));
    invariants::check(store, package_id, invariants::InvariantLevel::PostGc);
    if matches!(stage, PipelineStage::Gc) {
        return result;
    }

    // Item DCE: remove unreachable callable items and dead type items.
    // Callers may pin items via `pinned_items` to keep them (and their
    // transitive dependencies) alive through DCE and exec-graph-rebuild.
    let pinned_errors = validate_pinned_items(store, pinned_items);
    if !pinned_errors.is_empty() {
        result.errors = pinned_errors;
        return result;
    }
    run_item_dce_and_gc(store, package_id, pinned_items);
    invariants::check(store, package_id, invariants::InvariantLevel::PostItemDce);
    if matches!(stage, PipelineStage::ItemDce) {
        return result;
    }

    let structurally_mutated_external_specs: Vec<_> = structurally_mutated_specs
        .into_iter()
        .filter(|spec_id| spec_id.callable.package != package_id)
        .collect();
    exec_graph_rebuild::rebuild_exec_graphs_with_external_specs(
        store,
        package_id,
        pinned_items,
        &structurally_mutated_external_specs,
    );
    invariants::check_external_spec_exec_graphs(store, &structurally_mutated_external_specs);
    if matches!(stage, PipelineStage::ExecGraphRebuild) {
        return result;
    }

    // PostAll uses entry-only reachability. Pinned items (original target kept
    // for fir_to_qir_from_callable) retain pre-transform types and are not checked.
    invariants::check(store, package_id, invariants::InvariantLevel::PostAll);
    result
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
    assigner: &mut Assigner,
) {
    let mut rounds = 0;
    loop {
        let tuple_decompose_changed = tuple_decompose::tuple_decompose(store, package_id, assigner);
        invariants::check(
            store,
            package_id,
            invariants::InvariantLevel::PostArgPromote,
        );
        let promote_changed = arg_promote::promote_to_fixed_point(store, package_id, assigner);
        invariants::check(
            store,
            package_id,
            invariants::InvariantLevel::PostArgPromote,
        );
        if !tuple_decompose_changed && !promote_changed {
            break;
        }
        rounds += 1;
        if rounds >= TUPLE_DECOMPOSE_ARG_PROMOTE_FIXPOINT_CAP {
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

/// Runs item-level DCE with optional pinned-root expansion, followed by
/// conditional GC if any items were removed.
///
/// Pinned items are validated by `run_pipeline_to_impl` before this helper is
/// called. They are NOT invariant-checked; `PostAll` uses entry-only
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
    let removed = item_dce::eliminate_dead_items(package_id, store.get_mut(package_id), &reachable);
    if removed > 0 {
        gc_unreachable::gc_unreachable(store.get_mut(package_id));
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
/// # Panics
///
/// Panics if the package has no entry expression.
pub fn run_pipeline_with_diagnostics(
    store: &mut PackageStore,
    package_id: PackageId,
) -> PipelineResult {
    run_pipeline_to_with_diagnostics(store, package_id, PipelineStage::Full, &[])
}
