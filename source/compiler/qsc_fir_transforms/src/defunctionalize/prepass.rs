// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Pre-pass rewrites before collecting call sites for defunctionalization.
//! These rewrites preserve capture-creation timing and callable-selection
//! decisions, and simplify indirection before call-site collection and lattice
//! analysis.
//!
//! # Responsibilities
//!
//! - Bind inline struct operands in source order when a callable field creates
//!   captures, before specialization moves those captures into call arguments.
//! - Expose capture bindings and saved selection guards from callable-initializer
//!   blocks at their original evaluation point, rather than replaying them
//!   when the callable is invoked.
//! - Inline statically known callable captures into lifted target bodies,
//!   retaining all remaining capture slots (via
//!   [`inline_static_closure_captures`]).
//! - Run the single-use local promotion that replaces single-use immutable
//!   callable locals with direct references to their initializer (via
//!   [`promote_single_use_callable_locals`]).
//! - Run the adjacent aggregate-alias promotion that replaces
//!   `let pair = aggregate; let (...) = pair;` with direct aggregate
//!   destructuring when `pair` has a callable-typed field and no other uses.
//! - Decompose immutable callable-bearing tuple aliases used by tuple assignments,
//!   retaining a whole-tuple binding when a closure captures the alias.
//! - Request shared condition normalization for callable selections, including
//!   fresh expressions introduced by earlier defunc iterations.
//! - Run the identity-closure peephole that replaces `(args) => f(args)`
//!   closures with direct references to `f` (via
//!   [`identity_closure_peephole`]).
//!

use crate::fir_builder::alloc_local_var_expr;
use qsc_data_structures::span::Span;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    Block, BlockId, CallableImpl, Expr, ExprId, ExprKind, ItemKind, LocalItemId, LocalVarId,
    Mutability, Package, PackageId, PackageLookup, PackageStore, Pat, PatId, PatKind, Res, Stmt,
    StmtId, StmtKind, UnOp,
};
use qsc_fir::ty::{FunctorSet, Ty};
use qsc_fir::visit::{self, Visitor};
use rustc_hash::{FxHashMap, FxHashSet};

/// Runs pre-pass rewrites before collecting call sites for defunctionalization. See
/// [`promote_single_use_callable_locals`], [`promote_adjacent_aggregate_callable_aliases`],
/// and [`identity_closure_peephole`] for details.
///
/// The supplied expression IDs filter capture-operand candidates, static capture
/// inlining, single-use callable promotion, and identity reduction. Guard and
/// aggregate/tuple normalization inspect all callable and entry scopes. Newly
/// allocated operand initializers are included before capture-binding exposure.
///
/// Returns a map from collapsed identity-closure expression IDs to their former
/// body-call spans, for re-stamping the rewritten invocation sites.
pub(super) fn run(
    store: &mut PackageStore,
    package_id: PackageId,
    reachable_expr_ids: &[ExprId],
    assigner: &mut Assigner,
) -> FxHashMap<ExprId, Span> {
    crate::cond_normalize::normalize_callable_selections(store.get_mut(package_id), assigner);
    let reachable_expr_ids =
        normalize_capture_operands(store.get_mut(package_id), reachable_expr_ids, assigner);
    let reachable_expr_ids = reachable_expr_ids.as_slice();
    inline_static_closure_captures(store, package_id, reachable_expr_ids);
    promote_single_use_callable_locals(store, package_id, reachable_expr_ids);
    promote_adjacent_aggregate_callable_aliases(store, package_id);
    decompose_assignment_tuple_aliases(store.get_mut(package_id), assigner);
    identity_closure_peephole(store, package_id, reachable_expr_ids, assigner)
}

/// Normalizes capture creation in the supplied reachable expression IDs. Callee
/// control-flow rewriting calls this again for fresh branch-local arguments
/// before analysis observes them.
pub(super) fn normalize_capture_operands(
    package: &mut Package,
    reachable_expr_ids: &[ExprId],
    assigner: &mut Assigner,
) -> Vec<ExprId> {
    let mut reachable_expr_ids = reachable_expr_ids.to_vec();
    let initializers = materialize_inline_struct_captures(package, &reachable_expr_ids, assigner);
    reachable_expr_ids.extend(initializers);
    expose_callable_initializer_prefixes(package, &reachable_expr_ids);
    reachable_expr_ids
}

/// Keeps inline struct operands ahead of specialization's capture relocation.
///
/// # Before
/// ```text
/// Read(new Payload { Tail=Log("tail"), F=Add(Log("capture"), _), Head=Log("head") })
/// ```
/// # After
/// ```text
/// {
///     let tail = Log("tail");
///     let f = Add(Log("capture"), _);
///     let head = Log("head");
///     Read(new Payload { Tail=tail, F=f, Head=head })
/// }
/// ```
///
/// Tuple operands and struct copy sources retain their original evaluation
/// order. Only direct item callees qualify, so evaluating the callee cannot
/// itself depend on writes in the arguments. The returned initializer IDs let
/// [`expose_callable_initializer_prefixes`] process the new callable bindings.
/// Fresh initializers are normalized in this pass too: moving a nested call to
/// a new ID must not hide its inline struct captures from the original walk.
fn materialize_inline_struct_captures(
    pkg: &mut Package,
    reachable: &[ExprId],
    assigner: &mut Assigner,
) -> Vec<ExprId> {
    let mut initializers = Vec::new();
    let mut pending = reachable.to_vec();
    let mut next = 0;
    while let Some(&expr_id) = pending.get(next) {
        next += 1;
        let expr = pkg.get_expr(expr_id).clone();
        let ExprKind::Call(callee, args) = expr.kind else {
            continue;
        };
        let (base, _) = super::types::peel_body_functors(pkg, callee);
        if !matches!(pkg.get_expr(base).kind, ExprKind::Var(Res::Item(_), _))
            || !has_inline_struct_capture(pkg, args)
        {
            continue;
        }
        let mut statements = Vec::new();
        let first_new_initializer = initializers.len();
        bind_struct_argument_operands(pkg, args, assigner, &mut statements, &mut initializers);
        pending.extend_from_slice(&initializers[first_new_initializer..]);
        let call =
            crate::fir_builder::alloc_expr(pkg, assigner, expr.ty.clone(), expr.kind, expr.span);
        statements.push(crate::fir_builder::alloc_expr_stmt(
            pkg, assigner, call, expr.span,
        ));
        let block = crate::fir_builder::alloc_block(pkg, assigner, statements, expr.ty, expr.span);
        pkg.exprs.get_mut(expr_id).expect("call exists").kind = ExprKind::Block(block);
    }
    initializers
}

fn has_inline_struct_capture(pkg: &Package, id: ExprId) -> bool {
    match &pkg.get_expr(id).kind {
        ExprKind::Tuple(items) => items
            .iter()
            .any(|&item| has_inline_struct_capture(pkg, item)),
        ExprKind::Struct(_, _, fields) => fields.iter().any(|field| {
            (matches!(pkg.get_expr(field.value).ty, Ty::Arrow(_))
                && matches!(pkg.get_expr(field.value).kind, ExprKind::Block(_))
                && !crate::walk_utils::expr_is_safe_to_discard(pkg, pkg.id, field.value))
                || has_inline_struct_capture(pkg, field.value)
        }),
        _ => false,
    }
}

/// Reuses aggregate nodes but replaces each evaluated leaf with a stored read.
/// Moving the leaf's kind to a fresh initializer avoids an additional reference
/// from an orphaned aggregate preventing capture-binding exposure.
fn bind_struct_argument_operands(
    pkg: &mut Package,
    id: ExprId,
    assigner: &mut Assigner,
    statements: &mut Vec<StmtId>,
    initializers: &mut Vec<ExprId>,
) {
    let expr = pkg.get_expr(id).clone();
    match expr.kind {
        ExprKind::Tuple(items) => {
            for item in items {
                bind_struct_argument_operands(pkg, item, assigner, statements, initializers);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(copy) = copy {
                bind_struct_argument_operands(pkg, copy, assigner, statements, initializers);
            }
            for field in fields {
                bind_struct_argument_operands(pkg, field.value, assigner, statements, initializers);
            }
        }
        ExprKind::Lit(_) | ExprKind::Var(Res::Item(_), _) => {}
        _ => {
            let init = crate::fir_builder::alloc_expr(
                pkg,
                assigner,
                expr.ty.clone(),
                expr.kind,
                expr.span,
            );
            let (local, statement) = crate::fir_builder::alloc_local_var(
                pkg,
                assigner,
                "_.struct_operand",
                &expr.ty,
                init,
                Mutability::Immutable,
            );
            statements.push(statement);
            initializers.push(init);
            pkg.exprs.get_mut(id).expect("operand exists").kind =
                ExprKind::Var(Res::Local(local), Vec::new());
        }
    }
}

fn decompose_assignment_tuple_aliases(pkg: &mut Package, assigner: &mut Assigner) {
    let candidates: Vec<_> = collect_promotion_scopes(pkg)
        .into_iter()
        .flat_map(|scope| {
            let mut needed = FxHashSet::default();
            let mut captured = FxHashSet::default();
            for &expr_id in &scope.exprs {
                match &pkg.get_expr(expr_id).kind {
                    ExprKind::Assign(lhs, rhs)
                        if matches!(pkg.get_expr(*lhs).kind, ExprKind::Tuple(_)) =>
                    {
                        crate::walk_utils::for_each_expr(pkg, *rhs, &mut |_, expr| {
                            if let ExprKind::Var(Res::Local(local), _) = expr.kind {
                                needed.insert(local);
                            }
                        });
                    }
                    ExprKind::Closure(locals, _) => captured.extend(locals.iter().copied()),
                    _ => {}
                }
            }
            let bindings: Vec<_> = scope
                .stmts
                .iter()
                .flat_map(|stmt_id| {
                    let StmtKind::Local(Mutability::Immutable, pat_id, init) =
                        pkg.get_stmt(*stmt_id).kind
                    else {
                        return Vec::new();
                    };
                    collect_callable_tuple_bindings(pkg, pat_id)
                        .into_iter()
                        .map(|(local, pat_id)| (local, pat_id, init, *stmt_id))
                        .collect()
                })
                .collect();
            loop {
                let previous = needed.len();
                for &(local, _, init, _) in &bindings {
                    if needed.contains(&local) {
                        crate::walk_utils::for_each_expr(pkg, init, &mut |_, expr| {
                            if let ExprKind::Var(Res::Local(dependency), _) = expr.kind {
                                needed.insert(dependency);
                            }
                        });
                    }
                }
                if previous == needed.len() {
                    break;
                }
            }
            bindings
                .into_iter()
                .filter(|(local, _, _, _)| needed.contains(local))
                .map(|(local, pat_id, _, stmt_id)| {
                    let reads: Vec<_> = scope
                        .exprs
                        .iter()
                        .copied()
                        .filter(|expr_id| {
                            matches!(pkg.get_expr(*expr_id).kind,
                            ExprKind::Var(Res::Local(var), _) if var == local)
                        })
                        .collect();
                    let captured_binding = captured.contains(&local).then(|| {
                        let block_id = scope
                            .blocks
                            .iter()
                            .copied()
                            .find(|&id| pkg.get_block(id).stmts.contains(&stmt_id))
                            .expect("captured binding has a live containing block");
                        (block_id, stmt_id)
                    });
                    (pat_id, reads, captured_binding)
                })
                .collect::<Vec<_>>()
        })
        .collect();

    for (pat_id, reads, captured_binding) in candidates {
        decompose_tuple_alias(pkg, assigner, pat_id, &reads, captured_binding);
    }
}

fn decompose_tuple_alias(
    pkg: &mut Package,
    assigner: &mut Assigner,
    pat_id: PatId,
    reads: &[ExprId],
    captured_binding: Option<(BlockId, StmtId)>,
) {
    let capture_pat = captured_binding.map(|_| {
        let mut pat = pkg.get_pat(pat_id).clone();
        pat.id = assigner.next_pat();
        let id = pat.id;
        pkg.pats.insert(id, pat);
        id
    });
    decompose_tuple_pattern(pkg, assigner, pat_id);
    if let (Some((block_id, stmt_id)), Some(capture_pat)) = (captured_binding, capture_pat) {
        let span = pkg.get_stmt(stmt_id).span;
        let value = tuple_pattern_value(pkg, assigner, pat_id, span);
        let aggregate_binding = crate::fir_builder::alloc_local_stmt(
            pkg,
            assigner,
            Mutability::Immutable,
            capture_pat,
            value,
            span,
        );
        let block = pkg.blocks.get_mut(block_id).expect("live block exists");
        let position = block
            .stmts
            .iter()
            .position(|id| *id == stmt_id)
            .expect("captured binding remains in its live block");
        block.stmts.insert(position + 1, aggregate_binding);
    }
    for &expr_id in reads {
        let span = pkg.get_expr(expr_id).span;
        let replacement = tuple_pattern_value(pkg, assigner, pat_id, span);
        let kind = pkg.get_expr(replacement).kind.clone();
        pkg.exprs.get_mut(expr_id).expect("tuple read exists").kind = kind;
    }
}

fn collect_callable_tuple_bindings(pkg: &Package, pat_id: PatId) -> Vec<(LocalVarId, PatId)> {
    let mut pending = vec![pat_id];
    let mut bindings = Vec::new();
    while let Some(pat_id) = pending.pop() {
        let pat = pkg.get_pat(pat_id);
        match &pat.kind {
            PatKind::Tuple(children) => pending.extend(children.iter().copied()),
            PatKind::Bind(ident)
                if matches!(pat.ty, Ty::Tuple(_)) && super::ty_contains_arrow(&pat.ty) =>
            {
                bindings.push((ident.id, pat_id));
            }
            _ => {}
        }
    }
    bindings
}

fn decompose_tuple_pattern(pkg: &mut Package, assigner: &mut Assigner, pat_id: PatId) {
    let pat = pkg.get_pat(pat_id).clone();
    if let (PatKind::Bind(ident), Ty::Tuple(types)) = (pat.kind, pat.ty) {
        crate::fir_builder::decompose_binding(pkg, assigner, pat_id, &ident.name, &types);
        let PatKind::Tuple(children) = pkg.get_pat(pat_id).kind.clone() else {
            unreachable!("decomposed pattern is a tuple")
        };
        for child in children {
            decompose_tuple_pattern(pkg, assigner, child);
        }
    }
}

fn tuple_pattern_value(
    pkg: &mut Package,
    assigner: &mut Assigner,
    pat_id: PatId,
    span: qsc_fir::fir::PackageSpan,
) -> ExprId {
    let pat = pkg.get_pat(pat_id).clone();
    match pat.kind {
        PatKind::Bind(ident) => {
            crate::fir_builder::alloc_local_var_expr(pkg, assigner, ident.id, pat.ty, span)
        }
        PatKind::Tuple(children) => {
            let values = children
                .into_iter()
                .map(|child| tuple_pattern_value(pkg, assigner, child, span))
                .collect();
            crate::fir_builder::alloc_tuple_expr(pkg, assigner, values, pat.ty, span)
        }
        PatKind::Discard => unreachable!("decomposed binding has no discarded fields"),
    }
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

/// Exposes closure-construction bindings in the enclosing block so later
/// defunctionalization can pass stored captures instead of replaying their
/// initializers at invocation. Evaluation remains at closure creation, including
/// when the declaration is inside a branch or loop.
///
/// # Before
/// ```text
/// let f = {
///     Message("creating");
///     let capture = Logged(17);
///     Closure([capture], target)
/// };
/// Message("ready");
/// f(1)
/// ```
///
/// # After
/// ```text
/// Message("creating");
/// let capture = Logged(17);
/// let f = Closure([capture], target);
/// Message("ready");
/// f(1)   // later rewrite can pass the stored capture to target
/// ```
///
/// # Eligibility
/// - The reachable initializer belongs to an immutable callable binding and
///   consists of immutable bindings and expression statements followed by a
///   closure or conditional callable selection, possibly through nested tail blocks.
/// - At least one prefix expression is not proven side-effect-free and total.
///   Proven discard-safe prefixes retain their existing expression-replay path.
/// - The binding statement, initializer expression, and each traversed block
///   have one incoming reference. Shared candidates are left alone rather than
///   exposing the same locals in multiple contexts.
///
/// # Mutations
/// - Inserts the prefix statement IDs immediately before the callable binding,
///   preserving their order and existing local IDs.
/// - Replaces the initializer's `Block` kind with the tail callable expression.
///   The now-detached inner block remains in the arena for later cleanup.
/// - Processes nested blocks before their owners so an initializer's complete
///   capture prefix is normalized before its statements move to an outer block.
///
/// Saved guard assignments also remain before the binding. Later cleanup can
/// remove an unused selection without either deleting or repeating its guard.
fn expose_callable_initializer_prefixes(pkg: &mut Package, reachable_expr_ids: &[ExprId]) {
    // Reachability limits candidates, but sharing counts cover the whole package:
    // even a reference outside the current traversal prevents a unique-owner rewrite.
    let reachable: FxHashSet<_> = reachable_expr_ids.iter().copied().collect();
    let mut expr_uses: FxHashMap<ExprId, usize> = FxHashMap::default();
    let mut block_uses: FxHashMap<BlockId, usize> = FxHashMap::default();
    let mut stmt_uses: FxHashMap<StmtId, usize> = FxHashMap::default();

    // Count block-to-statement and statement-to-expression edges first.
    for (_, block) in &pkg.blocks {
        for &stmt in &block.stmts {
            *stmt_uses.entry(stmt).or_default() += 1;
        }
    }
    for (_, stmt) in &pkg.stmts {
        match stmt.kind {
            StmtKind::Local(_, _, expr) | StmtKind::Expr(expr) | StmtKind::Semi(expr) => {
                *expr_uses.entry(expr).or_default() += 1;
            }
            StmtKind::Item(_) => {}
        }
    }

    // Count direct expression edges, not recursive visits: a shared parent must
    // not multiply the count of its children's own incoming references.
    for (_, expr) in &pkg.exprs {
        crate::walk_utils::for_each_direct_child(&expr.kind, |child| match child {
            crate::walk_utils::DirectChild::Expr(expr) => {
                *expr_uses.entry(expr).or_default() += 1;
            }
            crate::walk_utils::DirectChild::Block(block) => {
                *block_uses.entry(block).or_default() += 1;
            }
        });
    }

    // Callable bodies and the entry expression are roots, so they contribute
    // references that are not represented by another expression's child edges.
    for (_, item) in &pkg.items {
        if let ItemKind::Callable(decl) = &item.kind
            && let CallableImpl::Spec(spec) = &decl.implementation
        {
            *block_uses.entry(spec.body.block).or_default() += 1;
            for spec in crate::fir_builder::functored_specs(spec) {
                *block_uses.entry(spec.block).or_default() += 1;
            }
        }
    }
    if let Some(entry) = pkg.entry {
        *expr_uses.entry(entry).or_default() += 1;
    }

    // Preserve postorder: an owner can detach its initializer block while
    // retaining that block's statements in the live scope.
    let mut seen_blocks = FxHashSet::default();
    let block_ids: Vec<_> = collect_promotion_scopes(pkg)
        .into_iter()
        .flat_map(|scope| scope.blocks)
        .filter(|&id| seen_blocks.insert(id))
        .collect();
    for block_id in block_ids {
        let mut statements = Vec::new();
        for stmt_id in pkg.get_block(block_id).stmts.clone() {
            // Reusing the existing locals requires a uniquely referenced binding
            // and initializer block; cloning or remapping shared captures is not
            // part of this normalization.
            if let StmtKind::Local(Mutability::Immutable, _, init_id) = pkg.get_stmt(stmt_id).kind
                && reachable.contains(&init_id)
                && stmt_uses.get(&stmt_id) == Some(&1)
                && expr_uses.get(&init_id) == Some(&1)
                && matches!(pkg.get_expr(init_id).ty, Ty::Arrow(_))
                && let Some((prefix, tail)) = callable_initializer_prefix(pkg, init_id, &block_uses)
            {
                // Splice the complete prefix before `let f`, preserving both
                // capture dependencies and any surrounding effects.
                statements.extend(prefix);
                let callable = pkg.get_expr(tail).kind.clone();
                pkg.exprs.get_mut(init_id).expect("initializer exists").kind = callable;
            }
            // Keep the original callable binding (and all unrelated statements)
            // in order; later defunc cleanup decides whether the binding is dead.
            statements.push(stmt_id);
        }
        pkg.blocks.get_mut(block_id).expect("block exists").stmts = statements;
    }
}

/// Follows only unconditional tail blocks; effects inside conditional branches
/// remain there. Mutable declarations and item statements are not moved.
fn callable_initializer_prefix(
    pkg: &Package,
    mut expr: ExprId,
    block_uses: &FxHashMap<BlockId, usize>,
) -> Option<(Vec<StmtId>, ExprId)> {
    let mut prefix = Vec::new();
    let mut needs_storage = false;
    while let ExprKind::Block(block) = pkg.get_expr(expr).kind {
        if block_uses.get(&block) != Some(&1) {
            return None;
        }
        let (&tail, statements) = pkg.get_block(block).stmts.split_last()?;
        for &statement in statements {
            let (StmtKind::Local(Mutability::Immutable, _, value)
            | StmtKind::Semi(value)
            | StmtKind::Expr(value)) = pkg.get_stmt(statement).kind
            else {
                return None;
            };
            needs_storage |= !crate::walk_utils::expr_is_safe_to_discard(pkg, pkg.id, value);
            prefix.push(statement);
        }
        let StmtKind::Expr(tail) = pkg.get_stmt(tail).kind else {
            return None;
        };
        expr = tail;
    }
    (needs_storage
        && matches!(
            pkg.get_expr(expr).kind,
            ExprKind::Closure(..) | ExprKind::If(..)
        ))
    .then_some((prefix, expr))
}

/// Removes statically known callable captures from a partial application by
/// inlining them into the lifted target body, while retaining the other captures.
/// A partial application such as `Repeat(H, 1, _)` lowers to a closure that
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
/// - Targets with item references retain their input: an earlier iteration may
///   already have emitted direct calls carrying the original capture operands.
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
        // Item references can live in other packages. Include detached arena
        // nodes conservatively; this optimization does not rewrite direct uses.
        let referenced_targets: FxHashSet<_> = store
            .iter()
            .flat_map(|(_, package)| package.exprs.iter())
            .filter_map(|(_, expr)| match expr.kind {
                ExprKind::Var(Res::Item(item), _) if item.package == package_id => Some(item.item),
                _ => None,
            })
            .collect();
        let pkg = store.get_mut(package_id);
        for mut group in inlinings {
            let target_rewrite = group.pop().expect("inlining group should not be empty");
            if referenced_targets.contains(&target_rewrite.target) {
                continue;
            }
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
/// binding or assignment. This preserves evaluation order because there is no intervening
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
/// destructures or assigns that binding with exactly one use, the alias statement is
/// elided and the consuming RHS is repointed directly at the original init.
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
                match pkg.get_stmt(stmt_ids[index + 1]).kind {
                    StmtKind::Local(_, _, _) => {
                        if let StmtKind::Local(_, _, expr_id) = &mut pkg
                            .stmts
                            .get_mut(stmt_ids[index + 1])
                            .expect("statement should exist")
                            .kind
                        {
                            *expr_id = init_expr_id;
                        }
                    }
                    StmtKind::Semi(assign_id) => {
                        if let ExprKind::Assign(_, rhs_id) = &mut pkg
                            .exprs
                            .get_mut(assign_id)
                            .expect("assignment should exist")
                            .kind
                        {
                            *rhs_id = init_expr_id;
                        }
                    }
                    _ => unreachable!("promotion requires a binding or assignment"),
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
/// 2. `use_stmt_id` destructures that exact binding via a tuple pattern or target.
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
    if !super::ty_contains_arrow(&alias_pat.ty) {
        return None;
    }

    let use_stmt = pkg.get_stmt(use_stmt_id);
    let use_expr_id = match use_stmt.kind {
        StmtKind::Local(_, use_pat_id, use_expr_id)
            if matches!(pkg.get_pat(use_pat_id).kind, PatKind::Tuple(_)) =>
        {
            use_expr_id
        }
        StmtKind::Semi(assign_id) => {
            let ExprKind::Assign(lhs_id, rhs_id) = pkg.get_expr(assign_id).kind else {
                return None;
            };
            if !matches!(pkg.get_expr(lhs_id).kind, ExprKind::Tuple(_)) {
                return None;
            }
            rhs_id
        }
        _ => return None,
    };
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
    /// Live blocks in postorder, with nested initializers before their owners.
    blocks: Vec<BlockId>,
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
            blocks: Vec::new(),
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
            self.blocks.push(block_id);
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
/// The callee must have the same callable kind, input type, and output type,
/// and support every functor required by the closure. Adapting a function to an
/// operation, changing tuple shape, or discarding a result is not an identity.
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
/// UnOp(Functor(Adj), Var(Item(callee_item)))   // single functor preserved
/// ```
///
/// # Mutations
/// - Rewrites `Expr.kind` at each identity-closure site in place.
/// - Allocates an occurrence-owned operand for a captured-local functor wrapper;
///   the lifted callable's expressions remain unchanged.
fn identity_closure_peephole(
    store: &mut PackageStore,
    package_id: PackageId,
    reachable_expr_ids: &[ExprId],
    assigner: &mut Assigner,
) -> FxHashMap<ExprId, Span> {
    let replacements = {
        let pkg = store.get_mut(package_id);
        collect_identity_closures(pkg, reachable_expr_ids, assigner)
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
            collapsed_spans.insert(expr_id, inner_span);
        }
    }
    collapsed_spans
}

/// Scans reachable expressions and collects `(ExprId, replacement ExprKind,
/// Span)` triples for identity closures. The span is the discarded lambda-body
/// call span, returned to analysis for re-stamping rewritten invocation sites.
/// The closure expression's own span is unchanged.
fn collect_identity_closures(
    pkg: &mut Package,
    reachable_expr_ids: &[ExprId],
    assigner: &mut Assigner,
) -> Vec<(ExprId, ExprKind, Span)> {
    let mut replacements = Vec::new();

    for &expr_id in reachable_expr_ids {
        let expr = pkg.get_expr(expr_id);
        if let ExprKind::Closure(captures, target) = &expr.kind {
            let (captures, target) = (captures.clone(), *target);
            replacements.extend(check_identity_closure(
                pkg, expr_id, &captures, target, assigner,
            ));
        }
    }

    replacements
}

/// Checks whether a closure is an identity wrapper `(args) => f(args)` or a
/// functor-wrapped identity `(args) => Adjoint f(args)` /
/// `(args) => Controlled f(args)`, and returns expression replacements that
/// collapse the closure to a direct reference (optionally functor-applied).
fn check_identity_closure(
    pkg: &mut Package,
    closure_expr_id: ExprId,
    captures: &[LocalVarId],
    target: qsc_fir::fir::LocalItemId,
    assigner: &mut Assigner,
) -> Vec<(ExprId, ExprKind, Span)> {
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

    if !identity_signature_matches(pkg, closure_expr_id, callee_id) {
        return Vec::new();
    }

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
                inner_span.span,
            )]
        }
        // Callee is a global item — replace with the global reference.
        ExprKind::Var(Res::Item(item_id), generic_args) => {
            vec![(
                closure_expr_id,
                ExprKind::Var(Res::Item(*item_id), generic_args.clone()),
                inner_span.span,
            )]
        }
        // Shared lifted targets must retain their own capture parameters.
        ExprKind::UnOp(UnOp::Functor(functor), inner_id) => {
            let inner_expr = pkg.get_expr(*inner_id);
            match &inner_expr.kind {
                ExprKind::Var(Res::Local(var), _) => {
                    let Some(capture_idx) = capture_param_vars.iter().position(|&v| v == *var)
                    else {
                        return Vec::new();
                    };
                    let functor = *functor;
                    let ty = inner_expr.ty.clone();
                    let span = inner_expr.span;
                    let operand =
                        alloc_local_var_expr(pkg, assigner, captures[capture_idx], ty, span);
                    vec![(
                        closure_expr_id,
                        ExprKind::UnOp(UnOp::Functor(functor), operand),
                        inner_span.span,
                    )]
                }
                ExprKind::Var(Res::Item(_), _) => {
                    // Inner expression already references the global item; only
                    // the closure expression needs replacing.
                    vec![(
                        closure_expr_id,
                        ExprKind::UnOp(UnOp::Functor(*functor), *inner_id),
                        inner_span.span,
                    )]
                }
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

/// Checks callable kind, exact input/output shape, and required functor support.
/// Extra callee functors are allowed; unresolved sets must match exactly.
fn identity_signature_matches(pkg: &Package, closure: ExprId, callee: ExprId) -> bool {
    matches!(
        (&pkg.get_expr(closure).ty, &pkg.get_expr(callee).ty),
        (Ty::Arrow(closure), Ty::Arrow(callee))
            if closure.kind == callee.kind
                && closure.input == callee.input
                && closure.output == callee.output
                && match (callee.functors, closure.functors) {
                    (FunctorSet::Value(actual), FunctorSet::Value(required)) => {
                        actual.intersect(&required) == required
                    }
                    (actual, required) => actual == required,
                }
    )
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
