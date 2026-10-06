// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Shared capture-operand materialization for specialization and call rewriting.
//! Callers establish scope and replay eligibility; this module preserves the
//! agreed operand order and applies recorded producer-to-caller substitutions.

use super::types::{CaptureScope, CaptureSubstitution, CapturedVar};
use crate::fir_builder::{alloc_expr, alloc_local_var_expr};
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::PackageSpan;
use qsc_fir::fir::{CallableKind, ExprId, ExprKind, LocalVarId, Package, PackageLookup, Res};
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

/// Materializes capture operands for rewritten call arguments in capture order.
///
/// Reuses recorded expressions when no substitutions are needed; otherwise
/// reconstructs supported expression forms with caller operands substituted.
/// Bare captures become local reads. The ownership assertion is debug-only;
/// callers must establish destination ownership before requesting a write.
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
                let substitutions: FxHashMap<LocalVarId, &CaptureSubstitution> = capture
                    .caller_substitutions
                    .iter()
                    .map(|substitution| (substitution.local, substitution))
                    .collect();
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

/// The expression children reconstructed by capture substitution. `None` means
/// the node is kept verbatim: analysis must reject any producer locals beneath
/// it. Both admission and cloning use this contract, including Parallel limits.
/// Callers separately reject operation calls; function kind alone is not purity.
pub(super) fn capture_expr_children(kind: &mut ExprKind) -> Option<Vec<&mut ExprId>> {
    Some(match kind {
        ExprKind::Tuple(elements) | ExprKind::Array(elements) | ExprKind::ArrayLit(elements) => {
            elements.iter_mut().collect()
        }
        ExprKind::ArrayRepeat(a, b)
        | ExprKind::Call(a, b)
        | ExprKind::BinOp(_, a, b)
        | ExprKind::Index(a, b)
        | ExprKind::UpdateField(a, _, b) => vec![a, b],
        ExprKind::Struct(_, copy, fields) => copy
            .iter_mut()
            .chain(fields.iter_mut().map(|field| &mut field.value))
            .collect(),
        ExprKind::UnOp(_, operand) | ExprKind::Field(operand, _) => vec![operand],
        ExprKind::UpdateIndex(container, index, value) => vec![container, index, value],
        ExprKind::Range(start, step, end) => start
            .iter_mut()
            .chain(step.iter_mut())
            .chain(end.iter_mut())
            .collect(),
        ExprKind::Parallel(limit, body) => limit.iter_mut().chain(std::iter::once(body)).collect(),
        ExprKind::Assign(..)
        | ExprKind::AssignOp(..)
        | ExprKind::AssignField(..)
        | ExprKind::AssignIndex(..)
        | ExprKind::Block(..)
        | ExprKind::Closure(..)
        | ExprKind::Fail(..)
        | ExprKind::Hole
        | ExprKind::If(..)
        | ExprKind::Lit(..)
        | ExprKind::Return(..)
        | ExprKind::String(..)
        | ExprKind::Var(..)
        | ExprKind::While(..) => return None,
    })
}

/// Clones the expression forms admitted by capture rebinding. A substitution's
/// replacement is interpreted only in its own nested environment.
fn clone_capture_literal_with_substitutions(
    package: &mut Package,
    expr_id: ExprId,
    substitutions: &FxHashMap<LocalVarId, &CaptureSubstitution>,
    assigner: &mut Assigner,
) -> ExprId {
    let expr = package.get_expr(expr_id).clone();
    if let ExprKind::Var(Res::Local(var), _) = &expr.kind
        && let Some(substitution) = substitutions.get(var)
    {
        if substitution.substitutions.is_empty() {
            return substitution.expr;
        }
        let nested = substitution
            .substitutions
            .iter()
            .map(|substitution| (substitution.local, substitution))
            .collect();
        return clone_capture_literal_with_substitutions(
            package,
            substitution.expr,
            &nested,
            assigner,
        );
    }

    let mut new_kind = expr.kind;
    if let Some(children) = capture_expr_children(&mut new_kind) {
        for child in children {
            *child =
                clone_capture_literal_with_substitutions(package, *child, substitutions, assigner);
        }
    }

    alloc_expr(package, assigner, expr.ty, new_kind, expr.span)
}

/// Checks callable kind, not whether evaluating the function is unobservable.
pub(super) fn callee_has_function_kind(package: &Package, callee: ExprId) -> bool {
    matches!(
        &package.get_expr(callee).ty,
        Ty::Arrow(arrow) if arrow.kind == CallableKind::Function
    )
}
