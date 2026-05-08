// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! FIR-to-FIR transformation passes for the Q# compiler.
//!
//! The FIR transform pipeline should run after FIR lowering and before
//! partial evaluation and codegen. It is responsible for monomorphizing
//! generics, eliminating callable-valued expressions, erasing UDTs, and
//! performing various structural rewrites that simplify later stages.
//! The transformations in this crate are not intended to be used as
//! independent passes. Instead, they are ordered and orchestrated by the
//! [`run_pipeline`] function, which applies the full sequence of
//! transformations in one shot. This is because the passes are not designed
//! to be individually sound or to preserve FIR invariants on their own.
//! For example, defunctionalization produces FIR that violates invariants
//! expected by later passes, but the subsequent UDT erasure and tuple
//! comparison lowering restore those invariants before the next major
//! stage (SROA).
//!
//! At the end of the pipeline, the FIR should be in a form that is
//! semantically equivalent to the input but more amenable to partial
//! evaluation and codegen.
//!
//! This crate defines the production FIR rewrite schedule that runs after FIR
//! lowering. The pipeline monomorphizes reachable callables, rewrites returns
//! to a single-exit form, defunctionalizes callable values, erases UDTs,
//! lowers non-empty tuple
//! equality and inequality, scalarizes tuple locals and parameters, and then
//! rebuilds execution-graph metadata.
//!
//! Several passes reuse [`cloner::FirCloner`] for deep-cloning FIR subtrees,
//! while others rewrite nodes in place or rebuild derived structures from
//! scratch.
//!
//! # Cross-pass contracts
//!
//! - **Single [`Assigner`] continuity.** The pipeline constructs one
//!   [`Assigner`] from the input package and threads it through every pass
//!   (`monomorphize`, `return_unify`, `defunctionalize`, `udt_erase`,
//!   `tuple_compare_lower`, `sroa`, `arg_promote`). Each pass allocates fresh
//!   IDs against that shared counter so synthesized nodes from earlier stages
//!   stay disjoint from IDs allocated later. Passes must not construct a
//!   fresh [`Assigner`] mid-pipeline.
//! - **`EMPTY_EXEC_RANGE` sentinel.** Passes that synthesize new
//!   [`fir::Expr`](qsc_fir::fir::Expr) or [`fir::Stmt`](qsc_fir::fir::Stmt)
//!   nodes attach `EMPTY_EXEC_RANGE` as their `exec_graph_range`. The final
//!   [`exec_graph_rebuild`] pass consumes that sentinel and repopulates the
//!   execution graph from the rewritten FIR.
//! - **Pipeline result consumption.** Fatal diagnostics mean the FIR store may
//!   be left at an intermediate stage that does not satisfy the requested
//!   invariant boundary. Callers must only consume the transformed FIR when the
//!   fatal error list is empty. Non-fatal warnings are preserved by
//!   [`PipelineResult`] and do not block successful output.

pub mod cloner;
pub(crate) mod fir_builder;
pub mod invariants;
pub mod pretty;
pub mod reachability;

pub mod arg_promote;
pub mod defunctionalize;
pub mod exec_graph_rebuild;
pub mod gc_unreachable;
pub mod item_dce;
pub mod monomorphize;
pub mod return_unify;
pub mod sroa;
pub mod tuple_compare_lower;
pub mod udt_erase;

#[cfg(any(test, feature = "testutil"))]
pub mod test_utils;

pub(crate) mod walk_utils;

use miette::Diagnostic;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{ExecGraphIdx, ItemKind, PackageId, PackageStore, StoreItemId};
use thiserror::Error;

/// Identifies a specific callable specialization within a package store item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CallableSpecId {
    /// The callable item that owns the specialization.
    pub callable: StoreItemId,
    /// The specialization kind on the callable.
    pub kind: CallableSpecKind,
}

impl CallableSpecId {
    /// Creates a callable specialization identifier.
    #[must_use]
    pub fn new(callable: StoreItemId, kind: CallableSpecKind) -> Self {
        Self { callable, kind }
    }
}

/// Kinds of callable specializations that carry execution graphs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CallableSpecKind {
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

/// Diagnostics produced by the FIR transform pipeline.
///
/// Wraps pass-specific diagnostic types so callers handle a single diagnostic
/// type from [`run_pipeline`], [`run_pipeline_to`], and warning-aware result APIs.
#[derive(Clone, Debug, Diagnostic, Error)]
pub enum PipelineError {
    /// A return-unification error (e.g., unsupported return type inside a loop).
    #[error(transparent)]
    #[diagnostic(transparent)]
    ReturnUnify(#[from] return_unify::Error),

    /// A defunctionalization error (e.g., dynamic callable, convergence failure).
    #[error(transparent)]
    #[diagnostic(transparent)]
    Defunctionalize(#[from] defunctionalize::Error),

    /// A pinned item requested by a caller was not present in the FIR store.
    #[error("pinned item {0} does not exist")]
    #[diagnostic(code("Qsc.FirTransform.MissingPinnedItem"))]
    MissingPinnedItem(StoreItemId),

    /// A pinned item requested by a caller was present but was not a callable.
    #[error("pinned item {0} is not a callable")]
    #[diagnostic(code("Qsc.FirTransform.PinnedItemNotCallable"))]
    PinnedItemNotCallable(StoreItemId),
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
    /// Run through SROA.
    Sroa,
    /// Run through argument promotion.
    ArgPromote,
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
/// If [`return_unify::unify_returns`] or
/// [`defunctionalize::defunctionalize`] reports fatal diagnostics the function
/// returns them immediately, skipping subsequent passes and invariant checks.
/// The intermediate FIR at that point intentionally violates downstream
/// invariants, so running later passes would produce misleading failures.
/// Non-fatal defunctionalization warnings are preserved and the schedule
/// continues to the requested stage.
///
/// `pinned_items` are validated before seeded item DCE and exec graph rebuild.
/// Missing or non-callable pins are fatal diagnostics because pinned items are
/// explicit preservation requests from callers.
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
    let mut result = PipelineResult::default();
    let mut assigner = Assigner::from_package(store.get(package_id));

    monomorphize::monomorphize(store, package_id, &mut assigner);
    invariants::check(store, package_id, invariants::InvariantLevel::PostMono);
    if matches!(stage, PipelineStage::Mono) {
        return result;
    }

    let ru_errors = return_unify::unify_returns(store, package_id, &mut assigner);
    if !ru_errors.is_empty() {
        result.errors = ru_errors.into_iter().map(PipelineError::from).collect();
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

    sroa::sroa(store, package_id, &mut assigner);
    invariants::check(store, package_id, invariants::InvariantLevel::PostSroa);
    if matches!(stage, PipelineStage::Sroa) {
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

/// Runs the authoritative FIR optimization schedule up to the requested stage.
///
/// Production codegen uses this hidden API with [`PipelineStage::Full`] and
/// non-empty `pinned_items` to retain callable IDs that may no longer be
/// entry-reachable after defunctionalization. Intermediate cut points exist so
/// crate tests can reuse the real production ordering without re-implementing
/// it in helper code.
///
/// `pinned_items` must identify existing callable items. Invalid pins are
/// Runs the authoritative FIR optimization schedule up to the requested stage,
/// returning fatal errors and non-fatal warnings separately.
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
/// - SROA (iterative): decomposes tuple-typed locals into scalars
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
