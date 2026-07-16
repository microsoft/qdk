// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Monomorphization pass — the first pass in the pipeline.
//!
//! Replaces every generic callable reference in entry-reachable code with a
//! concrete specialization, one per unique `(callable, generic_args)` pair,
//! and rewrites call sites to use it. `Identity(42)` becomes
//! `Call(Var(Identity<Int>, []), 42)` with a freshly cloned `Identity<Int>`
//! callable inserted into the package that owns the generic source callable.
//!
//! # What to know before diving in
//!
//! - **Establishes [`crate::invariants::InvariantLevel::PostMono`]:** no
//!   `Ty::Param`, no `FunctorSet::Param`, no non-empty `ExprKind::Var`
//!   generic-argument lists, and no reachable generic callable items remain
//!   in reachable code.
//! - **Three phases:** *Discovery* collects concrete generic references;
//!   *Specialization* drives a worklist that clones each body, substitutes
//!   type params, and feeds back transitive generic references it finds;
//!   *Rewrite* redirects call sites and closure targets across every reachable
//!   package and (via `collect_rewrite_scope_for_package`) walks closure items
//!   so generic sites in lifted lambdas are not missed.
//! - **Special cases:** identity instantiations (`[Param(0), ...]`) are
//!   skipped (they would duplicate the original); intrinsics get their
//!   argument lists cleared in place with no new callable; generic references
//!   in foreign bodies are specialized in place in the package that owns the
//!   source callable, so each package receives its own concrete
//!   specializations.

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "slow-proptest-tests"))]
mod semantic_equivalence_tests;

use crate::cloner::FirCloner;
use crate::fir_builder::{functored_specs, reachable_local_callables};
use crate::package_assigners::PackageAssigners;
use crate::reachability::{collect_reachable_from_entry, collect_reachable_package_closure};
use crate::walk_utils::{
    CallableNode, collect_expr_ids_in_entry_and_local_callables,
    collect_expr_ids_in_local_callables, extend_expr_ids_in_local_callables,
    for_each_node_from_expr_root, for_each_node_in_callable,
};
use qsc_fir::fir::{
    BlockId, CallableDecl, CallableImpl, ExprId, ExprKind, Ident, Item, ItemId, ItemKind,
    LocalItemId, LocalVarId, Package, PackageId, PackageLookup, PackageStore, PatId, PatKind, Res,
    StmtId, StmtKind, StoreItemId, Visibility,
};
use qsc_fir::ty::{Arrow, FunctorSet, GenericArg, ParamId, Ty, TypeParameter};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;
use std::rc::Rc;

/// A recorded specialization: the source callable + args, and where it was
/// placed in the package that owns the source callable.
struct Specialization {
    source: StoreItemId,
    args: Vec<GenericArg>,
    new_item_id: ItemId,
}

/// Monomorphizes all generic callable references in the entry-reachable portion
/// of a package.
///
/// After this pass, no `Ty::Param` or `FunctorSet::Param` values remain in
/// reachable code, and all `ExprKind::Var` nodes have empty generic-argument
/// lists.
///
/// # Panics
///
/// Panics if the package has no entry expression. The reachability scans
/// in this pass go through [`collect_reachable_from_entry`], which asserts
/// `package.entry.is_some()`.
pub fn monomorphize(
    store: &mut PackageStore,
    package_id: PackageId,
    assigners: &mut PackageAssigners,
) {
    let instantiations = discover_instantiations(store, package_id);
    if !instantiations.is_empty() {
        // Create specialized callables, allocating each into the package that owns
        // the generic source callable (specialize-in-place). The per-package
        // assigners thread the advanced id watermarks back into the pipeline.
        let specializations = create_specializations(store, instantiations, assigners);

        // Rewrite generic call sites across every reachable package so foreign
        // bodies that call the now-specialized generics point at the concrete
        // specializations in their own arena.
        rewrite_all_packages(store, package_id, &specializations);
    }

    // After monomorphization no generic callable may remain reachable in any
    // package. Holds even when no instantiation was discovered (a reachable
    // generic would have produced one).
    assert_no_reachable_generic(store, package_id);
}

/// Asserts the post-monomorphization invariant that no generic callable remains
/// reachable in any package.
///
/// Generics are specialized in place in their owning package and their call
/// sites are redirected to the concrete specializations, leaving the original
/// generic entry-unreachable (item DCE prunes it later). Codegen rejects any
/// surviving generic, and the implicit coupling that keeps a generic from
/// staying reachable — for example via a first-class or closure use in a
/// library body that the call-site rewrite cannot redirect — is otherwise
/// unasserted. This hard `assert!` makes a violation fail
/// deterministically in release rather than miscompiling downstream.
///
/// # Panics
///
/// Panics if any reachable callable in any reachable package still carries
/// generic type parameters.
fn assert_no_reachable_generic(store: &PackageStore, entry_pkg_id: PackageId) {
    let reachable = collect_reachable_from_entry(store, entry_pkg_id);
    for store_item_id in &reachable {
        let package = store.get(store_item_id.package);
        if let Some(item) = package.items.get(store_item_id.item)
            && let ItemKind::Callable(decl) = &item.kind
        {
            assert!(
                decl.generics.is_empty(),
                "monomorphization left a generic callable `{}` reachable in package {:?}",
                decl.name.name,
                store_item_id.package
            );
        }
    }
}

/// Rewrites generic call sites and generic closure targets in every reachable package.
///
/// For each package in the entry-reachable closure, collects the rewrite
/// scope (reachable callables, the entry expression for the entry package,
/// the new specializations owned by that package, and their transitive
/// closures) and redirects `ExprKind::Var(Item(generic), [concrete])` sites
/// plus `ExprKind::Closure` targets to the matching specialization.
fn rewrite_all_packages(
    store: &mut PackageStore,
    entry_pkg_id: PackageId,
    specializations: &[Specialization],
) {
    // Build a single lookup from (source key) → new ItemId, shared across all
    // packages so foreign callers resolve specializations regardless of which
    // package owns them.
    let lookup: FxHashMap<String, ItemId> = specializations
        .iter()
        .map(|s| (mono_key(s.source, &s.args), s.new_item_id))
        .collect();

    let reachable = collect_reachable_from_entry(store, entry_pkg_id);
    let packages = collect_reachable_package_closure(entry_pkg_id, &reachable);

    // Group the newly created specialization local ids by their owning package
    // so each package's rewrite scope can include its own fresh specializations
    // (which are not yet reachable from entry until call sites are redirected).
    let mut new_specs_by_pkg: FxHashMap<PackageId, Vec<LocalItemId>> = FxHashMap::default();
    for s in specializations {
        new_specs_by_pkg
            .entry(s.new_item_id.package)
            .or_default()
            .push(s.new_item_id.item);
    }

    let empty: Vec<LocalItemId> = Vec::new();
    for &pkg_id in &packages {
        // This package's own fresh specializations (empty for packages that
        // received none). They must be threaded through explicitly because they
        // are not yet reachable from entry — nothing points at them until the
        // call sites below are redirected.
        let new_specs = new_specs_by_pkg.get(&pkg_id).unwrap_or(&empty);

        // Gather the set of expressions to rewrite while the store is still
        // borrowed immutably. This has to happen before `get_mut` below, because
        // computing the scope reads across packages (reachable callables, the
        // entry expression, and the transitive closure items), which cannot
        // coexist with the mutable borrow the rewrites require.
        let expr_ids =
            collect_rewrite_scope_for_package(store, entry_pkg_id, pkg_id, &reachable, new_specs);

        // Now take the mutable borrow of just this package and apply both
        // rewrites to it.
        let package = store.get_mut(pkg_id);

        // Redirect direct call sites first: this is what actually makes the new
        // specializations reachable from entry.
        rewrite_call_sites(package, pkg_id, &lookup, &expr_ids);

        // Then repoint generic closure targets. This runs after the call-site
        // rewrite (and drives its own worklist over the now-reachable
        // specializations) so closures nested in freshly reachable bodies are
        // not missed.
        rewrite_closure_targets_in_package(
            package,
            pkg_id,
            entry_pkg_id,
            &reachable,
            new_specs,
            &lookup,
        );
    }
}

/// Redirects every generic `ExprKind::Closure` target in a package to its
/// concrete specialization, following closures nested inside other closures.
///
/// A closure whose target callable is generic (e.g. a lambda over `Identity<'T>`)
/// must be repointed at the monomorphized clone, just like a direct call site.
/// Because a specialized body can itself contain fresh closures that are not yet
/// reachable from entry, this drives a worklist that discovers newly referenced
/// closure targets as it rewrites, so no nested generic closure is missed.
///
/// # Transformation
///
/// ```text
/// // before: closure targets the generic item
/// Closure([...captures], Identity)          // Identity is generic <'T>
/// // after: repointed at the concrete clone for the inferred args
/// Closure([...captures], Identity<Int>)
/// ```
///
/// The seed worklist is every reachable callable in the package plus the newly
/// created specializations owned by it (which are not reachable from entry until
/// call sites are redirected). For the entry package the entry expression is
/// rewritten directly and its closure targets are added as extra seeds.
fn rewrite_closure_targets_in_package(
    package: &mut Package,
    pkg_id: PackageId,
    entry_pkg_id: PackageId,
    reachable: &FxHashSet<StoreItemId>,
    new_spec_items: &[LocalItemId],
    lookup: &FxHashMap<String, ItemId>,
) {
    // Seed the worklist with every callable reachable in this package, plus the
    // specializations just created for it (still unreachable from entry until
    // call sites are redirected, so they must be added explicitly).
    let mut worklist: Vec<LocalItemId> = reachable_local_callables(package, pkg_id, reachable)
        .map(|(id, _)| id)
        .collect();
    worklist.extend_from_slice(new_spec_items);

    // The entry expression lives outside any callable item, so rewrite it here
    // and feed the closure items it references back into the worklist.
    if pkg_id == entry_pkg_id
        && let Some(entry_id) = package.entry
    {
        worklist.extend(collect_closure_targets_in_expr_root(package, entry_id));
        rewrite_closure_targets_in_expr_root(package, pkg_id, entry_id, lookup);
    }

    // Process each callable once. Rewriting a body can surface further closure
    // items (closures nested in closures), which are appended to the worklist
    // and picked up on a later iteration until the set closes.
    let mut seen = FxHashSet::default();
    while let Some(item_id) = worklist.pop() {
        if !seen.insert(item_id) {
            continue;
        }
        let Some(item) = package.items.get(item_id) else {
            continue;
        };
        let ItemKind::Callable(decl) = &item.kind else {
            continue;
        };
        // Discover any closure targets this body references, then repoint the
        // generic ones at their concrete specializations.
        worklist.extend(collect_closure_targets_in_callable(package, decl));
        rewrite_closure_targets_in_callable(package, pkg_id, item_id, lookup);
    }
}

/// Collects all expression IDs in `pkg_id` that may contain generic call sites
/// requiring rewriting: reachable callables in that package, the entry
/// expression (entry package only), the newly created specializations owned by
/// that package, and any closure items transitively referenced by those.
fn collect_rewrite_scope_for_package(
    store: &PackageStore,
    entry_pkg_id: PackageId,
    pkg_id: PackageId,
    reachable: &FxHashSet<StoreItemId>,
    new_spec_items: &[LocalItemId],
) -> Vec<ExprId> {
    let package = store.get(pkg_id);
    let local_item_ids: Vec<_> = reachable_local_callables(package, pkg_id, reachable)
        .map(|(id, _)| id)
        .collect();
    let mut expr_ids = if pkg_id == entry_pkg_id {
        collect_expr_ids_in_entry_and_local_callables(package, &local_item_ids)
    } else {
        collect_expr_ids_in_local_callables(package, &local_item_ids)
    };
    let mut seen: FxHashSet<ExprId> = expr_ids.iter().copied().collect();

    // We computed reachability after creating specializations but before
    // rewriting call sites, so new specializations aren't reachable from
    // entry yet. Those new specializations may reference newly-cloned
    // closure items that are also unreachable from entry until call sites
    // are redirected.
    let mut walked_items: FxHashSet<LocalItemId> = local_item_ids.into_iter().collect();
    walked_items.extend(new_spec_items.iter().copied());

    let mut scan_start = expr_ids.len();
    extend_expr_ids_in_local_callables(package, new_spec_items, &mut expr_ids, &mut seen);

    // Transitively walk closure items whose bodies may also contain generic
    // call sites that need rewriting.
    loop {
        let mut new_closures = Vec::new();
        for &expr_id in &expr_ids[scan_start..] {
            if let ExprKind::Closure(_, local_item_id) = &package.get_expr(expr_id).kind
                && walked_items.insert(*local_item_id)
            {
                new_closures.push(*local_item_id);
            }
        }
        if new_closures.is_empty() {
            break;
        }
        scan_start = expr_ids.len();
        extend_expr_ids_in_local_callables(package, &new_closures, &mut expr_ids, &mut seen);
    }

    expr_ids
}

/// Walks all entry-reachable code and collects every unique
/// `(StoreItemId, Vec<GenericArg>)` pair where the generic args are non-empty
/// and fully concrete.
fn discover_instantiations(
    store: &PackageStore,
    package_id: PackageId,
) -> Vec<(StoreItemId, Vec<GenericArg>)> {
    let reachable = collect_reachable_from_entry(store, package_id);
    let mut found: Vec<(StoreItemId, Vec<GenericArg>)> = Vec::new();
    let mut seen_keys: FxHashSet<String> = FxHashSet::default();

    let package = store.get(package_id);

    // Walk the entry expression.
    if let Some(entry_id) = package.entry {
        collect_generic_refs_in_expr(package_id, package, entry_id, &mut found, &mut seen_keys);
    }

    // Walk every reachable callable body.
    for item_id in &reachable {
        let pkg = store.get(item_id.package);
        let Some(item) = pkg.items.get(item_id.item) else {
            // Interpreter entry expressions can carry runtime-unbound item references
            // after a rejected callable definition. Leave those for later evaluation
            // diagnostics instead of panicking during reachability discovery.
            continue;
        };
        if let ItemKind::Callable(decl) = &item.kind {
            collect_generic_refs_in_callable(
                item_id.package,
                pkg,
                decl,
                &mut found,
                &mut seen_keys,
            );
        }
    }

    found.retain(|(_, args)| is_fully_concrete(args));

    found
}

/// Deterministic dedup key for a `(StoreItemId, &[GenericArg])` pair.
fn mono_key(source: StoreItemId, args: &[GenericArg]) -> String {
    use std::fmt::Write;
    let mut key = format!("{source}:");
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            key.push(',');
        }
        write!(key, "{arg}").expect("formatting should not fail");
    }
    key
}

/// Builds a unique mangled name for a monomorphized callable by appending the
/// concrete generic arguments to the base name using `<Arg1, Arg2>` notation.
///
/// Functor set arguments use compact identifiers (`Empty`, `Adj`, `Ctl`,
/// `AdjCtl`) instead of the user-facing display forms. The intrinsic `Length`
/// is exempt because downstream passes match on that name literally.
fn mono_name(decl: &CallableDecl, args: &[GenericArg]) -> Rc<str> {
    use std::fmt::Write;
    if matches!(decl.implementation, CallableImpl::Intrinsic) && decl.name.name.as_ref() == "Length"
    {
        return Rc::clone(&decl.name.name);
    }
    let mut name = decl.name.name.to_string();
    name.push('<');
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            name.push_str(", ");
        }
        match arg {
            GenericArg::Ty(ty) => write!(name, "{ty}").expect("formatting should not fail"),
            GenericArg::Functor(FunctorSet::Value(v)) => name.push_str(v.mangle_name()),
            GenericArg::Functor(f) => write!(name, "{f}").expect("formatting should not fail"),
        }
    }
    name.push('>');
    Rc::from(name.as_str())
}

/// Walks a callable's body collecting every concrete generic reference from
/// `ExprKind::Var(Res::Item(..), args)` sites and `ExprKind::Closure` targets,
/// deduplicated via `mono_key` in `seen`.
fn collect_generic_refs_in_callable(
    pkg_id: PackageId,
    pkg: &Package,
    decl: &CallableDecl,
    found: &mut Vec<(StoreItemId, Vec<GenericArg>)>,
    seen: &mut FxHashSet<String>,
) {
    let (expr_ids, local_tys) = collect_expr_ids_and_local_tys_in_callable(pkg, decl);
    collect_generic_refs_in_expr_ids(pkg_id, pkg, &expr_ids, &local_tys, found, seen);
}

/// Walks a single expression subtree collecting `(StoreItemId, Vec<GenericArg>)`
/// pairs the same way as [`collect_generic_refs_in_callable`], used for the
/// package entry expression.
fn collect_generic_refs_in_expr(
    pkg_id: PackageId,
    pkg: &Package,
    expr_id: ExprId,
    found: &mut Vec<(StoreItemId, Vec<GenericArg>)>,
    seen: &mut FxHashSet<String>,
) {
    let (expr_ids, local_tys) = collect_expr_ids_and_local_tys_in_expr_root(pkg, expr_id);
    collect_generic_refs_in_expr_ids(pkg_id, pkg, &expr_ids, &local_tys, found, seen);
}

/// Scans a flat list of expression ids for the two kinds of generic reference
/// this pass must specialize, recording each unique `(callable, args)` pair.
///
/// A `Var(Res::Item(id), args)` with a non-empty `args` list is a direct
/// generic reference (a call or a first-class use). A `Closure(captures, item)`
/// may target a generic lambda whose concrete type arguments are not written
/// out anywhere, so they are reconstructed from the capture and arrow types via
/// [`infer_closure_generic_args`]. Both kinds are deduplicated through `seen`.
fn collect_generic_refs_in_expr_ids(
    pkg_id: PackageId,
    pkg: &Package,
    expr_ids: &[ExprId],
    local_tys: &FxHashMap<LocalVarId, Ty>,
    found: &mut Vec<(StoreItemId, Vec<GenericArg>)>,
    seen: &mut FxHashSet<String>,
) {
    for &expr_id in expr_ids {
        let expr = pkg.get_expr(expr_id);
        match &expr.kind {
            // A named reference that carries generic args (e.g. `Identity<Int>`):
            // record it directly.
            ExprKind::Var(Res::Item(item_id), generic_args) if !generic_args.is_empty() => {
                let store_id = StoreItemId::from((item_id.package, item_id.item));
                record_generic_ref(store_id, generic_args.clone(), found, seen);
            }
            // A closure may point at a generic lambda without spelling out its
            // args; recover them from the closure's concrete type before
            // recording. A non-generic (or un-inferable) closure is skipped.
            ExprKind::Closure(captures, local_item_id) => {
                let store_id = StoreItemId::from((pkg_id, *local_item_id));
                if let Some(generic_args) =
                    infer_closure_generic_args(pkg, *local_item_id, captures, &expr.ty, local_tys)
                {
                    record_generic_ref(store_id, generic_args, found, seen);
                }
            }
            _ => {}
        }
    }
}

/// Records a `(callable, args)` pair into `found`, keyed and deduplicated by its
/// [`mono_key`] so the same instantiation is never queued for specialization
/// twice.
fn record_generic_ref(
    store_id: StoreItemId,
    generic_args: Vec<GenericArg>,
    found: &mut Vec<(StoreItemId, Vec<GenericArg>)>,
    seen: &mut FxHashSet<String>,
) {
    let key = mono_key(store_id, &generic_args);
    if seen.insert(key) {
        found.push((store_id, generic_args));
    }
}

/// Walks a callable's body once, returning every expression id it contains and
/// a map from each binding's `LocalVarId` to its type.
///
/// The local-type map lets [`infer_closure_generic_args`] resolve a closure's
/// capture types, since captures are referenced by `LocalVarId` rather than by
/// carrying their own type.
fn collect_expr_ids_and_local_tys_in_callable(
    pkg: &Package,
    decl: &CallableDecl,
) -> (Vec<ExprId>, FxHashMap<LocalVarId, Ty>) {
    let mut expr_ids = Vec::new();
    let mut local_tys = FxHashMap::default();
    for_each_node_in_callable(pkg, decl, &mut |node| match node {
        CallableNode::Expr(expr_id) => expr_ids.push(expr_id),
        CallableNode::Pat(pat_id) => collect_pat_bind_ty(pkg, pat_id, &mut local_tys),
        CallableNode::Block(_) | CallableNode::Stmt(_) => {}
    });
    (expr_ids, local_tys)
}

/// Same as [`collect_expr_ids_and_local_tys_in_callable`] but walks a single
/// root expression subtree, used for the package entry expression.
fn collect_expr_ids_and_local_tys_in_expr_root(
    pkg: &Package,
    expr_id: ExprId,
) -> (Vec<ExprId>, FxHashMap<LocalVarId, Ty>) {
    let mut expr_ids = Vec::new();
    let mut local_tys = FxHashMap::default();
    for_each_node_from_expr_root(pkg, expr_id, &mut |node| match node {
        CallableNode::Expr(expr_id) => expr_ids.push(expr_id),
        CallableNode::Pat(pat_id) => collect_pat_bind_ty(pkg, pat_id, &mut local_tys),
        CallableNode::Block(_) | CallableNode::Stmt(_) => {}
    });
    (expr_ids, local_tys)
}

/// Records a single `LocalVarId -> Ty` entry when `pat_id` is a `Bind` pattern.
/// Tuple and discard patterns bind no single local directly, so they add
/// nothing (their leaf `Bind`s are visited separately by the walker).
fn collect_pat_bind_ty(pkg: &Package, pat_id: PatId, local_tys: &mut FxHashMap<LocalVarId, Ty>) {
    let pat = pkg.get_pat(pat_id);
    if let PatKind::Bind(ident) = &pat.kind {
        local_tys.insert(ident.id, pat.ty.clone());
    }
}

/// Returns the target item id of every `Closure` reachable from a root
/// expression (the entry expression), used to seed the rewrite worklist.
fn collect_closure_targets_in_expr_root(pkg: &Package, expr_id: ExprId) -> Vec<LocalItemId> {
    let (expr_ids, _) = collect_expr_ids_and_local_tys_in_expr_root(pkg, expr_id);
    collect_closure_targets_in_expr_ids(pkg, &expr_ids)
}

/// Returns the target item id of every `Closure` in a callable body, used to
/// discover lambdas nested inside a body so the worklist can visit them too.
fn collect_closure_targets_in_callable(pkg: &Package, decl: &CallableDecl) -> Vec<LocalItemId> {
    let (expr_ids, _) = collect_expr_ids_and_local_tys_in_callable(pkg, decl);
    collect_closure_targets_in_expr_ids(pkg, &expr_ids)
}

/// Filters a flat expression list down to the target item ids of its `Closure`
/// nodes.
fn collect_closure_targets_in_expr_ids(pkg: &Package, expr_ids: &[ExprId]) -> Vec<LocalItemId> {
    expr_ids
        .iter()
        .filter_map(|&expr_id| match pkg.get_expr(expr_id).kind {
            ExprKind::Closure(_, local_item_id) => Some(local_item_id),
            _ => None,
        })
        .collect()
}

/// Repoints the generic `Closure` targets in a single callable body at their
/// concrete specializations (per [`rewrite_closure_targets_in_expr_ids`]).
fn rewrite_closure_targets_in_callable(
    pkg: &mut Package,
    pkg_id: PackageId,
    item_id: LocalItemId,
    lookup: &FxHashMap<String, ItemId>,
) {
    let Some(item) = pkg.items.get(item_id) else {
        return;
    };
    let ItemKind::Callable(decl) = &item.kind else {
        return;
    };
    let (expr_ids, local_tys) = collect_expr_ids_and_local_tys_in_callable(pkg, decl);
    rewrite_closure_targets_in_expr_ids(pkg, pkg_id, &expr_ids, &local_tys, lookup);
}

/// Repoints the generic `Closure` targets in a root expression (the entry
/// expression) at their concrete specializations.
fn rewrite_closure_targets_in_expr_root(
    pkg: &mut Package,
    pkg_id: PackageId,
    expr_id: ExprId,
    lookup: &FxHashMap<String, ItemId>,
) {
    let (expr_ids, local_tys) = collect_expr_ids_and_local_tys_in_expr_root(pkg, expr_id);
    rewrite_closure_targets_in_expr_ids(pkg, pkg_id, &expr_ids, &local_tys, lookup);
}

/// Repoints each generic `Closure` target in `expr_ids` at the matching
/// concrete specialization found in `lookup`.
///
/// A closure carries no explicit generic-argument list, so the target's
/// concrete args are inferred from the closure's capture and arrow types,
/// keyed with [`mono_key`], and looked up. A closure is left unchanged when its
/// target is not generic, no matching specialization exists, or the
/// specialization lives in another package (a `Closure` target id is only
/// meaningful inside its own package's arena).
fn rewrite_closure_targets_in_expr_ids(
    pkg: &mut Package,
    pkg_id: PackageId,
    expr_ids: &[ExprId],
    local_tys: &FxHashMap<LocalVarId, Ty>,
    lookup: &FxHashMap<String, ItemId>,
) {
    for &expr_id in expr_ids {
        // Only closures are candidates; skip everything else.
        let Some((captures, local_item_id, expr_ty)) = closure_parts(pkg, expr_id) else {
            continue;
        };
        // Recover the concrete generic args from the closure's type; a
        // non-generic or un-inferable closure has none and is left alone.
        let Some(generic_args) =
            infer_closure_generic_args(pkg, local_item_id, &captures, &expr_ty, local_tys)
        else {
            continue;
        };
        // Find the specialization created for this exact instantiation.
        let key = mono_key(StoreItemId::from((pkg_id, local_item_id)), &generic_args);
        let Some(new_item_id) = lookup.get(&key).copied() else {
            continue;
        };
        // A closure target id is package-local, so a foreign specialization
        // cannot be named here; leave it for that package's own rewrite pass.
        if new_item_id.package != pkg_id {
            continue;
        }
        // Redirect the closure to the concrete clone.
        let expr = pkg.exprs.get_mut(expr_id).expect("expr should exist");
        if let ExprKind::Closure(_, target) = &mut expr.kind {
            *target = new_item_id.item;
        }
    }
}

/// Extracts the `(captures, target item, closure type)` of a `Closure`
/// expression, or `None` for any other kind. The parts are cloned out so the
/// shared borrow ends before the package is mutated in place by the caller.
fn closure_parts(pkg: &Package, expr_id: ExprId) -> Option<(Vec<LocalVarId>, LocalItemId, Ty)> {
    let expr = pkg.get_expr(expr_id);
    if let ExprKind::Closure(captures, local_item_id) = &expr.kind {
        Some((captures.clone(), *local_item_id, expr.ty.clone()))
    } else {
        None
    }
}

/// Reconstructs the concrete generic arguments of a closure whose target is a
/// generic lambda, by unifying the lambda's declared (generic) signature
/// against the closure's actual capture and arrow types.
///
/// A closure expression carries no explicit generic-argument list, but its FIR
/// type is fully concrete and its captures thread in the outer values. The
/// lambda item's input pattern is `(captures..., original_input)`, so the
/// actual input is rebuilt as a tuple of the capture types followed by the
/// closure arrow's input, then unified position-by-position against the
/// lambda's formal input, output, and functor set to solve each parameter.
///
/// # Example
/// ```text
/// // generic lambda item:  <'T>(cap : 'T, x : 'T) -> 'T
/// // closure expr type:    (Int) -> Int, capturing `cap : Int`
/// // rebuilt actual input: (Int, Int)   =>  unify 'T := Int
/// // inferred args:        ['T = Int]
/// ```
///
/// Returns `None` when the target is non-generic, its type is not an arrow, a
/// capture type is unknown, or unification fails or leaves a parameter unsolved.
fn infer_closure_generic_args(
    pkg: &Package,
    local_item_id: LocalItemId,
    captures: &[LocalVarId],
    closure_ty: &Ty,
    local_tys: &FxHashMap<LocalVarId, Ty>,
) -> Option<Vec<GenericArg>> {
    // Only a generic callable target has args to infer.
    let ItemKind::Callable(decl) = &pkg.get_item(local_item_id).kind else {
        return None;
    };
    if decl.generics.is_empty() {
        return None;
    }
    // The closure's own type must be an arrow to unify against.
    let Ty::Arrow(closure_arrow) = closure_ty else {
        return None;
    };

    // Rebuild the lambda's actual input as `(capture_tys..., closure_input)` to
    // match the lambda item's `(captures..., original_input)` parameter shape.
    // Bail if any capture's type is unknown.
    let mut arg_map = FxHashMap::default();
    let capture_tys: Vec<_> = captures
        .iter()
        .map(|local| local_tys.get(local).cloned())
        .collect::<Option<_>>()?;
    let actual_input = Ty::Tuple(
        capture_tys
            .into_iter()
            .chain(std::iter::once((*closure_arrow.input).clone()))
            .collect(),
    );

    // Unify formal-vs-actual input, output, and functors; each step solves more
    // parameters into `arg_map`. Any shape mismatch aborts the inference.
    if !infer_generic_ty_args(&pkg.get_pat(decl.input).ty, &actual_input, &mut arg_map) {
        return None;
    }
    if !infer_generic_ty_args(&decl.output, &closure_arrow.output, &mut arg_map) {
        return None;
    }
    if !infer_generic_functor_args(
        FunctorSet::Value(decl.functors),
        closure_arrow.functors,
        &mut arg_map,
    ) {
        return None;
    }

    // Emit one arg per declared parameter in order; if any went unsolved (or
    // resolved to the wrong kind), the whole inference fails.
    decl.generics
        .iter()
        .enumerate()
        .map(
            |(idx, param)| match (param, arg_map.get(&ParamId::from(idx))) {
                (TypeParameter::Ty { .. }, Some(GenericArg::Ty(ty))) => {
                    Some(GenericArg::Ty(ty.clone()))
                }
                (TypeParameter::Functor(_), Some(GenericArg::Functor(functors))) => {
                    Some(GenericArg::Functor(*functors))
                }
                _ => None,
            },
        )
        .collect()
}

/// Structurally unifies a `formal` (possibly generic) type against a concrete
/// `actual` type, recording every solved type/functor parameter in `arg_map`.
///
/// A bare `Ty::Param` binds directly to whatever `actual` is; compound types
/// (array, arrow, tuple) recurse into their matching components; leaf types
/// must be equal. Returns `false` on any shape mismatch or a conflicting
/// re-binding of an already-solved parameter.
fn infer_generic_ty_args(
    formal: &Ty,
    actual: &Ty,
    arg_map: &mut FxHashMap<ParamId, GenericArg>,
) -> bool {
    match (formal, actual) {
        (Ty::Param(param), _) => {
            record_inferred_arg(*param, GenericArg::Ty(actual.clone()), arg_map)
        }
        (Ty::Array(formal), Ty::Array(actual)) => infer_generic_ty_args(formal, actual, arg_map),
        (Ty::Arrow(formal), Ty::Arrow(actual)) => {
            formal.kind == actual.kind
                && infer_generic_ty_args(&formal.input, &actual.input, arg_map)
                && infer_generic_ty_args(&formal.output, &actual.output, arg_map)
                && infer_generic_functor_args(formal.functors, actual.functors, arg_map)
        }
        (Ty::Tuple(formal), Ty::Tuple(actual)) if formal.len() == actual.len() => formal
            .iter()
            .zip(actual)
            .all(|(formal, actual)| infer_generic_ty_args(formal, actual, arg_map)),
        (Ty::Prim(formal), Ty::Prim(actual)) => formal == actual,
        (Ty::Udt(formal), Ty::Udt(actual)) => formal == actual,
        (Ty::Infer(formal), Ty::Infer(actual)) => formal == actual,
        (Ty::Err, Ty::Err) => true,
        _ => false,
    }
}

/// Unifies a `formal` functor set against a concrete `actual` one: a
/// `FunctorSet::Param` binds to `actual`, otherwise the two must be equal.
fn infer_generic_functor_args(
    formal: FunctorSet,
    actual: FunctorSet,
    arg_map: &mut FxHashMap<ParamId, GenericArg>,
) -> bool {
    match formal {
        FunctorSet::Param(param) => {
            record_inferred_arg(param, GenericArg::Functor(actual), arg_map)
        }
        _ => formal == actual,
    }
}

/// Records a solved parameter binding, returning `false` if the parameter was
/// already bound to a different argument (an inconsistent unification).
fn record_inferred_arg(
    param: ParamId,
    arg: GenericArg,
    arg_map: &mut FxHashMap<ParamId, GenericArg>,
) -> bool {
    if let Some(existing) = arg_map.get(&param) {
        existing == &arg
    } else {
        arg_map.insert(param, arg);
        true
    }
}

/// Returns `true` when all generic args map to their own parameter position —
/// e.g., `[Param(0), Param(1)]` for a 2-parameter callable. Cloning with such
/// args would produce a useless duplicate identical to the original generic.
fn is_identity_instantiation(args: &[GenericArg]) -> bool {
    args.iter().enumerate().all(|(i, arg)| match arg {
        GenericArg::Ty(Ty::Param(p)) | GenericArg::Functor(FunctorSet::Param(p)) => {
            *p == ParamId::from(i)
        }
        _ => false,
    })
}

/// Returns `true` when no `Ty::Param` or `FunctorSet::Param` appears at any
/// depth inside the given generic args.
fn is_fully_concrete(args: &[GenericArg]) -> bool {
    args.iter().all(|arg| match arg {
        GenericArg::Ty(ty) => !ty_contains_param(ty),
        GenericArg::Functor(FunctorSet::Param(_)) => false,
        GenericArg::Functor(_) => true,
    })
}

/// Returns `true` when a `Ty` contains a `Ty::Param` or `FunctorSet::Param`
/// anywhere in its structure.
fn ty_contains_param(ty: &Ty) -> bool {
    match ty {
        Ty::Param(_) => true,
        Ty::Array(inner) => ty_contains_param(inner),
        Ty::Arrow(arrow) => {
            ty_contains_param(&arrow.input)
                || ty_contains_param(&arrow.output)
                || matches!(arrow.functors, FunctorSet::Param(_))
        }
        Ty::Tuple(items) => items.iter().any(ty_contains_param),
        _ => false,
    }
}

/// Walks a cloned callable body and collects every fully concrete generic
/// reference from `ExprKind::Var(Res::Item(id), args)` sites and
/// `ExprKind::Closure` targets.
fn scan_for_concrete_generic_refs(
    pkg_id: PackageId,
    pkg: &Package,
    decl: &CallableDecl,
) -> Vec<(StoreItemId, Vec<GenericArg>)> {
    let mut found = Vec::new();
    let mut seen = FxHashSet::default();
    collect_generic_refs_in_callable(pkg_id, pkg, decl, &mut found, &mut seen);
    found.retain(|(_, args)| is_fully_concrete(args));
    found
}

/// Drives the worklist that clones each requested `(callable, args)` pair
/// into the package that owns the generic source callable (specialize-in-place),
/// substitutes type parameters, and scans the cloned bodies for additional
/// transitively-referenced generic sites.
///
/// Each worklist item allocates into `source_id.package` via that package's
/// assigner (from [`PackageAssigners`]), so foreign packages receive their own
/// concrete specializations rather than having their callables cloned into the
/// entry package.
fn create_specializations(
    store: &mut PackageStore,
    instantiations: Vec<(StoreItemId, Vec<GenericArg>)>,
    assigners: &mut PackageAssigners,
) -> Vec<Specialization> {
    let mut specializations = Vec::new();

    // Pre-populate seen keys from initial discovery.
    let mut seen_keys: FxHashSet<String> = instantiations
        .iter()
        .map(|(source, args)| mono_key(*source, args))
        .collect();
    let mut worklist: VecDeque<(StoreItemId, Vec<GenericArg>)> = instantiations.into();

    while let Some((source_id, args)) = worklist.pop_front() {
        // Skip identity instantiations — cloning with these produces a
        // useless duplicate identical to the original generic callable.
        if is_identity_instantiation(&args) {
            continue;
        }

        let owning_pkg_id = source_id.package;

        // Allocate into the owning package using its own assigner. A fresh
        // `FirCloner` per item (empty maps, persisted assigner watermark) is
        // equivalent to reusing one cloner with `reset_maps` between items.
        let (new_item_id, new_refs) =
            assigners.with_package(store, owning_pkg_id, |store, assigner| {
                // Take the owning package out of the store so we can read the
                // source callable and mutate the package simultaneously. The
                // source and target are the same package, so no cross-package
                // borrow is required.
                let mut owning_pkg = std::mem::take(store.get_mut(owning_pkg_id));

                let mut cloner = FirCloner::from_assigner(assigner);

                let result = specialize_one(
                    &mut owning_pkg,
                    owning_pkg_id,
                    source_id,
                    &args,
                    &mut cloner,
                );

                *store.get_mut(owning_pkg_id) = owning_pkg;
                (cloner.into_assigner(), result)
            });

        // Feed transitively-referenced generic sites back into the worklist.
        for (ref_id, ref_args) in new_refs {
            let key = mono_key(ref_id, &ref_args);
            if seen_keys.insert(key) {
                worklist.push_back((ref_id, ref_args));
            }
        }

        specializations.push(Specialization {
            source: source_id,
            args,
            new_item_id,
        });
    }

    specializations
}

/// Clones a single `(callable, args)` pair into its owning package, substitutes
/// type parameters, and returns the new item id plus any concrete generic
/// references discovered in the cloned body that require their own
/// specializations.
fn specialize_one(
    owning_pkg: &mut Package,
    owning_pkg_id: PackageId,
    source_id: StoreItemId,
    args: &[GenericArg],
    cloner: &mut FirCloner,
) -> (ItemId, Vec<(StoreItemId, Vec<GenericArg>)>) {
    // Extract read-only data from the owning package.
    let (body_pkg, decl_snapshot) = {
        let source_item = owning_pkg.get_item(source_id.item);
        let ItemKind::Callable(source_decl) = &source_item.kind else {
            panic!("expected StoreItemId {source_id} to refer to a callable");
        };
        let source_decl = source_decl.as_ref();
        let body_pkg = extract_callable_body(owning_pkg, source_decl);
        let decl_snapshot = source_decl.clone();
        (body_pkg, decl_snapshot)
    };

    // Clone body into the owning package, substitute types, and insert.
    let new_local_id = cloner.alloc_item();
    let new_item_id = ItemId {
        package: owning_pkg_id,
        item: new_local_id,
    };
    let old_item_id = ItemId {
        package: source_id.package,
        item: source_id.item,
    };

    // `alloc_item()` already reserved the id at `new_local_id`, and nested
    // items cloned during `clone_callable_impl` receive larger ids. Nothing
    // reads that slot before the real callable is inserted below, so no
    // placeholder is needed.
    cloner.set_self_item_remap(old_item_id, new_item_id);

    // Clone the input before the impl so `local_map` holds the input
    // parameter mappings when the callable body is walked.
    let new_input = cloner.clone_input_pat(&body_pkg, decl_snapshot.input, owning_pkg);
    let new_impl = cloner.clone_callable_impl(&body_pkg, &decl_snapshot.implementation, owning_pkg);

    // Substitute Ty::Param / FunctorSet::Param in all cloned nodes.
    let arg_map = build_arg_map(args);
    substitute_types_in_cloned_nodes(owning_pkg, cloner, &arg_map);

    let output = substitute_ty(&decl_snapshot.output, &arg_map);

    let spec_name = mono_name(&decl_snapshot, args);
    let spec_decl = CallableDecl {
        span: decl_snapshot.span,
        kind: decl_snapshot.kind,
        name: Ident {
            id: LocalVarId::default(),
            span: decl_snapshot.name.span,
            name: spec_name,
        },
        generics: vec![],
        input: new_input,
        output,
        functors: decl_snapshot.functors,
        implementation: new_impl,
        attrs: decl_snapshot.attrs.clone(),
    };

    let new_item = Item {
        id: new_local_id,
        span: decl_snapshot.span,
        parent: None,
        doc: Rc::from(""),
        attrs: vec![],
        visibility: Visibility::Public,
        kind: ItemKind::Callable(Box::new(spec_decl)),
    };
    owning_pkg.items.insert(new_local_id, new_item);

    let refs_out = collect_new_generic_refs(owning_pkg, owning_pkg_id, new_local_id);

    (new_item_id, refs_out)
}

/// Scans a freshly created monomorphized callable for concrete generic
/// references that require their own specializations.
///
/// References to items already non-generic in the owning package (for example
/// self-references from a recursive callable remapped by `set_self_item_remap`)
/// are dropped; every other concrete reference is returned for the caller to
/// enqueue.
fn collect_new_generic_refs(
    owning_pkg: &Package,
    owning_pkg_id: PackageId,
    new_local_id: LocalItemId,
) -> Vec<(StoreItemId, Vec<GenericArg>)> {
    // Scan the newly created callable for additional concrete generic
    // references that need their own specializations. Skip references to
    // items in the owning package that are already non-generic (e.g.,
    // self-references from recursive callables that were remapped by
    // set_self_item_remap).
    let mut refs_out = Vec::new();
    let created_item = owning_pkg.items.get(new_local_id).expect("just inserted");
    if let ItemKind::Callable(created_decl) = &created_item.kind {
        let new_refs = scan_for_concrete_generic_refs(owning_pkg_id, owning_pkg, created_decl);
        for (ref_id, ref_args) in new_refs {
            if ref_id.package == owning_pkg_id
                && let Some(ref_item) = owning_pkg.items.get(ref_id.item)
                && let ItemKind::Callable(ref_decl) = &ref_item.kind
                && ref_decl.generics.is_empty()
            {
                continue;
            }
            refs_out.push((ref_id, ref_args));
        }
    }
    refs_out
}

/// Builds a standalone `Package` holding all nodes transitively referenced
/// by a callable's body so that [`FirCloner`] can read from it without
/// holding a reference to the original source package.
fn extract_callable_body(source_pkg: &Package, decl: &CallableDecl) -> Package {
    let mut body_pkg = Package::default();

    // Input pattern.
    extract_pat(source_pkg, decl.input, &mut body_pkg);

    match &decl.implementation {
        CallableImpl::Intrinsic => {}
        CallableImpl::Spec(spec_impl) => {
            extract_spec_decl_body(source_pkg, &spec_impl.body, &mut body_pkg);
            for spec in functored_specs(spec_impl) {
                extract_spec_decl_body(source_pkg, spec, &mut body_pkg);
            }
        }
        CallableImpl::SimulatableIntrinsic(spec) => {
            extract_spec_decl_body(source_pkg, spec, &mut body_pkg);
        }
    }

    body_pkg
}

/// Copies the input pattern and body block of a `SpecDecl` from `source` into
/// `target`.
fn extract_spec_decl_body(source: &Package, spec: &qsc_fir::fir::SpecDecl, target: &mut Package) {
    if let Some(pat_id) = spec.input {
        extract_pat(source, pat_id, target);
    }
    extract_block(source, spec.block, target);
}

/// Recursively copies a block and all statements it references.
fn extract_block(source: &Package, block_id: BlockId, target: &mut Package) {
    if target.blocks.contains_key(block_id) {
        return;
    }
    let block = source.get_block(block_id);
    target.blocks.insert(block_id, block.clone());
    for &stmt_id in &block.stmts {
        extract_stmt(source, stmt_id, target);
    }
}

/// Recursively copies a statement and any patterns, expressions, or items it
/// references.
fn extract_stmt(source: &Package, stmt_id: StmtId, target: &mut Package) {
    if target.stmts.contains_key(stmt_id) {
        return;
    }
    let stmt = source.get_stmt(stmt_id);
    target.stmts.insert(stmt_id, stmt.clone());
    match &stmt.kind {
        StmtKind::Expr(e) | StmtKind::Semi(e) => extract_expr(source, *e, target),
        StmtKind::Local(_, pat, expr) => {
            extract_pat(source, *pat, target);
            extract_expr(source, *expr, target);
        }
        StmtKind::Item(item_id) => {
            extract_item(source, *item_id, target);
        }
    }
}

/// Recursively copies an expression and its transitive references.
///
/// This is intentionally a separate implementation from the nearly
/// identical `extract_expr` in `defunctionalize/specialize.rs`. The key
/// difference is the `ExprKind::Closure` arm: monomorphize follows the
/// closure's lifted item via [`extract_item`] because type substitution
/// (`Ty::Param` → concrete) must be applied to the lambda body when a
/// generic callable is monomorphized. Without extracting the item,
/// `substitute_types_in_cloned_nodes` would miss it.
fn extract_expr(source: &Package, expr_id: ExprId, target: &mut Package) {
    if target.exprs.contains_key(expr_id) {
        return;
    }
    let expr = source.get_expr(expr_id);
    target.exprs.insert(expr_id, expr.clone());
    match &expr.kind {
        ExprKind::Array(es) | ExprKind::ArrayLit(es) | ExprKind::Tuple(es) => {
            for &e in es {
                extract_expr(source, e, target);
            }
        }
        ExprKind::ArrayRepeat(a, b)
        | ExprKind::Assign(a, b)
        | ExprKind::AssignOp(_, a, b)
        | ExprKind::BinOp(_, a, b)
        | ExprKind::Call(a, b)
        | ExprKind::Index(a, b)
        | ExprKind::AssignField(a, _, b)
        | ExprKind::UpdateField(a, _, b) => {
            extract_expr(source, *a, target);
            extract_expr(source, *b, target);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            extract_expr(source, *a, target);
            extract_expr(source, *b, target);
            extract_expr(source, *c, target);
        }
        ExprKind::Block(block_id) => extract_block(source, *block_id, target),
        ExprKind::Fail(e) | ExprKind::Field(e, _) | ExprKind::Return(e) | ExprKind::UnOp(_, e) => {
            extract_expr(source, *e, target);
        }
        ExprKind::If(cond, body, otherwise) => {
            extract_expr(source, *cond, target);
            extract_expr(source, *body, target);
            if let Some(e) = otherwise {
                extract_expr(source, *e, target);
            }
        }
        ExprKind::Range(s, st, e) => {
            for x in [s, st, e].into_iter().flatten() {
                extract_expr(source, *x, target);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(c) = copy {
                extract_expr(source, *c, target);
            }
            for fa in fields {
                extract_expr(source, fa.value, target);
            }
        }
        ExprKind::String(components) => {
            for c in components {
                if let qsc_fir::fir::StringComponent::Expr(e) = c {
                    extract_expr(source, *e, target);
                }
            }
        }
        ExprKind::While(cond, block) => {
            extract_expr(source, *cond, target);
            extract_block(source, *block, target);
        }
        ExprKind::Closure(_, local_item_id) => {
            extract_item(source, *local_item_id, target);
        }
        ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
    }
}

/// Recursively copies a local item (callable, namespace, or UDT) and every
/// body node it references so nested items referenced via `StmtKind::Item`
/// or `ExprKind::Closure` remain resolvable.
fn extract_item(source: &Package, item_id: LocalItemId, target: &mut Package) {
    if target.items.contains_key(item_id) {
        return;
    }
    let item = source.get_item(item_id);
    target.items.insert(item_id, item.clone());
    if let ItemKind::Callable(decl) = &item.kind {
        // Extract all nodes transitively referenced by this callable into
        // the target body package.
        extract_pat(source, decl.input, target);
        match &decl.implementation {
            CallableImpl::Intrinsic => {}
            CallableImpl::Spec(spec_impl) => {
                extract_spec_decl_body(source, &spec_impl.body, target);
                for spec in functored_specs(spec_impl) {
                    extract_spec_decl_body(source, spec, target);
                }
            }
            CallableImpl::SimulatableIntrinsic(spec) => {
                extract_spec_decl_body(source, spec, target);
            }
        }
    }
}

/// Recursively copies a pattern and its sub-patterns.
fn extract_pat(source: &Package, pat_id: PatId, target: &mut Package) {
    if target.pats.contains_key(pat_id) {
        return;
    }
    let pat = source.get_pat(pat_id);
    target.pats.insert(pat_id, pat.clone());
    if let PatKind::Tuple(sub_pats) = &pat.kind {
        for &p in sub_pats {
            extract_pat(source, p, target);
        }
    }
}

/// Builds a `ParamId → GenericArg` map by pairing positional arguments with
/// their index as the parameter identifier.
fn build_arg_map(args: &[GenericArg]) -> FxHashMap<ParamId, GenericArg> {
    args.iter()
        .enumerate()
        .map(|(ix, arg)| (ParamId::from(ix), arg.clone()))
        .collect()
}

/// Replaces every `Ty::Param` in `ty` with its mapped concrete type.
fn substitute_ty(ty: &Ty, arg_map: &FxHashMap<ParamId, GenericArg>) -> Ty {
    match ty {
        Ty::Param(param) => match arg_map.get(param) {
            Some(GenericArg::Ty(concrete)) => concrete.clone(),
            _ => ty.clone(),
        },
        Ty::Array(inner) => Ty::Array(Box::new(substitute_ty(inner, arg_map))),
        Ty::Arrow(arrow) => Ty::Arrow(Box::new(substitute_arrow(arrow, arg_map))),
        Ty::Tuple(items) => Ty::Tuple(items.iter().map(|t| substitute_ty(t, arg_map)).collect()),
        Ty::Prim(_) | Ty::Udt(_) | Ty::Infer(_) | Ty::Err => ty.clone(),
    }
}

/// Applies [`substitute_ty`] and [`substitute_functor_set`] to each field of
/// an `Arrow` type.
fn substitute_arrow(arrow: &Arrow, arg_map: &FxHashMap<ParamId, GenericArg>) -> Arrow {
    Arrow {
        kind: arrow.kind,
        input: Box::new(substitute_ty(&arrow.input, arg_map)),
        output: Box::new(substitute_ty(&arrow.output, arg_map)),
        functors: substitute_functor_set(arrow.functors, arg_map),
    }
}

/// Replaces a `FunctorSet::Param` with its mapped concrete functor set.
fn substitute_functor_set(
    functors: FunctorSet,
    arg_map: &FxHashMap<ParamId, GenericArg>,
) -> FunctorSet {
    match functors {
        FunctorSet::Param(param) => match arg_map.get(&param) {
            Some(GenericArg::Functor(concrete)) => *concrete,
            _ => functors,
        },
        _ => functors,
    }
}

/// Walks all nodes that the cloner inserted into the target package and
/// replaces `Ty::Param` / `FunctorSet::Param` with concrete types.
/// Also substitutes types inside generic args on `ExprKind::Var` expressions
/// and clears generic args that become concrete after substitution.
///
/// # Before
/// ```text
/// Expr { ty: Ty::Param(0), kind: Var(item, [Ty(Param(0))]) }
/// Block { ty: Ty::Param(0) }
/// Pat { ty: Ty::Param(0) }
/// ```
/// # After
/// ```text
/// Expr { ty: Int, kind: Var(item, [Ty(Int)]) }   // Param(0) → Int
/// Block { ty: Int }
/// Pat { ty: Int }
/// ```
///
/// # Mutations
/// - Rewrites `Expr.ty`, `Block.ty`, and `Pat.ty` for every cloned node.
/// - Substitutes generic args on `ExprKind::Var` expressions.
/// - Substitutes callable declaration output types for nested items.
fn substitute_types_in_cloned_nodes(
    target: &mut Package,
    cloner: &FirCloner,
    arg_map: &FxHashMap<ParamId, GenericArg>,
) {
    // Blocks.
    for &new_id in cloner.block_map().values() {
        if let Some(block) = target.blocks.get_mut(new_id) {
            block.ty = substitute_ty(&block.ty, arg_map);
        }
    }

    // Expressions — substitute types and handle generic args on Var.
    for &new_id in cloner.expr_map().values() {
        if let Some(expr) = target.exprs.get_mut(new_id) {
            expr.ty = substitute_ty(&expr.ty, arg_map);

            // Substitute types within generic args on Var.
            if let ExprKind::Var(_, ref mut generic_args) = expr.kind
                && !generic_args.is_empty()
            {
                for ga in generic_args.iter_mut() {
                    *ga = substitute_generic_arg(ga, arg_map);
                }
                // Do not clear here — rewrite_call_sites needs the
                // substituted args to find the monomorphized target.
            }
        }
    }

    // Patterns.
    for &new_id in cloner.pat_map().values() {
        if let Some(pat) = target.pats.get_mut(new_id) {
            pat.ty = substitute_ty(&pat.ty, arg_map);
        }
    }

    // Nested callable items cloned into a specialization may capture outer
    // generic parameters in their signatures even when they do not declare
    // generics of their own (for example, lifted lambdas inside generic
    // stdlib helpers). Rewrite those declaration-level types as well.
    for &new_id in cloner.item_map().values() {
        let Some(item) = target.items.get_mut(new_id) else {
            continue;
        };
        let ItemKind::Callable(decl) = &mut item.kind else {
            continue;
        };
        if decl.generics.is_empty() {
            decl.output = substitute_ty(&decl.output, arg_map);
        }
    }
}

/// Substitutes type parameters inside a `GenericArg`.
fn substitute_generic_arg(ga: &GenericArg, arg_map: &FxHashMap<ParamId, GenericArg>) -> GenericArg {
    match ga {
        GenericArg::Ty(ty) => GenericArg::Ty(substitute_ty(ty, arg_map)),
        GenericArg::Functor(fs) => GenericArg::Functor(substitute_functor_set(*fs, arg_map)),
    }
}

/// Rewrites every generic `Var` call site in the target package to point at
/// the monomorphized callable produced by [`create_specializations`].
///
/// # Before
/// ```text
/// Var(Item(generic_callable), [Ty(Int), Functor(Adj)])
/// ```
/// # After
/// ```text
/// Var(Item(monomorphized_callable), [])   // generic args cleared
/// ```
///
/// Residual non-empty generic argument lists on sites whose target has no
/// matching specialization (e.g. intrinsics) are cleared so no `Ty::Param`
/// survives the pass.
///
/// # Mutations
/// - Rewrites `ExprKind::Var` nodes to reference monomorphized items and
///   clears their generic-argument lists.
fn rewrite_call_sites(
    package: &mut Package,
    package_id: PackageId,
    lookup: &FxHashMap<String, ItemId>,
    expr_ids: &[ExprId],
) {
    // Walk scoped expressions and rewrite generic Var references.
    for &expr_id in expr_ids {
        let expr = package.exprs.get(expr_id).expect("expr should exist");
        if let ExprKind::Var(Res::Item(item_id), ref generic_args) = expr.kind {
            if generic_args.is_empty() {
                continue;
            }
            let store_id = StoreItemId::from((item_id.package, item_id.item));
            let key = mono_key(store_id, generic_args);
            if let Some(&new_id) = lookup.get(&key) {
                let expr_mut = package.exprs.get_mut(expr_id).expect("expr should exist");
                expr_mut.kind = ExprKind::Var(Res::Item(new_id), vec![]);
            } else {
                // No specialization found — still clear the generic args since
                // the types have been substituted already (e.g., intrinsics that
                // don't need cloning but whose type params were resolved).

                // Check if this is expected (intrinsic) or a potential bug.
                // Only flag when all generic args are concrete — call sites
                // inside uninstantiated generic bodies still carry Ty::Param
                // references, and those are expected to remain unresolved.
                let all_concrete = is_fully_concrete(generic_args);
                if all_concrete
                    && item_id.package == package_id
                    && let Some(item) = package.items.get(item_id.item)
                    && let ItemKind::Callable(decl) = &item.kind
                {
                    // Only flag if the target callable actually declares
                    // type parameters. Call sites pointing at a specialization
                    // carry an empty generic-arg list; any residual non-empty
                    // list on a non-specialized target (e.g. an intrinsic) is
                    // cleared here.
                    if !decl.generics.is_empty()
                        && !matches!(decl.implementation, CallableImpl::Intrinsic)
                    {
                        panic!(
                            "Non-intrinsic same-package callable has no monomorphized specialization: \
                                     item={item_id:?}, args={generic_args:?}"
                        );
                    }
                }

                let expr_mut = package.exprs.get_mut(expr_id).expect("expr should exist");
                if let ExprKind::Var(_, ref mut args) = expr_mut.kind {
                    args.clear();
                }
            }
        }
    }

    // No separate entry-expression rewrite is needed here. The package entry
    // is stored as an ExprId in `package.exprs`, whether it came from an
    // explicit entry expression or a synthesized `Main()` call.
}
