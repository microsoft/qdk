// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Shared FIR node allocation helpers.
//!
//! Every transform pass that synthesizes new FIR nodes must:
//! - Allocate a fresh ID from the pipeline-global [`Assigner`].
//! - Insert the node into the package's arena.
//! - Attach [`EMPTY_EXEC_RANGE`] for `Expr` and
//!   `Stmt` nodes so the final [`exec_graph_rebuild`](crate::exec_graph_rebuild)
//!   pass can replace them with correct ranges.
//!
//! This module provides composable helpers that encapsulate this pattern,
//! reducing boilerplate across passes and centralizing the
//! `EMPTY_EXEC_RANGE` convention.
//!
//! # Why use this builder
//!
//! Every helper is `pub(crate)`, keeping the `EMPTY_EXEC_RANGE` contract a
//! transform-pass internal detail. Synthesizing an `Expr` or `Stmt` outside
//! these helpers silently misses the
//!   [`EMPTY_EXEC_RANGE`] sentinel that
//! [`exec_graph_rebuild`](crate::exec_graph_rebuild) keys off to recompute
//! ranges, producing a stale execution graph with no compile-time error. New
//! passes should route every `Expr`/`Stmt` allocation through the helpers
//! below.

#[cfg(test)]
mod tests;

use crate::EMPTY_EXEC_RANGE;
use qsc_data_structures::functors::FunctorApp;
use qsc_data_structures::span::Span;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    BinOp, Block, BlockId, CallableDecl, Expr, ExprId, ExprKind, Field, FieldPath, Functor, Ident,
    ItemId, ItemKind, Lit, LocalItemId, LocalVarId, Mutability, Package, PackageId, PackageLookup,
    Pat, PatId, PatKind, Res, SpecDecl, SpecImpl, Stmt, StmtId, StmtKind, StoreItemId, UnOp,
};
use rustc_hash::FxHashSet;

use qsc_fir::ty::{Arrow, Prim, Ty};
use std::rc::Rc;

/// Allocates an `Expr` with the given kind and inserts it into the package.
pub(crate) fn alloc_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    ty: Ty,
    kind: ExprKind,
    span: Span,
) -> ExprId {
    let id = assigner.next_expr();
    package.exprs.insert(
        id,
        Expr {
            id,
            span,
            ty,
            kind,
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    id
}

/// Allocates a `Var(Res::Local(var_id))` expression.
pub(crate) fn alloc_local_var_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    var_id: LocalVarId,
    ty: Ty,
    span: Span,
) -> ExprId {
    alloc_expr(
        package,
        assigner,
        ty,
        ExprKind::Var(Res::Local(var_id), Vec::new()),
        span,
    )
}

/// Allocates a `Field(record, Path([index]))` expression.
pub(crate) fn alloc_field_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    record_id: ExprId,
    index: usize,
    ty: Ty,
    span: Span,
) -> ExprId {
    alloc_expr(
        package,
        assigner,
        ty,
        ExprKind::Field(
            record_id,
            Field::Path(FieldPath {
                indices: vec![index],
            }),
        ),
        span,
    )
}

/// Allocates a `Field(record, Path(indices))` expression whose projection
/// path may descend multiple tuple levels in one node.
///
/// Companion to [`alloc_field_expr`], which projects a single level.
pub(crate) fn alloc_field_path_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    record_id: ExprId,
    indices: Vec<usize>,
    ty: Ty,
    span: Span,
) -> ExprId {
    alloc_expr(
        package,
        assigner,
        ty,
        ExprKind::Field(record_id, Field::Path(FieldPath { indices })),
        span,
    )
}

/// Allocates a `Var(Res::Item(item_id))` expression referencing a global item.
pub(crate) fn alloc_item_var_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    item_id: ItemId,
    ty: Ty,
    span: Span,
) -> ExprId {
    alloc_expr(
        package,
        assigner,
        ty,
        ExprKind::Var(Res::Item(item_id), Vec::new()),
        span,
    )
}

/// Allocates a `Call(callee, args)` expression.
pub(crate) fn alloc_call_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    callee_id: ExprId,
    args_id: ExprId,
    ty: Ty,
    span: Span,
) -> ExprId {
    alloc_expr(
        package,
        assigner,
        ty,
        ExprKind::Call(callee_id, args_id),
        span,
    )
}

/// Allocates an integer literal expression with `Int` type.
pub(crate) fn alloc_int_lit(
    package: &mut Package,
    assigner: &mut Assigner,
    value: i64,
    span: Span,
) -> ExprId {
    alloc_expr(
        package,
        assigner,
        Ty::Prim(Prim::Int),
        ExprKind::Lit(Lit::Int(value)),
        span,
    )
}

/// Strips a single controlled-functor input layer from an arrow type,
/// returning the inner, less-controlled arrow type.
///
/// `Controlled` turns an operation `I => O` into `(Qubit[], I) => O`, wrapping
/// the input in a `(Qubit[], _)` tuple. This peels one such layer by replacing
/// the arrow input with the second element of that tuple.
///
/// # Panics
///
/// Panics if `ty` is not a controlled arrow — a `Ty::Arrow` whose input is a
/// tuple of at least two elements `(Qubit[], _)`. This helper is only ever
/// asked to peel a control layer that the caller's `FunctorApp` already claims
/// exists, so any other shape is an internal compiler bug (a
/// `functor.controlled` count that disagrees with the wrapped type) rather than
/// recoverable input.
fn strip_controlled_input_layer(ty: &Ty) -> Ty {
    let Ty::Arrow(arrow) = ty else {
        panic!("expected a controlled arrow type to strip a control layer from, found {ty:?}");
    };
    let Ty::Tuple(items) = arrow.input.as_ref() else {
        panic!(
            "expected a controlled arrow input tuple `(Qubit[], _)`, found input {:?}",
            arrow.input
        );
    };
    assert!(
        items.len() >= 2,
        "expected a controlled arrow input tuple `(Qubit[], _)` with at least two elements, found {items:?}"
    );
    Ty::Arrow(Box::new(Arrow {
        kind: arrow.kind,
        input: Box::new(items[1].clone()),
        output: arrow.output.clone(),
        functors: arrow.functors,
    }))
}

/// Computes the arrow type at each controlled depth of a functor-wrapper
/// chain, from the outermost node down to the base.
///
/// The returned vector has `controlled + 1` entries: index `0` is `outer_ty`
/// (the fully controlled, outermost node), and each subsequent entry strips one
/// `(Qubit[], _)` input layer, so the last entry is the un-controlled base
/// type. Adjoint is intentionally not modeled here because it preserves the
/// arrow type.
fn controlled_layer_types(outer_ty: &Ty, controlled: u8) -> Vec<Ty> {
    let mut layer_tys = Vec::with_capacity(usize::from(controlled) + 1);
    layer_tys.push(outer_ty.clone());
    for _ in 0..controlled {
        let inner = strip_controlled_input_layer(layer_tys.last().expect("seeded with outer_ty"));
        layer_tys.push(inner);
    }
    layer_tys
}

/// Wraps `base_id` in a chain of functor applications (`Adj` then `controlled`
/// layers of `Ctl`) as described by `functor`, allocating one `UnOp` `Expr`
/// per layer. Returns the id of the outermost expression, which equals
/// `base_id` when `functor` requests no functors.
///
/// `ty` is the arrow type of the outermost (fully functor-wrapped) expression.
/// Each `Controlled` layer wraps the callable's input in another `(Qubit[], _)`
/// tuple, so the layers do not share one type: the outermost `Ctl` node carries
/// `ty`, every inner node strips one control layer, and the base node plus any
/// `Adj` node carry the un-controlled base type `base_id` is assumed to already
/// carry that base type.
pub(crate) fn wrap_in_functors(
    package: &mut Package,
    assigner: &mut Assigner,
    base_id: ExprId,
    functor: FunctorApp,
    ty: &Ty,
    span: Span,
) -> ExprId {
    // `layer_tys[0]` is the outermost type; `layer_tys[controlled]` is the base.
    let layer_tys = controlled_layer_types(ty, functor.controlled);
    let controlled = usize::from(functor.controlled);

    let mut current_id = base_id;
    if functor.adjoint {
        // Adjoint preserves the arrow type, so this node shares the base type.
        current_id = alloc_expr(
            package,
            assigner,
            layer_tys[controlled].clone(),
            ExprKind::UnOp(UnOp::Functor(Functor::Adj), current_id),
            span,
        );
    }
    for depth in 1..=controlled {
        // The node at control depth `depth` (counted up from the base) carries
        // the type with `depth` control layers applied.
        current_id = alloc_expr(
            package,
            assigner,
            layer_tys[controlled - depth].clone(),
            ExprKind::UnOp(UnOp::Functor(Functor::Ctl), current_id),
            span,
        );
    }
    current_id
}

/// Allocates a base expression of `base_kind` and wraps it in the functor
/// chain described by `functor` (see [`wrap_in_functors`]). Returns the id of
/// the outermost expression.
///
/// `ty` is the arrow type of the outermost (fully functor-wrapped) expression.
/// The base node is allocated with the un-controlled base type derived by
/// stripping `functor.controlled` control layers from `ty`; `wrap_in_functors`
/// then re-adds one `(Qubit[], _)` input layer per `Ctl` node back up to `ty`.
pub(crate) fn alloc_functor_wrapped_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    base_kind: ExprKind,
    functor: FunctorApp,
    ty: &Ty,
    span: Span,
) -> ExprId {
    let mut base_ty = ty.clone();
    for _ in 0..functor.controlled {
        base_ty = strip_controlled_input_layer(&base_ty);
    }
    let base_id = alloc_expr(package, assigner, base_ty, base_kind, span);
    wrap_in_functors(package, assigner, base_id, functor, ty, span)
}

/// Allocates a `BinOp(op, lhs, rhs)` expression.
pub(crate) fn alloc_bin_op_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    op: BinOp,
    lhs: ExprId,
    rhs: ExprId,
    ty: Ty,
    span: Span,
) -> ExprId {
    alloc_expr(package, assigner, ty, ExprKind::BinOp(op, lhs, rhs), span)
}

/// Allocates a `UnOp(NotL, operand)` expression with `Bool` type.
pub(crate) fn alloc_not_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    operand: ExprId,
    span: Span,
) -> ExprId {
    alloc_expr(
        package,
        assigner,
        Ty::Prim(Prim::Bool),
        ExprKind::UnOp(UnOp::NotL, operand),
        span,
    )
}

/// Allocates an `If(cond, then, else)` expression.
pub(crate) fn alloc_if_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    cond: ExprId,
    then_expr: ExprId,
    else_expr: Option<ExprId>,
    ty: Ty,
    span: Span,
) -> ExprId {
    alloc_expr(
        package,
        assigner,
        ty,
        ExprKind::If(cond, then_expr, else_expr),
        span,
    )
}

/// Allocates a `Block(block_id)` expression.
pub(crate) fn alloc_block_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    block_id: BlockId,
    ty: Ty,
    span: Span,
) -> ExprId {
    alloc_expr(package, assigner, ty, ExprKind::Block(block_id), span)
}

/// Allocates an `Assign(lhs, rhs)` expression with Unit type.
pub(crate) fn alloc_assign_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    lhs: ExprId,
    rhs: ExprId,
    span: Span,
) -> ExprId {
    alloc_expr(
        package,
        assigner,
        Ty::UNIT,
        ExprKind::Assign(lhs, rhs),
        span,
    )
}

/// Allocates a boolean literal expression.
pub(crate) fn alloc_bool_lit(
    package: &mut Package,
    assigner: &mut Assigner,
    value: bool,
    span: Span,
) -> ExprId {
    alloc_expr(
        package,
        assigner,
        Ty::Prim(Prim::Bool),
        ExprKind::Lit(qsc_fir::fir::Lit::Bool(value)),
        span,
    )
}

/// Allocates a Unit `()` expression.
pub(crate) fn alloc_unit_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    span: Span,
) -> ExprId {
    alloc_expr(
        package,
        assigner,
        Ty::UNIT,
        ExprKind::Tuple(Vec::new()),
        span,
    )
}

/// Allocates a `Tuple(exprs)` expression.
pub(crate) fn alloc_tuple_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    exprs: Vec<ExprId>,
    ty: Ty,
    span: Span,
) -> ExprId {
    alloc_expr(package, assigner, ty, ExprKind::Tuple(exprs), span)
}

/// Allocates a `Stmt` with the given kind and inserts it into the package.
pub(crate) fn alloc_stmt(
    package: &mut Package,
    assigner: &mut Assigner,
    kind: StmtKind,
    span: Span,
) -> StmtId {
    let id = assigner.next_stmt();
    package.stmts.insert(
        id,
        Stmt {
            id,
            span,
            kind,
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    id
}

/// Allocates an `Expr` statement (trailing expression, no semicolon).
pub(crate) fn alloc_expr_stmt(
    package: &mut Package,
    assigner: &mut Assigner,
    expr_id: ExprId,
    span: Span,
) -> StmtId {
    alloc_stmt(package, assigner, StmtKind::Expr(expr_id), span)
}

/// Allocates a `Semi` statement (expression with trailing semicolon).
pub(crate) fn alloc_semi_stmt(
    package: &mut Package,
    assigner: &mut Assigner,
    expr_id: ExprId,
    span: Span,
) -> StmtId {
    alloc_stmt(package, assigner, StmtKind::Semi(expr_id), span)
}

/// Allocates a `Local` statement (variable declaration).
pub(crate) fn alloc_local_stmt(
    package: &mut Package,
    assigner: &mut Assigner,
    mutability: Mutability,
    pat_id: PatId,
    init_expr: ExprId,
    span: Span,
) -> StmtId {
    alloc_stmt(
        package,
        assigner,
        StmtKind::Local(mutability, pat_id, init_expr),
        span,
    )
}

/// Allocates a `Block` and inserts it into the package.
pub(crate) fn alloc_block(
    package: &mut Package,
    assigner: &mut Assigner,
    stmts: Vec<StmtId>,
    ty: Ty,
    span: Span,
) -> BlockId {
    let id = assigner.next_block();
    package.blocks.insert(
        id,
        Block {
            id,
            span,
            ty,
            stmts,
        },
    );
    id
}

/// Allocates a `Pat` with `PatKind::Bind` and inserts it into the package.
pub(crate) fn alloc_bind_pat(
    package: &mut Package,
    assigner: &mut Assigner,
    name: &str,
    ty: Ty,
    span: Span,
) -> (LocalVarId, PatId) {
    let local_id = assigner.next_local();
    let pat_id = assigner.next_pat();
    package.pats.insert(
        pat_id,
        Pat {
            id: pat_id,
            span,
            ty,
            kind: PatKind::Bind(Ident {
                id: local_id,
                span,
                name: Rc::from(name),
            }),
        },
    );
    (local_id, pat_id)
}

/// Allocates a `Pat` with `PatKind::Discard` and inserts it into the package.
pub(crate) fn alloc_discard_pat(
    package: &mut Package,
    assigner: &mut Assigner,
    ty: Ty,
    span: Span,
) -> PatId {
    let pat_id = assigner.next_pat();
    package.pats.insert(
        pat_id,
        Pat {
            id: pat_id,
            span,
            ty,
            kind: PatKind::Discard,
        },
    );
    pat_id
}

/// Creates a local variable declaration and returns its `(LocalVarId, StmtId)`.
///
/// Combines [`alloc_bind_pat`] + [`alloc_local_stmt`].
pub(crate) fn alloc_local_var(
    package: &mut Package,
    assigner: &mut Assigner,
    name: &str,
    ty: &Ty,
    init_expr: ExprId,
    mutability: Mutability,
) -> (LocalVarId, StmtId) {
    let (local_id, pat_id) = alloc_bind_pat(package, assigner, name, ty.clone(), Span::default());
    let stmt_id = alloc_local_stmt(
        package,
        assigner,
        mutability,
        pat_id,
        init_expr,
        Span::default(),
    );
    (local_id, stmt_id)
}

/// Decomposes a `PatKind::Bind` pattern into a `PatKind::Tuple` of per-element
/// bindings.
///
/// Allocates `n` new `LocalVarId`/`PatId` pairs (where `n = elem_types.len()`),
/// each named `{name}.{i}`, and rewrites the original pattern to
/// `PatKind::Tuple(new_pat_ids)`. The `.` separator is a sentinel that is never
/// a valid Q# identifier character; the Parseable render (`render_ident`) maps
/// it back to `_`, so e.g. `t.0` / `x.0` render to today's `t_0` / `x_0` text.
///
/// Returns the newly allocated local variable IDs.
pub(crate) fn decompose_binding(
    package: &mut Package,
    assigner: &mut Assigner,
    pat_id: PatId,
    name: &str,
    elem_types: &[Ty],
) -> Vec<LocalVarId> {
    let n = elem_types.len();
    let mut new_locals: Vec<LocalVarId> = Vec::with_capacity(n);
    let mut new_pat_ids: Vec<PatId> = Vec::with_capacity(n);

    for (i, elem_ty) in elem_types.iter().enumerate() {
        let new_local = assigner.next_local();
        new_locals.push(new_local);

        let new_pat_id = assigner.next_pat();
        let elem_name: Rc<str> = Rc::from(format!("{name}.{i}"));
        let new_pat = Pat {
            id: new_pat_id,
            span: Span::default(),
            ty: elem_ty.clone(),
            kind: PatKind::Bind(Ident {
                id: new_local,
                span: Span::default(),
                name: elem_name,
            }),
        };
        package.pats.insert(new_pat_id, new_pat);
        new_pat_ids.push(new_pat_id);
    }

    // Rewrite the original binding pattern in-place.
    let pat = package
        .pats
        .get_mut(pat_id)
        .expect("candidate pat should exist");
    pat.kind = PatKind::Tuple(new_pat_ids);

    new_locals
}

/// Fully decomposes a `PatKind::Bind` pattern of (possibly deeply nested)
/// tuple type into a single flat `PatKind::Tuple` of scalar-leaf bindings.
///
/// Unlike [`decompose_binding`], which peels a single tuple level into a
/// tuple of per-element `Bind`s (leaving nested elements as further tuple
/// binds for a subsequent pass), this walks `ty` to its non-tuple leaves and
/// produces one `Bind` per leaf in a single flat tuple. For example, a
/// parameter `x : (Int, (Int, (Int, Int)))` becomes the flat pattern
/// `(x_0, x_1_0, x_1_1_0, x_1_1_1)` with flat type `(Int, Int, Int, Int)`.
/// The rewritten pattern satisfies the `PostArgPromote` shape invariant
/// trivially because both the pattern and the pattern's `ty` are set to the
/// same flat tuple.
///
/// Each leaf is named cumulatively from `name` and its positional path in the
/// original nested type, e.g. a leaf at original path `[1, 1, 0]` of parameter
/// `x` has the in-memory name `x.1.1.0`. The `.` path separator is a sentinel
/// that is never a valid Q# identifier character; the Parseable render
/// (`render_ident`) maps it back to `_`, so `x.1.1.0` renders to today's
/// `x_1_1_0` text. Every type leaf — read or unread in the body —
/// receives a placeholder `Bind`, so the flat pattern arity equals the leaf
/// count.
///
/// Returns one `(index_path, leaf_local, leaf_ty)` entry per leaf, where
/// `index_path` is the positional path of the leaf in the original nested
/// type relative to the decomposed parameter (used to project the leaf from
/// the original argument value at call sites and to remap field reads in the
/// body).
pub(crate) fn decompose_binding_to_leaves(
    package: &mut Package,
    assigner: &mut Assigner,
    pat_id: PatId,
    name: &str,
    ty: &Ty,
) -> Vec<(Vec<usize>, LocalVarId, Ty)> {
    let mut leaves: Vec<(Vec<usize>, LocalVarId, Ty)> = Vec::new();
    let mut leaf_pat_ids: Vec<PatId> = Vec::new();
    let mut path: Vec<usize> = Vec::new();
    collect_leaf_binds(
        package,
        assigner,
        name,
        ty,
        &mut path,
        &mut leaves,
        &mut leaf_pat_ids,
    );

    let flat_tys: Vec<Ty> = leaves
        .iter()
        .map(|(_, _, leaf_ty)| leaf_ty.clone())
        .collect();
    let pat = package
        .pats
        .get_mut(pat_id)
        .expect("candidate pat should exist");
    pat.kind = PatKind::Tuple(leaf_pat_ids);
    pat.ty = Ty::Tuple(flat_tys);

    leaves
}

/// Recursively walks `ty` collecting one scalar-leaf `Bind` pattern per
/// non-tuple leaf for [`decompose_binding_to_leaves`].
///
/// For a `Ty::Tuple`, recurses into each element with the element index
/// pushed onto `path`; for any other (leaf) type, allocates a `Bind` named
/// from the cumulative `path` and records both the leaf metadata (in
/// `leaves`) and its `PatId` (in `leaf_pat_ids`, in flat left-to-right
/// order). `path` is pushed/popped around each child so callers see it
/// unchanged on return.
fn collect_leaf_binds(
    package: &mut Package,
    assigner: &mut Assigner,
    name: &str,
    ty: &Ty,
    path: &mut Vec<usize>,
    leaves: &mut Vec<(Vec<usize>, LocalVarId, Ty)>,
    leaf_pat_ids: &mut Vec<PatId>,
) {
    match ty {
        Ty::Tuple(elems) if !elems.is_empty() => {
            for (i, elem_ty) in elems.iter().enumerate() {
                path.push(i);
                collect_leaf_binds(package, assigner, name, elem_ty, path, leaves, leaf_pat_ids);
                path.pop();
            }
        }
        _ => {
            let mut leaf_name = name.to_string();
            for index in path.iter() {
                leaf_name.push('.');
                leaf_name.push_str(&index.to_string());
            }
            let (local_id, leaf_pat_id) =
                alloc_bind_pat(package, assigner, &leaf_name, ty.clone(), Span::default());
            leaves.push((path.clone(), local_id, ty.clone()));
            leaf_pat_ids.push(leaf_pat_id);
        }
    }
}

/// Returns an iterator-like collection of `(LocalItemId, &CallableDecl)` for
/// every reachable callable that belongs to the given package.
///
/// Filters `reachable` to items in `package_id` that are `ItemKind::Callable`.
pub(crate) fn reachable_local_callables<'a>(
    package: &'a Package,
    package_id: PackageId,
    reachable: &'a FxHashSet<StoreItemId>,
) -> impl Iterator<Item = (LocalItemId, &'a CallableDecl)> {
    reachable.iter().filter_map(move |item_id| {
        if item_id.package != package_id {
            return None;
        }
        let item = package.get_item(item_id.item);
        match &item.kind {
            ItemKind::Callable(decl) => Some((item_id.item, decl.as_ref())),
            ItemKind::Ty(..) => None,
        }
    })
}

/// Returns an iterator over the functored specializations (`adj`, `ctl`, `ctl_adj`)
/// of a `SpecImpl`, skipping `None` entries.
pub(crate) fn functored_specs(spec_impl: &SpecImpl) -> impl Iterator<Item = &SpecDecl> {
    [
        spec_impl.adj.as_ref(),
        spec_impl.ctl.as_ref(),
        spec_impl.ctl_adj.as_ref(),
    ]
    .into_iter()
    .flatten()
}
