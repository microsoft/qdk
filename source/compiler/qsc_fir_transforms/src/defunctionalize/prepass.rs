// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Pre-pass rewrites before collecting call sites for defunctionalization.
//! These rewrites expose callable captures as explicit input slots and simplify
//! indirection before call-site collection and lattice analysis. Changes to a
//! lifted closure target's input are paired with changes to its closure occurrences.
//!
//! # Responsibilities
//!
//! - Normalize callable-bearing tuple and local UDT capture environments into
//!   leaf capture slots, reconstructing aggregates at reads in the target body
//!   (via [`normalize_closure_environments`]). Arrays and foreign UDTs remain opaque.
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
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    Block, BlockId, CallableImpl, Expr, ExprId, ExprKind, ItemKind, LocalItemId, LocalVarId,
    Mutability, Package, PackageId, PackageLookup, PackageStore, Pat, PatId, PatKind, Res, Stmt,
    StmtId, StmtKind, UnOp,
};
use qsc_fir::ty::Ty;
use qsc_fir::visit::{self, Visitor};
use rustc_hash::{FxHashMap, FxHashSet};

/// Runs pre-pass rewrites before collecting call sites for defunctionalization. See
/// [`normalize_closure_environments`], [`inline_static_closure_captures`],
/// [`promote_single_use_callable_locals`], [`promote_adjacent_aggregate_callable_aliases`],
/// and [`identity_closure_peephole`] for details.
///
/// Closure-environment normalization scans the entry and callable implementations
/// package-wide to keep shared target inputs and their occurrences consistent.
/// Single-use local promotion and identity-closure scans use `reachable_expr_ids`
/// to restrict their candidates to entry-reachable code.
///
/// Returns the map of collapsed identity-closure call expressions to the spans
/// that should be re-stamped onto their rewritten call sites.
pub(super) fn run(
    store: &mut PackageStore,
    package_id: PackageId,
    reachable_expr_ids: &[ExprId],
    assigner: &mut Assigner,
) -> FxHashMap<ExprId, Span> {
    normalize_closure_environments(store.get_mut(package_id), package_id, assigner);
    inline_static_closure_captures(store, package_id, reachable_expr_ids);
    promote_single_use_callable_locals(store, package_id, reachable_expr_ids);
    promote_adjacent_aggregate_callable_aliases(store, package_id);
    decompose_assignment_tuple_aliases(store.get_mut(package_id), assigner);
    identity_closure_peephole(store, package_id, reachable_expr_ids)
}

/// The decomposable shape of one capture, preserving enough type information to
/// reconstruct its original value inside the lifted target.
///
/// Tuples and resolvable UDTs owned by this package are traversed recursively.
/// Arrays, foreign UDTs, and other types remain leaves, even if their types contain
/// callables. All leaf consumers use depth-first, left-to-right field order.
#[derive(Clone)]
enum CaptureEnvironment {
    /// An opaque value retained as one capture slot, including a direct callable.
    Leaf(Ty),
    Tuple(Vec<Self>),
    /// The original constructor identity and its decomposable payload shape.
    Udt(qsc_fir::fir::ItemId, Box<Self>),
}

impl CaptureEnvironment {
    fn from_ty(pkg: &Package, package_id: PackageId, ty: &Ty) -> Self {
        match ty {
            Ty::Tuple(items) => Self::Tuple(
                items
                    .iter()
                    .map(|ty| Self::from_ty(pkg, package_id, ty))
                    .collect(),
            ),
            Ty::Udt(Res::Item(item_id)) if item_id.package == package_id => {
                if let Some(item) = pkg.items.get(item_id.item)
                    && let ItemKind::Ty(_, udt) = &item.kind
                {
                    Self::Udt(
                        *item_id,
                        Box::new(Self::from_ty(pkg, package_id, &udt.get_pure_ty())),
                    )
                } else {
                    Self::Leaf(ty.clone())
                }
            }
            _ => Self::Leaf(ty.clone()),
        }
    }

    fn ty(&self) -> Ty {
        match self {
            Self::Leaf(ty) => ty.clone(),
            Self::Tuple(items) => Ty::Tuple(items.iter().map(Self::ty).collect()),
            Self::Udt(item, _) => Ty::Udt(Res::Item(*item)),
        }
    }

    /// Finds a callable exposed by decomposition, without looking inside opaque leaves.
    fn has_arrow(&self) -> bool {
        match self {
            Self::Leaf(ty) => matches!(ty, Ty::Arrow(_)),
            Self::Tuple(items) => items.iter().any(Self::has_arrow),
            Self::Udt(_, inner) => inner.has_arrow(),
        }
    }

    /// Appends types in the same order as projection and aggregate reconstruction.
    fn leaf_types(&self, types: &mut Vec<Ty>) {
        match self {
            Self::Leaf(ty) => types.push(ty.clone()),
            Self::Tuple(items) => {
                for item in items {
                    item.leaf_types(types);
                }
            }
            Self::Udt(_, inner) => inner.leaf_types(types),
        }
    }
}

/// A target-wide plan collected before any input or expression is rewritten.
struct EnvironmentNormalization {
    /// The existing tuple input whose identity is retained during expansion.
    input: PatId,
    /// Original capture-prefix patterns followed by ordinary argument patterns.
    patterns: Vec<PatId>,
    /// One entry per original capture; `None` leaves that capture slot unchanged.
    slots: Vec<Option<(LocalVarId, CaptureEnvironment)>>,
    /// Every scanned closure expression referring to this target.
    occurrences: Vec<ExprId>,
    body_exprs: Vec<ExprId>,
}

/// Nested closure occurrence -> enclosing capture binding -> replacement leaf locals.
/// Closure captures store local IDs rather than expressions, so nested recaptures
/// cannot use the aggregate reconstruction applied to ordinary local reads.
type RecapturedLeaves = FxHashMap<ExprId, FxHashMap<LocalVarId, Vec<LocalVarId>>>;

/// Exposes callable-bearing aggregate captures as explicit leaves for Defunc analysis.
///
/// For example, one `(Int, (Int -> Int, Int))` capture becomes three capture slots.
/// The lifted input, every closure occurrence, and reads of the original capture
/// must agree on this layout. Ordinary arguments and ineligible captures retain
/// their positions relative to the expanded capture slots.
fn normalize_closure_environments(
    pkg: &mut Package,
    package_id: PackageId,
    assigner: &mut Assigner,
) {
    let mut plans = collect_environment_normalizations(pkg, package_id);
    // An outer capture can expand only if every nested recapture expands the
    // corresponding inner slot. Removing an inner plan can invalidate another
    // outer plan, so prune to a fixed point before mutating the package.
    loop {
        let incompatible: Vec<_> = plans
            .iter()
            .filter_map(|(target, plan)| {
                let compatible = plan.body_exprs.iter().all(|expr_id| {
                    let ExprKind::Closure(captures, inner_target) = &pkg.get_expr(*expr_id).kind
                    else {
                        return true;
                    };
                    captures.iter().enumerate().all(|(index, capture)| {
                        !plan
                            .slots
                            .iter()
                            .flatten()
                            .any(|(local, _)| local == capture)
                            || plans.get(inner_target).is_some_and(|inner| {
                                inner.slots.get(index).is_some_and(Option::is_some)
                            })
                    })
                });
                (!compatible).then_some(*target)
            })
            .collect();
        if incompatible.is_empty() {
            break;
        }
        for target in incompatible {
            plans.remove(&target);
        }
    }
    let mut ordered: Vec<_> = plans.into_iter().collect();
    // Stable target order keeps allocation deterministic despite hash-map iteration.
    ordered.sort_unstable_by_key(|(target, _)| *target);
    let mut recaptures = RecapturedLeaves::default();
    // Allocate all replacement bindings before rewriting occurrences: an inner
    // target may sort before the enclosing target that supplies its new captures.
    for (_, plan) in &ordered {
        normalize_closure_input(pkg, assigner, plan, &mut recaptures);
    }
    for (target, plan) in ordered {
        normalize_closure_occurrences(pkg, assigner, target, &plan, &recaptures);
    }
}

/// Collects eligible capture-prefix slots and all occurrences sharing each target.
///
/// Scanning entry and callable bodies, rather than only the current reachable set,
/// also finds direct references that require the target's existing input layout.
/// Only bound aggregate captures exposing a direct arrow leaf are expanded; a
/// direct arrow capture already has the desired shape and needs no plan entry.
fn collect_environment_normalizations(
    pkg: &Package,
    package_id: PackageId,
) -> FxHashMap<LocalItemId, EnvironmentNormalization> {
    let exprs: FxHashSet<_> = collect_promotion_scopes(pkg)
        .into_iter()
        .flat_map(|scope| scope.exprs)
        .collect();
    let mut references: FxHashMap<LocalItemId, Vec<ExprId>> = FxHashMap::default();
    let mut direct = FxHashSet::default();
    for &expr_id in &exprs {
        match &pkg.get_expr(expr_id).kind {
            ExprKind::Closure(_, target) => references.entry(*target).or_default().push(expr_id),
            ExprKind::Var(Res::Item(item), _) if item.package == package_id => {
                direct.insert(item.item);
            }
            _ => {}
        }
    }
    let mut plans = FxHashMap::default();
    for (target, mut occurrences) in references {
        if direct.contains(&target) {
            // Direct item references do not carry a closure capture list that
            // this rewrite can expand alongside the target input.
            continue;
        }
        let Some(item) = pkg.items.get(target) else {
            continue;
        };
        let ItemKind::Callable(decl) = &item.kind else {
            continue;
        };
        if !matches!(decl.implementation, CallableImpl::Spec(_)) {
            continue;
        }
        let PatKind::Tuple(patterns) = &pkg.get_pat(decl.input).kind else {
            continue;
        };
        occurrences.sort_unstable();
        let ExprKind::Closure(captures, _) = &pkg.get_expr(occurrences[0]).kind else {
            unreachable!()
        };
        let count = captures.len();
        // The leading input patterns represent captures. A shared target needs
        // one consistent capture-prefix length across all of its occurrences.
        if patterns.len() < count
            || occurrences.iter().any(|expr_id| {
                !matches!(&pkg.get_expr(*expr_id).kind, ExprKind::Closure(captures, _) if captures.len() == count)
            })
        {
            continue;
        }
        let slots: Vec<_> = patterns[..count]
            .iter()
            .map(|pat_id| {
                let pat = pkg.get_pat(*pat_id);
                let PatKind::Bind(ident) = &pat.kind else {
                    return None;
                };
                let environment = CaptureEnvironment::from_ty(pkg, package_id, &pat.ty);
                (!matches!(environment, CaptureEnvironment::Leaf(_)) && environment.has_arrow())
                    .then_some((ident.id, environment))
            })
            .collect();
        if slots.iter().all(Option::is_none) {
            continue;
        }
        let mut scope = PromotionScope::new(pkg);
        scope.visit_callable_impl(&decl.implementation);
        plans.insert(
            target,
            EnvironmentNormalization {
                input: decl.input,
                patterns: patterns.clone(),
                slots,
                occurrences,
                body_exprs: scope.exprs,
            },
        );
    }
    plans
}

/// Replaces planned input bindings with leaf bindings and reconstructs their reads.
///
/// The original input pattern ID and body expression IDs remain valid. Nested
/// closure capture lists are deferred through `recaptures` until all target inputs
/// have been expanded; ordinary arguments and unplanned capture bindings are reused.
fn normalize_closure_input(
    pkg: &mut Package,
    assigner: &mut Assigner,
    plan: &EnvironmentNormalization,
    recaptures: &mut RecapturedLeaves,
) {
    let mut input = Vec::new();
    let mut replacements = FxHashMap::default();
    for (index, &pat_id) in plan.patterns.iter().enumerate() {
        if let Some(Some((local, environment))) = plan.slots.get(index) {
            let mut types = Vec::new();
            environment.leaf_types(&mut types);
            let mut locals = Vec::new();
            for ty in types {
                let (leaf, pattern) = crate::fir_builder::alloc_bind_pat(
                    pkg,
                    assigner,
                    "capture_leaf",
                    ty,
                    pkg.get_pat(pat_id).span,
                );
                locals.push(leaf);
                input.push(pattern);
            }
            replacements.insert(*local, (environment, locals));
        } else {
            input.push(pat_id);
        }
    }
    let ty = Ty::Tuple(
        input
            .iter()
            .map(|pat_id| pkg.get_pat(*pat_id).ty.clone())
            .collect(),
    );
    let pat = pkg.pats.get_mut(plan.input).expect("closure input exists");
    pat.kind = PatKind::Tuple(input);
    pat.ty = ty;
    for &expr_id in &plan.body_exprs {
        let expr = pkg.get_expr(expr_id).clone();
        match expr.kind {
            ExprKind::Var(Res::Local(local), _) => {
                if let Some((environment, locals)) = replacements.get(&local) {
                    let replacement = environment_value(
                        pkg,
                        assigner,
                        environment,
                        &mut locals.iter().copied(),
                        expr.span,
                    );
                    // The reconstructed value has the original capture type;
                    // replacing only the kind preserves this read's ID and metadata.
                    pkg.exprs
                        .get_mut(expr_id)
                        .expect("capture read exists")
                        .kind = pkg.get_expr(replacement).kind.clone();
                }
            }
            ExprKind::Closure(_, _) => {
                recaptures.insert(
                    expr_id,
                    replacements
                        .iter()
                        .map(|(local, (_, leaves))| (*local, leaves.clone()))
                        .collect(),
                );
            }
            _ => {}
        }
    }
}

/// Expands capture operands to match the target's new input layout.
///
/// Existing leaf locals are forwarded for nested recaptures. Other aggregates
/// are projected into immutable locals at the original closure-creation site,
/// preserving the captured values even if the source local is later reassigned.
/// The closure expression keeps its original callable type and expression ID.
fn normalize_closure_occurrences(
    pkg: &mut Package,
    assigner: &mut Assigner,
    target: LocalItemId,
    plan: &EnvironmentNormalization,
    recaptures: &RecapturedLeaves,
) {
    for &expr_id in &plan.occurrences {
        let expr = pkg.get_expr(expr_id).clone();
        let ExprKind::Closure(captures, _) = expr.kind else {
            unreachable!()
        };
        let mut expanded = Vec::new();
        let mut statements = Vec::new();
        for (capture, slot) in captures.into_iter().zip(&plan.slots) {
            let Some((_, environment)) = slot else {
                expanded.push(capture);
                continue;
            };
            if let Some(leaves) = recaptures
                .get(&expr_id)
                .and_then(|locals| locals.get(&capture))
            {
                // This enclosing aggregate binding was replaced by leaf inputs;
                // forward them instead of reading a local that no longer exists.
                expanded.extend(leaves);
                continue;
            }
            let value = crate::fir_builder::alloc_local_var_expr(
                pkg,
                assigner,
                capture,
                environment.ty(),
                expr.span,
            );
            let mut projections = Vec::new();
            project_environment(
                pkg,
                assigner,
                environment,
                value,
                expr.span,
                &mut projections,
            );
            for projection in projections {
                let ty = pkg.get_expr(projection).ty.clone();
                let (local, statement) = crate::fir_builder::alloc_local_var(
                    pkg,
                    assigner,
                    "capture_leaf",
                    &ty,
                    projection,
                    Mutability::Immutable,
                );
                expanded.push(local);
                statements.push(statement);
            }
        }
        let kind = if statements.is_empty() {
            ExprKind::Closure(expanded, target)
        } else {
            // Keep projection evaluation at closure creation, before producing
            // the closure value, rather than moving reads into its eventual call.
            let closure = crate::fir_builder::alloc_expr(
                pkg,
                assigner,
                expr.ty.clone(),
                ExprKind::Closure(expanded, target),
                expr.span,
            );
            statements.push(crate::fir_builder::alloc_expr_stmt(
                pkg, assigner, closure, expr.span,
            ));
            ExprKind::Block(crate::fir_builder::alloc_block(
                pkg, assigner, statements, expr.ty, expr.span,
            ))
        };
        pkg.exprs
            .get_mut(expr_id)
            .expect("closure occurrence exists")
            .kind = kind;
    }
}

/// Appends leaf projections in the order used for the normalized target input.
///
/// `value` is a captured-local read or a projection derived from one, not an
/// effectful producer. This builds expressions only; the caller binds the leaves
/// at the closure occurrence to retain capture-time evaluation.
fn project_environment(
    pkg: &mut Package,
    assigner: &mut Assigner,
    environment: &CaptureEnvironment,
    value: ExprId,
    span: qsc_fir::fir::PackageSpan,
    leaves: &mut Vec<ExprId>,
) {
    match environment {
        CaptureEnvironment::Leaf(_) => leaves.push(value),
        CaptureEnvironment::Tuple(items) => {
            for (index, item) in items.iter().enumerate() {
                let field = crate::fir_builder::alloc_field_expr(
                    pkg,
                    assigner,
                    value,
                    index,
                    item.ty(),
                    span,
                );
                project_environment(pkg, assigner, item, field, span, leaves);
            }
        }
        CaptureEnvironment::Udt(_, inner) => {
            // Tuple-backed UDTs support field projection directly. A non-tuple
            // payload needs an explicit unwrap before further decomposition.
            if matches!(inner.as_ref(), CaptureEnvironment::Tuple(_)) {
                project_environment(pkg, assigner, inner, value, span, leaves);
            } else {
                let unwrapped = crate::fir_builder::alloc_expr(
                    pkg,
                    assigner,
                    inner.ty(),
                    ExprKind::UnOp(UnOp::Unwrap, value),
                    span,
                );
                project_environment(pkg, assigner, inner, unwrapped, span, leaves);
            }
        }
    }
}

/// Reconstructs an original aggregate from its ordered replacement leaf bindings.
///
/// Consumes exactly one local per leaf, in the order established by `leaf_types`
/// and `project_environment`. UDT reconstruction uses the original type item's
/// constructor, preserving nominal type identity rather than leaving a bare payload.
fn environment_value(
    pkg: &mut Package,
    assigner: &mut Assigner,
    environment: &CaptureEnvironment,
    locals: &mut impl Iterator<Item = LocalVarId>,
    span: qsc_fir::fir::PackageSpan,
) -> ExprId {
    match environment {
        CaptureEnvironment::Leaf(ty) => crate::fir_builder::alloc_local_var_expr(
            pkg,
            assigner,
            locals.next().expect("environment leaf exists"),
            ty.clone(),
            span,
        ),
        CaptureEnvironment::Tuple(items) => {
            let values = items
                .iter()
                .map(|item| environment_value(pkg, assigner, item, locals, span))
                .collect();
            crate::fir_builder::alloc_tuple_expr(pkg, assigner, values, environment.ty(), span)
        }
        CaptureEnvironment::Udt(item, inner) => {
            let value = environment_value(pkg, assigner, inner, locals, span);
            let constructor_ty = Ty::Arrow(Box::new(qsc_fir::ty::Arrow {
                kind: qsc_fir::fir::CallableKind::Function,
                input: Box::new(inner.ty()),
                output: Box::new(environment.ty()),
                functors: qsc_fir::ty::FunctorSet::Value(qsc_fir::ty::FunctorSetValue::Empty),
            }));
            let constructor =
                crate::fir_builder::alloc_item_var_expr(pkg, assigner, *item, constructor_ty, span);
            crate::fir_builder::alloc_call_expr(
                pkg,
                assigner,
                constructor,
                value,
                environment.ty(),
                span,
            )
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
                    (pat_id, reads, captured.contains(&local).then_some(stmt_id))
                })
                .collect::<Vec<_>>()
        })
        .collect();

    for (pat_id, reads, captured_stmt) in candidates {
        decompose_tuple_alias(pkg, assigner, pat_id, &reads, captured_stmt);
    }
}

fn decompose_tuple_alias(
    pkg: &mut Package,
    assigner: &mut Assigner,
    pat_id: PatId,
    reads: &[ExprId],
    captured_stmt: Option<qsc_fir::fir::StmtId>,
) {
    let capture_pat = captured_stmt.map(|_| {
        let mut pat = pkg.get_pat(pat_id).clone();
        pat.id = assigner.next_pat();
        let id = pat.id;
        pkg.pats.insert(id, pat);
        id
    });
    decompose_tuple_pattern(pkg, assigner, pat_id);
    if let (Some(stmt_id), Some(capture_pat)) = (captured_stmt, capture_pat) {
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
        for (_, block) in pkg.blocks.iter_mut() {
            if let Some(position) = block.stmts.iter().position(|id| *id == stmt_id) {
                block.stmts.insert(position + 1, aggregate_binding);
                break;
            }
        }
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
                if matches!(pat.ty, Ty::Tuple(_)) && ty_contains_arrow(&pat.ty) =>
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
    if !ty_contains_arrow(&alias_pat.ty) {
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
                Some(inner_span.span),
            )]
        }
        // Callee is a global item — replace with the global reference.
        ExprKind::Var(Res::Item(item_id), generic_args) => {
            vec![(
                closure_expr_id,
                ExprKind::Var(Res::Item(*item_id), generic_args.clone()),
                Some(inner_span.span),
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
                            Some(inner_span.span),
                        ),
                    ]
                }
                ExprKind::Var(Res::Item(_), _) => {
                    // Inner expression already references the global item; only
                    // the closure expression needs replacing.
                    vec![(
                        closure_expr_id,
                        ExprKind::UnOp(UnOp::Functor(*functor), *inner_id),
                        Some(inner_span.span),
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
