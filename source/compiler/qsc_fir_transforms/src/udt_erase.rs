// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! UDT erasure pass — runs after defunctionalization, before tuple-compare
//! lowering. A standard ML-family type-erasure technique.
//!
//! Replaces every `Ty::Udt` with its pure tuple/scalar type (`get_pure_ty()`)
//! and rewrites UDT-shaped expressions into plain tuples/scalars. `Struct`
//! construction becomes `Tuple`, UDT constructor calls become the underlying
//! value, and `UpdateField`/`AssignField`/`Field` with `Field::Path` become
//! explicit tuple constructions with field extractions (single-field newtype
//! reads collapse to the inner value). Must run before partial eval and codegen,
//! which inspect reachable cross-package FIR but do not support UDTs or
//! `ExprKind::Struct`.
//!
//! # What to know before diving in
//!
//! - **Establishes [`crate::invariants::InvariantLevel::PostUdtErase`]:** no
//!   `Ty::Udt`, `ExprKind::Struct`, UDT constructor call, UDT-targeted
//!   `UpdateField`/`AssignField`, or `Field::Path` on non-tuple types remains.
//! - **Whole-closure scope — the pipeline outlier.** Unlike every other pass
//!   (which rewrites the entry package only), this mutates the target package
//!   *and every package reachable from its entry*, because entry-reachable
//!   paths cross into library callables. UDT definitions are resolved from the
//!   whole store via the UDT cache.
//! - **Feeds [`crate::exec_graph_rebuild`].** Returns
//!   `Vec<CallableSpecId>` (`structurally_mutated_specs`) — the specs whose
//!   structure changed. The pipeline driver filters these to cross-package
//!   entries and forwards them as the `external_specs` whose exec graphs must
//!   be rebuilt; this pass is their sole producer.
//! - Synthesized expressions use `EMPTY_EXEC_RANGE`;
//!   [`crate::exec_graph_rebuild`] rebuilds exec graphs later.

#[cfg(test)]
mod tests;

#[cfg(test)]
mod semantic_equivalence_tests;

use crate::cloner::FirCloner;
use crate::reachability::{collect_reachable_from_entry, collect_reachable_package_closure};
use crate::{CallableSpecId, CallableSpecKind, EMPTY_EXEC_RANGE};
use qsc_data_structures::span::Span;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    BlockId, Expr, ExprId, ExprKind, Field, FieldAssign, FieldPath, ItemKind, LocalItemId, Package,
    PackageId, PackageStore, PatId, Res, SpecDecl, StoreItemId,
};
use qsc_fir::ty::{Arrow, Ty};

use rustc_hash::{FxHashMap, FxHashSet};

/// Maps `StoreItemId` → pure `Ty` for every UDT definition
/// in the store.
type UdtCache = FxHashMap<StoreItemId, Ty>;

/// Erases UDT types and UDT-shaped expressions in the target package's
/// reachable package closure, while resolving UDT definitions from the
/// whole store. Specifically, rewrites:
///
/// - Every `Ty::Udt` to its pure tuple or scalar type (via `get_pure_ty()`)
///   on expressions, patterns, blocks, and callable signatures.
/// - `ExprKind::Struct` construction (with or without a copy-update source)
///   into tuple or scalar expressions.
/// - UDT constructor calls (`ExprKind::Call` whose callee is an
///   `ItemKind::Ty` item) into the underlying tuple or scalar value.
/// - `ExprKind::UpdateField` and `ExprKind::AssignField` with `Field::Path`
///   into explicit tuple constructions with field extractions.
/// - `ExprKind::Field` read access on scalar-erased single-field newtypes
///   into the underlying scalar expression.
///
/// See the module-level documentation for the full list of input patterns
/// and their rewrites, including the single-field newtype case below:
///
/// ```text
/// // Before — newtype Wrapped = Int; let v = w::Inner;
/// Field(w, Path([0]))
///
/// // After
/// w
/// ```
///
/// # Requires
/// - Package with `package_id` has an entry expression
///
/// # Panics
///
/// Panics if the package has no entry expression. The reachability scans
/// in this pass go through [`collect_reachable_from_entry`], which asserts
/// `package.entry.is_some()`.
///
/// # Returns
/// `Vec<CallableSpecId>` — the `structurally_mutated_specs`: reachable
/// callable specs whose expression structure changed during erasure, deduped
/// across packages and filtered to entry-reachable callables. The pipeline
/// driver [`crate::run_pipeline_to_with_diagnostics`] partitions this set by
/// package and forwards the cross-package members — those whose
/// `callable.package` is not the target `package_id` — to
/// [`crate::exec_graph_rebuild::rebuild_exec_graphs_with_external_specs`] as
/// its `external_specs` argument, so exec graphs in upstream packages are
/// rebuilt against the freshly lowered FIR.
pub fn erase_udts(
    store: &mut PackageStore,
    package_id: PackageId,
    assigner: &mut Assigner,
) -> Vec<CallableSpecId> {
    // Build a resolution cache from all UDT items across all packages.
    let udt_cache = build_udt_cache(store);
    let reachable = collect_reachable_from_entry(store, package_id);

    // Erase UDTs in the target package and in any package that contains an
    // entry-reachable callable. UDT definition lookup still spans the whole
    // store so cross-package references resolve correctly.
    let pkg_ids: Vec<PackageId> = collect_reachable_package_closure(package_id, &reachable)
        .into_iter()
        .collect();

    let mut structurally_mutated_specs = FxHashSet::default();
    for pkg_id in pkg_ids {
        let mutated_exprs = if pkg_id == package_id {
            // Use the threaded assigner for the target package.
            let owned = std::mem::take(assigner);
            let mut cloner = FirCloner::from_assigner(owned);
            let mutated_exprs =
                erase_udts_in_package(store.get_mut(pkg_id), &udt_cache, &mut cloner);
            *assigner = cloner.into_assigner();
            mutated_exprs
        } else {
            let mut cloner = FirCloner::new(store.get(pkg_id));
            erase_udts_in_package(store.get_mut(pkg_id), &udt_cache, &mut cloner)
        };

        let package = store.get(pkg_id);
        structurally_mutated_specs.extend(
            collect_structurally_mutated_specs(pkg_id, package, &mutated_exprs)
                .into_iter()
                .filter(|spec_id| {
                    spec_id.callable.package == package_id || reachable.contains(&spec_id.callable)
                }),
        );
    }

    structurally_mutated_specs.into_iter().collect()
}

/// Erases UDT types and struct expressions in a single package, rewriting
/// every expression type, pattern type, block type, callable signature,
/// and struct construction in place. Called once per package in the
/// entry-reachable closure.
///
/// # Before
/// ```text
/// Expr { ty: Udt(MyStruct), kind: Struct(res, None, fields) }
/// Pat { ty: Udt(MyStruct) }
/// Block { ty: Udt(MyStruct) }
/// ```
/// # After
/// ```text
/// Expr { ty: Tuple([Int, Bool]), kind: Tuple([v0, v1]) }
/// Pat { ty: Tuple([Int, Bool]) }
/// Block { ty: Tuple([Int, Bool]) }
/// ```
///
/// # Mutations
/// - Rewrites `Expr.ty`, `Expr.kind`, `Pat.ty`, `Block.ty`, and callable
///   output types in place.
/// - Allocates field-extraction `Expr` nodes through `cloner` for
///   copy-update and field-update lowering.
fn erase_udts_in_package(
    package: &mut Package,
    udt_cache: &UdtCache,
    cloner: &mut FirCloner,
) -> FxHashSet<ExprId> {
    let mut structurally_mutated_exprs = FxHashSet::default();

    // Rewrite all expression types and Struct expressions.
    let expr_ids: Vec<ExprId> = package.exprs.iter().map(|(id, _)| id).collect();
    for expr_id in expr_ids {
        // Rewrite the expression's type.
        let expr = package.exprs.get(expr_id).expect("expr should exist");
        let new_ty = resolve_ty(udt_cache, &expr.ty);
        let kind = expr.kind.clone();
        let expr_span = expr.span;

        let expr_mut = package.exprs.get_mut(expr_id).expect("expr should exist");
        expr_mut.ty = new_ty;

        // Convert Struct expressions to Tuple expressions.
        if let ExprKind::Struct(_res, copy, fields) = &kind {
            if let Some(copy_id) = copy {
                lower_copy_update_struct(
                    package, cloner, udt_cache, expr_id, *copy_id, fields, expr_span,
                );
                structurally_mutated_exprs.insert(expr_id);
            } else {
                let mut indexed: Vec<(usize, ExprId)> = fields
                    .iter()
                    .filter_map(|fa| {
                        if let Field::Path(FieldPath { indices }) = &fa.field {
                            indices.first().map(|&idx| (idx, fa.value))
                        } else {
                            None
                        }
                    })
                    .collect();
                indexed.sort_by_key(|(idx, _)| *idx);
                let values: Vec<ExprId> = indexed.into_iter().map(|(_, v)| v).collect();

                if values.len() == 1 {
                    // The expression type has already been resolved to the
                    // UDT's pure type. For struct-syntax UDTs the pure type
                    // is Tuple([T]), while for `newtype X = T` it is scalar T.
                    let is_tuple_ty = matches!(
                        &package.exprs.get(expr_id).expect("expr should exist").ty,
                        Ty::Tuple(_)
                    );
                    if is_tuple_ty {
                        // Struct syntax: pure type is Tuple([T]). Keep as
                        // tuple to match the pattern type.
                        let expr_mut = package.exprs.get_mut(expr_id).expect("expr should exist");
                        expr_mut.kind = ExprKind::Tuple(values);
                    } else {
                        // newtype X = T: pure type is scalar T. Unwrap to
                        // the inner expression directly.
                        let inner_expr = package
                            .exprs
                            .get(values[0])
                            .expect("inner expr should exist");
                        let inner_kind = inner_expr.kind.clone();
                        let inner_ty = inner_expr.ty.clone();
                        let expr_mut = package.exprs.get_mut(expr_id).expect("expr should exist");
                        expr_mut.kind = inner_kind;
                        expr_mut.ty = resolve_ty(udt_cache, &inner_ty);
                    }
                } else {
                    // Multi-field UDT: replace with a tuple of the field
                    // values in declaration order.
                    let expr_mut = package.exprs.get_mut(expr_id).expect("expr should exist");
                    expr_mut.kind = ExprKind::Tuple(values);
                }
                structurally_mutated_exprs.insert(expr_id);
            }
        }

        // Eliminate UDT constructor calls.
        if eliminate_udt_constructor_call(package, udt_cache, expr_id, &kind) {
            structurally_mutated_exprs.insert(expr_id);
        }

        // Lower UpdateField and AssignField with Field::Path into tuple
        // constructions.
        if lower_field_updates(package, cloner, udt_cache, expr_id, &kind, expr_span) {
            structurally_mutated_exprs.insert(expr_id);
        }

        // Lower Field read expressions on scalar-erased types (Field::Path
        // expressions where the record type is not a tuple).
        if lower_scalar_field_read(package, udt_cache, expr_id, &kind) {
            structurally_mutated_exprs.insert(expr_id);
        }
    }

    // Rewrite all pattern types.
    let pat_ids: Vec<PatId> = package.pats.iter().map(|(id, _)| id).collect();
    for pat_id in pat_ids {
        let pat = package.pats.get(pat_id).expect("pat should exist");
        let new_ty = resolve_ty(udt_cache, &pat.ty);
        let pat_mut = package.pats.get_mut(pat_id).expect("pat should exist");
        pat_mut.ty = new_ty;
    }

    // Rewrite all block types.
    let block_ids: Vec<BlockId> = package.blocks.iter().map(|(id, _)| id).collect();
    for block_id in block_ids {
        let block = package.blocks.get(block_id).expect("block should exist");
        let new_ty = resolve_ty(udt_cache, &block.ty);
        let block_mut = package
            .blocks
            .get_mut(block_id)
            .expect("block should exist");
        block_mut.ty = new_ty;
    }

    // Rewrite callable signatures (input pattern types are already handled
    // above, but output types are stored separately in CallableDecl).
    let item_ids: Vec<LocalItemId> = package.items.iter().map(|(id, _)| id).collect();
    for item_id in item_ids {
        let item = package.items.get(item_id).expect("item should exist");
        if let ItemKind::Callable(decl) = &item.kind {
            let new_output = resolve_ty(udt_cache, &decl.output);
            if new_output != decl.output {
                let item_mut = package.items.get_mut(item_id).expect("item should exist");
                if let ItemKind::Callable(decl_mut) = &mut item_mut.kind {
                    decl_mut.output = new_output;
                }
            }
        }
    }

    structurally_mutated_exprs
}

/// Finds callable specs whose bodies contain structurally mutated expressions.
fn collect_structurally_mutated_specs(
    package_id: PackageId,
    package: &Package,
    structurally_mutated_exprs: &FxHashSet<ExprId>,
) -> Vec<CallableSpecId> {
    if structurally_mutated_exprs.is_empty() {
        return Vec::new();
    }

    let mut mutated_specs = Vec::new();
    for (item_id, item) in &package.items {
        let ItemKind::Callable(decl) = &item.kind else {
            continue;
        };
        let callable = StoreItemId::from((package_id, item_id));
        match &decl.implementation {
            qsc_fir::fir::CallableImpl::Spec(spec_impl) => {
                push_if_spec_contains_mutated_expr(
                    package,
                    structurally_mutated_exprs,
                    callable,
                    CallableSpecKind::Body,
                    &spec_impl.body,
                    &mut mutated_specs,
                );
                for (kind, spec) in [
                    (CallableSpecKind::Adj, &spec_impl.adj),
                    (CallableSpecKind::Ctl, &spec_impl.ctl),
                    (CallableSpecKind::CtlAdj, &spec_impl.ctl_adj),
                ] {
                    if let Some(spec) = spec {
                        push_if_spec_contains_mutated_expr(
                            package,
                            structurally_mutated_exprs,
                            callable,
                            kind,
                            spec,
                            &mut mutated_specs,
                        );
                    }
                }
            }
            qsc_fir::fir::CallableImpl::Intrinsic
            | qsc_fir::fir::CallableImpl::SimulatableIntrinsic(_) => {}
        }
    }
    mutated_specs
}

/// Adds `spec` to `mutated_specs` when its body contains a tracked mutated
/// expression.
fn push_if_spec_contains_mutated_expr(
    package: &Package,
    structurally_mutated_exprs: &FxHashSet<ExprId>,
    callable: StoreItemId,
    kind: CallableSpecKind,
    spec: &SpecDecl,
    mutated_specs: &mut Vec<CallableSpecId>,
) {
    let mut contains_mutated_expr = false;
    crate::walk_utils::for_each_expr_in_block(package, spec.block, &mut |expr_id, _| {
        contains_mutated_expr |= structurally_mutated_exprs.contains(&expr_id);
    });

    if contains_mutated_expr {
        mutated_specs.push(CallableSpecId::new(callable, kind));
    }
}

/// Eliminates a UDT constructor call if `kind` is `ExprKind::Call` whose
/// callee resolves to an `ItemKind::Ty` item. After type resolution the
/// constructor is an identity/wrapping function.
///
/// # Before
/// ```text
/// Call(Var(Item(UdtConstructor)), arg)   // e.g. MyStruct(42)
/// ```
/// # After
/// ```text
/// arg   // or Tuple([arg]) for trailing-comma newtypes
/// ```
///
/// # Mutations
/// - Rewrites `expr_id`'s `ExprKind` and `Ty` in place.
fn eliminate_udt_constructor_call(
    package: &mut Package,
    udt_cache: &UdtCache,
    expr_id: ExprId,
    kind: &ExprKind,
) -> bool {
    let ExprKind::Call(callee_id, arg_id) = kind else {
        return false;
    };
    let callee = package.exprs.get(*callee_id).expect("callee should exist");
    let ExprKind::Var(Res::Item(item_id), _) = &callee.kind else {
        return false;
    };
    let Some(pure_ty) = udt_cache.get(&(item_id.package, item_id.item).into()) else {
        return false;
    };
    let resolved_pure = resolve_ty(udt_cache, pure_ty);
    let arg = package.exprs.get(*arg_id).expect("arg should exist");
    let arg_ty_resolved = resolve_ty(udt_cache, &arg.ty);

    if arg_ty_resolved != resolved_pure && matches!(&resolved_pure, Ty::Tuple(_)) {
        // Trailing-comma single-field: scalar arg doesn't match
        // Tuple([T]) pure type — wrap in a tuple.
        let expr_mut = package.exprs.get_mut(expr_id).expect("expr should exist");
        expr_mut.kind = ExprKind::Tuple(vec![*arg_id]);
        expr_mut.ty = resolved_pure;
        true
    } else {
        // Argument type matches the erased constructor input (multi-field
        // or scalar newtype) — replace the call with the argument.
        let arg = package.exprs.get(*arg_id).expect("arg should exist");
        let arg_kind = arg.kind.clone();
        let arg_ty = arg.ty.clone();
        let expr_mut = package.exprs.get_mut(expr_id).expect("expr should exist");
        expr_mut.kind = arg_kind;
        expr_mut.ty = resolve_ty(udt_cache, &arg_ty);
        true
    }
}

/// Lowers a copy-update struct expression `new Foo { ...copy, X = val }`
/// into a tuple construction, replacing the expression kind in place.
///
/// # Before
/// ```text
/// Struct(res, Some(copy_id), [FieldAssign(Path([1]), val)])
/// ```
/// # After
/// ```text
/// Tuple([Field(copy, Path([0])), val])   // field 0 extracted, field 1 replaced
/// ```
///
/// # Mutations
/// - Rewrites `expr_id`'s `ExprKind` and `Ty` in place.
/// - Allocates field-extraction `Expr` nodes through `cloner`.
fn lower_copy_update_struct(
    package: &mut Package,
    cloner: &mut FirCloner,
    udt_cache: &UdtCache,
    expr_id: ExprId,
    copy_id: ExprId,
    fields: &[FieldAssign],
    span: Span,
) {
    // Check for a whole-value replacement (single-field UDT where the
    // field path is empty).
    let whole_value_replace = fields.iter().find_map(|fa| {
        if let Field::Path(FieldPath { indices }) = &fa.field
            && indices.is_empty()
        {
            return Some(fa.value);
        }
        None
    });

    if let Some(replacement) = whole_value_replace {
        // Single-field UDT (scalar type): the copy-update replaces the
        // entire value.
        let replace_expr = package
            .exprs
            .get(replacement)
            .expect("replacement should exist");
        let replace_kind = replace_expr.kind.clone();
        let replace_ty = replace_expr.ty.clone();
        let expr_mut = package.exprs.get_mut(expr_id).expect("expr should exist");
        expr_mut.kind = replace_kind;
        expr_mut.ty = resolve_ty(udt_cache, &replace_ty);
        return;
    }

    // Build a map of field index → replacement ExprId.
    let updates: FxHashMap<usize, ExprId> = fields
        .iter()
        .filter_map(|fa| {
            if let Field::Path(FieldPath { indices }) = &fa.field {
                indices.first().map(|&idx| (idx, fa.value))
            } else {
                None
            }
        })
        .collect();

    // Resolve the type of the copy source to determine the tuple
    // structure (may not yet be resolved due to ID ordering).
    let copy_raw_ty = &package
        .exprs
        .get(copy_id)
        .expect("copy source should exist")
        .ty;
    let copy_ty = resolve_ty(udt_cache, copy_raw_ty);

    if let Ty::Tuple(elems) = &copy_ty {
        // Multi-field UDT: build a tuple with replacements at updated
        // indices and field extractions elsewhere.
        let mut field_ids = Vec::with_capacity(elems.len());
        for (j, elem_ty) in elems.iter().enumerate() {
            if let Some(&replacement) = updates.get(&j) {
                field_ids.push(replacement);
            } else {
                let field_id = alloc_field_expr(package, cloner, copy_id, j, elem_ty, span);
                field_ids.push(field_id);
            }
        }
        let expr_mut = package.exprs.get_mut(expr_id).expect("expr should exist");
        expr_mut.kind = ExprKind::Tuple(field_ids);
    } else {
        // Single-field UDTs erase to scalars. Depending on how the field
        // path was lowered upstream, the update may arrive as an empty path,
        // index 0, or a field marker that no longer carries a useful path.
        // Any explicit field assignment on a scalar-erased copy-update must
        // therefore replace the whole value.
        if let Some(&replacement) = updates
            .get(&0)
            .or_else(|| fields.first().map(|fa| &fa.value))
        {
            let replace_expr = package
                .exprs
                .get(replacement)
                .expect("replacement should exist");
            let replace_kind = replace_expr.kind.clone();
            let replace_ty = replace_expr.ty.clone();
            let expr_mut = package.exprs.get_mut(expr_id).expect("expr should exist");
            expr_mut.kind = replace_kind;
            expr_mut.ty = resolve_ty(udt_cache, &replace_ty);
        } else {
            // Defensive fallback: single-field UDT with no overrides after
            // scalar erasure. The frontend should simplify copy-update
            // expressions with zero overrides before they reach this point,
            // making this path unreachable in practice. The fallback
            // correctly propagates the copy source if it is ever hit.
            debug_assert!(
                false,
                "copy-update with no field overrides on a scalar-erased single-field UDT \
                 should be simplified before reaching lower_copy_update_struct"
            );
            let copy_expr = package
                .exprs
                .get(copy_id)
                .expect("copy source should exist");
            let copy_kind = copy_expr.kind.clone();
            let expr_mut = package.exprs.get_mut(expr_id).expect("expr should exist");
            expr_mut.kind = copy_kind;
        }
    }
}

/// Lowers `UpdateField` and `AssignField` with `Field::Path` for a single
/// expression, replacing the expression kind in place.
///
/// # Before
/// ```text
/// UpdateField(record, Field::Path([1]), new_val)   // record w/ field 1 updated
/// AssignField(record, Field::Path([1]), new_val)   // assign field 1
/// ```
/// # After
/// ```text
/// Tuple([Field(record, Path([0])), new_val])       // lowered tuple
/// Assign(record, Tuple([Field(record, Path([0])), new_val]))
/// ```
///
/// # Mutations
/// - Rewrites `expr_id`'s `ExprKind` in place.
/// - Allocates field-extraction and update `Expr` nodes through `cloner`.
fn lower_field_updates(
    package: &mut Package,
    cloner: &mut FirCloner,
    udt_cache: &UdtCache,
    expr_id: ExprId,
    kind: &ExprKind,
    span: Span,
) -> bool {
    let mut structurally_mutated = false;

    // Lower UpdateField(record, Field::Path(path), replace) into a
    // tuple construction that extracts all non-updated fields from the
    // record and inserts the replacement at the correct position.
    if let ExprKind::UpdateField(record_id, Field::Path(path), replace_id) = kind {
        // The record expression may not yet have its type resolved
        // (FIR parent IDs are allocated before children, so record_id
        // can be > expr_id). Resolve the type explicitly.
        let record_raw_ty = &package
            .exprs
            .get(*record_id)
            .expect("record should exist")
            .ty;
        let record_ty = resolve_ty(udt_cache, record_raw_ty);
        let lowered = lower_update_field(
            package,
            cloner,
            *record_id,
            &path.indices,
            *replace_id,
            &record_ty,
            span,
        );
        let expr_mut = package.exprs.get_mut(expr_id).expect("expr should exist");
        expr_mut.kind = lowered;
        structurally_mutated = true;
    }

    // Lower AssignField(record, Field::Path(path), value) into
    // Assign(record, <lowered-update-field-tuple>).
    if let ExprKind::AssignField(record_id, Field::Path(path), value_id) = kind {
        let record_raw_ty = &package
            .exprs
            .get(*record_id)
            .expect("record should exist")
            .ty;
        let record_ty = resolve_ty(udt_cache, record_raw_ty);
        let lowered = lower_update_field(
            package,
            cloner,
            *record_id,
            &path.indices,
            *value_id,
            &record_ty,
            span,
        );
        let update_expr_id = cloner.alloc_expr();
        package.exprs.insert(
            update_expr_id,
            Expr {
                id: update_expr_id,
                span,
                ty: record_ty,
                kind: lowered,
                exec_graph_range: EMPTY_EXEC_RANGE,
            },
        );
        let expr_mut = package.exprs.get_mut(expr_id).expect("expr should exist");
        expr_mut.kind = ExprKind::Assign(*record_id, update_expr_id);
        structurally_mutated = true;
    }

    structurally_mutated
}

/// Lowers `Field(record_id, Field::Path(_))` read expressions on scalar-erased
/// types, replacing the expression kind in place when the record type is not
/// a tuple.
///
/// For scalar-erased single-field newtypes, the record type after erasure is
/// a primitive or other scalar type (e.g., `Prim(Int)`) rather than a tuple.
/// In this case, a field access like `w::x` is semantically an identity access
/// on the scalar value and should be replaced with a direct reference to the
/// record. This maintains the `PostUdtErase` invariant that `Field::Path` only
/// appears on `Ty::Tuple` records.
///
/// For example:
/// - `newtype Wrapper = (x: Int); function Extract(w: Wrapper) : Int { w::x }`
/// - After UDT erasure: `w: Prim(Int)`, but `Field(w, Path([]))` remains
/// - This function replaces `Field(w, Path([]))` with `w` directly.
fn lower_scalar_field_read(
    package: &mut Package,
    udt_cache: &UdtCache,
    expr_id: ExprId,
    kind: &ExprKind,
) -> bool {
    if let ExprKind::Field(record_id, Field::Path(_)) = kind {
        let record_raw_ty = &package
            .exprs
            .get(*record_id)
            .expect("record should exist")
            .ty;
        let record_ty = resolve_ty(udt_cache, record_raw_ty);

        // If the record type is not a tuple, this is a scalar-erased
        // single-field newtype. Replace the field read with the record.
        if !matches!(&record_ty, Ty::Tuple(_)) {
            let record_expr = package.exprs.get(*record_id).expect("record should exist");
            let record_kind = record_expr.kind.clone();
            let record_ty_resolved = resolve_ty(udt_cache, &record_expr.ty);
            let expr_mut = package.exprs.get_mut(expr_id).expect("expr should exist");
            expr_mut.kind = record_kind;
            expr_mut.ty = record_ty_resolved;
            return true;
        }
    }
    false
}

/// Builds a `StoreItemId → pure Ty` cache for every UDT
/// definition in the package store so [`resolve_ty`] can perform O(1)
/// cross-package lookups.
fn build_udt_cache(store: &PackageStore) -> UdtCache {
    let mut cache = FxHashMap::default();
    for (pkg_id, package) in store {
        for (item_id, item) in &package.items {
            if let ItemKind::Ty(_, udt) = &item.kind {
                cache.insert((pkg_id, item_id).into(), udt.get_pure_ty());
            }
        }
    }
    cache
}

/// Lowers `UpdateField(record, Field::Path(indices), replace)` into a tuple
/// construction that extracts all non-updated elements from `record` and
/// inserts `replace` at the position indicated by `indices`.
///
/// For multi-level paths (`[i, j, ...]`), the lowering is recursive: the
/// element at index `i` is itself updated by lowering `[j, ...]` on the
/// extracted sub-record.
///
/// For single-field UDTs (where the post-erasure record type is scalar, not
/// a tuple), the entire record is replaced by `replace`, and the result is
/// simply the replacement expression's kind.
fn lower_update_field(
    package: &mut Package,
    cloner: &mut FirCloner,
    record_id: ExprId,
    indices: &[usize],
    replace_id: ExprId,
    record_ty: &Ty,
    span: Span,
) -> ExprKind {
    match (indices, record_ty) {
        // Single-level path on a tuple: build a new tuple with the
        // replacement at `idx` and field extractions everywhere else.
        (&[idx], Ty::Tuple(elems)) => {
            debug_assert!(
                idx < elems.len(),
                "field path indices are guaranteed valid by frontend and prior-pass type checking"
            );
            build_updated_tuple(package, cloner, record_id, idx, replace_id, elems, span)
        }

        // Multi-level path on a tuple: recursively lower the inner update
        // on the sub-record at index `idx`.
        (&[idx, ref rest @ ..], Ty::Tuple(elems)) => {
            debug_assert!(
                idx < elems.len(),
                "field path indices are guaranteed valid by frontend and prior-pass type checking"
            );
            // Extract the sub-record at position idx.
            let sub_id = alloc_field_expr(package, cloner, record_id, idx, &elems[idx], span);

            // Recursively lower the inner path on the sub-record.
            let inner_kind =
                lower_update_field(package, cloner, sub_id, rest, replace_id, &elems[idx], span);

            // Wrap the recursive result in a new expression.
            let inner_result_id = cloner.alloc_expr();
            package.exprs.insert(
                inner_result_id,
                Expr {
                    id: inner_result_id,
                    span,
                    ty: elems[idx].clone(),
                    kind: inner_kind,
                    exec_graph_range: EMPTY_EXEC_RANGE,
                },
            );

            // Build the outer tuple with the recursively updated element.
            build_updated_tuple(
                package,
                cloner,
                record_id,
                idx,
                inner_result_id,
                elems,
                span,
            )
        }

        // Empty path (single-field UDT whose wrapping was erased) or
        // single-level path on a non-tuple scalar type: the entire record
        // value is replaced.
        ([] | &[_], _) => {
            let replace_expr = package.exprs.get(replace_id).expect("replace should exist");
            replace_expr.kind.clone()
        }

        // Fallback: retained as a guarded branch so invariants violations
        // surface as a well-formed (but unlowered) UpdateField rather
        // than a panic. Under a correct
        // [`crate::invariants::InvariantLevel::PostUdtErase`] the path
        // shape and record type will always match one of the arms above,
        // making this arm unreachable.
        _ => ExprKind::UpdateField(
            record_id,
            Field::Path(FieldPath {
                indices: indices.to_vec(),
            }),
            replace_id,
        ),
    }
}

/// Builds `ExprKind::Tuple(fields)` where `fields[update_idx]` is
/// `replace_id` and every other position is a freshly allocated
/// `ExprKind::Field(record_id, Path([j]))`.
///
/// # Before
/// ```text
/// (no expression)
/// ```
/// # After
/// ```text
/// Tuple([Field(record, Path([0])), replace, Field(record, Path([2]))])
/// ```
///
/// # Mutations
/// - Allocates `Field` `Expr` nodes through `cloner` for non-updated positions.
fn build_updated_tuple(
    package: &mut Package,
    cloner: &mut FirCloner,
    record_id: ExprId,
    update_idx: usize,
    replace_id: ExprId,
    elems: &[Ty],
    span: Span,
) -> ExprKind {
    debug_assert!(
        update_idx < elems.len(),
        "field path indices are guaranteed valid by frontend and prior-pass type checking"
    );
    let mut field_ids = Vec::with_capacity(elems.len());
    for (j, elem_ty) in elems.iter().enumerate() {
        if j == update_idx {
            field_ids.push(replace_id);
        } else {
            let field_id = alloc_field_expr(package, cloner, record_id, j, elem_ty, span);
            field_ids.push(field_id);
        }
    }
    ExprKind::Tuple(field_ids)
}

/// Allocates a new `Expr` with `ExprKind::Field(record_id, Path([index]))`.
///
/// # Mutations
/// - Inserts one `Expr` node through `cloner`.
fn alloc_field_expr(
    package: &mut Package,
    cloner: &mut FirCloner,
    record_id: ExprId,
    index: usize,
    ty: &Ty,
    span: Span,
) -> ExprId {
    let field_id = cloner.alloc_expr();
    package.exprs.insert(
        field_id,
        Expr {
            id: field_id,
            span,
            ty: ty.clone(),
            kind: ExprKind::Field(
                record_id,
                Field::Path(FieldPath {
                    indices: vec![index],
                }),
            ),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    field_id
}

/// Recursively resolves `Ty::Udt` references to their pure types.
///
/// Uses the pre-built [`UdtCache`] for O(1) cross-package lookups and
/// recursively resolves embedded tuple, array, and arrow types so the
/// returned `Ty` is fully UDT-free.
fn resolve_ty(cache: &UdtCache, ty: &Ty) -> Ty {
    match ty {
        Ty::Udt(Res::Item(item_id)) => {
            let key = (item_id.package, item_id.item).into();
            if let Some(pure) = cache.get(&key) {
                // The pure type itself may contain nested Ty::Udt, so recurse.
                resolve_ty(cache, pure)
            } else {
                ty.clone()
            }
        }
        Ty::Array(elem) => {
            let resolved = resolve_ty(cache, elem);
            Ty::Array(Box::new(resolved))
        }
        Ty::Tuple(elems) => {
            let resolved: Vec<Ty> = elems.iter().map(|e| resolve_ty(cache, e)).collect();
            Ty::Tuple(resolved)
        }
        Ty::Arrow(arrow) => {
            let resolved_input = resolve_ty(cache, &arrow.input);
            let resolved_output = resolve_ty(cache, &arrow.output);
            Ty::Arrow(Box::new(Arrow {
                kind: arrow.kind,
                input: Box::new(resolved_input),
                output: Box::new(resolved_output),
                functors: arrow.functors,
            }))
        }
        // Primitives, Param, Infer, Err — no UDT references to resolve.
        _ => ty.clone(),
    }
}
