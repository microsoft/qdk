// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_eval::{
    Variable,
    callable::TypeArgs,
    val::{Closure, Value, Var, VarTy},
};
use qsc_fir::fir::{LocalVarId, PackageId};
use qsc_rir::rir::Literal;
use std::rc::Rc;

use crate::evaluation_context::{Arg, Scope};

/// Creates the paired classical and hybrid bindings used by partial evaluation.
fn scope_with_array() -> Scope {
    let mut values = Vec::with_capacity(4);
    values.push(Value::Int(1));
    scope_with_value(Value::Array(Rc::new(values)))
}

fn scope_with_value(value: Value) -> Scope {
    Scope::new(
        PackageId::CORE,
        None,
        vec![Arg::Var(
            0_usize.into(),
            Variable {
                name: "values".into(),
                value,
                span: Default::default(),
            },
        )],
        None,
        false,
        Vec::new(),
    )
}

fn scope_with_tracked_double(value: f64) -> Scope {
    let mut scope = scope_with_value(Value::Double(value));
    scope.update_hybrid_local_value(
        0_usize.into(),
        Value::Var(Var {
            id: 0,
            ty: VarTy::Double,
        }),
    );
    scope.insert_static_var_mapping(0_usize.into(), Literal::Double(value));
    scope
}

fn collect_updates(mut scope: Scope, value: Value) -> Vec<(LocalVarId, Value)> {
    scope
        .env
        .update_variable_in_top_frame(0_usize.into(), value);
    scope.insert_mutable_var(0_usize.into());
    scope.collect_updated_local_values()
}

fn single_update(scope: Scope, value: Value) -> Value {
    let updates = collect_updates(scope, value);
    let [(id, actual)] = updates.as_slice() else {
        panic!("expected one local update, got {updates:?}");
    };
    assert_eq!(*id, 0_usize.into());
    actual.clone()
}

fn assert_double_update(scope: Scope, expected: f64) {
    let actual = single_update(scope, Value::Double(expected)).unwrap_double();
    assert_eq!(actual.to_bits(), expected.to_bits());
}

fn closure_with_value(value: Value) -> Value {
    Value::Closure(Box::new(Closure {
        fixed_args: vec![value].into(),
        id: (PackageId::CORE, 0_usize.into()).into(),
        functor: Default::default(),
        type_args: TypeArgs::Empty,
    }))
}

#[test]
fn collecting_updates_detects_positive_to_negative_zero() {
    assert_double_update(scope_with_value(Value::Double(0.0)), -0.0);
}

#[test]
fn collecting_updates_detects_negative_to_positive_zero() {
    assert_double_update(scope_with_value(Value::Double(-0.0)), 0.0);
}

#[test]
fn collecting_updates_detects_tracked_positive_to_negative_zero() {
    assert_double_update(scope_with_tracked_double(0.0), -0.0);
}

#[test]
fn collecting_updates_detects_tracked_negative_to_positive_zero() {
    assert_double_update(scope_with_tracked_double(-0.0), 0.0);
}

#[test]
fn collecting_updates_reports_array_element_changed_to_negative_zero() {
    let scope = scope_with_value(Value::Array(vec![Value::Double(0.0)].into()));
    let values =
        single_update(scope, Value::Array(vec![Value::Double(-0.0)].into())).unwrap_array();
    let [Value::Double(value)] = values.as_slice() else {
        panic!("expected one Double array element, got {values:?}");
    };
    assert_eq!(value.to_bits(), (-0.0_f64).to_bits());
}

#[test]
fn collecting_updates_reports_tuple_field_changed_to_negative_zero() {
    let scope = scope_with_value(Value::Tuple(vec![Value::Double(0.0)].into(), None));
    let actual = single_update(scope, Value::Tuple(vec![Value::Double(-0.0)].into(), None));
    let Value::Tuple(fields, None) = actual else {
        panic!("expected an untyped tuple, got {actual:?}");
    };
    let [Value::Double(value)] = fields.as_ref() else {
        panic!("expected one Double tuple field, got {fields:?}");
    };
    assert_eq!(value.to_bits(), (-0.0_f64).to_bits());
}

#[test]
fn collecting_updates_reports_nested_tuple_field_changed_to_negative_zero() {
    let scope = scope_with_value(Value::Array(
        vec![Value::Tuple(vec![Value::Double(0.0)].into(), None)].into(),
    ));
    let values = single_update(
        scope,
        Value::Array(vec![Value::Tuple(vec![Value::Double(-0.0)].into(), None)].into()),
    )
    .unwrap_array();
    let [Value::Tuple(fields, None)] = values.as_slice() else {
        panic!("expected one untyped tuple array element, got {values:?}");
    };
    let [Value::Double(value)] = fields.as_ref() else {
        panic!("expected one Double tuple field, got {fields:?}");
    };
    assert_eq!(value.to_bits(), (-0.0_f64).to_bits());
}

#[test]
fn collecting_updates_reports_closure_capture_changed_to_negative_zero() {
    let scope = scope_with_value(closure_with_value(Value::Double(0.0)));
    let actual = single_update(scope, closure_with_value(Value::Double(-0.0)));
    let Value::Closure(closure) = actual else {
        panic!("expected a closure, got {actual:?}");
    };
    let [Value::Double(value)] = closure.fixed_args.as_ref() else {
        panic!("expected one Double closure capture, got {closure:?}");
    };
    assert_eq!(value.to_bits(), (-0.0_f64).to_bits());
}

#[test]
fn collecting_updates_reports_array_growth_with_unchanged_prefix() {
    let scope = scope_with_value(Value::Array(vec![Value::Int(1)].into()));
    let expected = Value::Array(vec![Value::Int(1), Value::Int(2)].into());
    assert_eq!(single_update(scope, expected.clone()), expected);
}

#[test]
fn collecting_updates_reports_array_shrinkage_with_unchanged_prefix() {
    let scope = scope_with_value(Value::Array(vec![Value::Int(1), Value::Int(2)].into()));
    let expected = Value::Array(vec![Value::Int(1)].into());
    assert_eq!(single_update(scope, expected.clone()), expected);
}

#[test]
fn collecting_updates_reports_closure_target_change_with_shared_captures() {
    let Value::Closure(mut closure) = closure_with_value(Value::Int(1)) else {
        unreachable!();
    };
    let scope = scope_with_value(Value::Closure(closure.clone()));
    closure.id = (PackageId::CORE, 1_usize.into()).into();
    let expected = Value::Closure(closure);
    assert_eq!(single_update(scope, expected.clone()), expected);
}

#[test]
fn collecting_updates_reports_closure_adjoint_change_with_shared_captures() {
    let Value::Closure(mut closure) = closure_with_value(Value::Int(1)) else {
        unreachable!();
    };
    let scope = scope_with_value(Value::Closure(closure.clone()));
    closure.functor.adjoint = true;
    let expected = Value::Closure(closure);
    assert_eq!(single_update(scope, expected.clone()), expected);
}

#[test]
fn collecting_updates_reports_closure_controlled_count_change_with_shared_captures() {
    let Value::Closure(mut closure) = closure_with_value(Value::Int(1)) else {
        unreachable!();
    };
    let scope = scope_with_value(Value::Closure(closure.clone()));
    closure.functor.controlled = 1;
    let expected = Value::Closure(closure);
    assert_eq!(single_update(scope, expected.clone()), expected);
}

#[test]
fn collecting_updates_reports_closure_capture_count_change_with_unchanged_prefix() {
    let Value::Closure(mut closure) = closure_with_value(Value::Int(1)) else {
        unreachable!();
    };
    let scope = scope_with_value(Value::Closure(closure.clone()));
    closure.fixed_args = vec![Value::Int(1), Value::Int(2)].into();
    let expected = Value::Closure(closure);
    assert_eq!(single_update(scope, expected.clone()), expected);
}

#[test]
fn collecting_updates_skips_unchanged_nan() {
    let scope = scope_with_value(Value::Double(f64::NAN));
    assert!(collect_updates(scope, Value::Double(f64::NAN)).is_empty());
}

#[test]
fn collecting_updates_skips_unchanged_tracked_nan() {
    let scope = scope_with_tracked_double(f64::NAN);
    assert!(collect_updates(scope, Value::Double(f64::NAN)).is_empty());
}

#[test]
fn collecting_updates_detects_changed_nan_payload() {
    let scope = scope_with_value(Value::Double(f64::NAN));
    assert_double_update(scope, f64::from_bits(f64::NAN.to_bits() ^ 1));
}

#[test]
fn collecting_updates_skips_unchanged_nan_in_distinct_arrays() {
    let scope = scope_with_value(Value::Array(vec![Value::Double(f64::NAN)].into()));
    assert!(collect_updates(scope, Value::Array(vec![Value::Double(f64::NAN)].into())).is_empty());
}

#[test]
fn collecting_updates_detects_tuple_type_changes() {
    let values: Rc<[Value]> = vec![Value::Int(1)].into();
    let scope = scope_with_value(Value::Tuple(values.clone(), None));
    let type_id = (PackageId::CORE, 0_usize.into()).into();
    let expected = Value::Tuple(values, Some(Rc::new(type_id)));
    assert_eq!(single_update(scope, expected.clone()), expected);
}

#[test]
fn collecting_updates_detects_closure_type_argument_changes() {
    let value = closure_with_value(Value::Int(1));
    let scope = scope_with_value(value.clone());
    let Value::Closure(mut closure) = value else {
        unreachable!();
    };
    closure.type_args = TypeArgs::Unbound;
    let expected = Value::Closure(closure);
    assert_eq!(single_update(scope, expected.clone()), expected);
}

#[test]
/// Verifies that internal bindings do not force a copy or discard spare vector capacity.
fn taking_unaliased_array_reuses_storage() {
    let mut scope = scope_with_array();
    let original = scope
        .get_classical_local_value(0_usize.into())
        .clone()
        .unwrap_array()
        .as_ptr();
    let mut values = scope.take_local_array(0_usize.into());
    assert_eq!(Rc::strong_count(&values), 1);
    Rc::make_mut(&mut values).push(Value::Int(2));
    assert_eq!(values.as_ptr(), original);
}

#[test]
/// Verifies that a real Q# alias still triggers copy-on-write.
fn taking_aliased_array_preserves_alias() {
    let mut scope = scope_with_array();
    let alias = scope
        .get_classical_local_value(0_usize.into())
        .clone()
        .unwrap_array();
    let mut values = scope.take_local_array(0_usize.into());
    Rc::make_mut(&mut values).push(Value::Int(2));
    assert_eq!(&*alias, &[Value::Int(1)]);
    assert_eq!(&*values, &[Value::Int(1), Value::Int(2)]);
}

#[test]
/// Verifies that branch-local mutation preserves the classical and hybrid snapshots.
fn taking_array_in_dynamic_branch_preserves_bindings() {
    let mut scope = scope_with_array();
    scope.active_block_count += 1;
    let mut values = scope.take_local_array(0_usize.into());
    Rc::make_mut(&mut values).push(Value::Int(2));
    for binding in [
        scope.get_classical_local_value(0_usize.into()),
        scope.get_hybrid_local_value(0_usize.into()),
    ] {
        assert_eq!(binding, &Value::Array(vec![Value::Int(1)].into()));
    }
}

#[test]
/// Shared arrays are unchanged even when their elements do not compare equal to themselves.
fn collecting_updates_skips_identical_array_storage() {
    let mut scope = scope_with_array();
    let value = Value::Array(vec![Value::Double(f64::NAN)].into());
    scope.update_hybrid_local_value(0_usize.into(), value.clone());
    scope
        .env
        .update_variable_in_top_frame(0_usize.into(), value);
    scope.insert_mutable_var(0_usize.into());
    assert!(scope.collect_updated_local_values().is_empty());
}

#[test]
/// Arrays updated by the classical evaluator still propagate to the hybrid binding.
fn collecting_updates_reports_changed_array_contents() {
    let mut scope = scope_with_array();
    let value = Value::Array(vec![Value::Int(2)].into());
    scope
        .env
        .update_variable_in_top_frame(0_usize.into(), value.clone());
    scope.insert_mutable_var(0_usize.into());
    assert_eq!(
        scope.collect_updated_local_values(),
        vec![(0_usize.into(), value)]
    );
}
