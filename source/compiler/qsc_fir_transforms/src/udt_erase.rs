// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! UDT erasure pass.
//!
//! Replaces every `Ty::Udt` in the entry-reachable package closure with its
//! pure tuple or scalar type (via `get_pure_ty()`) and converts
//! `ExprKind::Struct` construction expressions into tuple or scalar
//! expressions. Also eliminates UDT constructor calls (`ExprKind::Call`
//! whose callee is an `ItemKind::Ty` item) and lowers
//! `ExprKind::UpdateField` and `ExprKind::AssignField` with `Field::Path`
//! into explicit tuple constructions with field extractions. Additionally,
//! lowers `ExprKind::Field` read access expressions on scalar-erased
//! single-field newtypes. After this pass, no `Ty::Udt`, `ExprKind::Struct`,
//! UDT constructor call, UDT-targeted `UpdateField`/`AssignField`, or
//! `Field::Path` on non-tuple types remains in the target package or in any
//! package that contains an entry-reachable callable.
//!
//! Establishes [`crate::invariants::InvariantLevel::PostUdtErase`].
//!
//! This must run before partial evaluation and backend code generation, which
//! may inspect reachable cross-package FIR but do not support UDT types or
//! `ExprKind::Struct` in the code they consume.
//!
//! UDT erasure is a standard type-erasure technique common in ML-family
//! compilers and functional languages targeting lower-level IRs.
//!
//! # Input patterns
//!
//! - `ExprKind::Struct(Udt, copy_opt, fields)` — UDT construction (with or
//!   without a copy-update source).
//! - `ExprKind::UpdateField(record, Field::Path, replace)` / `AssignField`
//!   — field-path-based record updates.
//! - Any expression, pattern, block, or callable signature carrying a
//!   `Ty::Udt`.
//!
//! # Rewrites
//!
//! Construction of `newtype Pair = (Int, Int); new Pair { First = 1, Second = 2 }`:
//!
//! ```text
//! // Before
//! Struct(Pair, None, [First = 1, Second = 2])
//!
//! // After
//! Tuple([1, 2])
//! ```
//!
//! Copy-update `new Pair { ...src, First = 9 }`:
//!
//! ```text
//! // Before
//! Struct(Pair, Some(src), [First = 9])
//!
//! // After
//! Tuple([9, Field(src, Path([1]))])
//! ```
//!
//! Update-field `record w/ ::First <- 9`:
//!
//! ```text
//! // Before
//! UpdateField(record, Path([0]), 9)
//!
//! // After
//! Tuple([9, Field(record, Path([1]))])
//! ```
//!
//! # Notes
//!
//! - Scope: the target package and every package reachable from its entry
//!   expression are mutated in place. Cross-package UDT resolution still
//!   uses the whole store via the UDT cache.
//! - Synthesized expressions use `EMPTY_EXEC_RANGE`;
//!   [`crate::exec_graph_rebuild`] rebuilds correct exec graphs at the end
//!   of the pipeline.

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "slow-proptest-tests"))]
mod semantic_equivalence_tests;

use crate::cloner::FirCloner;
use crate::{EMPTY_EXEC_RANGE, reachability::collect_reachable_package_closure_from_entry};
use qsc_data_structures::span::Span;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    BlockId, Expr, ExprId, ExprKind, Field, FieldAssign, FieldPath, ItemKind, LocalItemId, Package,
    PackageId, PackageStore, PatId, Res,
};
use qsc_fir::ty::{Arrow, Ty};

use rustc_hash::FxHashMap;

/// Maps `(PackageId, LocalItemId)` → pure `Ty` for every UDT definition
/// in the store.
type UdtCache = FxHashMap<(PackageId, LocalItemId), Ty>;

/// Erases all `Ty::Udt` types and `ExprKind::Struct` expressions in the
/// target package's reachable package closure, while resolving UDT
/// definitions from the whole store.
///
/// Returns immediately without modification if the target package has no
/// entry expression (nothing is reachable to rewrite).
pub fn erase_udts(store: &mut PackageStore, package_id: PackageId, assigner: &mut Assigner) {
    let package = store.get(package_id);
    if package.entry.is_none() {
        return;
    }

    // Build a resolution cache from all UDT items across all packages.
    let udt_cache = build_udt_cache(store);

    // Erase UDTs in the target package and in any package that contains an
    // entry-reachable callable. UDT definition lookup still spans the whole
    // store so cross-package references resolve correctly.
    let pkg_ids: Vec<PackageId> = collect_reachable_package_closure_from_entry(store, package_id)
        .into_iter()
        .collect();
    for pkg_id in pkg_ids {
        if pkg_id == package_id {
            // Use the threaded assigner for the target package.
            let owned = std::mem::take(assigner);
            let mut cloner = FirCloner::from_assigner(owned);
            erase_udts_in_package(store.get_mut(pkg_id), &udt_cache, &mut cloner);
            *assigner = cloner.into_assigner();
        } else {
            let mut cloner = FirCloner::new(store.get(pkg_id));
            erase_udts_in_package(store.get_mut(pkg_id), &udt_cache, &mut cloner);
        }
    }
}

/// Erases UDT types and struct expressions in a single package, rewriting
/// every expression type, pattern type, block type, callable signature,
/// and struct construction in place. Called once per package in the
/// entry-reachable closure.
fn erase_udts_in_package(package: &mut Package, udt_cache: &UdtCache, cloner: &mut FirCloner) {
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
            }
        }

        // Eliminate UDT constructor calls.
        eliminate_udt_constructor_call(package, udt_cache, expr_id, &kind);

        // Lower UpdateField and AssignField with Field::Path into tuple
        // constructions.
        lower_field_updates(package, cloner, udt_cache, expr_id, &kind, expr_span);

        // Lower Field read expressions on scalar-erased types (Field::Path
        // expressions where the record type is not a tuple).
        lower_scalar_field_read(package, udt_cache, expr_id, &kind);
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
}

/// Eliminates a UDT constructor call if `kind` is `ExprKind::Call` whose
/// callee resolves to an `ItemKind::Ty` item. After type resolution the
/// constructor is an identity/wrapping function:
///
/// - When the argument type already matches the resolved pure type
///   (multi-field or scalar newtypes), replaces the Call with the argument.
/// - When the pure type is `Tuple([T])` but the argument is scalar `T`
///   (trailing-comma newtypes), wraps the argument in a single-element
///   tuple.
fn eliminate_udt_constructor_call(
    package: &mut Package,
    udt_cache: &UdtCache,
    expr_id: ExprId,
    kind: &ExprKind,
) {
    let ExprKind::Call(callee_id, arg_id) = kind else {
        return;
    };
    let callee = package.exprs.get(*callee_id).expect("callee should exist");
    let ExprKind::Var(Res::Item(item_id), _) = &callee.kind else {
        return;
    };
    let Some(pure_ty) = udt_cache.get(&(item_id.package, item_id.item)) else {
        return;
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
    } else {
        // Argument type matches the erased constructor input (multi-field
        // or scalar newtype) — replace the call with the argument.
        let arg = package.exprs.get(*arg_id).expect("arg should exist");
        let arg_kind = arg.kind.clone();
        let arg_ty = arg.ty.clone();
        let expr_mut = package.exprs.get_mut(expr_id).expect("expr should exist");
        expr_mut.kind = arg_kind;
        expr_mut.ty = resolve_ty(udt_cache, &arg_ty);
    }
}

/// Lowers a copy-update struct expression `new Foo { ...copy, X = val }`
/// into a tuple construction, replacing the expression kind in place.
///
/// For multi-field UDTs, builds a tuple where explicitly assigned fields
/// use the provided value and remaining fields are extracted from the copy
/// source. The entire value is replaced by the assignment value in any of
/// the following whole-value-replace cases:
///
/// - the assigned field's path is empty (single-field UDT whose wrapper
///   was erased),
/// - the assigned field's path is exactly `[0]` and the record type
///   resolves to a scalar (the wrapping tuple has already been erased to
///   its sole element),
/// - any non-empty path when the resolved record type is a scalar rather
///   than a tuple (erasure collapsed the surrounding tuple).
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
fn lower_field_updates(
    package: &mut Package,
    cloner: &mut FirCloner,
    udt_cache: &UdtCache,
    expr_id: ExprId,
    kind: &ExprKind,
    span: Span,
) {
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
    }
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
) {
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
        }
    }
}

/// Builds a `(PackageId, LocalItemId) → pure Ty` cache for every UDT
/// definition in the package store so [`resolve_ty`] can perform O(1)
/// cross-package lookups.
fn build_udt_cache(store: &PackageStore) -> UdtCache {
    let mut cache = FxHashMap::default();
    for (pkg_id, package) in store {
        for (item_id, item) in &package.items {
            if let ItemKind::Ty(_, udt) = &item.kind {
                cache.insert((pkg_id, item_id), udt.get_pure_ty());
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
            let key = (item_id.package, item_id.item);
            if let Some(pure) = cache.get(&key) {
                // The pure type itself may contain Ty::Udt (nested UDTs),
                // so recurse.
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
