// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Shared FIR node allocation helpers.
//!
//! Every transform pass that synthesizes new FIR nodes must:
//! - Allocate a fresh ID from the pipeline-global [`Assigner`].
//! - Insert the node into the package's arena.
//! - Attach [`EMPTY_EXEC_RANGE`](crate::EMPTY_EXEC_RANGE) for `Expr` and
//!   `Stmt` nodes so the final [`exec_graph_rebuild`](crate::exec_graph_rebuild)
//!   pass can replace them with correct ranges.
//!
//! This module provides composable helpers that encapsulate this pattern,
//! reducing boilerplate across passes and centralizing the
//! `EMPTY_EXEC_RANGE` convention.

use crate::EMPTY_EXEC_RANGE;
use qsc_data_structures::span::Span;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    BinOp, Block, BlockId, CallableDecl, Expr, ExprId, ExprKind, Field, FieldPath, Ident, ItemId,
    ItemKind, LocalItemId, LocalVarId, Mutability, Package, PackageId, PackageLookup, PackageStore,
    Pat, PatId, PatKind, Res, SpecDecl, SpecImpl, Stmt, StmtId, StmtKind, StoreItemId, UnOp,
};
use rustc_hash::FxHashSet;

use qsc_fir::ty::{Prim, Ty};
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
#[allow(dead_code)]
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

/// Resolves a `Ty::Udt(Res::Item(item_id))` to its constituent field types
/// via `get_pure_ty()`. Returns `None` for single-field UDTs or non-UDT items.
pub(crate) fn resolve_udt_element_types(store: &PackageStore, item_id: &ItemId) -> Option<Vec<Ty>> {
    let package = store.get(item_id.package);
    let item = package.get_item(item_id.item);
    if let ItemKind::Ty(_, udt) = &item.kind {
        match udt.get_pure_ty() {
            Ty::Tuple(elems) if !elems.is_empty() => Some(elems),
            _ => None,
        }
    } else {
        None
    }
}

/// Decomposes a `PatKind::Bind` pattern into a `PatKind::Tuple` of per-element
/// bindings.
///
/// Allocates `n` new `LocalVarId`/`PatId` pairs (where `n = elem_types.len()`),
/// each named `{name}_{i}`, and rewrites the original pattern to
/// `PatKind::Tuple(new_pat_ids)`.
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
        let elem_name: Rc<str> = Rc::from(format!("{name}_{i}"));
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
            _ => None,
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
