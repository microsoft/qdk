// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::types::{CaptureScope, CapturedVar};
use crate::fir_builder::{alloc_expr, alloc_local_var_expr};
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::PackageSpan;
use qsc_fir::fir::{
    CallableKind, ExprId, ExprKind, FieldAssign, LocalVarId, Package, PackageLookup, Res,
};
use qsc_fir::ty::Ty;
use rustc_hash::FxHashMap;

#[derive(Clone, Copy)]
pub(super) enum CaptureDestination {
    Known(CaptureScope),
    Unknown,
}

impl From<CaptureScope> for CaptureDestination {
    fn from(scope: CaptureScope) -> Self {
        Self::Known(scope)
    }
}

impl From<Option<qsc_fir::fir::LocalItemId>> for CaptureDestination {
    fn from(owner: Option<qsc_fir::fir::LocalItemId>) -> Self {
        owner.map_or(Self::Unknown, |owner| {
            Self::Known(CaptureScope::Callable(owner))
        })
    }
}

pub(super) fn captures_belong_to_destination(
    destination: impl Into<CaptureDestination>,
    captures: &[CapturedVar],
) -> bool {
    let CaptureDestination::Known(destination) = destination.into() else {
        return false;
    };
    captures
        .iter()
        .all(|capture| capture.local.scope == destination)
}

/// Materializes capture operands for rewritten call arguments.
pub(super) fn allocate_capture_exprs(
    package: &mut Package,
    span: PackageSpan,
    destination: impl Into<CaptureDestination>,
    captures: &[CapturedVar],
    assigner: &mut Assigner,
) -> Vec<ExprId> {
    if captures.is_empty() {
        return Vec::new();
    }

    debug_assert!(captures_belong_to_destination(destination, captures));

    let mut ids = Vec::with_capacity(captures.len());
    for capture in captures {
        if let Some(expr_id) = capture.expr {
            if capture.caller_substitutions.is_empty() {
                ids.push(expr_id);
            } else {
                let substitutions: FxHashMap<LocalVarId, ExprId> =
                    capture.caller_substitutions.iter().copied().collect();
                ids.push(clone_capture_literal_with_substitutions(
                    package,
                    expr_id,
                    &substitutions,
                    assigner,
                ));
            }
            continue;
        }

        ids.push(alloc_local_var_expr(
            package,
            assigner,
            capture.local.var,
            capture.ty.clone(),
            span,
        ));
    }
    ids
}

#[allow(clippy::too_many_lines)]
fn clone_capture_literal_with_substitutions(
    package: &mut Package,
    expr_id: ExprId,
    substitutions: &FxHashMap<LocalVarId, ExprId>,
    assigner: &mut Assigner,
) -> ExprId {
    let expr = package.get_expr(expr_id).clone();
    if let ExprKind::Var(Res::Local(var), _) = &expr.kind
        && let Some(&caller_expr) = substitutions.get(var)
    {
        return caller_expr;
    }

    let clone_expr = |package: &mut Package, expr_id, assigner: &mut Assigner| {
        clone_capture_literal_with_substitutions(package, expr_id, substitutions, assigner)
    };
    let new_kind = match &expr.kind {
        ExprKind::Tuple(elements) => ExprKind::Tuple(
            elements
                .iter()
                .map(|&element| clone_expr(package, element, assigner))
                .collect(),
        ),
        ExprKind::Array(elements) => ExprKind::Array(
            elements
                .iter()
                .map(|&element| clone_expr(package, element, assigner))
                .collect(),
        ),
        ExprKind::ArrayLit(elements) => ExprKind::ArrayLit(
            elements
                .iter()
                .map(|&element| clone_expr(package, element, assigner))
                .collect(),
        ),
        ExprKind::ArrayRepeat(value, size) => ExprKind::ArrayRepeat(
            clone_expr(package, *value, assigner),
            clone_expr(package, *size, assigner),
        ),
        ExprKind::Struct(name, copy, fields) => ExprKind::Struct(
            *name,
            copy.map(|copy| clone_expr(package, copy, assigner)),
            fields
                .iter()
                .map(|field| FieldAssign {
                    span: field.span,
                    field: field.field.clone(),
                    value: clone_expr(package, field.value, assigner),
                })
                .collect(),
        ),
        ExprKind::Call(callee, arg) if callee_is_pure_function(package, *callee) => ExprKind::Call(
            clone_expr(package, *callee, assigner),
            clone_expr(package, *arg, assigner),
        ),
        ExprKind::BinOp(op, lhs, rhs) => ExprKind::BinOp(
            *op,
            clone_expr(package, *lhs, assigner),
            clone_expr(package, *rhs, assigner),
        ),
        ExprKind::UnOp(op, operand) => ExprKind::UnOp(*op, clone_expr(package, *operand, assigner)),
        ExprKind::Field(base, field) => {
            ExprKind::Field(clone_expr(package, *base, assigner), field.clone())
        }
        ExprKind::Index(base, index) => ExprKind::Index(
            clone_expr(package, *base, assigner),
            clone_expr(package, *index, assigner),
        ),
        ExprKind::UpdateIndex(container, index, value) => ExprKind::UpdateIndex(
            clone_expr(package, *container, assigner),
            clone_expr(package, *index, assigner),
            clone_expr(package, *value, assigner),
        ),
        ExprKind::UpdateField(record, field, value) => ExprKind::UpdateField(
            clone_expr(package, *record, assigner),
            field.clone(),
            clone_expr(package, *value, assigner),
        ),
        ExprKind::Range(start, step, end) => ExprKind::Range(
            start.map(|part| clone_expr(package, part, assigner)),
            step.map(|part| clone_expr(package, part, assigner)),
            end.map(|part| clone_expr(package, part, assigner)),
        ),
        _ => expr.kind.clone(),
    };

    alloc_expr(package, assigner, expr.ty.clone(), new_kind, expr.span)
}

fn callee_is_pure_function(package: &Package, callee: ExprId) -> bool {
    matches!(
        &package.get_expr(callee).ty,
        Ty::Arrow(arrow) if arrow.kind == CallableKind::Function
    )
}
