// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Regressions for closure-dispatch argument-layout matching.
//!
//! When a higher-order operation is specialized, the dispatch rewrite must
//! splice the closure's captured values in front of the original call
//! arguments and reshape the result into the target's declared input. A
//! captured operation carries its *runtime* functor set — `Adj + Ctl` in the
//! shape modeled here — while the target's input slot records only the
//! capability its body *requires*, which is `Empty`. Those two types are not
//! equal, so a strictly structural comparison rejects the entire layout,
//! leaves the original scalar payload in place, and yields a call whose
//! argument type does not match its callee. That is the `PostArgPromote`
//! violation these regressions pin down.
//!
//! [`dispatch_layout_types_compatible`] therefore compares callable functor
//! sets as capability requirements and everything else exactly. The tests
//! below prove both halves: the capability relation is load-bearing, and it
//! does not leak into general type compatibility.
//!
//! # Why hand-built FIR
//!
//! Reduced Q# shapes resolve a captured callable into a specialization *key*
//! rather than a value slot, so compiled source does not reach this builder with
//! an arrow-typed capture in the dispatch layout. These fixtures make the
//! matcher's contract directly testable.

// Fixtures pair layout construction with structural assertions, which pushes
// the table-driven negative test past the line limit.
#![allow(clippy::too_many_lines)]

use super::*;
use crate::test_utils::assert_panics_with;
use qsc_fir::fir::{CallableKind, Lit};

/// Restores the dispatch-layout capability control on scope exit, including
/// when the test body panics.
struct CapabilityMatchingGuard(bool);

impl CapabilityMatchingGuard {
    /// Disables capability matching so callable functors must match exactly.
    fn disable() -> Self {
        Self(DISPATCH_LAYOUT_CAPABILITY_MATCHING.with(|enabled| enabled.replace(false)))
    }
}

impl Drop for CapabilityMatchingGuard {
    fn drop(&mut self) {
        DISPATCH_LAYOUT_CAPABILITY_MATCHING.with(|enabled| enabled.set(self.0));
    }
}

/// `Qubit[]`, the scalar payload and the captured operation's input.
fn qubit_array_ty() -> Ty {
    Ty::Array(Box::new(Ty::Prim(Prim::Qubit)))
}

/// An arrow type with an explicit functor set value.
fn arrow_ty(kind: CallableKind, input: Ty, output: Ty, functors: FunctorSetValue) -> Ty {
    Ty::Arrow(Box::new(Arrow {
        kind,
        input: Box::new(input),
        output: Box::new(output),
        functors: FunctorSet::Value(functors),
    }))
}

/// `Qubit[] => Unit is <functors>`, the shape of the captured operation.
fn qubit_array_op_ty(functors: FunctorSetValue) -> Ty {
    arrow_ty(
        CallableKind::Operation,
        qubit_array_ty(),
        Ty::UNIT,
        functors,
    )
}

/// The specialization target's input, `(op, tag, qs)`, where `op` declares
/// only the functor capability the target body requires.
fn target_input_ty(op_ty: Ty, tag_ty: Ty) -> Ty {
    Ty::Tuple(vec![op_ty, tag_ty, qubit_array_ty()])
}

/// Reads an expression's arrow functor set, panicking if it is not an arrow.
fn expr_functors(package: &Package, expr_id: ExprId) -> FunctorSet {
    match &package.get_expr(expr_id).ty {
        Ty::Arrow(arrow) => arrow.functors,
        other => panic!("expected an arrow-typed expression, found {other:?}"),
    }
}

/// Reads the functor set of a tuple type's element, panicking on a mismatch.
fn tuple_element_functors(ty: &Ty, index: usize) -> FunctorSet {
    let Ty::Tuple(items) = ty else {
        panic!("expected a tuple type, found {ty:?}");
    };
    match &items[index] {
        Ty::Arrow(arrow) => arrow.functors,
        other => panic!("expected element {index} to be an arrow, found {other:?}"),
    }
}

/// One hand-built closure-dispatch site.
///
/// Models the reduced shape that motivated the repair: a specialized target
/// whose input is `(op, tag, qs)` reached through a dispatch branch that still
/// passes only the scalar `qs` payload, with `op` and `tag` held as closure
/// captures. The `op` capture carries a richer functor set than the target
/// slot declares.
struct DispatchLayoutFixture {
    package: Package,
    assigner: Assigner,
    destination: CaptureScope,
    /// The dispatch branch's argument expression, initially the scalar payload.
    args_id: ExprId,
    captures: Vec<CapturedVar>,
    /// The specialization target's declared input type.
    target_input: Ty,
    /// The captured operation's local, declared with its runtime functors.
    op_capture: LocalVarId,
    /// The captured `Int`'s literal value, unique per fixture.
    tag_value: i64,
    /// The local referenced by the scalar payload argument.
    payload_local: LocalVarId,
}

/// Builds a dispatch site whose captures are an operation and an `Int` tag,
/// and whose original argument is a bare `Qubit[]` payload.
///
/// The tag capture carries a recorded initializer so the two capture
/// allocation paths in [`allocate_capture_exprs`] — reused initializer and
/// synthesized `Var` reference — are both exercised. `op_capture_id`,
/// `payload_local_id`, and `tag_value` distinguish otherwise identical
/// fixtures without relying on generated IDs.
fn make_dispatch_layout_fixture(
    op_capture_id: u32,
    payload_local_id: u32,
    tag_value: i64,
    capture_op_ty: Ty,
    capture_tag_ty: Ty,
    target_input: Ty,
) -> DispatchLayoutFixture {
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let span = package.synthetic_span();
    let destination = CaptureScope::CloneScope(LocalItemId::from(0usize));

    let op_capture = LocalVarId::from(op_capture_id);
    let payload_local = LocalVarId::from(payload_local_id);

    let tag_expr = alloc_int_lit(&mut package, &mut assigner, tag_value, span);
    let args_id = alloc_local_var_expr(
        &mut package,
        &mut assigner,
        payload_local,
        qubit_array_ty(),
        span,
    );

    let captures = vec![
        CapturedVar {
            local: ScopedLocal::new(op_capture, destination),
            ty: capture_op_ty,
            expr: None,
            caller_substitutions: Vec::new(),
        },
        CapturedVar {
            local: ScopedLocal::new(LocalVarId::from(op_capture_id + 1), destination),
            ty: capture_tag_ty,
            expr: Some(tag_expr),
            caller_substitutions: Vec::new(),
        },
    ];

    DispatchLayoutFixture {
        package,
        assigner,
        destination,
        args_id,
        captures,
        target_input,
        op_capture,
        tag_value,
        payload_local,
    }
}

/// Builds the canonical fixture: a `CtlAdj` operation capture against a target
/// slot that requires only `Empty`.
fn make_ctladj_into_empty_fixture(
    op_capture_id: u32,
    payload_local_id: u32,
    tag_value: i64,
) -> DispatchLayoutFixture {
    make_dispatch_layout_fixture(
        op_capture_id,
        payload_local_id,
        tag_value,
        qubit_array_op_ty(FunctorSetValue::CtlAdj),
        Ty::Prim(Prim::Int),
        target_input_ty(
            qubit_array_op_ty(FunctorSetValue::Empty),
            Ty::Prim(Prim::Int),
        ),
    )
}

/// Wraps the fixture's dispatch argument in a direct call to a target-typed
/// callee, mirroring the specialized call site the rewrite must repair.
///
/// Returns `(call_id, callee_id)`.
fn add_direct_target_call(fixture: &mut DispatchLayoutFixture) -> (ExprId, ExprId) {
    let span = fixture.package.synthetic_span();
    let callee_ty = arrow_ty(
        CallableKind::Operation,
        fixture.target_input.clone(),
        Ty::UNIT,
        FunctorSetValue::Empty,
    );
    let callee_id = alloc_local_var_expr(
        &mut fixture.package,
        &mut fixture.assigner,
        LocalVarId::from(0_u32),
        callee_ty,
        span,
    );
    let call_id = alloc_call_expr(
        &mut fixture.package,
        &mut fixture.assigner,
        callee_id,
        fixture.args_id,
        Ty::UNIT,
        span,
    );

    (call_id, callee_id)
}

/// Runs the dispatch-argument builder and asserts the grouped layout
/// `(op, tag, payload)` was produced with the target's declared type.
///
/// Returns the constructed tuple's element IDs so callers can add
/// case-specific checks.
fn build_and_assert_grouped_dispatch_layout(fixture: &mut DispatchLayoutFixture) -> Vec<ExprId> {
    let original_args_id = fixture.args_id;
    let Some((kind, ty)) = build_closure_dispatch_branch_args_data(
        &mut fixture.package,
        fixture.destination,
        original_args_id,
        &fixture.captures,
        &fixture.target_input,
        &mut fixture.assigner,
    ) else {
        panic!(
            "capability-compatible captures should produce a grouped dispatch layout for target \
             input {:?}",
            fixture.target_input,
        );
    };

    assert_eq!(
        ty, fixture.target_input,
        "the constructed tuple must be declared with the target's input type",
    );

    let ExprKind::Tuple(elements) = kind else {
        panic!("grouped dispatch layout should be a tuple, found {kind:?}");
    };
    assert_eq!(
        elements.len(),
        3,
        "grouped layout must place both captures ahead of the single payload element",
    );

    let op_element = elements[0];
    assert!(
        matches!(
            &fixture.package.get_expr(op_element).kind,
            ExprKind::Var(Res::Local(var), _) if *var == fixture.op_capture,
        ),
        "first element must reference the captured operation local",
    );
    assert_eq!(
        fixture.package.get_expr(op_element).ty,
        fixture.captures[0].ty,
        "the captured operation expression must keep its runtime type, not the target's \
         requirement",
    );

    let tag_element = elements[1];
    assert!(
        matches!(
            &fixture.package.get_expr(tag_element).kind,
            ExprKind::Lit(Lit::Int(value)) if *value == fixture.tag_value,
        ),
        "second element must reuse this fixture's recorded tag initializer",
    );

    let payload_element = elements[2];
    assert_ne!(
        payload_element, original_args_id,
        "the payload must be copied into a fresh node so the rewritten args expression can be \
         overwritten in place",
    );
    assert!(
        matches!(
            &fixture.package.get_expr(payload_element).kind,
            ExprKind::Var(Res::Local(var), _) if *var == fixture.payload_local,
        ),
        "third element must preserve the original scalar payload",
    );
    assert_eq!(
        fixture.package.get_expr(payload_element).ty,
        qubit_array_ty(),
        "the payload's own type must be unchanged by the layout rewrite",
    );

    elements
}

#[test]
fn dispatch_layout_groups_ctladj_capture_into_empty_target_slot() {
    // Two independent fixtures with distinct capture locals, distinct payload
    // locals, and distinct tag literals. Building both proves the matcher
    // reshapes each site from its own captures rather than conflating them.
    let mut first = make_ctladj_into_empty_fixture(10, 12, 7);
    let mut second = make_ctladj_into_empty_fixture(20, 22, 9);

    let first_elements = build_and_assert_grouped_dispatch_layout(&mut first);
    let second_elements = build_and_assert_grouped_dispatch_layout(&mut second);

    for (fixture, elements) in [(&first, &first_elements), (&second, &second_elements)] {
        assert_eq!(
            expr_functors(&fixture.package, elements[0]),
            FunctorSet::Value(FunctorSetValue::CtlAdj),
            "the captured operation must still be `Adj + Ctl` after the layout is built",
        );
        assert_eq!(
            tuple_element_functors(&fixture.target_input, 0),
            FunctorSet::Value(FunctorSetValue::Empty),
            "the target slot must still declare only the capability its body requires",
        );
    }
}

#[test]
fn callable_and_clone_scope_collision_declines_capture_write() {
    let mut fixture = make_ctladj_into_empty_fixture(10, 12, 7);
    let destination_capture = fixture.captures[0].clone();
    fixture.captures[0].local.scope = CaptureScope::Callable(LocalItemId::from(0usize));

    assert_ne!(
        fixture.captures[0], destination_capture,
        "callable and clone domains with the same integer id must remain distinct",
    );

    let before = fixture.package.get_expr(fixture.args_id).clone();
    rewrite_closure_dispatch_branch_args(
        &mut fixture.package,
        fixture.destination,
        fixture.args_id,
        &fixture.captures,
        &fixture.target_input,
        0,
        &mut fixture.assigner,
    );
    let after = fixture.package.get_expr(fixture.args_id);

    assert_eq!(
        after.kind, before.kind,
        "a numerically colliding capture from another callable must not be written",
    );
    assert_eq!(
        after.ty, before.ty,
        "a declined write must preserve its type"
    );
}

#[test]
fn dispatch_layout_capability_control_proves_grouped_capture_rewrite_causality() {
    // With capability matching disabled, the `CtlAdj` capture cannot populate
    // the `Empty` slot, so no layout is produced for either fixture.
    {
        let _guard = CapabilityMatchingGuard::disable();
        for (op_capture_id, payload_local_id, tag_value) in [(10, 12, 7), (20, 22, 9)] {
            let mut fixture =
                make_ctladj_into_empty_fixture(op_capture_id, payload_local_id, tag_value);
            let built = build_closure_dispatch_branch_args_data(
                &mut fixture.package,
                fixture.destination,
                fixture.args_id,
                &fixture.captures,
                &fixture.target_input,
                &mut fixture.assigner,
            );
            assert!(
                built.is_none(),
                "exact functor equality must reject the capture layout, leaving the scalar \
                 payload in place",
            );
        }
    }

    // The rejected layout is not a benign no-op: the surrounding direct call
    // still passes the bare payload to a callee expecting the full tuple,
    // which is exactly what the `PostArgPromote` call-shape check forbids.
    let mut fixture = make_ctladj_into_empty_fixture(10, 12, 7);
    let (call_id, callee_id) = add_direct_target_call(&mut fixture);
    {
        let _guard = CapabilityMatchingGuard::disable();
        rewrite_closure_dispatch_branch_args(
            &mut fixture.package,
            fixture.destination,
            fixture.args_id,
            &fixture.captures,
            &fixture.target_input,
            0,
            &mut fixture.assigner,
        );
    }
    assert_eq!(
        fixture.package.get_expr(fixture.args_id).ty,
        qubit_array_ty(),
        "the disabled control must leave the malformed scalar argument untouched",
    );

    // The callee is an arrow-typed local, so signature resolution reads the
    // callee expression's own type and never consults the store.
    let store = PackageStore::new();
    assert_panics_with("PostArgPromote/PostAll call invariant violation", || {
        crate::invariants::check_call_shape_matches_callee(
            &store,
            &fixture.package,
            call_id,
            callee_id,
            fixture.args_id,
        );
    });

    // With capability matching enabled, the same rewrite repairs the call in
    // place and the identical check now passes.
    let mut repaired = make_ctladj_into_empty_fixture(10, 12, 7);
    let (repaired_call_id, repaired_callee_id) = add_direct_target_call(&mut repaired);
    rewrite_closure_dispatch_branch_args(
        &mut repaired.package,
        repaired.destination,
        repaired.args_id,
        &repaired.captures,
        &repaired.target_input,
        0,
        &mut repaired.assigner,
    );
    assert_eq!(
        repaired.package.get_expr(repaired.args_id).ty,
        repaired.target_input,
        "the repaired argument must adopt the target's declared input type",
    );
    assert!(
        matches!(
            &repaired.package.get_expr(repaired.args_id).kind,
            ExprKind::Tuple(elements) if elements.len() == 3,
        ),
        "the repaired argument must carry both captures ahead of the payload",
    );

    let repaired_store = PackageStore::new();
    crate::invariants::check_call_shape_matches_callee(
        &repaired_store,
        &repaired.package,
        repaired_call_id,
        repaired_callee_id,
        repaired.args_id,
    );
}

#[test]
fn dispatch_layout_rejects_callable_input_mismatch() {
    // Capability matching relaxes callable functors and nothing else. Each
    // case differs from the target in exactly one non-functor dimension while
    // keeping a functor set that would otherwise be accepted.
    let cases: Vec<(&str, Ty, Ty, Ty)> = vec![
        (
            "callable input shape",
            arrow_ty(
                CallableKind::Operation,
                Ty::Prim(Prim::Qubit),
                Ty::UNIT,
                FunctorSetValue::CtlAdj,
            ),
            Ty::Prim(Prim::Int),
            target_input_ty(
                qubit_array_op_ty(FunctorSetValue::Empty),
                Ty::Prim(Prim::Int),
            ),
        ),
        (
            "callable output type",
            arrow_ty(
                CallableKind::Operation,
                qubit_array_ty(),
                Ty::Prim(Prim::Int),
                FunctorSetValue::CtlAdj,
            ),
            Ty::Prim(Prim::Int),
            target_input_ty(
                qubit_array_op_ty(FunctorSetValue::Empty),
                Ty::Prim(Prim::Int),
            ),
        ),
        (
            "callable kind",
            arrow_ty(
                CallableKind::Function,
                qubit_array_ty(),
                Ty::UNIT,
                FunctorSetValue::CtlAdj,
            ),
            Ty::Prim(Prim::Int),
            target_input_ty(
                qubit_array_op_ty(FunctorSetValue::Empty),
                Ty::Prim(Prim::Int),
            ),
        ),
        (
            "non-callable capture field",
            qubit_array_op_ty(FunctorSetValue::CtlAdj),
            Ty::Prim(Prim::Double),
            target_input_ty(
                qubit_array_op_ty(FunctorSetValue::Empty),
                Ty::Prim(Prim::Int),
            ),
        ),
        (
            "tuple arity",
            qubit_array_op_ty(FunctorSetValue::CtlAdj),
            Ty::Prim(Prim::Int),
            Ty::Tuple(vec![
                qubit_array_op_ty(FunctorSetValue::Empty),
                Ty::Prim(Prim::Int),
                qubit_array_ty(),
                Ty::Prim(Prim::Int),
            ]),
        ),
        (
            "unsatisfied functor requirement",
            qubit_array_op_ty(FunctorSetValue::Adj),
            Ty::Prim(Prim::Int),
            target_input_ty(qubit_array_op_ty(FunctorSetValue::Ctl), Ty::Prim(Prim::Int)),
        ),
    ];

    for (label, capture_op_ty, capture_tag_ty, target_input) in cases {
        let mut fixture = make_dispatch_layout_fixture(
            10,
            12,
            7,
            capture_op_ty,
            capture_tag_ty,
            target_input.clone(),
        );
        let built = build_closure_dispatch_branch_args_data(
            &mut fixture.package,
            fixture.destination,
            fixture.args_id,
            &fixture.captures,
            &fixture.target_input,
            &mut fixture.assigner,
        );
        assert!(
            built.is_none(),
            "a {label} mismatch must remain incompatible: capability matching applies to functor \
             sets only",
        );
    }
}

#[test]
fn dispatch_functors_follow_capability_not_equality() {
    let value = FunctorSet::Value;
    let satisfied = [
        (FunctorSetValue::Empty, FunctorSetValue::Empty),
        (FunctorSetValue::Adj, FunctorSetValue::Empty),
        (FunctorSetValue::Ctl, FunctorSetValue::Empty),
        (FunctorSetValue::CtlAdj, FunctorSetValue::Empty),
        (FunctorSetValue::Adj, FunctorSetValue::Adj),
        (FunctorSetValue::Ctl, FunctorSetValue::Ctl),
        (FunctorSetValue::CtlAdj, FunctorSetValue::Adj),
        (FunctorSetValue::CtlAdj, FunctorSetValue::Ctl),
        (FunctorSetValue::CtlAdj, FunctorSetValue::CtlAdj),
    ];
    for (actual, expected) in satisfied {
        assert!(
            dispatch_functors_compatible(value(actual), value(expected)),
            "{actual:?} should satisfy a requirement of {expected:?}",
        );
    }

    let unsatisfied = [
        (FunctorSetValue::Empty, FunctorSetValue::Adj),
        (FunctorSetValue::Empty, FunctorSetValue::Ctl),
        (FunctorSetValue::Empty, FunctorSetValue::CtlAdj),
        (FunctorSetValue::Adj, FunctorSetValue::Ctl),
        (FunctorSetValue::Adj, FunctorSetValue::CtlAdj),
        (FunctorSetValue::Ctl, FunctorSetValue::Adj),
        (FunctorSetValue::Ctl, FunctorSetValue::CtlAdj),
    ];
    for (actual, expected) in unsatisfied {
        assert!(
            !dispatch_functors_compatible(value(actual), value(expected)),
            "{actual:?} should not satisfy a requirement of {expected:?}",
        );
    }
}
