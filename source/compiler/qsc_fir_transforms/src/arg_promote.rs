// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Argument promotion pass — runs after tuple-decompose, before
//! unreachable-node GC; iterates with tuple-decompose to a fixed point (see
//! [`crate`]). Named after LLVM's `ArgumentPromotion`, but operates on tuple
//! aggregates rather than pointers.
//!
//! Decomposes tuple-typed callable parameters into individual scalar
//! parameters, eliminating tuple allocations at call sites and field-access
//! overhead in bodies. `Foo(p : (Int, Qubit))` becomes
//! `Foo(p_0 : Int, p_1 : Qubit)` and call sites pass fields directly.
//!
//! # What to know before diving in
//!
//! - **Establishes [`crate::invariants::InvariantLevel::PostArgPromote`]:**
//!   synthesized input tuple patterns agree with their input types.
//! - **Eligibility + safety filters.** A tuple `PatKind::Bind` parameter is
//!   promoted when it has at least one field access and no promotion-blocking
//!   use (whole-value reads are fine — they are reconstructed from the scalar
//!   leaves). The callable must **not** be used as a first-class value (a
//!   `Var(Res::Item)` with `Ty::Arrow` outside a `Call` callee position) or as
//!   a closure target, since indirect dispatch requires a stable parameter
//!   layout (this also covers partial-application cases).
//! - **Per iteration:** reachability scan → eligibility analysis
//!   ([`check_candidates`]) → safety filters
//!   ([`collect_first_class_callables`], [`collect_closure_targets`]) →
//!   signature/body rewrite ([`promote_callable`]) → call-site rewrite
//!   ([`rewrite_call_sites`]). Peels one tuple nesting level per round, like
//!   tuple-decompose.
//! - **Post-convergence normalization.** [`normalize_reachable_call_arg_types`]
//!   runs once after the fixed point to make argument expression types exactly
//!   match callable input types (e.g. `T` → `(T,)` wrapping for single-element
//!   tuple inputs). Run once, not per round, to avoid `(T,)` churn polluting
//!   change detection.
//! - **Functor-applied callees** (`Adjoint`/`Controlled`) are handled directly:
//!   [`resolve_direct_item_callee`] unwraps the `UnOp` functor wrappers and
//!   [`rewrite_controlled_call_site`] preserves the control-tuple layers and
//!   evaluation order.
//! - Synthesized expressions use `EMPTY_EXEC_RANGE`;
//!   [`crate::exec_graph_rebuild`] rebuilds exec graphs later.

#[cfg(test)]
mod tests;

#[cfg(test)]
mod cross_package_tests;

#[cfg(test)]
mod semantic_equivalence_tests;

use crate::EMPTY_EXEC_RANGE;
use crate::fir_builder::{alloc_local_var_expr, decompose_binding_to_leaves, functored_specs};
use crate::package_assigners::PackageAssigners;
use crate::reachability::collect_reachable_from_entry;
use crate::walk_utils::{
    ParamUse, classify_uses_in_block, collect_expr_ids_in_entry_and_local_callables,
    collect_expr_ids_in_local_callables, for_each_expr, for_each_expr_in_callable_impl,
};
use qsc_data_structures::span::Span;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    Block, CallableDecl, CallableImpl, Expr, ExprId, ExprKind, Field, FieldPath, Functor, Ident,
    ItemKind, LocalItemId, LocalVarId, Mutability, Package, PackageId, PackageLookup, PackageStore,
    Pat, PatId, PatKind, Res, SpecDecl, SpecImpl, Stmt, StmtId, StmtKind, StoreItemId, UnOp,
};
use qsc_fir::ty::{Prim, Ty};
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

/// Base name for the synthesized local that holds a materialized call
/// argument before it is projected into a promoted callable's scalar inputs
/// (see [`create_projection_temp_binding`]). A per-pass counter is appended, so
/// co-resident temps render with distinct suffixes, mirroring return
/// unification's `__operand_tmp_<n>` and condition normalization's
/// `__cond_<n>` schemes.
///
/// The in-memory `Ident.name` carries a `.` sentinel (`_.arg_promote_tmp_0`,
/// `_.arg_promote_tmp_1`, …), which is never a valid Q# identifier character;
/// the Parseable render restores the original `__arg_promote_tmp_0` spelling.
const ARG_PROMOTE_TMP_NAME: &str = "_.arg_promote_tmp";

/// Mints the next `_.arg_promote_tmp_<n>` temporary name and advances `counter`.
fn next_arg_promote_tmp_name(counter: &mut u32) -> String {
    let name = format!("{ARG_PROMOTE_TMP_NAME}_{counter}");
    *counter += 1;
    name
}

/// Leaf-relative remap for a single promoted parameter: maps each positional
/// sub-path within the parameter to the fresh scalar leaf local (and its type)
/// that replaces it during the body field-read rewrite.
type LeafRemap = FxHashMap<Vec<usize>, (LocalVarId, Ty)>;

/// Per-promoted-parameter remap entry: the original parameter local, its full
/// tuple type, and the [`LeafRemap`] used to rewrite the parameter's body field
/// reads to its scalar leaves.
type ParamLeafRemap = (LocalVarId, Ty, LeafRemap);

/// Runs argument promotion across the entry-reachable package closure.
///
/// # Before
/// ```text
/// operation Foo(p : (Int, Qubit)) : Unit { use(p::0); apply(p::1); }
/// Foo((42, q));
/// ```
///
/// # After
/// ```text
/// operation Foo(p_0 : Int, p_1 : Qubit) : Unit { use(p_0); apply(p_1); }
/// Foo(42, q);
/// ```
///
/// # Requires
/// - `package_id` exists in `store`.
/// - `assigners` supplies each package's id arena, preserving id continuity
///   across passes.
/// - Package with `package_id` has an entry expression.
///
/// # Ensures
/// - Rewrites only entry-reachable callables.
/// - Leaves first-class and closure-target callables unchanged.
/// - Normalizes call argument shapes to match callable input types via
///   [`normalize_reachable_call_arg_types`].
///
/// # Mutations
/// - Rewrites callable input patterns and specialization bodies.
/// - Rewrites direct call expressions targeting promoted callables.
/// - Allocates fresh FIR nodes via each package's assigner with `EMPTY_EXEC_RANGE`.
///
/// # Panics
///
/// Panics if the package has no entry expression. The reachability scans
/// in this pass go through [`collect_reachable_from_entry`], which asserts
/// `package.entry.is_some()`.
pub fn arg_promote(
    store: &mut PackageStore,
    package_id: PackageId,
    assigners: &mut PackageAssigners,
) -> bool {
    let mut tmp_counter: u32 = 0;
    let changed = promote_to_fixed_point(store, package_id, assigners, &mut tmp_counter);
    normalize_reachable_call_arg_types(store, package_id, assigners);
    changed
}

/// Iterates promotion rounds until no more candidates are found.
///
/// Each iteration peels one level of tuple nesting from eligible parameters,
/// rewrites their bodies and call sites, then recomputes reachability for
/// the next round.
///
/// `tmp_counter` names each minted projection temporary `_.arg_promote_tmp_<n>`;
/// it is owned by the caller so suffixes stay distinct across every round and
/// across interleaved tuple-decompose / arg-promote rounds in the full pipeline.
///
/// # Returns
///
/// `true` if any promotion or normalize rewrite was applied; `false` otherwise.
pub(crate) fn promote_to_fixed_point(
    store: &mut PackageStore,
    package_id: PackageId,
    assigners: &mut PackageAssigners,
    tmp_counter: &mut u32,
) -> bool {
    let mut changed = false;
    loop {
        let candidates = find_promotion_candidates(store, package_id);
        if candidates.is_empty() {
            break;
        }
        changed = true;
        apply_promotions(store, package_id, assigners, &candidates, tmp_counter);
    }
    changed
}

/// Finds all eligible promotion candidates in the current reachable set,
/// excluding callables used as first-class values or closure targets.
fn find_promotion_candidates(
    store: &PackageStore,
    package_id: PackageId,
) -> Vec<ArgPromoCandidate> {
    let reachable = collect_reachable_from_entry(store, package_id);

    // The entry callable lives in the true entry package only; resolving it
    // there keeps its input ABI excluded from flattening regardless of how many
    // other packages are reachable.
    let entry_item = resolve_entry_callable_item(store.get(package_id), package_id);
    // Safety filters are unioned across every reachable package: a callable used
    // first-class (or as a closure target) in any package must keep a stable
    // parameter layout, even when its declaration lives in another package.
    let first_class = collect_first_class_callables(store, package_id, &reachable);
    let closure_targets = collect_closure_targets(store, package_id, &reachable);

    // Candidate discovery spans the whole reachable package closure. Each
    // reachable callable is inspected in its own owning package so a library
    // callee is promoted in lockstep with its (possibly cross-package) call
    // sites.
    let mut candidates: Vec<ArgPromoCandidate> = Vec::new();
    for store_id in &reachable {
        let owner = *store_id;
        let package = store.get(owner.package);
        let Some(item) = package.items.get(owner.item) else {
            continue;
        };
        let ItemKind::Callable(decl) = &item.kind else {
            continue;
        };
        // The entry-point callable's input is the program's externally-visible
        // ABI and must never be flattened, regardless of its input shape.
        // This is a forward looking check as all inputs are currently `Unit`
        if Some(owner) == entry_item {
            continue;
        }
        if first_class.contains(&owner) || closure_targets.contains(&owner) {
            continue;
        }
        // Skip intrinsics entirely: their signatures must stay tuple-shaped and
        // simulatable bodies are never analyzed or rewritten. Any invalid types will
        // fail later in code generation
        if matches!(
            decl.implementation,
            CallableImpl::Intrinsic | CallableImpl::SimulatableIntrinsic(_)
        ) {
            continue;
        }
        candidates.extend(check_candidates(package, owner, decl));
    }
    candidates
}

/// Applies a batch of promotion candidates: decomposes parameters, rewrites
/// bodies, and rewrites call sites scoped to reachable expressions.
///
/// Promotion is **atomic across the reachable package closure** within a single
/// round: every promoted signature is rewritten in its owning package, then
/// *all* call sites — in every reachable package body — are rewritten against
/// the same set of promotions before the round returns. This keeps each
/// promoted callable's arity and parameter types in lockstep with its call
/// sites, even when callers and callees live in different packages.
fn apply_promotions(
    store: &mut PackageStore,
    package_id: PackageId,
    assigners: &mut PackageAssigners,
    candidates: &[ArgPromoCandidate],
    tmp_counter: &mut u32,
) {
    // Group candidates by their declaring callable so each callable's entire
    // input is flattened exactly once, dissolving all inter-parameter
    // grouping. Preserving first-seen order keeps ID allocation deterministic
    // per `FxHasher` (whose iteration is seedless and reproducible), not by
    // arena order.
    let mut order: Vec<StoreItemId> = Vec::new();
    let mut groups: FxHashMap<StoreItemId, Vec<&ArgPromoCandidate>> = FxHashMap::default();
    for candidate in candidates {
        if !groups.contains_key(&candidate.item_id) {
            order.push(candidate.item_id);
        }
        groups.entry(candidate.item_id).or_default().push(candidate);
    }

    // Promote each callable's signature and body in its owning package, grouped
    // by package so each package's fresh ids come from its own assigner.
    let mut owner_pkgs: Vec<PackageId> = Vec::new();
    for store_id in &order {
        if !owner_pkgs.contains(&store_id.package) {
            owner_pkgs.push(store_id.package);
        }
    }

    let mut promotions: Vec<PromotionResult> = Vec::new();
    for owner_pkg in owner_pkgs {
        let assigner = assigners.get_mut(store, owner_pkg);
        let package = store.get_mut(owner_pkg);
        for store_id in order.iter().filter(|s| s.package == owner_pkg) {
            let cands = &groups[store_id];
            if let Some(result) = promote_callable(package, assigner, *store_id, cands) {
                promotions.push(result);
            }
        }
    }

    if promotions.is_empty() {
        return;
    }

    // In the same round, rewrite every reachable call site against the
    // promotions just applied. A promoted callable in one package may be called
    // from another, so scan all reachable bodies and leave no call site with a
    // stale tuple argument shape. Projection temporaries are minted into the
    // caller's package via that package's own assigner.
    let promoted_map: FxHashMap<StoreItemId, PromotionResult> =
        promotions.into_iter().map(|p| (p.item_id, p)).collect();

    let reachable = collect_reachable_from_entry(store, package_id);
    let mut caller_pkgs: Vec<PackageId> = Vec::new();
    for store_id in &reachable {
        if !caller_pkgs.contains(&store_id.package) {
            caller_pkgs.push(store_id.package);
        }
    }
    for caller_pkg in caller_pkgs {
        let local_item_ids: Vec<LocalItemId> = reachable
            .iter()
            .filter(|s| s.package == caller_pkg)
            .map(|s| s.item)
            .collect();
        let package = store.get(caller_pkg);
        let reachable_expr_ids = if caller_pkg == package_id {
            collect_expr_ids_in_entry_and_local_callables(package, &local_item_ids)
        } else {
            collect_expr_ids_in_local_callables(package, &local_item_ids)
        };
        let assigner = assigners.get_mut(store, caller_pkg);
        let package = store.get_mut(caller_pkg);
        rewrite_call_sites(
            package,
            assigner,
            &promoted_map,
            &reachable_expr_ids,
            tmp_counter,
        );
    }
}

/// Normalizes call-argument types across all reachable call sites after
/// promotion has converged.
///
/// Runs once after the whole-closure fixed point: the expected input type of
/// every reachable callable is snapshotted up front (keyed by `StoreItemId`, so
/// foreign callees resolve correctly), then each reachable package body is
/// normalized against that snapshot with the package's own assigner.
pub(crate) fn normalize_reachable_call_arg_types(
    store: &mut PackageStore,
    package_id: PackageId,
    assigners: &mut PackageAssigners,
) {
    let reachable = collect_reachable_from_entry(store, package_id);

    // Snapshot every reachable callable's current input type, package-qualified,
    // so a direct call to a foreign promoted callee resolves to the callee's own
    // (already-promoted) input shape rather than the caller package's arena.
    let mut callable_inputs: FxHashMap<StoreItemId, Ty> = FxHashMap::default();
    for store_id in &reachable {
        let package = store.get(store_id.package);
        if let Some(item) = package.items.get(store_id.item)
            && let ItemKind::Callable(decl) = &item.kind
        {
            callable_inputs.insert(*store_id, package.get_pat(decl.input).ty.clone());
        }
    }

    let mut caller_pkgs: Vec<PackageId> = Vec::new();
    for store_id in &reachable {
        if !caller_pkgs.contains(&store_id.package) {
            caller_pkgs.push(store_id.package);
        }
    }
    for caller_pkg in caller_pkgs {
        let local_item_ids: Vec<LocalItemId> = reachable
            .iter()
            .filter(|s| s.package == caller_pkg)
            .map(|s| s.item)
            .collect();
        let package = store.get(caller_pkg);
        let reachable_expr_ids = if caller_pkg == package_id {
            collect_expr_ids_in_entry_and_local_callables(package, &local_item_ids)
        } else {
            collect_expr_ids_in_local_callables(package, &local_item_ids)
        };
        let assigner = assigners.get_mut(store, caller_pkg);
        let package = store.get_mut(caller_pkg);
        normalize_call_arg_types(package, &callable_inputs, assigner, &reachable_expr_ids);
    }
}

/// A candidate for argument promotion.
struct ArgPromoCandidate {
    /// The `StoreItemId` of the callable (package-qualified so candidates from
    /// foreign reachable packages never alias entry-package `LocalItemId`s).
    item_id: StoreItemId,
    /// The `LocalVarId` bound by the parameter.
    local_id: LocalVarId,
    /// Expression ids of the parameter's standalone whole-value reads. These
    /// sites are reconstructed from the parameter's scalar leaves during the
    /// body rewrite so they keep observing the original tuple value.
    whole_value_reads: Vec<ExprId>,
}

/// Result of promoting a callable — tracks the callable and the flat scalar
/// leaves of its fully-decomposed input so that call sites can be
/// rewritten to pass the flattened arguments.
#[derive(Clone)]
struct PromotionResult {
    /// The callable's `StoreItemId` (package-qualified).
    item_id: StoreItemId,
    /// One entry per scalar leaf of the callable's flattened input: the
    /// positional path of the leaf in the original (nested) input type and
    /// the leaf's type. The path projects the leaf from the original
    /// argument value at each call site. Promotable parameters contribute one
    /// entry per scalar leaf; kept (non-promotable) parameters contribute a
    /// single entry projecting their whole value.
    leaves: Vec<(Vec<usize>, Ty)>,
}

/// Collects the promotable tuple-typed parameter bindings of a callable.
/// Recurses into `PatKind::Tuple` sub-patterns to find inner bindings that
/// became eligible after a previous pass peeled an outer tuple level.
fn check_candidates(
    package: &Package,
    owner: StoreItemId,
    decl: &CallableDecl,
) -> Vec<ArgPromoCandidate> {
    let mut candidates = Vec::new();
    find_param_binds_in_pat(package, owner, decl, decl.input, &mut candidates);
    candidates
}

/// Recursively walks a callable's input pattern to find promotable
/// tuple-typed `PatKind::Bind` nodes (see [`param_is_promotable`]).
fn find_param_binds_in_pat(
    package: &Package,
    owner: StoreItemId,
    decl: &CallableDecl,
    pat_id: PatId,
    candidates: &mut Vec<ArgPromoCandidate>,
) {
    let pat = package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => {
            let is_tuple = matches!(&pat.ty, Ty::Tuple(elems) if !elems.is_empty());
            if is_tuple {
                let local_id = ident.id;
                let uses = classify_param_uses(package, decl, local_id);
                if let Some(whole_value_reads) = param_is_promotable(&uses) {
                    candidates.push(ArgPromoCandidate {
                        item_id: owner,
                        local_id,
                        whole_value_reads,
                    });
                }
            }
        }
        PatKind::Tuple(sub_pats) => {
            for &sub_pat_id in sub_pats {
                find_param_binds_in_pat(package, owner, decl, sub_pat_id, candidates);
            }
        }
        PatKind::Discard => {}
    }
}

/// Classifies every use of `local_id` across all specialization bodies of the
/// callable, returning the flat list of [`ParamUse`] classifications.
///
/// Only `CallableImpl::Spec` callables ever reach this function: the intrinsic
/// gate in `find_promotion_candidates` skips both `Intrinsic` and
/// `SimulatableIntrinsic` callables before any candidate is constructed, so the
/// non-`Spec` arms are unreachable.
fn classify_param_uses(
    package: &Package,
    decl: &CallableDecl,
    local_id: LocalVarId,
) -> Vec<ParamUse> {
    match &decl.implementation {
        CallableImpl::Spec(spec_impl) => classify_uses_in_spec_impl(package, spec_impl, local_id),
        // Dead arm: gated by the intrinsic skip in `find_promotion_candidates`
        CallableImpl::Intrinsic => unreachable!(
            "intrinsic callables are skipped by the intrinsic gate in \
             find_promotion_candidates before any candidate reaches \
             classify_param_uses"
        ),
        // Dead arm: same intrinsic gate as the `Intrinsic` arm above.
        CallableImpl::SimulatableIntrinsic(_) => unreachable!(
            "simulatable-intrinsic callables are skipped by the intrinsic gate in \
             find_promotion_candidates before any candidate reaches \
             classify_param_uses"
        ),
    }
}

/// Classifies every use of `local_id` across the body and all functored
/// specializations (adjoint, controlled, controlled-adjoint).
fn classify_uses_in_spec_impl(
    package: &Package,
    spec_impl: &SpecImpl,
    local_id: LocalVarId,
) -> Vec<ParamUse> {
    let mut uses = Vec::new();
    classify_uses_in_spec(package, &spec_impl.body, local_id, &mut uses);
    for spec in functored_specs(spec_impl) {
        classify_uses_in_spec(package, spec, local_id, &mut uses);
    }
    uses
}

/// Appends the classified uses of `local_id` in a single `SpecDecl` body to
/// `out` (per the classifier in [`classify_uses_in_block`]).
fn classify_uses_in_spec(
    package: &Package,
    spec: &SpecDecl,
    local_id: LocalVarId,
    out: &mut Vec<ParamUse>,
) {
    classify_uses_in_block(package, spec.block, local_id, out);
}

/// Decides whether a parameter is promotable from its classified uses and, when
/// it is, returns the expression ids of its standalone whole-value reads.
///
/// Promotion is blocked when any use hard-blocks it. Otherwise the parameter is
/// promotable when it has at least one field-access use, which skips pure
/// pass-through parameters (zero field uses) that gain nothing from flattening.
/// The returned whole-value read sites are reconstructed during the body
/// rewrite so they keep observing the original tuple value.
fn param_is_promotable(uses: &[ParamUse]) -> Option<Vec<ExprId>> {
    let mut field = 0_usize;
    let mut whole_value_reads = Vec::new();
    for use_kind in uses {
        match use_kind {
            ParamUse::HardBlock => return None,
            ParamUse::WholeValueRead(expr_id) => whole_value_reads.push(*expr_id),
            ParamUse::FieldAccess => field += 1,
        }
    }
    (field >= 1).then_some(whole_value_reads)
}

/// Collects the `StoreItemId`s of callables that appear as `Var(Res::Item(id))`
/// with an `Arrow` type (i.e., used as a first-class value rather than as the
/// callee of `Call`) anywhere in the reachable package closure.
///
/// Traversal is delegated to the shared [`for_each_expr`] /
/// [`for_each_expr_in_callable_impl`] walkers; only the first-class
/// classification is specific to this pass. A direct call —
/// `Call(Var(Item), _)` or a functor-applied direct call
/// `Call(UnOp(_, Var(Item)), _)` — does not count its callee as first-class.
/// Because the walk is pre-order, each `Call` is visited before its callee, so
/// recording the direct-callee position first lets the later `Var` visit skip
/// it.
///
/// The first-class use is recorded against the referenced item's own package, so
/// a foreign (e.g. library) callable used as a first-class value in another
/// package is excluded from promotion regardless of which package body the use
/// appears in.
fn collect_first_class_callables(
    store: &PackageStore,
    package_id: PackageId,
    reachable: &FxHashSet<StoreItemId>,
) -> FxHashSet<StoreItemId> {
    let mut first_class = FxHashSet::default();

    // Distinct packages owning reachable items, scanned independently because
    // the `direct_callees` set is keyed by `ExprId`, which is only unique within
    // a single package's arena.
    let mut pkgs: Vec<PackageId> = Vec::new();
    for store_id in reachable {
        if !pkgs.contains(&store_id.package) {
            pkgs.push(store_id.package);
        }
    }

    for pkg in pkgs {
        let package = store.get(pkg);
        let mut direct_callees: FxHashSet<ExprId> = FxHashSet::default();

        let mut visit = |expr_id: ExprId, expr: &Expr| match &expr.kind {
            ExprKind::Call(callee, _) => match &package.get_expr(*callee).kind {
                ExprKind::Var(Res::Item(_), _) => {
                    direct_callees.insert(*callee);
                }
                ExprKind::UnOp(_, inner)
                    if matches!(
                        package.get_expr(*inner).kind,
                        ExprKind::Var(Res::Item(_), _)
                    ) =>
                {
                    direct_callees.insert(*inner);
                }
                _ => {}
            },
            ExprKind::Var(Res::Item(item_id), _)
                if matches!(&expr.ty, Ty::Arrow(_)) && !direct_callees.contains(&expr_id) =>
            {
                first_class.insert(StoreItemId {
                    package: item_id.package,
                    item: item_id.item,
                });
            }
            _ => {}
        };

        // Scan the entry expression (entry package only).
        if pkg == package_id
            && let Some(entry_id) = package.entry
        {
            for_each_expr(package, entry_id, &mut visit);
        }

        // Scan every reachable callable body in this package.
        for store_id in reachable.iter().filter(|s| s.package == pkg) {
            if let ItemKind::Callable(decl) = &package.get_item(store_id.item).kind {
                for_each_expr_in_callable_impl(package, &decl.implementation, &mut visit);
            }
        }
    }

    first_class
}

/// Collects the `StoreItemId`s of callables that are targets of closure-like
/// dispatch in the reachable package closure. Before defunctionalization this
/// is a direct `Closure(_, local_item_id)` expression. After defunctionalization
/// an indexed closure-array dispatch branch calls the target item directly but
/// still uses the closure-call ABI `(captures..., original_args)`, so the target
/// signature must remain stable for those call sites.
fn collect_closure_targets(
    store: &PackageStore,
    package_id: PackageId,
    reachable: &FxHashSet<StoreItemId>,
) -> FxHashSet<StoreItemId> {
    let mut targets = FxHashSet::default();

    let mut pkgs: Vec<PackageId> = Vec::new();
    for store_id in reachable {
        if !pkgs.contains(&store_id.package) {
            pkgs.push(store_id.package);
        }
    }

    for pkg in pkgs {
        let package = store.get(pkg);

        if pkg == package_id
            && let Some(entry_id) = package.entry
        {
            for_each_expr(package, entry_id, &mut |_expr_id, expr| {
                if let ExprKind::Closure(_, local_item_id) = &expr.kind {
                    targets.insert(StoreItemId {
                        package: pkg,
                        item: *local_item_id,
                    });
                }
                collect_closure_abi_direct_call_target(package, pkg, expr, &mut targets);
            });
        }

        for store_id in reachable.iter().filter(|s| s.package == pkg) {
            let item = package.get_item(store_id.item);
            if let ItemKind::Callable(decl) = &item.kind {
                for_each_expr_in_callable_impl(
                    package,
                    &decl.implementation,
                    &mut |_expr_id, expr| {
                        if let ExprKind::Closure(_, local_item_id) = &expr.kind {
                            targets.insert(StoreItemId {
                                package: pkg,
                                item: *local_item_id,
                            });
                        }
                        collect_closure_abi_direct_call_target(package, pkg, expr, &mut targets);
                    },
                );
            }
        }
    }

    targets
}

/// Records `expr`'s callee as a closure-ABI dispatch target when `expr` is a
/// same-package direct call that still passes the grouped closure-call payload
/// `(captures..., original_args)`.
///
/// This is the post-defunctionalization case flagged by
/// [`collect_closure_targets`]: an indexed closure-array branch lowers to a
/// direct `Call(Var(Res::Item(id)), args)`, but the argument tuple keeps the
/// closure-call ABI shape rather than the callable's own parameter shape. Such
/// a target's signature must stay stable, so it is excluded from promotion.
///
/// Only in-package callees are recorded (a foreign item's arity is fixed by its
/// own package's passes, so it is never a promotion candidate here). Non-calls,
/// calls through non-item callees, and calls that use the plain direct-call
/// argument shape are ignored via [`call_uses_grouped_closure_payload`].
fn collect_closure_abi_direct_call_target(
    package: &Package,
    package_id: PackageId,
    expr: &Expr,
    targets: &mut FxHashSet<StoreItemId>,
) {
    // Only interested in call expressions...
    let ExprKind::Call(callee_id, arg_id) = expr.kind else {
        return;
    };
    // ...whose callee is a direct reference to a named item (not a first-class
    // value or a functor-applied callee).
    let callee_expr = package.get_expr(callee_id);
    let ExprKind::Var(Res::Item(item_id), _) = callee_expr.kind else {
        return;
    };
    // A foreign callee's arity is governed by its own package, so it is never a
    // promotion candidate we need to protect here.
    if item_id.package != package_id {
        return;
    }
    // Only record it if the call still passes the grouped closure payload; a
    // plain direct call with the callable's own argument shape is safe to
    // promote and must not be excluded.
    if !call_uses_grouped_closure_payload(package, item_id.item, arg_id, Some(&callee_expr.ty)) {
        return;
    }
    targets.insert(StoreItemId {
        package: item_id.package,
        item: item_id.item,
    });
}

/// Reports whether a call's argument tuple is shaped like the closure-call ABI
/// `(captures..., original_args)` rather than the callee's own parameter tuple.
///
/// The closure-call ABI passes each captured variable as a leading scalar and
/// bundles the callable's original arguments into a single trailing tuple, so
/// for a target whose flattened input is `(c0, c1, a0, a1)` the call site looks
/// like:
///
/// ```text
/// // target signature (flattened): (c0, c1, a0, a1)
/// f(cap0, cap1, (arg0, arg1))     // grouped closure payload  -> true
/// f(cap0, cap1, arg0, arg1)       // plain direct-call shape   -> false
/// ```
///
/// The shape is confirmed by matching arities: after replacing the trailing
/// tuple with its own elements, the total argument count must equal the
/// target's parameter count. The target's parameter tuple is read from
/// `callee_ty` when it is an `Arrow` (the call-site view), falling back to the
/// item's declared input pattern otherwise.
fn call_uses_grouped_closure_payload(
    package: &Package,
    item_id: LocalItemId,
    arg_id: ExprId,
    callee_ty: Option<&Ty>,
) -> bool {
    // The argument must be a tuple with at least a capture and the trailing
    // grouped-args tuple.
    let ExprKind::Tuple(args) = &package.get_expr(arg_id).kind else {
        return false;
    };
    if args.len() < 2 {
        return false;
    }

    // Determine the target's parameter tuple: prefer the call site's arrow view
    // of the callee, falling back to the item's declared input pattern type.
    let item_input = || {
        let Some(ItemKind::Callable(decl)) = package.items.get(item_id).map(|item| &item.kind)
        else {
            return None;
        };
        Some(package.get_pat(decl.input).ty.clone())
    };
    let target_input = match callee_ty {
        Some(Ty::Arrow(arrow)) => arrow.input.as_ref().clone(),
        _ => item_input().unwrap_or(Ty::Err),
    };
    let Ty::Tuple(target_items) = target_input else {
        return false;
    };
    // Under the grouped ABI the target has strictly more parameters than the
    // call has arguments (the trailing tuple stands in for several of them).
    if target_items.len() <= args.len() {
        return false;
    }

    // The trailing argument must itself be a tuple (the bundled original args).
    let trailing_arg_ty = &package
        .get_expr(*args.last().expect("args is non-empty"))
        .ty;
    let Ty::Tuple(trailing_items) = trailing_arg_ty else {
        return false;
    };
    // Confirm the shape by arity: leading captures plus the unbundled trailing
    // args must exactly account for every target parameter.
    args.len() - 1 + trailing_items.len() == target_items.len()
}

/// Flattens an entire callable input into one flat tuple of scalar leaves,
/// dissolving all inter-parameter grouping, then remaps every promotable
/// parameter's body field reads to its scalar leaves.
///
/// Every promotable parameter (those in `candidates`) is decomposed to its
/// scalar leaves; every other parameter (non-tuple, or a tuple read as a
/// whole value) is kept as a single leaf. The leaves of all parameters are
/// concatenated into one flat input tuple, so a multi-parameter callable such
/// as `Add(a : (Int, Int), b : (Int, Int))` flattens to
/// `Add(a_0 : Int, a_1 : Int, b_0 : Int, b_1 : Int)`, and a mixed callable
/// `UsePair(p : (Int, Int), q : Qubit)` flattens to
/// `UsePair(p_0 : Int, p_1 : Int, q : Qubit)` (keeping `q` as a singleton).
///
/// # Before
/// ```text
/// decl.input = Tuple([Bind(a : (Int, Int)), Bind(b : (Int, Int))])
/// body:  Field(Var(Local(a)), Path([0])); Field(Var(Local(b)), Path([1]))
/// ```
/// # After
/// ```text
/// decl.input = Tuple([Bind(a_0 : Int), Bind(a_1 : Int),
///                     Bind(b_0 : Int), Bind(b_1 : Int)])
/// body:  Var(Local(a_0)); Var(Local(b_1))
/// ```
///
/// # Mutations
/// - Rewrites `decl.input`'s `Pat` node (kind + `ty`) in place to the flat
///   tuple, and refreshes every specialization input `ty` to match.
/// - Allocates new `LocalVarId`/`PatId` leaf nodes through `assigner`.
/// - Remaps body expressions of every promoted parameter to read the
///   decomposed leaf locals.
///
/// # Returns
///
/// A `PromotionResult` whose `leaves` lists every flat input leaf with its
/// absolute positional path in the original (nested) input type, used to
/// rewrite call sites. Returns `None` only if the item is not a callable.
fn promote_callable(
    package: &mut Package,
    assigner: &mut Assigner,
    item_id: StoreItemId,
    candidates: &[&ArgPromoCandidate],
) -> Option<PromotionResult> {
    let input_pat_id = {
        let item = package.get_item(item_id.item);
        let ItemKind::Callable(decl) = &item.kind else {
            return None;
        };
        decl.input
    };

    // The set of parameter locals to expand to scalar leaves. Every other
    // parameter is kept as a single leaf.
    let promotable: FxHashSet<LocalVarId> = candidates.iter().map(|c| c.local_id).collect();

    // Recursively rebuild the input pattern into a flat list of leaf binds,
    // recording each leaf's absolute path/type and, per promoted parameter,
    // the leaf-relative map used to remap its body field reads.
    let mut leaf_pat_ids: Vec<PatId> = Vec::new();
    let mut leaf_entries: Vec<(Vec<usize>, Ty)> = Vec::new();
    let mut remaps: Vec<ParamLeafRemap> = Vec::new();
    let mut index_path: Vec<usize> = Vec::new();
    rebuild_input_leaves(
        package,
        assigner,
        input_pat_id,
        &mut index_path,
        &promotable,
        &mut leaf_pat_ids,
        &mut leaf_entries,
        &mut remaps,
    );

    // Set the callable input pattern to the flat tuple of leaf binds, in
    // lockstep with its flat tuple type. Controlled/adjoint specs share this
    // payload pattern node, so the in-place mutation is visible to them.
    let leaf_tys: Vec<Ty> = leaf_entries.iter().map(|(_, ty)| ty.clone()).collect();
    let pat = package
        .pats
        .get_mut(input_pat_id)
        .expect("input pat should exist");
    pat.kind = PatKind::Tuple(leaf_pat_ids);
    pat.ty = Ty::Tuple(leaf_tys);

    // Refresh every specialization input pattern's tuple type so the wrapper
    // control layers (e.g. `(ctls, payload)`) pick up the flattened payload.
    refresh_spec_input_types(package, item_id.item);

    // Remap each promoted parameter's body field reads to its scalar leaves;
    // interior whole-tuple reads are reconstructed as nested leaf tuples.
    // Each parameter's recorded whole-value read sites are carried alongside
    // so the body rewrite can reconstruct those standalone reads.
    let reads_by_local: FxHashMap<LocalVarId, &[ExprId]> = candidates
        .iter()
        .map(|c| (c.local_id, c.whole_value_reads.as_slice()))
        .collect();
    for (old_local, param_ty, leaf_map) in &remaps {
        let whole_value_reads = reads_by_local.get(old_local).copied().unwrap_or(&[]);
        rewrite_leaf_field_accesses(
            package,
            assigner,
            item_id.item,
            *old_local,
            param_ty,
            leaf_map,
            whole_value_reads,
        );
    }

    Some(PromotionResult {
        item_id,
        leaves: leaf_entries,
    })
}

/// Recursively rebuilds a callable input subtree into a flat list of leaf
/// binds, dissolving tuple grouping.
///
/// `index_path` carries the cumulative positional path from `decl.input` to
/// the current pattern; it is pushed/popped around each tuple element so
/// callers observe it unchanged on return.
///
/// - A `Bind` of a promotable parameter is decomposed (via
///   [`decompose_binding_to_leaves`]) into scalar-leaf binds, which are
///   hoisted directly into the flat leaf list (not left nested). The
///   parameter's leaf-relative `(path -> (local, ty))` map and full type are
///   recorded in `remaps` for body remapping.
/// - Any other `Bind` (non-tuple, or a tuple read as a whole value) and any
///   `Discard` is kept as a single leaf, reusing the existing pattern node.
/// - A `Tuple` recurses into each element and concatenates the children's
///   leaves, which is what flattens nested grouping.
#[allow(clippy::too_many_arguments)]
fn rebuild_input_leaves(
    package: &mut Package,
    assigner: &mut Assigner,
    pat_id: PatId,
    index_path: &mut Vec<usize>,
    promotable: &FxHashSet<LocalVarId>,
    leaf_pat_ids: &mut Vec<PatId>,
    leaf_entries: &mut Vec<(Vec<usize>, Ty)>,
    remaps: &mut Vec<ParamLeafRemap>,
) {
    let pat = package.get_pat(pat_id);
    let pat_ty = pat.ty.clone();
    let kind = pat.kind.clone();
    match kind {
        PatKind::Bind(ident) if promotable.contains(&ident.id) => {
            // Decompose this promotable parameter to a flat tuple of scalar
            // leaves in place, then hoist those leaf pat ids up into the
            // enclosing flat list (dissolving the per-parameter grouping).
            let rel_leaves =
                decompose_binding_to_leaves(package, assigner, pat_id, &ident.name, &pat_ty);
            let child_pat_ids = match &package.get_pat(pat_id).kind {
                PatKind::Tuple(children) => children.clone(),
                _ => unreachable!("decompose_binding_to_leaves sets a Tuple pattern"),
            };
            leaf_pat_ids.extend(child_pat_ids);

            let mut leaf_map: LeafRemap = FxHashMap::default();
            for (rel_path, leaf_local, leaf_ty) in &rel_leaves {
                let mut full_path = index_path.clone();
                full_path.extend_from_slice(rel_path);
                leaf_entries.push((full_path, leaf_ty.clone()));
                leaf_map.insert(rel_path.clone(), (*leaf_local, leaf_ty.clone()));
            }
            remaps.push((ident.id, pat_ty, leaf_map));
        }
        PatKind::Bind(_) | PatKind::Discard => {
            // Kept parameter: a single leaf projecting the whole value.
            leaf_pat_ids.push(pat_id);
            leaf_entries.push((index_path.clone(), pat_ty));
        }
        PatKind::Tuple(sub_pats) => {
            for (i, sub_pat_id) in sub_pats.into_iter().enumerate() {
                index_path.push(i);
                rebuild_input_leaves(
                    package,
                    assigner,
                    sub_pat_id,
                    index_path,
                    promotable,
                    leaf_pat_ids,
                    leaf_entries,
                    remaps,
                );
                index_path.pop();
            }
        }
    }
}

/// Recomputes the tuple types of every specialization input pattern of a
/// callable bottom-up from their child pattern types.
///
/// After a top-level parameter is flattened, the controlled/adjoint
/// specialization input patterns (which wrap the shared payload pattern in
/// control layers, e.g. `(ctls, payload)`) must have their tuple types
/// refreshed so the pattern shape continues to match the type shape, as
/// required by the `PostArgPromote` tuple-pattern invariant.
fn refresh_spec_input_types(package: &mut Package, item_id: LocalItemId) {
    let spec_input_pats: Vec<PatId> = {
        let item = package.get_item(item_id);
        let ItemKind::Callable(decl) = &item.kind else {
            return;
        };
        match &decl.implementation {
            CallableImpl::Spec(spec_impl) => functored_specs(spec_impl)
                .filter_map(|spec| spec.input)
                .chain(spec_impl.body.input)
                .collect(),
            CallableImpl::SimulatableIntrinsic(spec) => spec.input.into_iter().collect(),
            CallableImpl::Intrinsic => Vec::new(),
        }
    };
    for pat_id in spec_input_pats {
        refresh_pat_tuple_ty(package, pat_id);
    }
}

/// Recomputes a pattern's tuple type from its children, recursively. Leaf
/// (`Bind`/`Discard`) pattern types are authoritative and left unchanged.
fn refresh_pat_tuple_ty(package: &mut Package, pat_id: PatId) {
    let sub_pat_ids = match &package.get_pat(pat_id).kind {
        PatKind::Tuple(sub_pats) => sub_pats.clone(),
        PatKind::Bind(_) | PatKind::Discard => return,
    };
    let mut elem_tys = Vec::with_capacity(sub_pat_ids.len());
    for &sub_pat_id in &sub_pat_ids {
        refresh_pat_tuple_ty(package, sub_pat_id);
        elem_tys.push(package.get_pat(sub_pat_id).ty.clone());
    }
    package.pats.get_mut(pat_id).expect("pat should exist").ty = Ty::Tuple(elem_tys);
}

/// Remaps every body field read of a fully-flattened parameter to the
/// matching scalar leaf local, scoped to the promoted callable's bodies.
///
/// `whole_value_reads` carries the parameter's standalone whole-value read
/// sites so they can be reconstructed from the scalar leaves; it is consumed by
/// the standalone-read rewrite.
fn rewrite_leaf_field_accesses(
    package: &mut Package,
    assigner: &mut Assigner,
    item_id: LocalItemId,
    old_local: LocalVarId,
    param_ty: &Ty,
    leaf_map: &LeafRemap,
    whole_value_reads: &[ExprId],
) {
    let expr_ids = collect_expr_ids_in_local_callables(&*package, &[item_id]);
    for expr_id in expr_ids {
        rewrite_single_leaf_field_expr(package, assigner, expr_id, old_local, param_ty, leaf_map);
    }

    // Reconstruct each standalone whole-value read of the now-flattened
    // parameter. These are the exact `Var(old_local)` sites that are not the
    // base of a field projection (field bases are consumed when their parent
    // `Field` node is rewritten above), so reconstructing them in place is
    // safe and never clobbers a `Field(Var(old_local), Path)` base.
    for &expr_id in whole_value_reads {
        reconstruct_whole_value_read(package, assigner, expr_id, old_local, param_ty, leaf_map);
    }
}

/// Reconstructs a single standalone whole-value `Var(old_local)` read of a
/// fully-flattened parameter into a (possibly nested) tuple of its scalar leaf
/// `Var`s, overwriting the node's kind and type in place so the reconstructed
/// value has the same shape and type as the original parameter.
fn reconstruct_whole_value_read(
    package: &mut Package,
    assigner: &mut Assigner,
    expr_id: ExprId,
    old_local: LocalVarId,
    param_ty: &Ty,
    leaf_map: &LeafRemap,
) {
    let expr = package.exprs.get(expr_id).expect("expr should exist");
    let ExprKind::Var(Res::Local(var_id), _) = &expr.kind else {
        return;
    };
    if *var_id != old_local {
        return;
    }

    let new_id = build_leaf_tuple(package, assigner, param_ty, &[], leaf_map);
    let kind = package
        .exprs
        .get(new_id)
        .expect("rebuilt expr exists")
        .kind
        .clone();
    let ty = package
        .exprs
        .get(new_id)
        .expect("rebuilt expr exists")
        .ty
        .clone();
    let expr_mut = package.exprs.get_mut(expr_id).expect("expr exists");
    expr_mut.kind = kind;
    expr_mut.ty = ty;
}

/// Rewrites a single body expression that projects a field of the fully
/// flattened parameter.
///
/// An exact-path read (`Field(Var(old), Path(p))` where `p` is a leaf path)
/// becomes a direct `Var(leaf)`. An interior whole-tuple read (`p` is a strict
/// prefix of one or more leaf paths) is reconstructed as a nested
/// `Tuple([Var(leaf), ...])` of all leaves under `p`, so callers that read a
/// sub-tuple of the parameter whole still observe the same value.
fn rewrite_single_leaf_field_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    expr_id: ExprId,
    old_local: LocalVarId,
    param_ty: &Ty,
    leaf_map: &LeafRemap,
) {
    let expr = package.exprs.get(expr_id).expect("expr should exist");
    let ExprKind::Field(inner_id, Field::Path(path)) = expr.kind.clone() else {
        return;
    };
    let inner = package
        .exprs
        .get(inner_id)
        .expect("inner expr should exist");
    let ExprKind::Var(Res::Local(var_id), _) = &inner.kind else {
        return;
    };
    if *var_id != old_local || path.indices.is_empty() {
        return;
    }

    if let Some((leaf_local, leaf_ty)) = leaf_map.get(&path.indices) {
        let leaf_local = *leaf_local;
        let leaf_ty = leaf_ty.clone();
        let expr_mut = package.exprs.get_mut(expr_id).expect("expr exists");
        expr_mut.kind = ExprKind::Var(Res::Local(leaf_local), vec![]);
        expr_mut.ty = leaf_ty;
    } else {
        // Interior whole-tuple read: reconstruct a nested tuple of the leaf
        // locals under this prefix path.
        let new_id = build_leaf_tuple(package, assigner, param_ty, &path.indices, leaf_map);
        let kind = package
            .exprs
            .get(new_id)
            .expect("rebuilt expr exists")
            .kind
            .clone();
        let ty = package
            .exprs
            .get(new_id)
            .expect("rebuilt expr exists")
            .ty
            .clone();
        let expr_mut = package.exprs.get_mut(expr_id).expect("expr exists");
        expr_mut.kind = kind;
        expr_mut.ty = ty;
    }
}

/// Reconstructs a (possibly nested) tuple of leaf-local `Var`s for the
/// sub-tree of `param_ty` rooted at `prefix`, used for interior whole-tuple
/// reads of a flattened parameter.
fn build_leaf_tuple(
    package: &mut Package,
    assigner: &mut Assigner,
    param_ty: &Ty,
    prefix: &[usize],
    leaf_map: &LeafRemap,
) -> ExprId {
    if let Some((leaf_local, leaf_ty)) = leaf_map.get(prefix) {
        return alloc_local_var_expr(
            package,
            assigner,
            *leaf_local,
            leaf_ty.clone(),
            Span::default(),
        );
    }

    let sub_ty = navigate_tuple_ty(param_ty, prefix);
    let Ty::Tuple(elems) = sub_ty else {
        // Defensive totality: every non-tuple leaf path is present in `leaf_map`
        // (handled by the early return above), so this fallback is unreachable for
        // well-formed flattened inputs. Fall back to a unit tuple to keep the
        // rewrite total.
        let expr_id = assigner.next_expr();
        package.exprs.insert(
            expr_id,
            Expr {
                id: expr_id,
                span: Span::default(),
                ty: sub_ty.clone(),
                kind: ExprKind::Tuple(vec![]),
                exec_graph_range: EMPTY_EXEC_RANGE,
            },
        );
        return expr_id;
    };

    let mut child_ids = Vec::with_capacity(elems.len());
    let mut child_path = prefix.to_vec();
    for i in 0..elems.len() {
        child_path.push(i);
        child_ids.push(build_leaf_tuple(
            package,
            assigner,
            param_ty,
            &child_path,
            leaf_map,
        ));
        child_path.pop();
    }

    let expr_id = assigner.next_expr();
    package.exprs.insert(
        expr_id,
        Expr {
            id: expr_id,
            span: Span::default(),
            ty: sub_ty.clone(),
            kind: ExprKind::Tuple(child_ids),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    expr_id
}

/// Navigates a (possibly nested) tuple type by a positional `path`, returning
/// the type at that path.
fn navigate_tuple_ty<'a>(ty: &'a Ty, path: &[usize]) -> &'a Ty {
    let mut current = ty;
    for &index in path {
        match current {
            Ty::Tuple(elems) => {
                current = elems.get(index).expect("path index within tuple arity");
            }
            // Dead arm: `build_leaf_tuple` recurses only on `Ty::Tuple` and
            // intercepts leaves via `leaf_map` before recursing, so a non-tuple
            // type never reaches here for well-formed flattened inputs.
            _ => panic!("path navigates into non-tuple type"),
        }
    }
    current
}

/// Rewrites all call sites for promoted callables. At each direct item call,
/// including `Call(UnOp(Functor, Var(Item(id))), arg)`, where `id` is a
/// promoted callable, replaces the payload tuple argument with explicit field
/// extractions wrapped in a `Tuple`.
///
/// # Before
/// ```text
/// Foo(struct_arg)   // single composite argument
/// ```
/// # After
/// ```text
/// Foo((struct_arg.0, struct_arg.1))   // explicit field projections
/// ```
///
/// # Mutations
/// - Rewrites call-site `Expr.kind` in place or wraps in a block when
///   a temporary is needed to avoid evaluating the argument multiple times.
/// - Allocates field-projection and tuple `Expr` nodes through `assigner`.
fn rewrite_call_sites(
    package: &mut Package,
    assigner: &mut Assigner,
    promoted_map: &FxHashMap<StoreItemId, PromotionResult>,
    reachable_expr_ids: &[ExprId],
    tmp_counter: &mut u32,
) {
    // Collect all call-site ExprIds that target a promoted callable.
    let call_sites: Vec<(ExprId, StoreItemId, usize)> = reachable_expr_ids
        .iter()
        .filter_map(|&expr_id| {
            let expr = package.exprs.get(expr_id)?;
            if let ExprKind::Call(callee_id, _) = &expr.kind {
                let callee =
                    resolve_promoted_direct_item_callee(package, *callee_id, promoted_map)?;
                return Some((expr_id, callee.item_id, callee.controlled_depth));
            }
            None
        })
        .collect();

    for (call_expr_id, item_id, controlled_depth) in call_sites {
        let promotion = promoted_map
            .get(&item_id)
            .expect("promotion should exist for promoted item");
        if controlled_depth == 0 {
            rewrite_single_call_site(package, assigner, call_expr_id, promotion, tmp_counter);
        } else {
            rewrite_controlled_call_site(
                package,
                assigner,
                call_expr_id,
                promotion,
                controlled_depth,
                tmp_counter,
            );
        }
    }
}

#[derive(Clone, Copy)]
struct DirectItemCallee {
    item_id: StoreItemId,
    controlled_depth: usize,
}

/// Resolves `callee_id` as a promoted direct item callee, including functor
/// wrappers around the direct item reference.
fn resolve_promoted_direct_item_callee(
    package: &Package,
    callee_id: ExprId,
    promoted: &FxHashMap<StoreItemId, PromotionResult>,
) -> Option<DirectItemCallee> {
    let callee = resolve_direct_item_callee(package, callee_id)?;
    promoted.contains_key(&callee.item_id).then_some(callee)
}

/// Resolves a callee expression to its `StoreItemId`, unwrapping adjoint and
/// controlled functor applications while counting controlled layers.
///
/// The resolved [`StoreItemId`] preserves the callee's own package, so a direct
/// call to a foreign (e.g. library) callable is resolved across package
/// boundaries — required for cross-package call-site rewriting after promotion.
fn resolve_direct_item_callee(package: &Package, callee_id: ExprId) -> Option<DirectItemCallee> {
    let mut current = callee_id;
    let mut controlled_depth = 0usize;

    loop {
        let expr = package.exprs.get(current)?;
        match &expr.kind {
            ExprKind::Var(Res::Item(item_id), _) => {
                return Some(DirectItemCallee {
                    item_id: StoreItemId {
                        package: item_id.package,
                        item: item_id.item,
                    },
                    controlled_depth,
                });
            }
            ExprKind::UnOp(UnOp::Functor(Functor::Adj), inner_id) => {
                current = *inner_id;
            }
            ExprKind::UnOp(UnOp::Functor(Functor::Ctl), inner_id) => {
                controlled_depth += 1;
                current = *inner_id;
            }
            _ => return None,
        }
    }
}

/// Resolves the entry-point callable's [`StoreItemId`] from `package.entry`.
///
/// The entry callable's input is the program's externally-visible ABI and must
/// never be flattened by argument promotion. The entry expression is a direct
/// `Call(callee, _)`; its callee is resolved via [`resolve_direct_item_callee`]
/// so adjoint/controlled functor wrappers are unwrapped. Returns `None` when
/// there is no entry expression or it is not a direct call, leaving behavior
/// unchanged in those cases.
fn resolve_entry_callable_item(package: &Package, _package_id: PackageId) -> Option<StoreItemId> {
    let entry_id = package.entry?;
    if let ExprKind::Call(callee_id, _) = &package.get_expr(entry_id).kind {
        resolve_direct_item_callee(package, *callee_id).map(|c| c.item_id)
    } else {
        None
    }
}

/// Returns `true` when an argument expression can be projected repeatedly
/// without side effects (e.g. literals, plain `Var` references), letting
/// the caller inline each projected field without introducing a
/// temporary.
fn expr_is_safe_to_project_repeatedly(package: &Package, expr_id: ExprId) -> bool {
    match &package.get_expr(expr_id).kind {
        ExprKind::Var(Res::Local(_), _) => true,
        ExprKind::Field(inner_id, Field::Path(_)) => {
            expr_is_safe_to_project_repeatedly(package, *inner_id)
        }
        _ => false,
    }
}

/// Creates a temporary `let temp = arg_expr;` binding for argument
/// expressions that cannot be projected repeatedly without
/// side-effect duplication. The caller replaces subsequent field
/// projections with references to `temp`.
///
/// The temp is named `_.arg_promote_tmp_<n>` via [`next_arg_promote_tmp_name`],
/// advancing `tmp_counter` so co-resident temps render with distinct suffixes.
///
/// # Before
/// ```text
/// (no binding)
/// ```
/// # After
/// ```text
/// let __arg_promote_tmp_0 : T = arg_expr;
/// ```
///
/// # Mutations
/// - Allocates a new `Pat`, `LocalVarId`, and `Stmt` through `assigner`.
fn create_projection_temp_binding(
    package: &mut Package,
    assigner: &mut Assigner,
    arg_id: ExprId,
    arg_ty: &Ty,
    tmp_counter: &mut u32,
) -> (LocalVarId, StmtId) {
    let local_id = assigner.next_local();
    let pat_id = assigner.next_pat();
    let temp_name = next_arg_promote_tmp_name(tmp_counter);
    package.pats.insert(
        pat_id,
        Pat {
            id: pat_id,
            span: Span::default(),
            ty: arg_ty.clone(),
            kind: PatKind::Bind(Ident {
                id: local_id,
                span: Span::default(),
                name: Rc::from(temp_name.as_str()),
            }),
        },
    );

    let stmt_id = assigner.next_stmt();
    package.stmts.insert(
        stmt_id,
        Stmt {
            id: stmt_id,
            span: Span::default(),
            kind: StmtKind::Local(Mutability::Immutable, pat_id, arg_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    (local_id, stmt_id)
}

/// Returns `true` when the promotion leaf at `path` can be projected out of the
/// tuple-literal argument `arg_id` by reusing existing sub-expressions, without
/// introducing a temporary.
///
/// Navigation descends through nested tuple literals. Once a non-literal
/// sub-expression is reached with path remaining, the remainder is a field
/// projection that is only duplication-safe when that sub-expression is itself
/// safe to project repeatedly. A leaf whose path is fully consumed by tuple
/// literals lands on a sub-expression that is referenced exactly once, so it is
/// always safe to reuse in place.
fn leaf_projects_through_tuple_literal(package: &Package, arg_id: ExprId, path: &[usize]) -> bool {
    let mut current = arg_id;
    let mut rest = path;
    while !rest.is_empty() {
        let ExprKind::Tuple(elems) = &package.get_expr(current).kind else {
            return expr_is_safe_to_project_repeatedly(package, current);
        };
        let Some(&next) = elems.get(rest[0]) else {
            return false;
        };
        current = next;
        rest = &rest[1..];
    }
    true
}

/// Projects the promotion leaf at `path` out of the tuple-literal argument
/// `arg_id`, reusing existing sub-expressions in place. Descends through nested
/// tuple literals; if a non-literal sub-expression is reached with path
/// remaining, a `Field` projection of that sub-expression is allocated.
///
/// Callers must first confirm the leaf is projectable via
/// [`leaf_projects_through_tuple_literal`].
fn project_leaf_through_tuple_literal(
    package: &mut Package,
    assigner: &mut Assigner,
    arg_id: ExprId,
    path: &[usize],
    leaf_ty: &Ty,
) -> ExprId {
    let mut current = arg_id;
    let mut rest = path;
    while !rest.is_empty() {
        let next = {
            let ExprKind::Tuple(elems) = &package.get_expr(current).kind else {
                break;
            };
            elems[rest[0]]
        };
        current = next;
        rest = &rest[1..];
    }

    if rest.is_empty() {
        return current;
    }

    let field_expr_id = assigner.next_expr();
    package.exprs.insert(
        field_expr_id,
        Expr {
            id: field_expr_id,
            span: Span::default(),
            ty: leaf_ty.clone(),
            kind: ExprKind::Field(
                current,
                Field::Path(FieldPath {
                    indices: rest.to_vec(),
                }),
            ),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    field_expr_id
}

/// Attempts to build the flat projected tuple argument directly from a
/// tuple-literal argument by reusing each leaf sub-expression in place, instead
/// of binding the whole argument to a temporary and projecting from it.
///
/// Returns `None` when the argument is not a tuple literal, or when some leaf
/// would require duplicating a sub-expression that is not safe to project
/// repeatedly, in which case the caller falls back to a temporary binding.
///
/// # Before
/// ```text
/// Foo(((a, b), c - 1))   // nested tuple literal argument
/// ```
/// # After
/// ```text
/// Foo((a, b, c - 1))     // flat leaf projection, no temporary
/// ```
///
/// This keeps a promoted multi-leaf call site in clean flat form with no
/// surviving projection temporary, the common shape for promoted self-calls and
/// tuple-literal arguments.
///
/// # Mutations
/// - Allocates per-leaf `Field` `Expr` nodes (only for residual sub-paths) and
///   the outer `Tuple` `Expr` through `assigner`.
fn try_inline_tuple_literal_projection(
    package: &mut Package,
    assigner: &mut Assigner,
    promotion: &PromotionResult,
    arg_id: ExprId,
) -> Option<ExprId> {
    if !matches!(package.get_expr(arg_id).kind, ExprKind::Tuple(_)) {
        return None;
    }
    if !promotion
        .leaves
        .iter()
        .all(|(path, _)| leaf_projects_through_tuple_literal(package, arg_id, path))
    {
        return None;
    }

    let field_expr_ids: Vec<ExprId> = promotion
        .leaves
        .iter()
        .map(|(path, leaf_ty)| {
            project_leaf_through_tuple_literal(package, assigner, arg_id, path, leaf_ty)
        })
        .collect();

    let tuple_ty = Ty::Tuple(
        promotion
            .leaves
            .iter()
            .map(|(_, leaf_ty)| leaf_ty.clone())
            .collect(),
    );
    let new_arg_id = assigner.next_expr();
    package.exprs.insert(
        new_arg_id,
        Expr {
            id: new_arg_id,
            span: Span::default(),
            ty: tuple_ty,
            kind: ExprKind::Tuple(field_expr_ids),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    Some(new_arg_id)
}

/// Builds the projected tuple that replaces the original tuple argument at
/// a call site, projecting each flat scalar leaf of the promoted callable's
/// (fully decomposed) parameter from the original argument value.
///
/// # Before
/// ```text
/// (no expression)
/// ```
/// # After
/// ```text
/// Tuple([Field(arg, Path(p_0)), ..., Field(arg, Path(p_{n-1}))])
/// ```
/// where each `p_i` is the positional path of a leaf in the original
/// (possibly nested) parameter type.
///
/// # Mutations
/// - Allocates per-leaf `Field` `Expr` nodes and the outer `Tuple`
///   `Expr` through `assigner`.
fn create_projected_tuple_arg(
    package: &mut Package,
    assigner: &mut Assigner,
    promotion: &PromotionResult,
    arg_id: ExprId,
    arg_ty: &Ty,
    temp_local: Option<LocalVarId>,
) -> ExprId {
    let mut field_expr_ids: Vec<ExprId> = Vec::with_capacity(promotion.leaves.len());

    for (path, leaf_ty) in &promotion.leaves {
        let field_base_id = if let Some(temp_local) = temp_local {
            alloc_local_var_expr(
                package,
                assigner,
                temp_local,
                arg_ty.clone(),
                Span::default(),
            )
        } else {
            arg_id
        };
        let field_expr_id = assigner.next_expr();
        let field_expr = qsc_fir::fir::Expr {
            id: field_expr_id,
            span: Span::default(),
            ty: leaf_ty.clone(),
            kind: ExprKind::Field(
                field_base_id,
                Field::Path(FieldPath {
                    indices: path.clone(),
                }),
            ),
            exec_graph_range: EMPTY_EXEC_RANGE,
        };
        package.exprs.insert(field_expr_id, field_expr);
        field_expr_ids.push(field_expr_id);
    }

    let new_arg_id = assigner.next_expr();
    let tuple_ty = Ty::Tuple(
        promotion
            .leaves
            .iter()
            .map(|(_, leaf_ty)| leaf_ty.clone())
            .collect(),
    );
    let new_arg = qsc_fir::fir::Expr {
        id: new_arg_id,
        span: Span::default(),
        ty: tuple_ty,
        kind: ExprKind::Tuple(field_expr_ids),
        exec_graph_range: EMPTY_EXEC_RANGE,
    };
    package.exprs.insert(new_arg_id, new_arg);
    new_arg_id
}

/// Wraps a single promoted payload expression in a one-element tuple argument.
fn create_single_tuple_arg(
    package: &mut Package,
    assigner: &mut Assigner,
    arg_id: ExprId,
    elem_types: &[Ty],
) -> ExprId {
    let new_arg_id = assigner.next_expr();
    let new_arg = qsc_fir::fir::Expr {
        id: new_arg_id,
        span: Span::default(),
        ty: Ty::Tuple(elem_types.to_vec()),
        kind: ExprKind::Tuple(vec![arg_id]),
        exec_graph_range: EMPTY_EXEC_RANGE,
    };
    package.exprs.insert(new_arg_id, new_arg);
    new_arg_id
}

/// Builds a block expression that evaluates a leading statement before
/// returning `result_expr_id`.
fn create_payload_block(
    package: &mut Package,
    assigner: &mut Assigner,
    leading_stmt_id: StmtId,
    result_expr_id: ExprId,
) -> ExprId {
    let result_ty = package.get_expr(result_expr_id).ty.clone();

    let result_stmt_id = assigner.next_stmt();
    package.stmts.insert(
        result_stmt_id,
        Stmt {
            id: result_stmt_id,
            span: Span::default(),
            kind: StmtKind::Expr(result_expr_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    let block_id = assigner.next_block();
    package.blocks.insert(
        block_id,
        Block {
            id: block_id,
            span: Span::default(),
            ty: result_ty.clone(),
            stmts: vec![leading_stmt_id, result_stmt_id],
        },
    );

    let block_expr_id = assigner.next_expr();
    package.exprs.insert(
        block_expr_id,
        Expr {
            id: block_expr_id,
            span: Span::default(),
            ty: result_ty,
            kind: ExprKind::Block(block_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    block_expr_id
}

/// Returns `true` when `elems` is already the fully-flattened argument list:
/// one element per promotion leaf, each carrying the leaf's scalar type. A
/// top-level arity match alone is insufficient, because an element may itself
/// be a nested tuple (for example a single-field struct erased to a 1-tuple)
/// that still needs projection into the flat leaf list.
fn arg_tuple_matches_flat_leaves(
    package: &Package,
    elems: &[ExprId],
    promotion: &PromotionResult,
) -> bool {
    elems.len() == promotion.leaves.len()
        && elems
            .iter()
            .zip(&promotion.leaves)
            .all(|(elem_id, (_, leaf_ty))| {
                package
                    .exprs
                    .get(*elem_id)
                    .expect("arg element expr exists")
                    .ty
                    == *leaf_ty
            })
}

/// Creates a promoted payload argument, returning `None` when the existing
/// payload already has the expected tuple shape.
fn create_rewritten_payload_arg(
    package: &mut Package,
    assigner: &mut Assigner,
    promotion: &PromotionResult,
    arg_id: ExprId,
    tmp_counter: &mut u32,
) -> Option<ExprId> {
    let arg_expr = package.exprs.get(arg_id).expect("arg expr exists");
    let arg_ty = arg_expr.ty.clone();
    let arg_tuple_elems = match &arg_expr.kind {
        ExprKind::Tuple(elems) => Some(elems.clone()),
        _ => None,
    };

    if let Some(elems) = &arg_tuple_elems
        && arg_tuple_matches_flat_leaves(package, elems, promotion)
    {
        return None;
    }

    if promotion.leaves.len() == 1 {
        let leaf_tys: Vec<Ty> = promotion.leaves.iter().map(|(_, ty)| ty.clone()).collect();
        return Some(create_single_tuple_arg(
            package, assigner, arg_id, &leaf_tys,
        ));
    }

    if let Some(new_arg_id) =
        try_inline_tuple_literal_projection(package, assigner, promotion, arg_id)
    {
        return Some(new_arg_id);
    }

    let temp_binding = if expr_is_safe_to_project_repeatedly(package, arg_id) {
        None
    } else {
        Some(create_projection_temp_binding(
            package,
            assigner,
            arg_id,
            &arg_ty,
            tmp_counter,
        ))
    };
    let new_arg_id = create_projected_tuple_arg(
        package,
        assigner,
        promotion,
        arg_id,
        &arg_ty,
        temp_binding.map(|(temp_local, _)| temp_local),
    );

    Some(if let Some((_, temp_stmt_id)) = temp_binding {
        create_payload_block(package, assigner, temp_stmt_id, new_arg_id)
    } else {
        new_arg_id
    })
}

/// Wraps an existing `Call` expression in a synthesized block that places
/// a pre-built leading statement (typically a temporary binding) before
/// the call, preserving evaluation order.
///
/// # Before
/// ```text
/// call_expr_id = Call(callee_id, _)
/// ```
/// # After
/// ```text
/// call_expr_id = Block {
///     leading_stmt;                       // supplied by caller
///     Expr(Call(callee_id, new_arg_id))   // inner call with rewritten args
/// }
/// ```
///
/// # Mutations
/// - Replaces `call_expr_id`'s `ExprKind` with `Block(..)` in place.
/// - Allocates inner `Call`, `Stmt`, and `Block` nodes through `assigner`.
fn wrap_call_in_block(
    package: &mut Package,
    assigner: &mut Assigner,
    call_expr_id: ExprId,
    callee_id: ExprId,
    new_arg_id: ExprId,
    call_ty: &Ty,
    leading_stmt_id: StmtId,
) {
    let inner_call_id = assigner.next_expr();
    package.exprs.insert(
        inner_call_id,
        Expr {
            id: inner_call_id,
            span: Span::default(),
            ty: call_ty.clone(),
            kind: ExprKind::Call(callee_id, new_arg_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    let call_stmt_id = assigner.next_stmt();
    package.stmts.insert(
        call_stmt_id,
        Stmt {
            id: call_stmt_id,
            span: Span::default(),
            kind: StmtKind::Expr(inner_call_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );

    let block_id = assigner.next_block();
    package.blocks.insert(
        block_id,
        Block {
            id: block_id,
            span: Span::default(),
            ty: call_ty.clone(),
            stmts: vec![leading_stmt_id, call_stmt_id],
        },
    );

    let call_mut = package
        .exprs
        .get_mut(call_expr_id)
        .expect("call expr exists");
    call_mut.kind = ExprKind::Block(block_id);
}

/// Rewrites a single call site: `Foo(arg)` → `Foo((arg.0, arg.1, ...))`.
///
/// # Before
/// ```text
/// Call(Var(Foo), composite_arg)
/// ```
/// # After
/// ```text
/// Call(Var(Foo), Tuple([arg.0, arg.1, ...]))   // or Block wrapping
/// ```
///
/// If the argument is already a `Tuple(...)` with the correct arity, the
/// existing tuple elements are used directly. Otherwise, field-extraction
/// expressions are created.
///
/// # Mutations
/// - Rewrites `call_expr_id`'s `ExprKind` in place.
/// - May allocate projection, tuple, and temporary `Expr`/`Stmt` nodes
///   through `assigner`.
fn rewrite_single_call_site(
    package: &mut Package,
    assigner: &mut Assigner,
    call_expr_id: ExprId,
    promotion: &PromotionResult,
    tmp_counter: &mut u32,
) {
    let call_expr = package.exprs.get(call_expr_id).expect("call expr exists");
    let ExprKind::Call(callee_id, arg_id) = call_expr.kind else {
        return;
    };
    let call_ty = call_expr.ty.clone();

    let arg_expr = package.exprs.get(arg_id).expect("arg expr exists");
    let arg_ty = arg_expr.ty.clone();
    let arg_tuple_elems = match &arg_expr.kind {
        ExprKind::Tuple(elems) => Some(elems.clone()),
        _ => None,
    };

    // If the argument is already a flat tuple literal whose elements match the
    // promotion leaf types, the call site is already structured correctly.
    if let Some(elems) = &arg_tuple_elems
        && arg_tuple_matches_flat_leaves(package, elems, promotion)
    {
        return;
    }

    if promotion.leaves.len() == 1 {
        let leaf_tys: Vec<Ty> = promotion.leaves.iter().map(|(_, ty)| ty.clone()).collect();
        let new_arg_id = create_single_tuple_arg(package, assigner, arg_id, &leaf_tys);

        let call_mut = package
            .exprs
            .get_mut(call_expr_id)
            .expect("call expr exists");
        call_mut.kind = ExprKind::Call(callee_id, new_arg_id);
        return;
    }

    if let Some(new_arg_id) =
        try_inline_tuple_literal_projection(package, assigner, promotion, arg_id)
    {
        let call_mut = package
            .exprs
            .get_mut(call_expr_id)
            .expect("call expr exists");
        call_mut.kind = ExprKind::Call(callee_id, new_arg_id);
        return;
    }

    let temp_binding = if expr_is_safe_to_project_repeatedly(package, arg_id) {
        None
    } else {
        Some(create_projection_temp_binding(
            package,
            assigner,
            arg_id,
            &arg_ty,
            tmp_counter,
        ))
    };
    let new_arg_id = create_projected_tuple_arg(
        package,
        assigner,
        promotion,
        arg_id,
        &arg_ty,
        temp_binding.map(|(temp_local, _)| temp_local),
    );

    if let Some((_, temp_stmt_id)) = temp_binding {
        wrap_call_in_block(
            package,
            assigner,
            call_expr_id,
            callee_id,
            new_arg_id,
            &call_ty,
            temp_stmt_id,
        );
    } else {
        let call_mut = package
            .exprs
            .get_mut(call_expr_id)
            .expect("call expr exists");
        call_mut.kind = ExprKind::Call(callee_id, new_arg_id);
    }
}

/// Rewrites the payload portion of a controlled call while preserving the
/// existing control layers and their evaluation order.
fn rewrite_controlled_call_site(
    package: &mut Package,
    assigner: &mut Assigner,
    call_expr_id: ExprId,
    promotion: &PromotionResult,
    controlled_depth: usize,
    tmp_counter: &mut u32,
) {
    let call_expr = package.exprs.get(call_expr_id).expect("call expr exists");
    let ExprKind::Call(callee_id, arg_id) = call_expr.kind else {
        return;
    };

    let Some((control_ids, payload_id)) =
        peel_controlled_arg_layers(package, arg_id, controlled_depth)
    else {
        return;
    };

    let Some(new_payload_id) =
        create_rewritten_payload_arg(package, assigner, promotion, payload_id, tmp_counter)
    else {
        return;
    };

    let new_arg_id = rebuild_controlled_arg_layers(package, assigner, &control_ids, new_payload_id);
    let call_mut = package
        .exprs
        .get_mut(call_expr_id)
        .expect("call expr exists");
    call_mut.kind = ExprKind::Call(callee_id, new_arg_id);
}

/// Peels nested controlled-call argument tuples into their control expressions
/// and the final payload expression.
fn peel_controlled_arg_layers(
    package: &Package,
    arg_id: ExprId,
    controlled_depth: usize,
) -> Option<(Vec<ExprId>, ExprId)> {
    let mut control_ids = Vec::with_capacity(controlled_depth);
    let mut current = arg_id;

    for _ in 0..controlled_depth {
        let expr = package.exprs.get(current)?;
        let ExprKind::Tuple(items) = &expr.kind else {
            return None;
        };
        let [controls, payload] = items.as_slice() else {
            return None;
        };
        control_ids.push(*controls);
        current = *payload;
    }

    Some((control_ids, current))
}

/// Rebuilds controlled-call argument tuple layers around a rewritten payload.
fn rebuild_controlled_arg_layers(
    package: &mut Package,
    assigner: &mut Assigner,
    control_ids: &[ExprId],
    payload_id: ExprId,
) -> ExprId {
    let mut current = payload_id;

    for &controls in control_ids.iter().rev() {
        let tuple_ty = Ty::Tuple(vec![
            package.get_expr(controls).ty.clone(),
            package.get_expr(current).ty.clone(),
        ]);
        let tuple_id = assigner.next_expr();
        package.exprs.insert(
            tuple_id,
            Expr {
                id: tuple_id,
                span: Span::default(),
                ty: tuple_ty,
                kind: ExprKind::Tuple(vec![controls, current]),
                exec_graph_range: EMPTY_EXEC_RANGE,
            },
        );
        current = tuple_id;
    }

    current
}

/// Normalizes call argument expression shapes to exactly match callee input
/// types.
///
/// This pass is intentionally run after fixed-point promotion converges,
/// because prior rewrites can leave call arguments with shape-equivalent but
/// type-distinct forms (most notably `T` vs `(T,)` for single-element tuples).
///
/// # Before
/// ```text
/// operation UseOne(p : (Qubit[],)) : Unit { ... }
/// UseOne(qs);        // arg type Qubit[]
/// ```
///
/// # After
/// ```text
/// operation UseOne(p : (Qubit[],)) : Unit { ... }
/// UseOne((qs,));     // arg type (Qubit[],)
/// ```
///
/// # Ensures
/// - For every direct call expression, argument type structure matches the
///   expected callable input type where normalization can be done locally.
/// - Does not rewrite callee declarations; only argument expression shape.
fn normalize_call_arg_types(
    package: &mut Package,
    callable_inputs: &FxHashMap<StoreItemId, Ty>,
    assigner: &mut Assigner,
    reachable_expr_ids: &[ExprId],
) {
    let call_sites: Vec<(ExprId, Ty)> = reachable_expr_ids
        .iter()
        .filter_map(|&expr_id| {
            let expr = package.exprs.get(expr_id)?;
            let ExprKind::Call(callee_id, arg_id) = expr.kind else {
                return None;
            };
            resolve_expected_input(package, callable_inputs, callee_id)
                .map(|expected_input| (arg_id, expected_input))
        })
        .collect();

    for (arg_id, expected_input) in call_sites {
        normalize_arg_to_expected_input(package, assigner, arg_id, &expected_input);
    }
}

fn resolve_expected_input(
    package: &Package,
    callable_inputs: &FxHashMap<StoreItemId, Ty>,
    callee_id: ExprId,
) -> Option<Ty> {
    if let Some(callee) = resolve_direct_item_callee(package, callee_id)
        && let Some(input_ty) = callable_inputs.get(&callee.item_id)
    {
        return Some(apply_controlled_input_layers(
            input_ty.clone(),
            callee.controlled_depth,
        ));
    }

    let callee = package.get_expr(callee_id);
    if let Ty::Arrow(arrow) = &callee.ty {
        return Some((*arrow.input).clone());
    }

    None
}

/// Applies one controlled-functor input layer per controlled wrapper.
fn apply_controlled_input_layers(mut input_ty: Ty, controlled_depth: usize) -> Ty {
    for _ in 0..controlled_depth {
        input_ty = Ty::Tuple(vec![Ty::Array(Box::new(Ty::Prim(Prim::Qubit))), input_ty]);
    }
    input_ty
}

/// Reconciles a rewritten call-site argument subtree with the callee's current
/// input type.
///
/// Before, `arg_id` may still reflect the pre-promotion shape, such as a scalar
/// where the promoted callee now expects `(scalar,)`, or nested tuple children
/// whose wrapper structure no longer matches the updated input pattern. After,
/// the subtree rooted at `arg_id` mirrors `expected_input`: single-element tuple
/// wrappers are inserted only where required and tuple types are refreshed after
/// recursive normalization.
fn normalize_arg_to_expected_input(
    package: &mut Package,
    assigner: &mut Assigner,
    arg_id: ExprId,
    expected_input: &Ty,
) {
    let arg = package.get_expr(arg_id).clone();
    if arg.ty == *expected_input {
        return;
    }

    let Ty::Tuple(expected_items) = expected_input else {
        return;
    };

    if expected_items.len() == 1 && arg.ty == expected_items[0] {
        wrap_arg_in_single_tuple(package, assigner, arg_id);
        return;
    }

    let ExprKind::Tuple(arg_items) = arg.kind else {
        return;
    };
    if arg_items.len() != expected_items.len() {
        return;
    }

    for (arg_item, expected_item) in arg_items.iter().zip(expected_items) {
        normalize_arg_to_expected_input(package, assigner, *arg_item, expected_item);
    }

    let updated_tys = arg_items
        .iter()
        .map(|arg_item| package.get_expr(*arg_item).ty.clone())
        .collect();
    let arg_mut = package.exprs.get_mut(arg_id).expect("arg expr exists");
    arg_mut.ty = Ty::Tuple(updated_tys);
}

/// Replaces `arg_id` with a one-element tuple node while preserving the
/// original argument under a freshly allocated child expression.
///
/// Before, `arg_id` points directly at the scalar or tuple element supplied at
/// the call site. After, the original payload lives at `preserved_arg_id` and
/// `arg_id` becomes `(payload)`, matching callees whose promoted signature still
/// expects a single tuple layer.
fn wrap_arg_in_single_tuple(package: &mut Package, assigner: &mut Assigner, arg_id: ExprId) {
    let original_arg = package.get_expr(arg_id).clone();
    let preserved_arg_id = assigner.next_expr();
    package.exprs.insert(
        preserved_arg_id,
        Expr {
            id: preserved_arg_id,
            span: original_arg.span,
            ty: original_arg.ty.clone(),
            kind: original_arg.kind,
            exec_graph_range: original_arg.exec_graph_range,
        },
    );

    let arg = package.exprs.get_mut(arg_id).expect("arg expr exists");
    arg.kind = ExprKind::Tuple(vec![preserved_arg_id]);
    arg.ty = Ty::Tuple(vec![original_arg.ty]);
}
