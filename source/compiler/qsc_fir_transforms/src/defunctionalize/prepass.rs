// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Pre-pass rewrites before collecting call sites for defunctionalization.
//! These rewrites are not strictly necessary for correctness, but they
//! simplify the analysis by eliminating certain patterns of indirection and
//! exposing more direct call sites. They are run before collecting call sites
//! and performing the lattice analysis.
//!
//! # Responsibilities
//!
//! - Run the static closure-capture inlining that normalizes a partial
//!   application closure into a capture-free explicit-lambda shape by inlining
//!   statically-known callable captures into the lifted target body (via
//!   [`inline_static_closure_captures`]).
//! - Run the single-use local promotion that replaces single-use immutable
//!   callable locals with direct references to their initializer (via
//!   [`promote_single_use_callable_locals`]).
//! - Run the adjacent aggregate-alias promotion that replaces
//!   `let pair = aggregate; let (...) = pair;` with direct aggregate
//!   destructuring when `pair` has a callable-typed field and no other uses.
//! - Run the identity-closure peephole that replaces `(args) => f(args)`
//!   closures with direct references to `f` (via
//!   [`identity_closure_peephole`]).
//!

use qsc_data_structures::span::Span;
use qsc_fir::fir::{
    Block, BlockId, CallableImpl, Expr, ExprId, ExprKind, ItemKind, LocalItemId, LocalVarId,
    Mutability, Package, PackageId, PackageLookup, PackageStore, Pat, PatId, PatKind, Res, Stmt,
    StmtId, StmtKind, UnOp,
};
use qsc_fir::ty::Ty;
use qsc_fir::visit::{self, Visitor};
use rustc_hash::{FxHashMap, FxHashSet};

/// Runs pre-pass rewrites before collecting call sites for defunctionalization. See
/// [`promote_single_use_callable_locals`], [`promote_adjacent_aggregate_callable_aliases`],
/// and [`identity_closure_peephole`] for details.
///
/// Only expressions in `reachable_expr_ids` are scanned for promotion candidates
/// and identity-closure patterns, restricting analysis to entry-reachable code.
///
/// Returns the map of collapsed identity-closure call expressions to the spans
/// that should be re-stamped onto their rewritten call sites.
pub(super) fn run(
    store: &mut PackageStore,
    package_id: PackageId,
    reachable_expr_ids: &[ExprId],
) -> FxHashMap<ExprId, Span> {
    inline_static_closure_captures(store, package_id, reachable_expr_ids);
    promote_single_use_callable_locals(store, package_id, reachable_expr_ids);
    promote_adjacent_aggregate_callable_aliases(store, package_id);
    identity_closure_peephole(store, package_id, reachable_expr_ids)
}

/// A planned normalization of one partial-application closure: the statically
/// known callable captures are inlined into the lifted target body, and the
/// corresponding capture slots are dropped from both the closure's capture list
/// and the target's input pattern.
///
/// This is computed under an immutable borrow and applied under a mutable
/// borrow, mirroring [`promote_single_use_callable_locals`].
struct ClosureCaptureInlining {
    /// The lifted target shared by the closure expressions in this plan group.
    target: LocalItemId,
    /// The closure expression whose capture list shrinks.
    closure_expr_id: ExprId,
    /// The rewritten capture list with the inlined capture slots removed.
    new_captures: Vec<LocalVarId>,
    /// The lifted target's top-level input tuple pattern id (rewritten in place).
    target_input_pat_id: PatId,
    /// The rewritten top-level tuple sub-pattern ids (inlined capture binds removed).
    new_input_sub_pats: Vec<PatId>,
    /// The rewritten top-level tuple type, aligned with `new_input_sub_pats`.
    new_input_ty: Ty,
    /// In-place body rewrites: each `Var(Res::Local(capture_param))` use in the
    /// target body is overwritten with a clone of the capture's initializer kind.
    body_rewrites: Vec<(ExprId, ExprKind)>,
}

/// Normalizes a partial-application closure into the capture-free explicit-lambda
/// shape by inlining statically known callable captures into the lifted target
/// body. A partial application such as `Repeat(H, 1, _)` lowers to a closure that
/// captures the fixed arguments (`H` and `1`) and forwards them, as parameters, to
/// a lifted lambda whose body re-invokes the callable. When the captured value is
/// a global callable (`Var(Res::Item(_))`), the capture never carries information
/// that a later analysis pass can resolve, so a partial application forwarded as a
/// recursive higher-order function's own callable argument fails to converge.
///
/// Inlining the callable capture directly into the lifted body makes the closure
/// structurally identical to the already-converging explicit-lambda form, so the
/// remaining defunctionalization analysis resolves it without special handling.
/// Non-callable captures (for example a literal `Int`) are left threaded, since
/// they never block callable resolution.
///
/// # Before
/// ```text
/// let arg0 = H;                 // Var(Res::Item(H))
/// let arg1 = 1;                 // Lit(Int)
/// Closure([arg0, arg1], target) // target body: Repeat(p0, p1, hole)
/// ```
/// # After
/// ```text
/// Closure([arg1], target)       // target body: Repeat(H, p1, hole)
/// ```
///
/// # Safety
/// - A capture is inlined only when its enclosing initializer is a bare global
///   item reference (`Var(Res::Item(_))`), so no enclosing local escapes into the
///   lifted body.
/// - When multiple reachable closures reference one target, every reference
///   must produce the same target rewrite. This permits equivalent copies in
///   generated functor bodies without affecting a differently-captured closure.
/// - The target's top-level input must be a flat tuple of bindings; nested or
///   tuple-destructuring parameters are skipped as a safe no-op.
/// - A capture parameter re-captured by a nested closure in the target body is
///   skipped, since dropping the parameter would leave the nested closure with a
///   dangling reference.
fn inline_static_closure_captures(
    store: &mut PackageStore,
    package_id: PackageId,
    reachable_expr_ids: &[ExprId],
) {
    // Collect the planned inlinings using an immutable borrow.
    let inlinings = {
        let pkg = store.get(package_id);
        collect_static_closure_capture_inlinings(pkg, reachable_expr_ids)
    };

    // Apply the planned inlinings using a mutable borrow. The target-level
    // rewrite is identical within each group, so apply it once before updating
    // each closure occurrence independently.
    if !inlinings.is_empty() {
        let pkg = store.get_mut(package_id);
        for mut group in inlinings {
            let target_rewrite = group.pop().expect("inlining group should not be empty");
            let ClosureCaptureInlining {
                closure_expr_id,
                new_captures,
                target_input_pat_id,
                new_input_sub_pats,
                new_input_ty,
                body_rewrites,
                ..
            } = target_rewrite;
            for (expr_id, new_kind) in body_rewrites {
                pkg.exprs
                    .get_mut(expr_id)
                    .expect("expression should exist")
                    .kind = new_kind;
            }
            let pat = pkg
                .pats
                .get_mut(target_input_pat_id)
                .expect("pattern should exist");
            pat.kind = PatKind::Tuple(new_input_sub_pats);
            pat.ty = new_input_ty;

            if let ExprKind::Closure(captures, _) = &mut pkg
                .exprs
                .get_mut(closure_expr_id)
                .expect("expression should exist")
                .kind
            {
                *captures = new_captures;
            }
            for inlining in group {
                if let ExprKind::Closure(captures, _) = &mut pkg
                    .exprs
                    .get_mut(inlining.closure_expr_id)
                    .expect("expression should exist")
                    .kind
                {
                    *captures = inlining.new_captures;
                }
            }
        }
    }
}

/// Scans reachable closures and collects a [`ClosureCaptureInlining`] for each one
/// whose statically known callable captures can be inlined. Plans are grouped by
/// lifted target and retained only when every reachable reference has a compatible
/// target rewrite. Capture-to-binding matching is scoped per owner boundary (via
/// [`collect_promotion_scopes`]) because `LocalVarId`s are unique only within a
/// single callable and collide across callables.
fn collect_static_closure_capture_inlinings(
    pkg: &Package,
    reachable_expr_ids: &[ExprId],
) -> Vec<Vec<ClosureCaptureInlining>> {
    let reachable: FxHashSet<ExprId> = reachable_expr_ids.iter().copied().collect();

    // Count every reachable reference so a target is rewritten only when all
    // of its closure occurrences participate in the same normalization.
    let mut target_ref_count: FxHashMap<LocalItemId, usize> = FxHashMap::default();
    for &expr_id in reachable_expr_ids {
        if let ExprKind::Closure(_, target) = &pkg.get_expr(expr_id).kind {
            *target_ref_count.entry(*target).or_default() += 1;
        }
    }

    let mut inlinings_by_target: FxHashMap<LocalItemId, Vec<ClosureCaptureInlining>> =
        FxHashMap::default();
    for scope in collect_promotion_scopes(pkg) {
        let callable_inits = collect_scope_callable_inits(pkg, &scope, &reachable);
        if callable_inits.is_empty() {
            continue;
        }
        for &expr_id in &scope.exprs {
            if !reachable.contains(&expr_id) {
                continue;
            }
            let ExprKind::Closure(captures, target) = &pkg.get_expr(expr_id).kind else {
                continue;
            };
            if let Some(inlining) =
                plan_closure_capture_inlining(pkg, expr_id, captures, *target, &callable_inits)
            {
                inlinings_by_target
                    .entry(*target)
                    .or_default()
                    .push(inlining);
            }
        }
    }

    inlinings_by_target
        .into_iter()
        .filter_map(|(target, group)| {
            let expected_count = target_ref_count.get(&target).copied()?;
            shared_target_group_is_compatible(&group, expected_count).then_some(group)
        })
        .collect()
}

/// Returns whether every reachable closure reference produced the same rewrite
/// for its shared lifted target.
fn shared_target_group_is_compatible(
    group: &[ClosureCaptureInlining],
    expected_count: usize,
) -> bool {
    let Some(first) = group.first() else {
        return false;
    };
    group.len() == expected_count
        && group
            .iter()
            .all(|candidate| target_rewrites_match(first, candidate))
}

/// Returns whether two closure plans make the same in-place change to their
/// shared lifted target. Closure expression ids and retained capture locals are
/// intentionally excluded because those belong to each occurrence's owner.
fn target_rewrites_match(left: &ClosureCaptureInlining, right: &ClosureCaptureInlining) -> bool {
    left.target == right.target
        && left.target_input_pat_id == right.target_input_pat_id
        && left.new_input_sub_pats == right.new_input_sub_pats
        && left.new_input_ty == right.new_input_ty
        && left.body_rewrites == right.body_rewrites
}

/// Collects the immutable `let arg = <item>;` bindings in one owner scope whose
/// initializer is a bare global callable reference (`Var(Res::Item(_))`), keyed
/// by the bound local. Only reachable initializers are considered.
fn collect_scope_callable_inits(
    pkg: &Package,
    scope: &PromotionScope<'_>,
    reachable: &FxHashSet<ExprId>,
) -> FxHashMap<LocalVarId, ExprKind> {
    let mut callable_inits = FxHashMap::default();
    for &stmt_id in &scope.stmts {
        let StmtKind::Local(Mutability::Immutable, pat_id, init_expr_id) =
            &pkg.get_stmt(stmt_id).kind
        else {
            continue;
        };
        if !reachable.contains(init_expr_id) {
            continue;
        }
        let PatKind::Bind(ident) = &pkg.get_pat(*pat_id).kind else {
            continue;
        };
        let init_expr = pkg.get_expr(*init_expr_id);
        if let ExprKind::Var(Res::Item(item_id), generic_args) = &init_expr.kind {
            callable_inits.insert(
                ident.id,
                ExprKind::Var(Res::Item(*item_id), generic_args.clone()),
            );
        }
    }
    callable_inits
}

/// Plans the capture inlining for a single closure, or returns `None` when the
/// closure does not match the normalizable shape (see [`inline_static_closure_captures`]
/// for the applied safety conditions).
fn plan_closure_capture_inlining(
    pkg: &Package,
    closure_expr_id: ExprId,
    captures: &[LocalVarId],
    target: LocalItemId,
    callable_inits: &FxHashMap<LocalVarId, ExprKind>,
) -> Option<ClosureCaptureInlining> {
    let item = pkg.items.get(target)?;
    let ItemKind::Callable(decl) = &item.kind else {
        return None;
    };
    // Only Spec implementations have a rewritable body.
    let CallableImpl::Spec(_) = &decl.implementation else {
        return None;
    };

    // The top-level input must be a flat tuple of bindings; the first
    // `captures.len()` binds are the capture parameters, aligned positionally
    // with `captures`.
    let input_pat = pkg.get_pat(decl.input);
    let PatKind::Tuple(sub_pats) = &input_pat.kind else {
        return None;
    };
    let num_captures = captures.len();
    if sub_pats.len() < num_captures {
        return None;
    }
    let mut param_vars = Vec::with_capacity(sub_pats.len());
    for &sub_pat_id in sub_pats {
        let PatKind::Bind(ident) = &pkg.get_pat(sub_pat_id).kind else {
            return None;
        };
        param_vars.push(ident.id);
    }
    let capture_param_vars = &param_vars[..num_captures];

    // Collect every expression in the target body so capture-parameter uses can
    // be rewritten and nested re-captures detected.
    let mut target_scope = PromotionScope::new(pkg);
    target_scope.visit_callable_impl(&decl.implementation);
    let mut recaptured: FxHashSet<LocalVarId> = FxHashSet::default();
    for &expr_id in &target_scope.exprs {
        if let ExprKind::Closure(inner_captures, _) = &pkg.get_expr(expr_id).kind {
            recaptured.extend(inner_captures.iter().copied());
        }
    }

    // Select the capture slots whose enclosing initializer is a known callable
    // and whose parameter is not re-captured by a nested closure.
    let mut inlined_indices: FxHashSet<usize> = FxHashSet::default();
    let mut inlined_params: FxHashMap<LocalVarId, ExprKind> = FxHashMap::default();
    for (index, (&capture_var, &param_var)) in captures.iter().zip(capture_param_vars).enumerate() {
        if recaptured.contains(&param_var) {
            continue;
        }
        if let Some(init_kind) = callable_inits.get(&capture_var) {
            inlined_indices.insert(index);
            inlined_params.insert(param_var, init_kind.clone());
        }
    }
    if inlined_indices.is_empty() {
        return None;
    }

    // Record the in-place body rewrites for each inlined capture parameter.
    let mut body_rewrites = Vec::new();
    for &expr_id in &target_scope.exprs {
        if let ExprKind::Var(Res::Local(var), _) = &pkg.get_expr(expr_id).kind
            && let Some(init_kind) = inlined_params.get(var)
        {
            body_rewrites.push((expr_id, init_kind.clone()));
        }
    }

    // Drop the inlined capture slots from the closure capture list and the
    // target's input tuple, recomputing the tuple type from the retained binds.
    let new_captures: Vec<LocalVarId> = captures
        .iter()
        .enumerate()
        .filter(|(index, _)| !inlined_indices.contains(index))
        .map(|(_, &var)| var)
        .collect();
    let new_input_sub_pats: Vec<PatId> = sub_pats
        .iter()
        .enumerate()
        .filter(|(index, _)| !inlined_indices.contains(index))
        .map(|(_, &pat_id)| pat_id)
        .collect();
    let new_input_ty = Ty::Tuple(
        new_input_sub_pats
            .iter()
            .map(|&pat_id| pkg.get_pat(pat_id).ty.clone())
            .collect(),
    );

    Some(ClosureCaptureInlining {
        target,
        closure_expr_id,
        new_captures,
        target_input_pat_id: decl.input,
        new_input_sub_pats,
        new_input_ty,
        body_rewrites,
    })
}

/// Promotes an adjacent, single-use aggregate local into a following tuple
/// destructure. This preserves evaluation order because there is no intervening
/// statement between the alias binding and its only use.
fn promote_adjacent_aggregate_callable_aliases(store: &mut PackageStore, package_id: PackageId) {
    let block_ids: Vec<_> = {
        let pkg = store.get(package_id);
        collect_promotion_scopes(pkg)
            .into_iter()
            .flat_map(|scope| scope.seen_blocks.into_iter())
            .collect()
    };

    let pkg = store.get_mut(package_id);
    for block_id in block_ids {
        promote_adjacent_aggregate_callable_aliases_in_block(pkg, block_id);
    }
}

/// Iterates the block until no further promotions apply, removing alias
/// statements whose single-use binding feeds a subsequent tuple destructure.
///
/// Each pass scans adjacent statement pairs: when the first is an immutable
/// `let` binding whose init is a callable-bearing aggregate and the second
/// destructures that binding with exactly one use, the alias statement is
/// elided and the destructure is repointed directly at the original init.
/// The loop re-runs because removing one alias may expose the next.
fn promote_adjacent_aggregate_callable_aliases_in_block(pkg: &mut Package, block_id: BlockId) {
    loop {
        let stmt_ids = pkg.get_block(block_id).stmts.clone();
        let mut retained = Vec::with_capacity(stmt_ids.len());
        let mut changed = false;
        let mut index = 0;

        while index < stmt_ids.len() {
            if index + 1 < stmt_ids.len()
                && let Some(init_expr_id) = aggregate_alias_promotion_init(
                    pkg,
                    block_id,
                    stmt_ids[index],
                    stmt_ids[index + 1],
                )
            {
                if let StmtKind::Local(_, _, expr_id) = &mut pkg
                    .stmts
                    .get_mut(stmt_ids[index + 1])
                    .expect("statement should exist")
                    .kind
                {
                    *expr_id = init_expr_id;
                }
                retained.push(stmt_ids[index + 1]);
                changed = true;
                index += 2;
                continue;
            }

            retained.push(stmt_ids[index]);
            index += 1;
        }

        pkg.blocks
            .get_mut(block_id)
            .expect("block should exist")
            .stmts = retained;

        if !changed {
            break;
        }
    }
}

/// Returns the initializer `ExprId` of `alias_stmt_id` when it forms a
/// promotable adjacent-aggregate pair with `use_stmt_id`.
///
/// The pair is promotable when:
/// 1. `alias_stmt_id` is an immutable `let` binding whose type contains an
///    arrow (callable-bearing aggregate).
/// 2. `use_stmt_id` destructures that exact binding via a tuple pattern.
/// 3. The alias local has exactly one use in the enclosing block, which is
///    the `use_stmt_id` reference.
///
/// Returns `None` when any condition fails.
fn aggregate_alias_promotion_init(
    pkg: &Package,
    block_id: BlockId,
    alias_stmt_id: StmtId,
    use_stmt_id: StmtId,
) -> Option<ExprId> {
    let alias_stmt = pkg.get_stmt(alias_stmt_id);
    let StmtKind::Local(Mutability::Immutable, alias_pat_id, alias_init_expr_id) = alias_stmt.kind
    else {
        return None;
    };
    let alias_pat = pkg.get_pat(alias_pat_id);
    let PatKind::Bind(alias_ident) = &alias_pat.kind else {
        return None;
    };
    if !ty_contains_arrow(&alias_pat.ty) {
        return None;
    }

    let use_stmt = pkg.get_stmt(use_stmt_id);
    let StmtKind::Local(_, use_pat_id, use_expr_id) = use_stmt.kind else {
        return None;
    };
    if !matches!(pkg.get_pat(use_pat_id).kind, PatKind::Tuple(_)) {
        return None;
    }
    if !matches!(pkg.get_expr(use_expr_id).kind, ExprKind::Var(Res::Local(var), _) if var == alias_ident.id)
    {
        return None;
    }

    if local_has_exactly_one_use_in_block(pkg, block_id, alias_ident.id, use_expr_id) {
        Some(alias_init_expr_id)
    } else {
        None
    }
}

/// Reports whether `local_id` has exactly one use in `block_id` and that
/// use is the expression `expected_use_expr_id`. Both direct `Var` references
/// and closure captures count as uses.
fn local_has_exactly_one_use_in_block(
    pkg: &Package,
    block_id: BlockId,
    local_id: LocalVarId,
    expected_use_expr_id: ExprId,
) -> bool {
    let mut use_count = 0;
    let mut saw_expected_use = false;
    crate::walk_utils::for_each_expr_in_block(
        pkg,
        block_id,
        &mut |expr_id, expr| match &expr.kind {
            ExprKind::Var(Res::Local(var), _) if *var == local_id => {
                use_count += 1;
                saw_expected_use |= expr_id == expected_use_expr_id;
            }
            ExprKind::Closure(captures, _) if captures.contains(&local_id) => {
                use_count += 1;
            }
            _ => {}
        },
    );

    use_count == 1 && saw_expected_use
}

/// Reports whether `ty` is or transitively contains an arrow type (callable).
/// Recurses through tuple types but does not expand UDTs — expanding a UDT
/// requires a `PackageStore` lookup (to read the type definition's underlying
/// structure), and this helper intentionally avoids that dependency because
/// the pre-pass promotions are best-effort simplifications, not correctness
/// requirements. A missed callable hidden behind a UDT wrapper is still
/// handled correctly by the full analysis phase, which uses the heavier
/// [`super::specialize::ty_contains_arrow_through_udts`] variant with store
/// access.
fn ty_contains_arrow(ty: &Ty) -> bool {
    match ty {
        Ty::Arrow(_) => true,
        Ty::Tuple(items) => items.iter().any(ty_contains_arrow),
        _ => false,
    }
}

/// Promotes single-use immutable callable locals whose initializer is a simple
/// item reference. For example, `let op = H; Apply(op, q)` is rewritten to
/// `Apply(H, q)`, eliminating the indirection before analysis runs.
///
/// # Before
/// ```text
/// let op = H;         // Local(pat, Var(Item(H)))
/// Apply(op, qubit);   // Call(Apply, (Var(Local(op)), qubit))
/// ```
/// # After
/// ```text
/// let op = H;         // binding still present (DCE removes later)
/// Apply(H, qubit);    // Call(Apply, (Var(Item(H)), qubit))
/// ```
///
/// # Mutations
/// - Rewrites `Expr.kind` at each single-use site from `Var(Local(..))`
///   to `Var(Item(..))` in place.
fn promote_single_use_callable_locals(
    store: &mut PackageStore,
    package_id: PackageId,
    reachable_expr_ids: &[ExprId],
) {
    let replacements = {
        let pkg = store.get(package_id);
        collect_single_use_promotions(pkg, reachable_expr_ids)
    };

    if !replacements.is_empty() {
        let pkg = store.get_mut(package_id);
        for (expr_id, new_kind) in replacements {
            pkg.exprs
                .get_mut(expr_id)
                .expect("expression should exist")
                .kind = new_kind;
        }
    }
}

/// Scans immutable local bindings whose initialiser is a simple item reference
/// (`Var(Res::Item(_))`), counts uses within reachable expressions in the same
/// owner scope, and collects replacements for locals that are used exactly once.
fn collect_single_use_promotions(
    pkg: &Package,
    reachable_expr_ids: &[ExprId],
) -> Vec<(ExprId, ExprKind)> {
    let reachable_expr_ids: FxHashSet<_> = reachable_expr_ids.iter().copied().collect();
    collect_promotion_scopes(pkg)
        .iter()
        .flat_map(|scope| collect_single_use_promotions_in_scope(pkg, scope, &reachable_expr_ids))
        .collect()
}

/// Collects single-use callable-local replacements within one owner scope.
fn collect_single_use_promotions_in_scope(
    pkg: &Package,
    scope: &PromotionScope<'_>,
    reachable_expr_ids: &FxHashSet<ExprId>,
) -> Vec<(ExprId, ExprKind)> {
    // find candidate immutable locals whose init is a simple item reference.
    let mut candidates: FxHashMap<LocalVarId, ExprKind> = FxHashMap::default();
    for &stmt_id in &scope.stmts {
        let stmt = pkg.get_stmt(stmt_id);
        if let StmtKind::Local(Mutability::Immutable, pat_id, init_expr_id) = &stmt.kind {
            if !reachable_expr_ids.contains(init_expr_id) {
                continue;
            }
            let pat = pkg.get_pat(*pat_id);
            if let PatKind::Bind(ident) = &pat.kind
                && matches!(pat.ty, Ty::Arrow(_))
            {
                let init_expr = pkg.get_expr(*init_expr_id);
                if let ExprKind::Var(Res::Item(item_id), generic_args) = &init_expr.kind {
                    candidates.insert(
                        ident.id,
                        ExprKind::Var(Res::Item(*item_id), generic_args.clone()),
                    );
                }
            }
        }
    }

    if candidates.is_empty() {
        return Vec::new();
    }

    // exclude candidates that are captured by closures (within reachable code).
    for &expr_id in &scope.exprs {
        if !reachable_expr_ids.contains(&expr_id) {
            continue;
        }
        let expr = pkg.get_expr(expr_id);
        if let ExprKind::Closure(captures, _) = &expr.kind {
            for var in captures {
                candidates.remove(var);
            }
        }
    }

    if candidates.is_empty() {
        return Vec::new();
    }

    // count uses and record use-site expression IDs (within reachable code).
    let mut use_info: FxHashMap<LocalVarId, Vec<ExprId>> =
        candidates.keys().map(|&var| (var, Vec::new())).collect();

    for &expr_id in &scope.exprs {
        if !reachable_expr_ids.contains(&expr_id) {
            continue;
        }
        let expr = pkg.get_expr(expr_id);
        if let ExprKind::Var(Res::Local(var), _) = &expr.kind
            && let Some(uses) = use_info.get_mut(var)
        {
            uses.push(expr_id);
        }
    }

    // build replacements for single-use locals.
    let mut replacements = Vec::new();
    for (var, uses) in &use_info {
        if uses.len() == 1 {
            replacements.push((uses[0], candidates[var].clone()));
        }
    }

    replacements
}

/// Builds the owner boundaries used for single-use local promotion.
///
/// Each scope is rooted at either the package entry expression or one callable
/// implementation. Keeping the scopes separate prevents local-use counts from
/// crossing callable and closure ownership boundaries.
fn collect_promotion_scopes(pkg: &Package) -> Vec<PromotionScope<'_>> {
    let mut scopes = Vec::new();

    if let Some(entry_expr_id) = pkg.entry {
        let mut scope = PromotionScope::new(pkg);
        scope.visit_expr(entry_expr_id);
        scopes.push(scope);
    }

    for (_, item) in &pkg.items {
        let ItemKind::Callable(decl) = &item.kind else {
            continue;
        };
        let mut scope = PromotionScope::new(pkg);
        scope.visit_callable_impl(&decl.implementation);
        scopes.push(scope);
    }

    scopes
}

/// FIR visited under one owner boundary for single-use local promotion.
///
/// A promotion scope is the entry expression or one callable implementation,
/// including its explicit specialization bodies. Local declarations in the
/// scope provide promotion candidates, and local references in the scope provide
/// use sites. Closure bodies are not walked through closure expressions here;
/// they are represented by their own callable scopes, while captured locals are
/// detected from the closure expression in the enclosing scope.
///
/// The `seen_*` sets make the traversal idempotent when a block, statement, or
/// expression is reachable from more than one root in the same callable
/// implementation.
struct PromotionScope<'a> {
    /// The package being analyzed.
    pkg: &'a Package,
    /// Statements that can introduce candidate immutable callable locals.
    stmts: Vec<StmtId>,
    /// Expressions whose local references are checked as use sites.
    exprs: Vec<ExprId>,
    /// Blocks already visited in this owner boundary.
    seen_blocks: FxHashSet<BlockId>,
    /// Statements already recorded in this owner boundary.
    seen_stmts: FxHashSet<StmtId>,
    /// Expressions already recorded in this owner boundary.
    seen_exprs: FxHashSet<ExprId>,
}

impl<'a> PromotionScope<'a> {
    fn new(pkg: &'a Package) -> Self {
        Self {
            pkg,
            stmts: Vec::new(),
            exprs: Vec::new(),
            seen_blocks: FxHashSet::default(),
            seen_stmts: FxHashSet::default(),
            seen_exprs: FxHashSet::default(),
        }
    }
}

impl<'a> Visitor<'a> for PromotionScope<'a> {
    fn get_block(&self, id: BlockId) -> &'a Block {
        self.pkg.get_block(id)
    }

    fn get_expr(&self, id: ExprId) -> &'a Expr {
        self.pkg.get_expr(id)
    }

    fn get_pat(&self, id: PatId) -> &'a Pat {
        self.pkg.get_pat(id)
    }

    fn get_stmt(&self, id: StmtId) -> &'a Stmt {
        self.pkg.get_stmt(id)
    }

    fn visit_block(&mut self, block_id: BlockId) {
        if self.seen_blocks.insert(block_id) {
            visit::walk_block(self, block_id);
        }
    }

    fn visit_stmt(&mut self, stmt_id: StmtId) {
        if self.seen_stmts.insert(stmt_id) {
            self.stmts.push(stmt_id);
            visit::walk_stmt(self, stmt_id);
        }
    }

    fn visit_expr(&mut self, expr_id: ExprId) {
        if self.seen_exprs.insert(expr_id) {
            self.exprs.push(expr_id);
            visit::walk_expr(self, expr_id);
        }
    }

    fn visit_pat(&mut self, _: PatId) {}
}

/// Replaces identity closures `(args) => f(args)` with direct references to
/// the callee in the package's expressions. An identity closure is one whose
/// body is a single call that forwards all actual parameters in order to a
/// callee that is either a global item or a single captured variable.
///
/// # Before
/// ```text
/// Closure([captures], target)   // target body: (args) => callee(args)
/// ```
/// # After (global callee)
/// ```text
/// Var(Item(callee_item))   // closure collapsed to direct item reference
/// ```
/// # After (captured-local callee)
/// ```text
/// Var(Local(outer_var))   // closure collapsed to outer-scope local
/// ```
/// # After (functor-wrapped callee)
/// ```text
/// UnOp(Functor(Adj), Var(Item(callee_item)))   // functor chain preserved
/// ```
///
/// # Mutations
/// - Rewrites `Expr.kind` at each identity-closure site in place.
fn identity_closure_peephole(
    store: &mut PackageStore,
    package_id: PackageId,
    reachable_expr_ids: &[ExprId],
) -> FxHashMap<ExprId, Span> {
    // Collect replacements using an immutable borrow.
    let replacements = {
        let pkg = store.get(package_id);
        collect_identity_closures(pkg, reachable_expr_ids)
    };

    // Apply replacements using a mutable borrow, recording the discarded
    // lambda-body call span for each collapsed identity-closure init node so
    // analysis can stamp it onto the surviving direct `Call`.
    let mut collapsed_spans = FxHashMap::default();
    if !replacements.is_empty() {
        let pkg = store.get_mut(package_id);
        for (expr_id, new_kind, inner_span) in replacements {
            pkg.exprs
                .get_mut(expr_id)
                .expect("expression should exist")
                .kind = new_kind;
            if let Some(span) = inner_span {
                collapsed_spans.insert(expr_id, span);
            }
        }
    }
    collapsed_spans
}

/// Scans reachable expressions and collects `(ExprId, replacement ExprKind,
/// Option<Span>)` triples for identity closures. The optional span is the
/// discarded lambda-body call span, set on the collapsed init-expr node so the
/// surviving direct `Call` can be re-stamped with the original body span.
fn collect_identity_closures(
    pkg: &Package,
    reachable_expr_ids: &[ExprId],
) -> Vec<(ExprId, ExprKind, Option<Span>)> {
    let mut replacements = Vec::new();

    for &expr_id in reachable_expr_ids {
        let expr = pkg.get_expr(expr_id);
        if let ExprKind::Closure(captures, target) = &expr.kind {
            replacements.extend(check_identity_closure(pkg, expr_id, captures, *target));
        }
    }

    replacements
}

/// Checks whether a closure is an identity wrapper `(args) => f(args)` or a
/// functor-wrapped identity `(args) => Adjoint f(args)` /
/// `(args) => Controlled f(args)`, and returns expression replacements that
/// collapse the closure to a direct reference (optionally functor-applied).
fn check_identity_closure(
    pkg: &Package,
    closure_expr_id: ExprId,
    captures: &[LocalVarId],
    target: qsc_fir::fir::LocalItemId,
) -> Vec<(ExprId, ExprKind, Option<Span>)> {
    // Get the closure's callable declaration.
    let Some(item) = pkg.items.get(target) else {
        return Vec::new();
    };
    let ItemKind::Callable(decl) = &item.kind else {
        return Vec::new();
    };

    // Only handle Spec implementations (not Intrinsic).
    let body_block_id = match &decl.implementation {
        CallableImpl::Spec(spec_impl) => spec_impl.body.block,
        _ => return Vec::new(),
    };

    let block = pkg.get_block(body_block_id);

    // Body must have exactly one statement.
    if block.stmts.len() != 1 {
        return Vec::new();
    }

    let stmt = pkg.get_stmt(block.stmts[0]);
    let call_expr_id = match &stmt.kind {
        StmtKind::Semi(e) | StmtKind::Expr(e) => *e,
        _ => return Vec::new(),
    };

    let call_expr = pkg.get_expr(call_expr_id);
    let inner_span = call_expr.span;
    let (callee_id, args_id) = match &call_expr.kind {
        ExprKind::Call(callee, args) => (*callee, *args),
        _ => return Vec::new(),
    };

    // Parse the callable's input pattern to separate capture params from actual params.
    let Some(all_param_vars) = extract_flat_param_vars(pkg, decl.input) else {
        return Vec::new();
    };
    let num_captures = captures.len();
    if all_param_vars.len() < num_captures {
        return Vec::new();
    }
    let capture_param_vars = &all_param_vars[..num_captures];
    let actual_param_vars = &all_param_vars[num_captures..];

    // Must have at least one actual parameter to be a meaningful identity wrapper.
    if actual_param_vars.is_empty() {
        return Vec::new();
    }

    // Verify that args forward all actual params in order.
    if !args_forward_params_in_order(pkg, args_id, actual_param_vars) {
        return Vec::new();
    }

    // Ensure no capture params appear in the arguments.
    if captures_appear_in_args(pkg, args_id, capture_param_vars) {
        return Vec::new();
    }

    // Determine the replacement based on the callee expression.
    let callee_expr = pkg.get_expr(callee_id);
    match &callee_expr.kind {
        // Callee is a captured local variable — replace with the enclosing scope's var.
        ExprKind::Var(Res::Local(var), _) => {
            let Some(capture_idx) = capture_param_vars.iter().position(|&v| v == *var) else {
                return Vec::new();
            };
            vec![(
                closure_expr_id,
                ExprKind::Var(Res::Local(captures[capture_idx]), Vec::new()),
                Some(inner_span),
            )]
        }
        // Callee is a global item — replace with the global reference.
        ExprKind::Var(Res::Item(item_id), generic_args) => {
            vec![(
                closure_expr_id,
                ExprKind::Var(Res::Item(*item_id), generic_args.clone()),
                Some(inner_span),
            )]
        }
        // Callee is a functor-wrapped expression — replace closure with the functor
        // application and rewrite the inner expression to reference the enclosing scope.
        ExprKind::UnOp(UnOp::Functor(functor), inner_id) => {
            let inner_expr = pkg.get_expr(*inner_id);
            match &inner_expr.kind {
                ExprKind::Var(Res::Local(var), _) => {
                    let Some(capture_idx) = capture_param_vars.iter().position(|&v| v == *var)
                    else {
                        return Vec::new();
                    };
                    vec![
                        (
                            *inner_id,
                            ExprKind::Var(Res::Local(captures[capture_idx]), Vec::new()),
                            None,
                        ),
                        (
                            closure_expr_id,
                            ExprKind::UnOp(UnOp::Functor(*functor), *inner_id),
                            Some(inner_span),
                        ),
                    ]
                }
                ExprKind::Var(Res::Item(_), _) => {
                    // Inner expression already references the global item; only
                    // the closure expression needs replacing.
                    vec![(
                        closure_expr_id,
                        ExprKind::UnOp(UnOp::Functor(*functor), *inner_id),
                        Some(inner_span),
                    )]
                }
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

/// Extracts a flat list of `LocalVarId`s from a pattern. Returns `None` if the
/// pattern contains discards that cannot be mapped to individual variables.
fn extract_flat_param_vars(pkg: &Package, pat_id: qsc_fir::fir::PatId) -> Option<Vec<LocalVarId>> {
    let pat = pkg.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => Some(vec![ident.id]),
        PatKind::Tuple(sub_pats) => {
            let mut variables = Vec::new();
            for &sub_pat_id in sub_pats {
                variables.extend(extract_flat_param_vars(pkg, sub_pat_id)?);
            }
            Some(variables)
        }
        PatKind::Discard => None,
    }
}

/// Checks whether the args expression forwards exactly the given parameter
/// variables in order. Handles both single-variable and tuple cases.
fn args_forward_params_in_order(
    pkg: &Package,
    args_id: ExprId,
    actual_param_vars: &[LocalVarId],
) -> bool {
    extract_flat_arg_vars(pkg, args_id).is_some_and(|variables| variables == actual_param_vars)
}

/// Extracts a flat list of `LocalVarId`s from an arguments expression. Returns `None`
/// if the expression is not a simple variable or tuple of variables (e.g. if it
/// contains discards, literals, or complex expressions).
fn extract_flat_arg_vars(pkg: &Package, args_id: ExprId) -> Option<Vec<LocalVarId>> {
    let args_expr = pkg.get_expr(args_id);
    match &args_expr.kind {
        ExprKind::Var(Res::Local(var), _) => Some(vec![*var]),
        ExprKind::Tuple(elements) => {
            let mut variables = Vec::new();
            for &element_id in elements {
                variables.extend(extract_flat_arg_vars(pkg, element_id)?);
            }
            Some(variables)
        }
        _ => None,
    }
}

/// Returns `true` if any of the capture parameter variables appear in the
/// arguments expression.
fn captures_appear_in_args(
    pkg: &Package,
    args_id: ExprId,
    capture_param_vars: &[LocalVarId],
) -> bool {
    if capture_param_vars.is_empty() {
        return false;
    }
    match extract_flat_arg_vars(pkg, args_id) {
        Some(variables) => variables
            .iter()
            .any(|variable| capture_param_vars.contains(variable)),
        _ => true, // Conservatively assume captures may be used in complex expressions.
    }
}
