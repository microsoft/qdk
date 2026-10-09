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
//!   `Ty::Udt`, `ExprKind::Struct`, UDT constructor call or value, UDT-targeted
//!   `UpdateField`/`AssignField`, or `Field::Path` on non-tuple types remains.
//! - Children are erased before a parent copies their kind, independently of
//!   arena allocation order. Constructor and identity-read elimination retain
//!   the child's source span.
//! - **Whole-package erasure across the reachable package closure.** This
//!   mutates every expression and callable signature in the target package
//!   and in packages reached from its entry or additional seeds, not just
//!   reachable callable bodies. Like defunctionalization, it handles paths into library
//!   packages. UDT definitions are resolved from the whole store via the UDT cache.
//! - **Feeds [`crate::exec_graph_rebuild`].** Structurally mutates reachable
//!   callable bodies in place; the pipeline driver unconditionally rebuilds the
//!   exec graph of every reachable spec in every reachable package afterwards,
//!   so this pass no longer tracks or returns which specs it mutated.
//! - **Relies on an acyclic UDT graph.** `resolve_ty` recurses through `Udt`,
//!   `Array`, `Tuple`, and `Arrow` with no visited set, which is sound only
//!   because a user-defined type cannot reference itself. The Q# type checker
//!   enforces this in `qsc_frontend::typeck::check`, which rejects any cyclic
//!   declaration with `Qdk.Qsc.TypeCk.RecursiveUdt` before HIR passes run. The
//!   guarantee covers the package under compilation and extends to the whole
//!   store only where dependency errors are also gated. Note that no form of
//!   indirection exempts a cycle: `A[]` and `A -> Int` are erased structurally
//!   just as a bare `A` is, so a cyclic type has no finite erased form at all —
//!   a visited-set guard would terminate into a state that still violates
//!   `PostUdtErase` rather than into a correct one.
//! - Synthesized expressions use `EMPTY_EXEC_RANGE`;
//!   [`crate::exec_graph_rebuild`] rebuilds exec graphs later.

#[cfg(test)]
mod tests;

#[cfg(test)]
mod semantic_equivalence_tests;

#[cfg(test)]
mod test_cases;

use crate::EMPTY_EXEC_RANGE;
use crate::fir_builder;
use crate::package_assigners::PackageAssigners;
use crate::reachability::{collect_reachable_package_closure, collect_reachable_with_seeds};
use crate::walk_utils::{
    expr_is_safe_to_discard, expressions_in_postorder, for_each_expr,
    for_each_expr_in_callable_impl,
};
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    BlockId, CallableDecl, CallableImpl, CallableKind, ExecGraph, Expr, ExprId, ExprKind, Field,
    FieldAssign, FieldPath, Ident, Item, ItemId, ItemKind, LocalItemId, Mutability, Package,
    PackageId, PackageLookup, PackageStore, PatId, Res, SpecDecl, SpecImpl, StmtId, StoreItemId,
    Visibility,
};
use qsc_fir::ty::{Arrow, FunctorSetValue, Ty};
use std::rc::Rc;

use qsc_fir::fir::PackageSpan;
use rustc_hash::FxHashMap;

/// Maps `StoreItemId` → pure `Ty` for every UDT definition
/// in the store.
type UdtCache = FxHashMap<StoreItemId, Ty>;

/// Test convenience wrapper for entry-rooted UDT erasure without extra seeds.
#[cfg(test)]
pub fn erase_udts(
    store: &mut PackageStore,
    package_id: PackageId,
    assigners: &mut PackageAssigners,
) {
    erase_udts_with_seeds(store, package_id, assigners, &[]);
}

/// Erases UDT types and UDT-shaped expressions in the target package's
/// entry- and seed-reachable package closure, while resolving UDT definitions
/// from the whole store. Specifically, rewrites:
///
/// - Every `Ty::Udt` to its pure tuple or scalar type (via `get_pure_ty()`)
///   on expressions, patterns, blocks, and callable signatures.
/// - `ExprKind::Struct` construction (with or without a copy-update source)
///   into tuple or scalar expressions. Copy sources and field initializers
///   retain their source evaluation order, using temporary bindings when
///   direct tuple construction could duplicate, skip, or reorder evaluation.
/// - UDT constructor calls (`ExprKind::Call` whose callee is an
///   `ItemKind::Ty` item) into the underlying tuple or scalar value.
///   Constructor values that survive in executable expressions reference
///   ordinary identity callables, so later type-item removal cannot invalidate
///   indirect calls. Callee selection and argument evaluation stay unchanged.
/// - `ExprKind::UpdateField` and `ExprKind::AssignField` with `Field::Path`
///   into explicit tuple constructions with field extractions, evaluating the
///   replacement before the record. This differs from struct copy syntax,
///   which evaluates its copy source before its field initializers.
/// - Identity `ExprKind::Field` reads on single-field newtypes into their
///   underlying values, including tuple-valued fields with an empty path.
///
/// See the module-level documentation for the full list of input patterns
/// and their rewrites, including the single-field newtype case below:
///
/// ```text
/// // Before — newtype Wrapped = (Inner : Int); let v = w::Inner;
/// Field(w, Path([]))
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
/// in this pass go through [`collect_reachable_with_seeds`], which asserts
/// `package.entry.is_some()`.
pub fn erase_udts_with_seeds(
    store: &mut PackageStore,
    package_id: PackageId,
    assigners: &mut PackageAssigners,
    seeds: &[StoreItemId],
) {
    // Build a resolution cache from all UDT items across all packages.
    let udt_cache = build_udt_cache(store);
    let reachable = collect_reachable_with_seeds(store, package_id, seeds);

    // Erase UDTs in the target package and in any package that contains an
    // entry- or seed-reachable item. UDT definition lookup still spans the whole
    // store so cross-package references resolve correctly.
    let pkg_ids: Vec<PackageId> = collect_reachable_package_closure(package_id, &reachable)
        .into_iter()
        .collect();

    for pkg_id in pkg_ids {
        let assigner = assigners.get_mut(store, pkg_id);
        erase_udts_in_package(store.get_mut(pkg_id), &udt_cache, assigner);
    }
}

/// Erases UDT types and struct expressions in a single package, rewriting
/// every expression type, pattern type, block type, callable signature,
/// and struct construction in place. Called once per package in the
/// entry- and seed-reachable closure.
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
/// - Allocates field-extraction expressions and ordered operand bindings through
///   `assigner` for struct construction and field-update lowering.
fn erase_udts_in_package(package: &mut Package, udt_cache: &UdtCache, assigner: &mut Assigner) {
    // Parents can copy a child's kind, so every copied child must already be erased.
    let mut expr_ids = expressions_in_postorder(package);
    let mut next = 0;
    while let Some(&expr_id) = expr_ids.get(next) {
        next += 1;
        // Rewrite the expression's type.
        let expr = package.exprs.get(expr_id).expect("expr should exist");
        let new_ty = resolve_ty(udt_cache, &expr.ty);
        let kind = expr.kind.clone();
        let expr_span = expr.span;

        let expr_mut = package.exprs.get_mut(expr_id).expect("expr should exist");
        expr_mut.ty = new_ty;

        // Convert Struct expressions to Tuple expressions.
        if let ExprKind::Struct(_res, copy, fields) = &kind {
            let materialize = !expr_is_safe_to_discard(package, package.id, expr_id)
                || copy.is_some_and(|id| {
                    !matches!(package.get_expr(id).kind, ExprKind::Var(Res::Local(_), _))
                });
            let (statements, copy, fields) = if materialize {
                materialize_struct_operands(package, udt_cache, assigner, *copy, fields)
            } else {
                (Vec::new(), *copy, fields.clone())
            };
            if let Some(copy_id) = copy {
                lower_copy_update_struct(
                    package, assigner, udt_cache, expr_id, copy_id, &fields, expr_span,
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
                        let inner_expr = package.get_expr(values[0]).clone();
                        let expr_mut = package.exprs.get_mut(expr_id).expect("expr should exist");
                        expr_mut.kind = inner_expr.kind;
                        expr_mut.ty = resolve_ty(udt_cache, &inner_expr.ty);
                        expr_mut.span = inner_expr.span;
                    }
                } else {
                    // Multi-field UDT: replace with a tuple of the field
                    // values in declaration order.
                    let expr_mut = package.exprs.get_mut(expr_id).expect("expr should exist");
                    expr_mut.kind = ExprKind::Tuple(values);
                }
            }
            if !statements.is_empty() {
                wrap_operand_evaluation(package, assigner, expr_id, statements);
            }
        }

        // Lower UpdateField and AssignField with Field::Path into tuple
        // constructions.
        lower_field_updates(package, assigner, udt_cache, expr_id, &kind, expr_span);

        if lower_identity_expr(package, udt_cache, expr_id) {
            // Revisit the copied root kind after peeling identity wrappers.
            expr_ids.push(expr_id);
        }
    }

    lower_constructor_values(package, udt_cache, assigner);

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

/// Replaces surviving constructor values, not the orphaned callee nodes left
/// behind by direct-call elimination. Each package reuses one identity callable
/// per package-qualified constructor, including values in currently dead bodies.
fn lower_constructor_values(package: &mut Package, udt_cache: &UdtCache, assigner: &mut Assigner) {
    let mut references = FxHashMap::default();
    let mut collect = |id, expr: &Expr| {
        if let ExprKind::Var(Res::Item(item), _) = expr.kind
            && udt_cache.contains_key(&(item.package, item.item).into())
        {
            references.insert(id, StoreItemId::from((item.package, item.item)));
        }
    };
    for item in package.items.values() {
        if let ItemKind::Callable(decl) = &item.kind {
            for_each_expr_in_callable_impl(package, &decl.implementation, &mut collect);
        }
    }
    if let Some(entry) = package.entry {
        for_each_expr(package, entry, &mut collect);
    }
    let mut references: Vec<_> = references.into_iter().collect();
    references.sort_unstable_by_key(|(id, _)| *id);

    let mut identities = FxHashMap::default();
    for (expr_id, constructor) in references {
        let target = *identities.entry(constructor).or_insert_with(|| {
            let ty = resolve_ty(udt_cache, &udt_cache[&constructor]);
            create_constructor_identity(package, assigner, ty)
        });
        let expr = package
            .exprs
            .get_mut(expr_id)
            .expect("constructor reference exists");
        expr.kind = ExprKind::Var(
            Res::Item(ItemId {
                package: package.id,
                item: target,
            }),
            Vec::new(),
        );
    }
}

/// Creates an internal function that returns its argument unchanged, providing
/// a runtime target for a constructor value after its UDT representation is
/// erased. Unlike the original type item, this callable survives item DCE when
/// referenced by an indirect call.
///
/// `ty` must be the fully erased constructor input/output type. Binding it as
/// one value preserves its entire shape, including Unit, nested tuples, and
/// singleton tuples; no additional tuple wrapper is introduced.
///
/// # Before / After
/// ```text
/// Before: constructor value Data : T -> Data
/// After:  __udt_constructor_N : erased(T) -> erased(T)
///         body(value) { value }
/// ```
/// The caller replaces constructor references and caches the returned local
/// item ID; this helper always creates a fresh callable.
///
/// # Mutations
/// Allocates the callable and its input pattern, local IDs, expression,
/// statement, and block in `package` using its assigner and synthetic span.
/// Execution graphs are left empty for [`crate::exec_graph_rebuild`] to rebuild.
fn create_constructor_identity(
    package: &mut Package,
    assigner: &mut Assigner,
    ty: Ty,
) -> LocalItemId {
    let span = package.synthetic_span();
    let (value, input) = fir_builder::alloc_bind_pat(package, assigner, "value", ty.clone(), span);
    let result = fir_builder::alloc_local_var_expr(package, assigner, value, ty.clone(), span);
    let statement = fir_builder::alloc_expr_stmt(package, assigner, result, span);
    let block = fir_builder::alloc_block(package, assigner, vec![statement], ty.clone(), span);
    let id = assigner.next_item();
    let decl = CallableDecl {
        span,
        kind: CallableKind::Function,
        name: Ident {
            id: assigner.next_local(),
            span,
            name: Rc::from(format!("__udt_constructor_{id}")),
        },
        generics: Vec::new(),
        input,
        output: ty,
        functors: FunctorSetValue::Empty,
        implementation: CallableImpl::Spec(SpecImpl {
            body: SpecDecl {
                span,
                block,
                input: None,
                exec_graph: ExecGraph::default(),
            },
            adj: None,
            ctl: None,
            ctl_adj: None,
        }),
        attrs: Vec::new(),
    };
    package.items.insert(
        id,
        Item {
            id,
            span,
            parent: None,
            doc: Rc::from(""),
            attrs: Vec::new(),
            visibility: Visibility::Internal,
            kind: ItemKind::Callable(Box::new(decl)),
        },
    );
    id
}

/// Stores the copy value first, then every initializer in source order. Once
/// stored, operands can be projected or arranged in declaration order without
/// replaying effects or reading values changed by a later initializer.
fn materialize_struct_operands(
    package: &mut Package,
    udt_cache: &UdtCache,
    assigner: &mut Assigner,
    copy: Option<ExprId>,
    fields: &[FieldAssign],
) -> (Vec<StmtId>, Option<ExprId>, Vec<FieldAssign>) {
    let mut statements = Vec::new();
    let mut bind = |id| bind_erased_operand(package, udt_cache, assigner, id, &mut statements);
    let copy = copy.map(&mut bind);
    let fields = fields
        .iter()
        .map(|field| FieldAssign {
            value: bind(field.value),
            ..field.clone()
        })
        .collect();
    (statements, copy, fields)
}

fn bind_erased_operand(
    package: &mut Package,
    udt_cache: &UdtCache,
    assigner: &mut Assigner,
    id: ExprId,
    statements: &mut Vec<StmtId>,
) -> ExprId {
    let expr = package.get_expr(id);
    let ty = resolve_ty(udt_cache, &expr.ty);
    let span = expr.span;
    let (local, statement) = fir_builder::alloc_local_var(
        package,
        assigner,
        "_.struct_value",
        &ty,
        id,
        Mutability::Immutable,
    );
    statements.push(statement);
    fir_builder::alloc_local_var_expr(package, assigner, local, ty, span)
}

fn wrap_operand_evaluation(
    package: &mut Package,
    assigner: &mut Assigner,
    expr_id: ExprId,
    mut statements: Vec<StmtId>,
) {
    let expr = package.get_expr(expr_id).clone();
    let value = fir_builder::alloc_expr(package, assigner, expr.ty.clone(), expr.kind, expr.span);
    // Tuple decomposition rewrites assignment statements, not tail expressions.
    let statement = if expr.ty == Ty::UNIT {
        fir_builder::alloc_semi_stmt(package, assigner, value, expr.span)
    } else {
        fir_builder::alloc_expr_stmt(package, assigner, value, expr.span)
    };
    statements.push(statement);
    let block = fir_builder::alloc_block(package, assigner, statements, expr.ty, expr.span);
    package
        .exprs
        .get_mut(expr_id)
        .expect("expr should exist")
        .kind = ExprKind::Block(block);
}

/// Eliminates UDT constructors and identity field reads after type erasure.
///
/// Empty field paths select the whole value, including tuple-valued fields of
/// single-field newtypes. Nonempty paths on tuple records still select real
/// fields. Peel nested constructors and identity reads together so arena order
/// cannot leave an already-visited expression holding an unlowered inner kind.
/// Argument promotion relies on these reads becoming direct parameter uses.
/// Returns whether an identity was removed, so other erasure steps can revisit
/// the resulting expression kind.
///
/// # Before
/// ```text
/// Call(Var(Item(UdtConstructor)), arg)
/// Field(record, Path([]))
/// ```
/// # After
/// ```text
/// arg   // or Tuple([arg]) when the erased constructor requires a wrapper
/// record
/// ```
///
/// # Mutations
/// - Rewrites `expr_id`'s `ExprKind`, `Ty`, and source span in place.
fn lower_identity_expr(package: &mut Package, udt_cache: &UdtCache, expr_id: ExprId) -> bool {
    let mut changed = false;
    loop {
        let expr = package.exprs.get(expr_id).expect("expr should exist");
        let source = match expr.kind {
            ExprKind::Call(callee_id, arg_id) => {
                let callee = package.exprs.get(callee_id).expect("callee should exist");
                let ExprKind::Var(Res::Item(item_id), _) = callee.kind else {
                    return changed;
                };
                let Some(pure_ty) = udt_cache.get(&(item_id.package, item_id.item).into()) else {
                    return changed;
                };
                let resolved_pure = resolve_ty(udt_cache, pure_ty);
                let arg = package.exprs.get(arg_id).expect("arg should exist");
                if resolve_ty(udt_cache, &arg.ty) != resolved_pure
                    && matches!(&resolved_pure, Ty::Tuple(_))
                {
                    let expr = package.exprs.get_mut(expr_id).expect("expr should exist");
                    expr.kind = ExprKind::Tuple(vec![arg_id]);
                    expr.ty = resolved_pure;
                    return changed;
                }
                arg_id
            }
            ExprKind::Field(record_id, Field::Path(ref path)) => {
                let record = package.exprs.get(record_id).expect("record should exist");
                if !path.indices.is_empty()
                    && matches!(resolve_ty(udt_cache, &record.ty), Ty::Tuple(_))
                {
                    return changed;
                }
                record_id
            }
            _ => return changed,
        };
        let source = package.exprs.get(source).expect("source should exist");
        let source_kind = source.kind.clone();
        let source_ty = resolve_ty(udt_cache, &source.ty);
        let source_span = source.span;
        let expr = package.exprs.get_mut(expr_id).expect("expr should exist");
        expr.kind = source_kind;
        expr.ty = source_ty;
        expr.span = source_span;
        changed = true;
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
/// - Allocates field-extraction `Expr` nodes through `assigner`.
///
/// The caller stores effectful operands before this layout-only rewrite.
fn lower_copy_update_struct(
    package: &mut Package,
    assigner: &mut Assigner,
    udt_cache: &UdtCache,
    expr_id: ExprId,
    copy_id: ExprId,
    fields: &[FieldAssign],
    span: PackageSpan,
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
                let field_id = alloc_field_expr(package, assigner, copy_id, j, elem_ty, span);
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
/// {
///     let value = new_val;
///     let snapshot = record;
///     Tuple([Field(snapshot, Path([0])), value])
/// }
/// // AssignField stores this tuple back into the original record local.
/// ```
///
/// Both forms evaluate the replacement first, then read the record once.
/// Simple local/item reads and literals need no temporary bindings. All other
/// operands are stored before reconstruction, even when a whole-value update
/// replaces every field: evaluating the discarded record can still have effects
/// or fail. The assignment target remains the original record expression, and
/// the store remains a statement so tuple decomposition can split it later.
///
/// # Mutations
/// - Rewrites `expr_id`'s `ExprKind` in place.
/// - Allocates ordered bindings, field extractions, and update nodes through
///   `assigner`.
fn lower_field_updates(
    package: &mut Package,
    assigner: &mut Assigner,
    udt_cache: &UdtCache,
    expr_id: ExprId,
    kind: &ExprKind,
    span: PackageSpan,
) {
    let (record_id, path, replace_id, assign) = match kind {
        ExprKind::UpdateField(record, Field::Path(path), replace) => {
            (*record, path, *replace, false)
        }
        ExprKind::AssignField(record, Field::Path(path), replace) => {
            (*record, path, *replace, true)
        }
        _ => return,
    };
    let record_ty = resolve_ty(udt_cache, &package.get_expr(record_id).ty);
    let mut statements = Vec::new();
    let simple_operands = [record_id, replace_id].iter().all(|&id| {
        matches!(
            package.get_expr(id).kind,
            ExprKind::Lit(_) | ExprKind::Var(_, _)
        )
    });
    let (record, replace) = if simple_operands {
        (record_id, replace_id)
    } else {
        let replace =
            bind_erased_operand(package, udt_cache, assigner, replace_id, &mut statements);
        let record = bind_erased_operand(package, udt_cache, assigner, record_id, &mut statements);
        (record, replace)
    };
    let lowered = lower_update_field(
        package,
        assigner,
        record,
        &path.indices,
        replace,
        &record_ty,
        span,
    );
    let replacement = if assign {
        let value = fir_builder::alloc_expr(package, assigner, record_ty, lowered, span);
        ExprKind::Assign(record_id, value)
    } else {
        lowered
    };
    package
        .exprs
        .get_mut(expr_id)
        .expect("expr should exist")
        .kind = replacement;
    if !statements.is_empty() {
        wrap_operand_evaluation(package, assigner, expr_id, statements);
    }
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
///
/// This is layout-only lowering: callers preserve operand evaluation before
/// requesting projections that may reuse the record or omit it entirely.
fn lower_update_field(
    package: &mut Package,
    assigner: &mut Assigner,
    record_id: ExprId,
    indices: &[usize],
    replace_id: ExprId,
    record_ty: &Ty,
    span: PackageSpan,
) -> ExprKind {
    match (indices, record_ty) {
        // Single-level path on a tuple: build a new tuple with the
        // replacement at `idx` and field extractions everywhere else.
        (&[idx], Ty::Tuple(elems)) => {
            debug_assert!(
                idx < elems.len(),
                "field path indices are guaranteed valid by frontend and prior-pass type checking"
            );
            build_updated_tuple(package, assigner, record_id, idx, replace_id, elems, span)
        }

        // Multi-level path on a tuple: recursively lower the inner update
        // on the sub-record at index `idx`.
        (&[idx, ref rest @ ..], Ty::Tuple(elems)) => {
            debug_assert!(
                idx < elems.len(),
                "field path indices are guaranteed valid by frontend and prior-pass type checking"
            );
            // Extract the sub-record at position idx.
            let sub_id = alloc_field_expr(package, assigner, record_id, idx, &elems[idx], span);

            // Recursively lower the inner path on the sub-record.
            let inner_kind = lower_update_field(
                package,
                assigner,
                sub_id,
                rest,
                replace_id,
                &elems[idx],
                span,
            );

            // Wrap the recursive result in a new expression.
            let inner_result_id = assigner.next_expr();
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
                assigner,
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
/// - Allocates `Field` `Expr` nodes through `assigner` for non-updated positions.
fn build_updated_tuple(
    package: &mut Package,
    assigner: &mut Assigner,
    record_id: ExprId,
    update_idx: usize,
    replace_id: ExprId,
    elems: &[Ty],
    span: PackageSpan,
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
            let field_id = alloc_field_expr(package, assigner, record_id, j, elem_ty, span);
            field_ids.push(field_id);
        }
    }
    ExprKind::Tuple(field_ids)
}

/// Allocates a new `Expr` with `ExprKind::Field(record_id, Path([index]))`.
///
/// # Mutations
/// - Inserts one `Expr` node through `assigner`.
fn alloc_field_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    record_id: ExprId,
    index: usize,
    ty: &Ty,
    span: PackageSpan,
) -> ExprId {
    let field_id = assigner.next_expr();
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
