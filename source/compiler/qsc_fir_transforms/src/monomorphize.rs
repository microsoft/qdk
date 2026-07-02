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
//!   `Ty::Param` and no non-empty `ExprKind::Var` generic-argument lists
//!   remain in reachable code.
//! - **Three phases:** *Discovery* collects concrete generic references;
//!   *Specialization* drives a worklist that clones each body, substitutes
//!   type params, and feeds back transitive generic references it finds;
//!   *Rewrite* redirects call sites across every reachable package and (via
//!   `collect_rewrite_scope_for_package`) walks closure items so generic call
//!   sites in lifted lambdas are not missed.
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
    collect_expr_ids_in_entry_and_local_callables, collect_expr_ids_in_local_callables,
    extend_expr_ids_in_local_callables,
};
use qsc_fir::fir::{
    BlockId, CallableDecl, CallableImpl, ExprId, ExprKind, Ident, Item, ItemId, ItemKind,
    LocalItemId, LocalVarId, Package, PackageId, PackageLookup, PackageStore, PatId, PatKind, Res,
    StmtId, StmtKind, StoreItemId, Visibility,
};
use qsc_fir::ty::{Arrow, FunctorSet, GenericArg, ParamId, Ty};
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

/// Rewrites generic call sites in every reachable package.
///
/// For each package in the entry-reachable closure, collects the rewrite
/// scope (reachable callables, the entry expression for the entry package,
/// the new specializations owned by that package, and their transitive
/// closures) and redirects `ExprKind::Var(Item(generic), [concrete])` sites
/// to the matching specialization.
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
        let new_specs = new_specs_by_pkg.get(&pkg_id).unwrap_or(&empty);
        let expr_ids =
            collect_rewrite_scope_for_package(store, entry_pkg_id, pkg_id, &reachable, new_specs);
        rewrite_call_sites(store.get_mut(pkg_id), pkg_id, &lookup, &expr_ids);
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
        collect_generic_refs_in_expr(package, entry_id, &mut found, &mut seen_keys);
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
            collect_generic_refs_in_callable(pkg, decl, &mut found, &mut seen_keys);
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

/// Walks a callable's body collecting every `(StoreItemId, Vec<GenericArg>)`
/// pair referenced by `ExprKind::Var(Res::Item(..), args)` with non-empty
/// generic arguments, deduplicated via `mono_key` in `seen`.
fn collect_generic_refs_in_callable(
    pkg: &Package,
    decl: &CallableDecl,
    found: &mut Vec<(StoreItemId, Vec<GenericArg>)>,
    seen: &mut FxHashSet<String>,
) {
    crate::walk_utils::for_each_expr_in_callable_impl(
        pkg,
        &decl.implementation,
        &mut |_eid, expr| {
            if let ExprKind::Var(Res::Item(item_id), generic_args) = &expr.kind
                && !generic_args.is_empty()
            {
                let store_id = StoreItemId::from((item_id.package, item_id.item));
                let key = mono_key(store_id, generic_args);
                if seen.insert(key) {
                    found.push((store_id, generic_args.clone()));
                }
            }
        },
    );
}

/// Walks a single expression subtree collecting `(StoreItemId, Vec<GenericArg>)`
/// pairs the same way as [`collect_generic_refs_in_callable`], used for the
/// package entry expression.
fn collect_generic_refs_in_expr(
    pkg: &Package,
    expr_id: ExprId,
    found: &mut Vec<(StoreItemId, Vec<GenericArg>)>,
    seen: &mut FxHashSet<String>,
) {
    crate::walk_utils::for_each_expr(pkg, expr_id, &mut |_eid, expr| {
        if let ExprKind::Var(Res::Item(item_id), generic_args) = &expr.kind
            && !generic_args.is_empty()
        {
            let store_id = StoreItemId::from((item_id.package, item_id.item));
            let key = mono_key(store_id, generic_args);
            if seen.insert(key) {
                found.push((store_id, generic_args.clone()));
            }
        }
    });
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

/// Walks a cloned callable body and collects every
/// `ExprKind::Var(Res::Item(id), args)` where `args` is non-empty and fully
/// concrete (no remaining `Ty::Param` or `FunctorSet::Param`).
fn scan_for_concrete_generic_refs(
    pkg: &Package,
    decl: &CallableDecl,
) -> Vec<(StoreItemId, Vec<GenericArg>)> {
    let mut found = Vec::new();
    let mut seen = FxHashSet::default();
    collect_generic_refs_in_callable(pkg, decl, &mut found, &mut seen);
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

#[allow(clippy::too_many_lines)]
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

    // Scan the newly created callable for additional concrete generic
    // references that need their own specializations. Skip references to
    // items in the owning package that are already non-generic (e.g.,
    // self-references from recursive callables that were remapped by
    // set_self_item_remap).
    let mut refs_out = Vec::new();
    let created_item = owning_pkg.items.get(new_local_id).expect("just inserted");
    if let ItemKind::Callable(created_decl) = &created_item.kind {
        let new_refs = scan_for_concrete_generic_refs(owning_pkg, created_decl);
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

    (new_item_id, refs_out)
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
        ExprKind::Parallel(limit, body) => {
            if let Some(l) = limit {
                extract_expr(source, *l, target);
            }
            extract_expr(source, *body, target);
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
