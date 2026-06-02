// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Return-slot and defaultability policy for return unification.

use crate::{
    EMPTY_EXEC_RANGE,
    fir_builder::{
        alloc_assign_expr, alloc_block, alloc_expr, alloc_expr_stmt, alloc_if_expr,
        alloc_local_var, alloc_local_var_expr,
    },
};
use num_bigint::BigInt;
use qsc_data_structures::span::Span;
use qsc_fir::{
    assigner::Assigner,
    fir::{
        CallableDecl, CallableImpl, Expr, ExprId, ExprKind, Ident, ItemId, ItemKind, Lit,
        LocalItemId, LocalVarId, Mutability, Package, PackageId, Pat, PatKind, Res, Result, StmtId,
        StoreItemId, StringComponent,
    },
    ty::{Prim, Ty},
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

use super::{
    ARRAY_RETURN_SLOT_UNWRITTEN_FAIL_MESSAGE, UdtPureTyCache, UdtResolutionContext, symbols,
};

/// Strategy used for the synthesized return-value slot in flag-based rewrites.
///
/// Selected once per callable by [`select_return_slot_strategy`] before the
/// package is mutably borrowed, and threaded through the rewrite via
/// [`ReturnSlot`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReturnSlotStrategy {
    /// Store the returned value directly in `__ret_val : T`.
    ///
    /// Used when `T` has a classical default. Reads of the slot need no
    /// further wrapping: `__ret_val` already has the right type and the
    /// initial value keeps unreachable false branches well-typed.
    Direct,
    /// Store the returned value as the single element of `__ret_val : T[]`.
    ///
    /// Used when `T` has no classical default but its structure is resolvable,
    /// so the universal array default `[]` is well-typed. Reads index `[0]`
    /// and are guarded by `__has_returned` (or by a typed `ExprKind::Fail`
    /// in statically dead branches).
    ArrayBacked,
}

/// Synthesized return-value slot shared by flag-lowered rewrites.
///
/// Carries both the slot's [`LocalVarId`] and the [`ReturnSlotStrategy`]
/// chosen for it, so downstream helpers can emit the right shape
/// (`__ret_val = v` vs `__ret_val = [v]`) without re-deriving the policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ReturnSlot {
    /// Local id for the synthesized `__ret_val` slot.
    pub(super) var_id: LocalVarId,
    /// Representation strategy selected for the slot.
    pub(super) strategy: ReturnSlotStrategy,
}

/// Conservative scan result for arrow-containing return types.
///
/// Used by [`arrow_scan_for_ty`] to decide whether an array-backed return
/// slot is safe. The lattice is [`ArrowScan::ContainsArrow`] >
/// [`ArrowScan::Unknown`] > [`ArrowScan::NoArrow`]; `Unknown` is the only
/// rejecting result for array-backed mode after Direct defaults are excluded,
/// so resolvable arrow-containing shapes remain supported.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ArrowScan {
    /// The scanned type is definitely arrow-free.
    NoArrow,
    /// The scanned type contains at least one arrow.
    ContainsArrow,
    /// The scanned type could not be resolved precisely enough.
    Unknown,
}

impl ArrowScan {
    /// Combines two scan results, preserving the most conservative outcome.
    ///
    /// `ContainsArrow` dominates `Unknown`, which dominates `NoArrow`. The
    /// operation is commutative and associative, so it is safe to fold over
    /// children of tuples/arrays/UDTs in any order.
    fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::ContainsArrow, _) | (_, Self::ContainsArrow) => Self::ContainsArrow,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::NoArrow, Self::NoArrow) => Self::NoArrow,
        }
    }
}

/// Selects the representation for flag lowering's synthesized return slot.
///
/// Choices in priority order:
///
/// | Condition                                              | Strategy                          |
/// |--------------------------------------------------------|-----------------------------------|
/// | `ty` has a classical default                           | [`ReturnSlotStrategy::Direct`]    |
/// | `ty` lacks a classical default but is resolvable       | [`ReturnSlotStrategy::ArrayBacked`] |
/// | `ty` has unresolved structure (`ArrowScan::Unknown`)   | `None`                            |
///
/// `None` signals that this callable cannot be rewritten by flag lowering and
/// the user must see an unsupported-return-type diagnostic.
///
/// Arrow-containing types are eligible for array-backed mode: the synthesized
/// `fail`-bodied default callable provides a well-typed bottom value for the
/// array read fallback, so arrays of callables are handled correctly.
pub(super) fn select_return_slot_strategy(
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    context: &UdtResolutionContext<'_>,
) -> Option<ReturnSlotStrategy> {
    if can_create_classical_default(ty, udt_pure_tys, context) {
        Some(ReturnSlotStrategy::Direct)
    } else if can_use_array_backed_return_slot(ty, udt_pure_tys, context) {
        Some(ReturnSlotStrategy::ArrayBacked)
    } else {
        None
    }
}

/// Returns true when a non-defaultable type can use an array-backed return slot.
///
/// The slot stores `T` inside `T[]`, whose `[]` default is always well-typed.
/// Eligibility requires both:
///
/// 1. `ty` has no classical default (otherwise [`ReturnSlotStrategy::Direct`]
///    is preferred, so this returns `false`).
/// 2. `ty` is resolvable per [`arrow_scan_for_ty`] (not
///    [`ArrowScan::Unknown`]). Arrow-containing types qualify because the
///    cached `fail`-bodied callable supplies a well-typed bottom value for
///    the array-read fallback.
pub(super) fn can_use_array_backed_return_slot(
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    context: &UdtResolutionContext<'_>,
) -> bool {
    !can_create_classical_default(ty, udt_pure_tys, context)
        && matches!(
            arrow_scan_for_ty(ty, udt_pure_tys, context, &mut FxHashSet::default()),
            ArrowScan::NoArrow | ArrowScan::ContainsArrow
        )
}

/// Conservatively scans a type for nested arrows.
///
/// Walks tuples, arrays, and UDTs (via their pure types) for [`Ty::Arrow`]
/// leaves, combining results with [`ArrowScan::combine`]. UDT recursion is
/// cycle-broken via `visiting_udts`, and unresolved or recursive UDTs return
/// [`ArrowScan::Unknown`], which makes [`can_use_array_backed_return_slot`]
/// reject the type so the strategy degrades into an unsupported-return-type
/// diagnostic.
fn arrow_scan_for_ty(
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    context: &UdtResolutionContext<'_>,
    visiting_udts: &mut FxHashSet<StoreItemId>,
) -> ArrowScan {
    match ty {
        Ty::Arrow(_) => ArrowScan::ContainsArrow,
        Ty::Array(elem_ty) => arrow_scan_for_ty(elem_ty, udt_pure_tys, context, visiting_udts),
        Ty::Tuple(elems) => elems.iter().fold(ArrowScan::NoArrow, |scan, elem_ty| {
            scan.combine(arrow_scan_for_ty(
                elem_ty,
                udt_pure_tys,
                context,
                visiting_udts,
            ))
        }),
        Ty::Udt(Res::Item(item_id)) => {
            let key = (item_id.package, item_id.item).into();
            if !visiting_udts.insert(key) {
                return ArrowScan::Unknown;
            }

            let scan = context
                .resolve_udt_pure_ty(udt_pure_tys, *item_id)
                .map_or(ArrowScan::Unknown, |pure_ty| {
                    arrow_scan_for_ty(&pure_ty, udt_pure_tys, context, visiting_udts)
                });
            visiting_udts.remove(&key);
            scan
        }
        Ty::Prim(_) => ArrowScan::NoArrow,
        Ty::Infer(_) | Ty::Param(_) | Ty::Err | Ty::Udt(_) => ArrowScan::Unknown,
    }
}

/// Checks whether `ty` has a classical default in the given UDT resolution context.
pub(super) fn can_create_classical_default(
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    context: &UdtResolutionContext<'_>,
) -> bool {
    match ty {
        Ty::Prim(
            Prim::Bool
            | Prim::Int
            | Prim::BigInt
            | Prim::Double
            | Prim::Pauli
            | Prim::Result
            | Prim::String
            | Prim::Range
            | Prim::RangeFrom
            | Prim::RangeTo
            | Prim::RangeFull,
        )
        | Ty::Array(_) => true,
        Ty::Tuple(elems) => elems
            .iter()
            .all(|e| can_create_classical_default(e, udt_pure_tys, context)),
        Ty::Udt(Res::Item(item_id)) => context
            .resolve_udt_pure_ty(udt_pure_tys, *item_id)
            .is_some_and(|pure_ty| can_create_classical_default(&pure_ty, udt_pure_tys, context)),
        // Arrow types always have a classical default: the fail-bodied
        // callable synthesized by `synthesize_fail_callable`. The body is
        // `fail "callable init expr"`, so no recursive output-type default
        // is needed. The only exclusion is non-Value functors, which should
        // not appear post-monomorphization.
        Ty::Arrow(arrow) => matches!(arrow.functors, qsc_fir::ty::FunctorSet::Value(_)),
        Ty::Infer(_) | Ty::Param(_) | Ty::Err | Ty::Prim(Prim::Qubit) | Ty::Udt(_) => false,
    }
}

/// Allocates the `mutable __ret_val` declaration for flag lowering.
pub(super) fn create_return_slot_decl(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    return_ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    arrow_default_cache: &mut ArrowDefaultCache,
    strategy: ReturnSlotStrategy,
) -> (ReturnSlot, StmtId) {
    let (slot_ty, init_expr) = match strategy {
        ReturnSlotStrategy::Direct => {
            let init_expr = require_classical_default(
                package,
                assigner,
                package_id,
                return_ty,
                udt_pure_tys,
                arrow_default_cache,
                UnsupportedDefaultSite::ReturnSlot,
            );
            (return_ty.clone(), init_expr)
        }
        ReturnSlotStrategy::ArrayBacked => {
            let slot_ty = Ty::Array(Box::new(return_ty.clone()));
            let init_expr = alloc_expr(
                package,
                assigner,
                slot_ty.clone(),
                ExprKind::Array(Vec::new()),
                Span::default(),
            );
            (slot_ty, init_expr)
        }
    };

    let (var_id, stmt_id) = alloc_local_var(
        package,
        assigner,
        symbols::RET_VAL,
        &slot_ty,
        init_expr,
        Mutability::Mutable,
    );
    (ReturnSlot { var_id, strategy }, stmt_id)
}

/// Builds the write expression that stores a returned value into `slot`.
pub(super) fn create_return_slot_write_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    slot: ReturnSlot,
    value_expr: ExprId,
    value_ty: &Ty,
) -> ExprId {
    match slot.strategy {
        ReturnSlotStrategy::Direct => {
            create_assign_expr(package, assigner, slot.var_id, value_expr, value_ty)
        }
        ReturnSlotStrategy::ArrayBacked => {
            let array_ty = Ty::Array(Box::new(value_ty.clone()));
            let singleton = alloc_expr(
                package,
                assigner,
                array_ty.clone(),
                ExprKind::Array(vec![value_expr]),
                Span::default(),
            );
            create_assign_expr(package, assigner, slot.var_id, singleton, &array_ty)
        }
    }
}

/// Builds an expression that reads the returned value out of `slot`.
pub(super) fn create_return_slot_read_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    slot: ReturnSlot,
    return_ty: &Ty,
) -> ExprId {
    match slot.strategy {
        ReturnSlotStrategy::Direct => alloc_local_var_expr(
            package,
            assigner,
            slot.var_id,
            return_ty.clone(),
            Span::default(),
        ),
        ReturnSlotStrategy::ArrayBacked => {
            let array_ty = Ty::Array(Box::new(return_ty.clone()));
            let array_expr =
                alloc_local_var_expr(package, assigner, slot.var_id, array_ty, Span::default());
            let zero = alloc_expr(
                package,
                assigner,
                Ty::Prim(Prim::Int),
                ExprKind::Lit(Lit::Int(0)),
                Span::default(),
            );
            alloc_expr(
                package,
                assigner,
                return_ty.clone(),
                ExprKind::Index(array_expr, zero),
                Span::default(),
            )
        }
    }
}

/// Builds a slot read that is safe to use without an enclosing flag guard.
pub(super) fn create_return_slot_read_or_fail_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    has_returned_var_id: LocalVarId,
    slot: ReturnSlot,
    return_ty: &Ty,
) -> ExprId {
    match slot.strategy {
        ReturnSlotStrategy::Direct => {
            create_return_slot_read_expr(package, assigner, slot, return_ty)
        }
        ReturnSlotStrategy::ArrayBacked => {
            let flag = alloc_local_var_expr(
                package,
                assigner,
                has_returned_var_id,
                Ty::Prim(Prim::Bool),
                Span::default(),
            );
            let read = create_return_slot_read_expr(package, assigner, slot, return_ty);
            let fail = create_typed_fail_expr(
                package,
                assigner,
                return_ty,
                ARRAY_RETURN_SLOT_UNWRITTEN_FAIL_MESSAGE,
            );
            alloc_if_expr(
                package,
                assigner,
                flag,
                read,
                Some(fail),
                return_ty.clone(),
                Span::default(),
            )
        }
    }
}

/// Builds the fallback expression used when the block has no fallthrough trailing value.
pub(super) fn create_return_slot_unwritten_fallback_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    slot: ReturnSlot,
    return_ty: &Ty,
) -> ExprId {
    match slot.strategy {
        ReturnSlotStrategy::Direct => {
            create_return_slot_read_expr(package, assigner, slot, return_ty)
        }
        ReturnSlotStrategy::ArrayBacked => create_typed_fail_expr(
            package,
            assigner,
            return_ty,
            ARRAY_RETURN_SLOT_UNWRITTEN_FAIL_MESSAGE,
        ),
    }
}

fn create_typed_fail_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    ty: &Ty,
    message: &str,
) -> ExprId {
    let message_expr = alloc_expr(
        package,
        assigner,
        Ty::Prim(Prim::String),
        ExprKind::String(vec![StringComponent::Lit(Rc::from(message))]),
        Span::default(),
    );
    alloc_expr(
        package,
        assigner,
        ty.clone(),
        ExprKind::Fail(message_expr),
        Span::default(),
    )
}

/// Synthesis site used in unsupported-default contract diagnostics.
#[derive(Clone, Copy, Debug)]
pub(super) enum UnsupportedDefaultSite {
    /// Default needed for the synthesized `__ret_val` return slot.
    ReturnSlot,
    /// Default needed when guarding a local initializer in place.
    GuardedLocalInitializer,
}

impl UnsupportedDefaultSite {
    /// Human-readable description included in contract-violation panic messages.
    fn description(self) -> &'static str {
        match self {
            Self::ReturnSlot => "flag-lowering return-slot (__ret_val) initialization",
            Self::GuardedLocalInitializer => "flag-lowering guarded Local initializer",
        }
    }
}

/// Enforces the unsupported-default policy for flag-lowering synthesis sites.
pub(super) fn require_classical_default(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    arrow_default_cache: &mut ArrowDefaultCache,
    site: UnsupportedDefaultSite,
) -> ExprId {
    create_default_value(
        package,
        assigner,
        package_id,
        ty,
        udt_pure_tys,
        arrow_default_cache,
    )
    .unwrap_or_else(|| {
        panic!(
            "return_unify unsupported-default contract violation: {} requires a classical default, but `{ty}` has none",
            site.description(),
        )
    })
}

pub(super) fn create_default_value(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    arrow_default_cache: &mut ArrowDefaultCache,
) -> Option<ExprId> {
    let kind = create_default_value_kind(
        package,
        assigner,
        package_id,
        ty,
        udt_pure_tys,
        arrow_default_cache,
    )?;

    let expr_id = assigner.next_expr();
    package.exprs.insert(
        expr_id,
        Expr {
            id: expr_id,
            span: Span::default(),
            ty: ty.clone(),
            kind,
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    Some(expr_id)
}

fn create_default_value_kind(
    package: &mut Package,
    assigner: &mut Assigner,
    package_id: PackageId,
    ty: &Ty,
    udt_pure_tys: &UdtPureTyCache,
    arrow_default_cache: &mut ArrowDefaultCache,
) -> Option<ExprKind> {
    match ty {
        Ty::Prim(Prim::Bool) => Some(ExprKind::Lit(Lit::Bool(false))),
        Ty::Prim(Prim::Int) => Some(ExprKind::Lit(Lit::Int(0))),
        Ty::Prim(Prim::BigInt) => Some(ExprKind::Lit(Lit::BigInt(BigInt::from(0)))),
        Ty::Prim(Prim::Double) => Some(ExprKind::Lit(Lit::Double(0.0))),
        Ty::Prim(Prim::Pauli) => Some(ExprKind::Lit(Lit::Pauli(qsc_fir::fir::Pauli::I))),
        Ty::Prim(Prim::Result) => Some(ExprKind::Lit(Lit::Result(Result::Zero))),
        Ty::Prim(Prim::String) => Some(ExprKind::String(Vec::new())),
        Ty::Tuple(elems) if elems.is_empty() => Some(ExprKind::Tuple(Vec::new())),
        Ty::Tuple(elems) => {
            let elem_exprs: Vec<ExprId> = elems
                .iter()
                .map(|elem_ty| {
                    create_default_value(
                        package,
                        assigner,
                        package_id,
                        elem_ty,
                        udt_pure_tys,
                        arrow_default_cache,
                    )
                })
                .collect::<Option<_>>()?;
            Some(ExprKind::Tuple(elem_exprs))
        }
        Ty::Array(_) => Some(ExprKind::Array(Vec::new())),
        Ty::Udt(Res::Item(item_id)) => {
            let pure_ty = udt_pure_tys.resolve_from_package(package_id, package, *item_id)?;
            create_default_value_kind(
                package,
                assigner,
                package_id,
                &pure_ty,
                udt_pure_tys,
                arrow_default_cache,
            )
        }
        Ty::Arrow(arrow) => {
            let qsc_fir::ty::FunctorSet::Value(functors) = arrow.functors else {
                return None;
            };
            let item_id = arrow_default_cache.get_or_insert(
                package,
                assigner,
                arrow.kind,
                &arrow.input,
                &arrow.output,
                functors,
            );
            Some(ExprKind::Var(
                Res::Item(ItemId {
                    package: package_id,
                    item: item_id,
                }),
                Vec::new(),
            ))
        }
        Ty::Prim(Prim::Range | Prim::RangeFrom | Prim::RangeTo | Prim::RangeFull) => {
            Some(ExprKind::Range(None, None, None))
        }
        Ty::Infer(_) | Ty::Param(_) | Ty::Err | Ty::Prim(Prim::Qubit) | Ty::Udt(_) => None,
    }
}

/// Read-only check whether `ty` has a synthesizable classical default.
pub(super) fn is_type_defaultable(package: &Package, package_id: PackageId, ty: &Ty) -> bool {
    match ty {
        Ty::Prim(
            Prim::Bool
            | Prim::Int
            | Prim::BigInt
            | Prim::Double
            | Prim::Pauli
            | Prim::Result
            | Prim::String
            | Prim::Range
            | Prim::RangeFrom
            | Prim::RangeTo
            | Prim::RangeFull,
        )
        | Ty::Array(_)
        | Ty::Arrow(_) => true,
        Ty::Tuple(elems) => elems
            .iter()
            .all(|e| is_type_defaultable(package, package_id, e)),
        Ty::Udt(Res::Item(item_id)) => {
            if item_id.package != package_id {
                return false;
            }
            let Some(item) = package.items.get(item_id.item) else {
                return false;
            };
            let ItemKind::Ty(_, udt) = &item.kind else {
                return false;
            };
            is_type_defaultable(package, package_id, &udt.get_pure_ty())
        }
        Ty::Prim(Prim::Qubit) | Ty::Infer(_) | Ty::Param(_) | Ty::Err | Ty::Udt(_) => false,
    }
}

type ArrowDefaultKey = (
    qsc_fir::fir::CallableKind,
    String,
    qsc_fir::ty::FunctorSetValue,
);

/// Caches fail-bodied callables synthesized for arrow-typed default values.
#[derive(Default)]
pub(super) struct ArrowDefaultCache {
    items: FxHashMap<ArrowDefaultKey, LocalItemId>,
}

impl ArrowDefaultCache {
    fn get_or_insert(
        &mut self,
        package: &mut Package,
        assigner: &mut Assigner,
        kind: qsc_fir::fir::CallableKind,
        input_ty: &Ty,
        output_ty: &Ty,
        functors: qsc_fir::ty::FunctorSetValue,
    ) -> LocalItemId {
        let key = (kind, format!("{input_ty} -> {output_ty}"), functors);
        if let Some(&id) = self.items.get(&key) {
            return id;
        }
        let new_id =
            synthesize_fail_callable(package, assigner, kind, input_ty, output_ty, functors);
        self.items.insert(key, new_id);
        new_id
    }
}

fn synthesize_fail_callable(
    package: &mut Package,
    assigner: &mut Assigner,
    kind: qsc_fir::fir::CallableKind,
    input_ty: &Ty,
    output_ty: &Ty,
    functors: qsc_fir::ty::FunctorSetValue,
) -> LocalItemId {
    let msg_expr_id = alloc_expr(
        package,
        assigner,
        Ty::Prim(Prim::String),
        ExprKind::String(vec![StringComponent::Lit("callable init expr".into())]),
        Span::default(),
    );
    let fail_expr_id = alloc_expr(
        package,
        assigner,
        output_ty.clone(),
        ExprKind::Fail(msg_expr_id),
        Span::default(),
    );
    let trailing_stmt = alloc_expr_stmt(package, assigner, fail_expr_id, Span::default());
    let body_block = alloc_block(
        package,
        assigner,
        vec![trailing_stmt],
        output_ty.clone(),
        Span::default(),
    );

    let input_pat_id = assigner.next_pat();
    package.pats.insert(
        input_pat_id,
        Pat {
            id: input_pat_id,
            span: Span::default(),
            ty: input_ty.clone(),
            kind: PatKind::Discard,
        },
    );

    let body_spec = qsc_fir::fir::SpecDecl {
        id: assigner.next_node(),
        span: Span::default(),
        block: body_block,
        input: None,
        exec_graph: qsc_fir::fir::ExecGraph::default(),
    };
    let body_impl = qsc_fir::fir::SpecImpl {
        body: body_spec,
        adj: None,
        ctl: None,
        ctl_adj: None,
    };

    let new_item_id = assigner.next_item();
    let callable_name: Rc<str> = Rc::from(format!("__return_unify_fail_{new_item_id}"));
    let decl = CallableDecl {
        id: assigner.next_node(),
        span: Span::default(),
        kind,
        name: Ident {
            id: LocalVarId::from(0_u32),
            span: Span::default(),
            name: callable_name,
        },
        generics: Vec::new(),
        input: input_pat_id,
        output: output_ty.clone(),
        functors,
        implementation: CallableImpl::Spec(body_impl),
        attrs: Vec::new(),
    };

    let item = qsc_fir::fir::Item {
        id: new_item_id,
        span: Span::default(),
        parent: None,
        doc: Rc::from(""),
        attrs: Vec::new(),
        visibility: qsc_fir::fir::Visibility::Internal,
        kind: ItemKind::Callable(Box::new(decl)),
    };
    package.items.insert(new_item_id, item);

    new_item_id
}

fn create_assign_expr(
    package: &mut Package,
    assigner: &mut Assigner,
    var_id: LocalVarId,
    value: ExprId,
    ty: &Ty,
) -> ExprId {
    let var_expr = alloc_local_var_expr(package, assigner, var_id, ty.clone(), Span::default());
    alloc_assign_expr(package, assigner, var_expr, value, Span::default())
}
