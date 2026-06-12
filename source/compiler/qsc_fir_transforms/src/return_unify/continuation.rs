// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Continuation-safety policy for return unification.

use qsc_fir::{
    fir::{Package, PackageId, PackageLookup, Res, StmtId, StmtKind},
    ty::{Prim, Ty},
};

use super::{UdtPureTyCache, UdtResolutionContext, slot::can_create_classical_default};

/// Checks whether a guarded local initializer can be synthesized eagerly.
///
/// This uses the policy context for the currently rewritten package so UDTs
/// that appear only in continuation locals can still be resolved lazily.
fn can_create_guarded_local_default(
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    context: &UdtResolutionContext<'_>,
) -> bool {
    can_create_classical_default(ty, udt_pure_tys, context)
}

/// Safety classification for keeping a continuation local behind an eager guard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContinuationSafety {
    /// The type can be guarded in place without changing quantum lifetime behavior.
    Safe,
    /// The type contains quantum state and must be moved into a lazy continuation.
    SplitRequired,
    /// The type could not be resolved; split conservatively.
    Unknown,
}

impl ContinuationSafety {
    /// Combines two continuation-safety classifications for compound types.
    fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::SplitRequired, _) | (_, Self::SplitRequired) => Self::SplitRequired,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::Safe, Self::Safe) => Self::Safe,
        }
    }

    /// Returns true when the suffix must be moved into a lazy continuation.
    fn requires_split(self) -> bool {
        !matches!(self, Self::Safe)
    }
}

/// Classify whether a continuation suffix type can be guarded in place.
fn continuation_safety_for_ty(
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    context: &UdtResolutionContext<'_>,
) -> ContinuationSafety {
    match ty {
        Ty::Prim(Prim::Qubit) => ContinuationSafety::SplitRequired,
        Ty::Array(elem_ty) => continuation_safety_for_ty(elem_ty, udt_pure_tys, context),
        Ty::Tuple(elems) => elems
            .iter()
            .fold(ContinuationSafety::Safe, |safety, elem_ty| {
                safety.combine(continuation_safety_for_ty(elem_ty, udt_pure_tys, context))
            }),
        Ty::Udt(Res::Item(item_id)) => context
            .resolve_udt_pure_ty(udt_pure_tys, *item_id)
            .map_or(ContinuationSafety::Unknown, |pure_ty| {
                continuation_safety_for_ty(&pure_ty, udt_pure_tys, context)
            }),
        Ty::Arrow(_) | Ty::Infer(_) | Ty::Param(_) | Ty::Prim(_) | Ty::Udt(_) | Ty::Err => {
            ContinuationSafety::Safe
        }
    }
}

/// Returns true when a type's continuation value requires lazy suffix splitting.
fn continuation_ty_requires_split(
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    context: &UdtResolutionContext<'_>,
) -> bool {
    continuation_safety_for_ty(ty, udt_pure_tys, context).requires_split()
}

/// Returns true when a local statement cannot be guarded eagerly after a return.
///
/// Non-defaultable initializers and quantum-containing local or initializer
/// types are moved into a lazy continuation so they are never evaluated after
/// `__has_returned` is set.
fn local_initializer_requires_split_continuation(
    package: &Package,
    stmt_id: StmtId,
    package_id: PackageId,
    udt_pure_tys: &UdtPureTyCache,
) -> bool {
    if let StmtKind::Local(_, pat_id, init_expr_id) = package.get_stmt(stmt_id).kind {
        let local_ty = &package.get_pat(pat_id).ty;
        let init_ty = &package.get_expr(init_expr_id).ty;
        let context = UdtResolutionContext::Package {
            package_id,
            package,
        };

        !can_create_guarded_local_default(init_ty, udt_pure_tys, &context)
            || continuation_ty_requires_split(local_ty, udt_pure_tys, &context)
            || continuation_ty_requires_split(init_ty, udt_pure_tys, &context)
    } else {
        false
    }
}

/// Scans a statement suffix for locals that require lazy continuation splitting.
pub(super) fn continuation_suffix_requires_split(
    package: &Package,
    original_stmts: &[StmtId],
    index: usize,
    package_id: PackageId,
    udt_pure_tys: &UdtPureTyCache,
) -> bool {
    original_stmts.get(index..).is_some_and(|suffix| {
        suffix.iter().any(|&stmt_id| {
            local_initializer_requires_split_continuation(
                package,
                stmt_id,
                package_id,
                udt_pure_tys,
            )
        })
    })
}
