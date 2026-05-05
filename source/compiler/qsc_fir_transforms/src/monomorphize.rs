// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Monomorphization pass.
//!
//! Eliminates all generic callable references in entry-reachable code by
//! creating concrete specializations for each unique `(callable, generic_args)`
//! pair and rewriting call sites to use those specializations.
//!
//! Establishes [`crate::invariants::InvariantLevel::PostMono`]: no `Ty::Param`
//! remains in reachable code and every `ExprKind::Var` node carries an empty
//! generic-argument list.
//!
//! The algorithm operates in three phases. Discovery walks all entry-reachable
//! code collecting every concrete generic reference. Specialization processes
//! these references via a worklist: for each `(callable, args)` pair it clones
//! the callable body, substitutes type parameters with concrete types, and
//! scans the result for transitive generic references that are fed back into
//! the worklist. Rewrite then redirects all call sites to the newly created
//! specialized callables.
//!
//! # Input patterns
//!
//! - `ExprKind::Var(Res::Item(id), [GenericArg::Ty(Int)])` — a generic call
//!   site whose arguments are fully concrete.
//! - `CallableDecl` with non-empty `generics` — a generic callable that will
//!   be cloned once per distinct concrete instantiation.
//!
//! # Rewrites
//!
//! Given `function Identity<'T>(x : 'T) : 'T { x }` invoked as `Identity(42)`:
//!
//! ```text
//! // Before
//! Call(Var(Identity, [Ty(Int)]), 42)
//!
//! // After
//! Call(Var(Identity<Int>, []), 42)
//! ```
//!
//! A new `Identity<Int>` callable is inserted into the target package with
//! all `Ty::Param` nodes substituted for `Int`, and the call site loses its
//! generic-argument list.
//!
//! # Notes
//!
//! - Identity instantiations (`[Param(0), Param(1), ...]`) are skipped; they
//!   would produce a duplicate identical to the original generic callable.
//! - Intrinsics whose call sites use concrete generic arguments have their
//!   argument lists cleared in place (no new callable is synthesized).
//! - Cross-package references are cloned into the target package so the
//!   specialized bodies are self-contained.

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "slow-proptest-tests"))]
mod semantic_equivalence_tests;

use crate::cloner::FirCloner;
use crate::reachability::collect_reachable_from_entry;
use qsc_fir::assigner::Assigner;
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
/// placed in the target package.
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
/// Returns immediately without modification if the package has no entry
/// expression.
pub fn monomorphize(store: &mut PackageStore, package_id: PackageId, assigner: &mut Assigner) {
    let package = store.get(package_id);
    if package.entry.is_none() {
        return;
    }

    // Discover all unique (callable, generic_args) pairs in
    // entry-reachable code.
    let instantiations = discover_instantiations(store, package_id);
    if instantiations.is_empty() {
        return;
    }

    // Take ownership of the assigner for the duration of specialization
    // and restore it afterward with advanced counters.
    let owned_assigner = std::mem::take(assigner);

    // Create specialized (monomorphized) callables.
    let (specializations, returned_assigner) =
        create_specializations(store, package_id, instantiations, owned_assigner);
    *assigner = returned_assigner;

    // Rewrite call sites to reference the specialized callables.
    rewrite_call_sites(store.get_mut(package_id), package_id, &specializations);
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
/// `AdjCtl`) instead of the user-facing display forms. The intrinsic callable
/// (`CallableImpl::Intrinsic`) `Length` is exempt because downstream passes
/// match on that name literally.
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

#[allow(clippy::too_many_lines)]
/// Drives the worklist that clones each requested `(callable, args)` pair
/// into the target package, substitutes type parameters, and scans the
/// cloned bodies for additional transitively-referenced generic sites.
///
/// Returns the inserted specializations plus the assigner so its counter
/// can be threaded back into the pipeline.
fn create_specializations(
    store: &mut PackageStore,
    target_pkg_id: PackageId,
    instantiations: Vec<(StoreItemId, Vec<GenericArg>)>,
    assigner: Assigner,
) -> (Vec<Specialization>, Assigner) {
    let mut specializations = Vec::new();

    // Pre-populate seen keys from initial discovery.
    let mut seen_keys: FxHashSet<String> = instantiations
        .iter()
        .map(|(source, args)| mono_key(*source, args))
        .collect();
    let mut worklist: VecDeque<(StoreItemId, Vec<GenericArg>)> = instantiations.into();

    // Temporarily take the target package out of the store so we can hold
    // `&source_pkg` (for cross-package) and `&mut target_pkg` simultaneously.
    let empty_pkg = empty_package();
    let mut target_pkg = std::mem::replace(store.get_mut(target_pkg_id), empty_pkg);

    let mut cloner = FirCloner::from_assigner(assigner);

    while let Some((source_id, args)) = worklist.pop_front() {
        // Skip identity instantiations — cloning with these produces a
        // useless duplicate identical to the original generic callable.
        if is_identity_instantiation(&args) {
            continue;
        }

        // Extract needed data from the source package (read-only).
        let (body_pkg, decl_snapshot) = {
            let source_pkg: &Package = if source_id.package == target_pkg_id {
                &target_pkg
            } else {
                store.get(source_id.package)
            };
            let source_item = source_pkg.get_item(source_id.item);
            let source_decl = match &source_item.kind {
                ItemKind::Callable(decl) => decl.as_ref(),
                _ => continue,
            };
            let body_pkg = extract_callable_body(source_pkg, source_decl);
            let decl_snapshot = source_decl.clone();
            (body_pkg, decl_snapshot)
        }; // source_pkg borrow released

        // Clone body into target, substitute types, insert (mutate).
        let new_local_id = cloner.alloc_item();
        let new_item_id = ItemId {
            package: target_pkg_id,
            item: new_local_id,
        };
        let old_item_id = ItemId {
            package: source_id.package,
            item: source_id.item,
        };

        // Reserve the item slot so that clone_nested_item (called during
        // clone_callable_impl for StmtKind::Item / ExprKind::Closure) does
        // not allocate the same LocalItemId for a nested item.
        target_pkg.items.insert(
            new_local_id,
            Item {
                id: new_local_id,
                span: decl_snapshot.span,
                parent: None,
                doc: Rc::from(""),
                attrs: vec![],
                visibility: Visibility::Public,
                kind: ItemKind::Namespace(
                    Ident {
                        id: LocalVarId::default(),
                        span: decl_snapshot.name.span,
                        name: Rc::from(""),
                    },
                    vec![],
                ),
            },
        );

        cloner.reset_maps();
        cloner.set_self_item_remap(old_item_id, new_item_id);

        // Clone input BEFORE impl so that `local_map` contains input
        // parameter mappings when the callable body is walked.
        let new_input = cloner.clone_input_pat(&body_pkg, decl_snapshot.input, &mut target_pkg);
        let new_impl =
            cloner.clone_callable_impl(&body_pkg, &decl_snapshot.implementation, &mut target_pkg);
        let new_node_id = cloner.next_node();

        // Substitute Ty::Param / FunctorSet::Param in all cloned nodes.
        let arg_map = build_arg_map(&args);
        substitute_types_in_cloned_nodes(&mut target_pkg, &cloner, &arg_map);

        let output = substitute_ty(&decl_snapshot.output, &arg_map);

        let spec_name = mono_name(&decl_snapshot, &args);
        let spec_decl = CallableDecl {
            id: new_node_id,
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
        target_pkg.items.insert(new_local_id, new_item);

        // Scan the newly created callable for additional concrete
        // generic references that need their own specializations. Skip
        // references to items in the target package that are already
        // non-generic (e.g., self-references from recursive callables that
        // were remapped by set_self_item_remap).
        let created_item = target_pkg.items.get(new_local_id).expect("just inserted");
        if let ItemKind::Callable(created_decl) = &created_item.kind {
            let new_refs = scan_for_concrete_generic_refs(&target_pkg, created_decl);
            for (ref_id, ref_args) in new_refs {
                if ref_id.package == target_pkg_id
                    && let Some(ref_item) = target_pkg.items.get(ref_id.item)
                    && let ItemKind::Callable(ref_decl) = &ref_item.kind
                    && ref_decl.generics.is_empty()
                {
                    continue;
                }
                let key = mono_key(ref_id, &ref_args);
                if seen_keys.insert(key) {
                    worklist.push_back((ref_id, ref_args));
                }
            }
        }

        specializations.push(Specialization {
            source: source_id,
            args,
            new_item_id,
        });
    }

    // Put the target package back.
    *store.get_mut(target_pkg_id) = target_pkg;

    (specializations, cloner.into_assigner())
}

/// Constructs an empty `Package` used as a scratch container for body
/// extraction and for temporarily swapping out the target package during
/// specialization.
fn empty_package() -> Package {
    Package::default()
}

/// Builds a standalone `Package` holding all nodes transitively referenced
/// by a callable's body so that [`FirCloner`] can read from it without
/// holding a reference to the original source package.
fn extract_callable_body(source_pkg: &Package, decl: &CallableDecl) -> Package {
    let mut body_pkg = empty_package();

    // Input pattern.
    extract_pat(source_pkg, decl.input, &mut body_pkg);

    match &decl.implementation {
        CallableImpl::Intrinsic => {}
        CallableImpl::Spec(spec_impl) => {
            extract_spec_decl_body(source_pkg, &spec_impl.body, &mut body_pkg);
            for spec in [&spec_impl.adj, &spec_impl.ctl, &spec_impl.ctl_adj]
                .into_iter()
                .flatten()
            {
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
                for spec in [&spec_impl.adj, &spec_impl.ctl, &spec_impl.ctl_adj]
                    .into_iter()
                    .flatten()
                {
                    extract_spec_decl_body(source, spec, target);
                }
            }
            CallableImpl::SimulatableIntrinsic(spec) => {
                extract_spec_decl_body(source, spec, target);
            }
        }
    }
}

/// Recursively copies a pattern and its sub-patterns (for tuple patterns).
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
                // Do NOT clear here — rewrite_call_sites needs the
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

/// Substitutes type parameters inside a `GenericArg` (delegating to
/// [`substitute_ty`] or [`substitute_functor_set`]).
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
    specializations: &[Specialization],
) {
    // Build a lookup from (source key) → new ItemId.
    let lookup: FxHashMap<String, ItemId> = specializations
        .iter()
        .map(|s| (mono_key(s.source, &s.args), s.new_item_id))
        .collect();

    // Walk all expressions in the package and rewrite generic Var references.
    let expr_ids: Vec<ExprId> = package.exprs.iter().map(|(id, _)| id).collect();
    for expr_id in expr_ids {
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
