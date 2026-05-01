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
//! `run_pipeline` function, which applies the full sequence of
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

use miette::Diagnostic;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{ExecGraphIdx, PackageId, PackageStore, StoreItemId};
use thiserror::Error;

/// An empty execution graph range for synthesized FIR nodes that do not
/// participate in the execution graph.
pub(crate) const EMPTY_EXEC_RANGE: std::ops::Range<ExecGraphIdx> = std::ops::Range {
    start: ExecGraphIdx::ZERO,
    end: ExecGraphIdx::ZERO,
};

/// Errors produced by the FIR transform pipeline.
///
/// Wraps pass-specific error types so callers handle a single diagnostic
/// type from [`run_pipeline`] and [`run_pipeline_to`].
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
}

/// How far through the FIR transform schedule to run.
///
/// `Sroa`, `ArgPromote`, and `ExecGraphRebuild` are mainly used by tests and
/// internal validation helpers.
/// Production uses `Full`.
#[doc(hidden)]
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

pub mod cloner;
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

/// Runs the FIR transform schedule up to `stage`, threading a single
/// [`Assigner`] through every pass.
///
/// The [`Assigner`] is constructed once from the input package and passed by
/// mutable reference to each pass so ID allocations from earlier stages are
/// observed by later stages. Between major stages the function invokes
/// [`invariants::check`] with the corresponding [`invariants::InvariantLevel`].
///
/// If [`return_unify::unify_returns`] or
/// [`defunctionalize::defunctionalize`] reports any diagnostics the function
/// returns them immediately, skipping subsequent passes and invariant checks.
/// The intermediate FIR at that point intentionally violates downstream
/// invariants, so running later passes would produce misleading failures.
/// Test helpers rely on this early-return to inspect the errors before
/// later invariant checks or downstream passes fail on the intentionally
/// invalid intermediate FIR.
fn run_pipeline_to_impl(
    store: &mut PackageStore,
    package_id: PackageId,
    stage: PipelineStage,
    pinned_items: &[StoreItemId],
) -> Vec<PipelineError> {
    let mut assigner = Assigner::from_package(store.get(package_id));

    monomorphize::monomorphize(store, package_id, &mut assigner);
    invariants::check(store, package_id, invariants::InvariantLevel::PostMono);
    if matches!(stage, PipelineStage::Mono) {
        return Vec::new();
    }

    let ru_errors = return_unify::unify_returns(store, package_id, &mut assigner);
    if !ru_errors.is_empty() {
        return ru_errors.into_iter().map(PipelineError::from).collect();
    }
    invariants::check(
        store,
        package_id,
        invariants::InvariantLevel::PostReturnUnify,
    );
    if matches!(stage, PipelineStage::ReturnUnify) {
        return Vec::new();
    }

    let errors = defunctionalize::defunctionalize(store, package_id, &mut assigner);
    if !errors.is_empty() {
        return errors.into_iter().map(PipelineError::from).collect();
    }

    invariants::check(store, package_id, invariants::InvariantLevel::PostDefunc);
    if matches!(stage, PipelineStage::Defunc) {
        return Vec::new();
    }

    udt_erase::erase_udts(store, package_id, &mut assigner);
    invariants::check(store, package_id, invariants::InvariantLevel::PostUdtErase);
    if matches!(stage, PipelineStage::UdtErase) {
        return Vec::new();
    }

    tuple_compare_lower::lower_tuple_comparisons(store, package_id, &mut assigner);
    invariants::check(
        store,
        package_id,
        invariants::InvariantLevel::PostTupleCompLower,
    );
    if matches!(stage, PipelineStage::TupleCompLower) {
        return Vec::new();
    }

    sroa::sroa(store, package_id, &mut assigner);
    invariants::check(store, package_id, invariants::InvariantLevel::PostSroa);
    if matches!(stage, PipelineStage::Sroa) {
        return Vec::new();
    }

    arg_promote::arg_promote(store, package_id, &mut assigner);
    invariants::check(
        store,
        package_id,
        invariants::InvariantLevel::PostArgPromote,
    );
    if matches!(stage, PipelineStage::ArgPromote) {
        return Vec::new();
    }

    gc_unreachable::gc_unreachable(store.get_mut(package_id));
    invariants::check(store, package_id, invariants::InvariantLevel::PostGc);
    if matches!(stage, PipelineStage::Gc) {
        return Vec::new();
    }

    // Item DCE: remove unreachable callable items and dead type items.
    // Runs as part of the Full pipeline after GC and before exec_graph_rebuild.
    // Callers may pin items via `pinned_items` to keep them (and their
    // transitive dependencies) alive through DCE.
    if store.get(package_id).entry.is_some() {
        let reachable = if pinned_items.is_empty() {
            reachability::collect_reachable_from_entry(store, package_id)
        } else {
            reachability::collect_reachable_with_seeds(store, package_id, pinned_items)
        };
        let removed =
            item_dce::eliminate_dead_items(package_id, store.get_mut(package_id), &reachable);
        if removed > 0 {
            gc_unreachable::gc_unreachable(store.get_mut(package_id));
        }
    }
    if matches!(stage, PipelineStage::ItemDce) {
        return Vec::new();
    }

    exec_graph_rebuild::rebuild_exec_graphs(store, package_id, pinned_items);
    if matches!(stage, PipelineStage::ExecGraphRebuild) {
        return Vec::new();
    }

    invariants::check_with_pinned_items(
        store,
        package_id,
        invariants::InvariantLevel::PostAll,
        pinned_items,
    );
    Vec::new()
}

/// Runs the authoritative FIR optimization schedule up to the requested stage.
///
/// Production uses `PipelineStage::Full`. Intermediate cut points exist so
/// crate tests can reuse the real production ordering without re-implementing
/// it in helper code.
#[doc(hidden)]
pub fn run_pipeline_to(
    store: &mut PackageStore,
    package_id: PackageId,
    stage: PipelineStage,
    pinned_items: &[StoreItemId],
) -> Vec<PipelineError> {
    run_pipeline_to_impl(store, package_id, stage, pinned_items)
}

/// Runs the full FIR optimization pipeline on the given package.
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
/// Returns any errors produced by the transform pipeline. An empty vector
/// indicates success.
pub fn run_pipeline(store: &mut PackageStore, package_id: PackageId) -> Vec<PipelineError> {
    run_pipeline_to(store, package_id, PipelineStage::Full, &[])
}
