// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Internal contracts for closure-dispatch argument layouts and capture scope.
//!
//! Negative fixtures start with compiled Q# and deliberately corrupt a
//! capture scope or layout type. Functor compatibility has a separate
//! type-level truth table.

use super::*;
use crate::test_utils::{callable_id_by_name, compile_to_monomorphized_fir};
use crate::walk_utils::collect_expr_ids_in_local_callables;
use qsc_fir::fir::CallableKind;

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

struct DispatchLayoutFixture {
    package: Package,
    assigner: Assigner,
    destination: CaptureScope,
    args_id: ExprId,
    captures: Vec<CapturedVar>,
    target_input: Ty,
}

#[allow(clippy::too_many_lines)]
fn make_dispatch_layout_fixture() -> DispatchLayoutFixture {
    let (store, package_id) = compile_to_monomorphized_fir(
        r#"
        operation Caller(op : Qubit[] => Unit is Adj + Ctl, tag : Int, payload : Qubit[]) : Unit {
            let closure = remaining => { if tag == 0 { op(remaining); } };
            closure(payload);
        }
        operation Idle(register : Qubit[]) : Unit is Adj + Ctl {}
        operation Main() : Unit {
            use register = Qubit[0];
            Caller(Idle, 7, register);
        }
        "#,
    );
    let mut package = store.get(package_id).clone();
    let mut assigner = Assigner::from_package(&package);
    let main = callable_id_by_name(&package, "Main");
    let caller_id = collect_expr_ids_in_local_callables(&package, &[main])
        .into_iter()
        .find_map(|id| {
            if let ExprKind::Var(Res::Item(item), _) = &package.get_expr(id).kind
                && item.package == package_id
                && let ItemKind::Callable(decl) = &package.get_item(item.item).kind
                && decl.name.name.starts_with("Caller")
            {
                Some(item.item)
            } else {
                None
            }
        })
        .expect("Main should reference a concrete Caller");
    let ItemKind::Callable(caller) = &package.get_item(caller_id).kind else {
        panic!("Caller should be callable");
    };
    let destination = CaptureScope::Callable(caller_id);
    let input = package.get_pat(caller.input);
    let caller_exprs = collect_expr_ids_in_local_callables(&package, &[caller_id]);
    let PatKind::Tuple(parameters) = &input.kind else {
        panic!("Caller should have tuple parameters");
    };
    let bindings: Vec<_> = parameters.iter().map(|id| package.get_pat(*id)).collect();
    let PatKind::Bind(payload) = &bindings[2].kind else {
        panic!("payload should be a binding");
    };
    let args_id = caller_exprs.iter().find_map(|id| {
        matches!(&package.get_expr(*id).kind, ExprKind::Var(Res::Local(local), _) if *local == payload.id)
            .then_some(*id)
    }).expect("Caller should reference its payload");
    let (capture_ids, target) = caller_exprs
        .iter()
        .find_map(|id| {
            if let ExprKind::Closure(captures, target) = &package.get_expr(*id).kind {
                Some((captures, *target))
            } else {
                None
            }
        })
        .expect("lambda should lower to a closure");
    assert_eq!(capture_ids.len(), 2);
    let captures: Vec<_> = capture_ids
        .iter()
        .map(|local| {
            let binding = bindings
                .iter()
                .find(|binding| matches!(&binding.kind, PatKind::Bind(ident) if ident.id == *local))
                .expect("closure should capture a Caller parameter");
            CapturedVar {
                local: ScopedLocal::new(*local, destination),
                ty: binding.ty.clone(),
                expr: None,
                caller_substitutions: Vec::new(),
            }
        })
        .collect();
    let ItemKind::Callable(target) = &package.get_item(target).kind else {
        panic!("closure target should be callable");
    };
    let target_input = package.get_pat(target.input).ty.clone();
    assert_eq!(captures[0].ty, qubit_array_op_ty(FunctorSetValue::CtlAdj));
    assert_eq!(captures[1].ty, Ty::Prim(Prim::Int));
    assert_eq!(
        target_input,
        target_input_ty(captures[0].ty.clone(), captures[1].ty.clone())
    );
    assert_eq!(package.get_expr(args_id).ty, qubit_array_ty());
    assert!(
        build_closure_dispatch_branch_args_data(
            &mut package,
            destination,
            args_id,
            &captures,
            &target_input,
            &mut assigner,
        )
        .is_some(),
        "source-derived layout should be valid before corruption"
    );

    DispatchLayoutFixture {
        package,
        assigner,
        destination,
        args_id,
        captures,
        target_input,
    }
}

#[test]
fn callable_and_clone_scope_collision_declines_capture_write() {
    let mut fixture = make_dispatch_layout_fixture();
    let destination_capture = fixture.captures[0].clone();
    let CaptureScope::Callable(owner) = fixture.destination else {
        panic!("source capture should belong to Caller");
    };
    fixture.captures[0].local.scope = CaptureScope::CloneScope(owner);

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
fn dispatch_layout_rejects_callable_input_mismatch() {
    let cases = [
        "callable input shape",
        "callable output type",
        "callable kind",
        "non-callable capture field",
        "tuple arity",
        "unsatisfied functor requirement",
    ];

    for label in cases {
        let mut fixture = make_dispatch_layout_fixture();
        let Ty::Arrow(captured_op) = &mut fixture.captures[0].ty else {
            panic!("source capture should be callable");
        };
        match label {
            "callable input shape" => *captured_op.input = Ty::Prim(Prim::Qubit),
            "callable output type" => *captured_op.output = Ty::Prim(Prim::Int),
            "callable kind" => captured_op.kind = CallableKind::Function,
            "non-callable capture field" => fixture.captures[1].ty = Ty::Prim(Prim::Double),
            "tuple arity" => {
                let Ty::Tuple(fields) = &mut fixture.target_input else {
                    panic!("source target should have tuple input");
                };
                fields.push(Ty::Prim(Prim::Int));
            }
            "unsatisfied functor requirement" => {
                captured_op.functors = FunctorSet::Value(FunctorSetValue::Adj);
            }
            _ => unreachable!("unknown mismatch dimension"),
        }
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
