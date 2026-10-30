// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{TypeArgs, TypeArgsError, TypeBudget};
use qsc_fir::{
    fir::StoreItemId,
    ty::{
        ClassConstraints, FunctorSet, FunctorSetValue, GenericArg, MAX_TYPE_DEPTH, MAX_TYPE_NODES,
        ParamId, Prim, Ty, TypeParameter,
    },
};
use std::rc::Rc;

fn owner(item: usize) -> StoreItemId {
    StoreItemId {
        package: 1usize.into(),
        item: item.into(),
    }
}

#[test]
fn checked_empty_instantiation_is_not_an_unbound_declaration() {
    assert_ne!(
        TypeArgs::bind(owner(0), Vec::new(), TypeArgs::Unbound),
        TypeArgs::Unbound
    );
    assert_eq!(
        TypeArgs::Empty.resolve_argument(ParamId::from(0), &mut TypeBudget::default()),
        Err(TypeArgsError::Missing(ParamId::from(0))),
    );
}

#[test]
fn identical_parameter_numbers_belong_to_distinct_declarations() {
    let args = vec![GenericArg::Ty(Ty::Prim(Prim::Int))];
    assert_ne!(
        TypeArgs::bind(owner(0), args.clone(), TypeArgs::Empty),
        TypeArgs::bind(owner(1), args, TypeArgs::Empty),
    );
}

#[test]
fn recursive_type_arguments_keep_shared_unexpanded_templates() {
    let parameter = Ty::Param(ParamId::from(0));
    let template = Ty::Tuple(vec![parameter.clone(), parameter]);
    let mut context = TypeArgs::bind(
        owner(0),
        vec![GenericArg::Ty(Ty::Prim(Prim::Int))],
        TypeArgs::Empty,
    );
    for _ in 0..30 {
        let previous = context.clone();
        context = TypeArgs::bind(owner(0), vec![GenericArg::Ty(template.clone())], context);
        let (TypeArgs::Bound(current), TypeArgs::Bound(previous)) = (&context, previous) else {
            panic!("expected bindings");
        };
        let TypeArgs::Bound(parent) = &current.context else {
            panic!("expected shared parent");
        };
        assert!(Rc::ptr_eq(parent, &previous));
        assert_eq!(current.args.as_ref(), &[GenericArg::Ty(template.clone())]);
    }
}

#[test]
fn escaping_instantiation_retains_only_its_shared_type_context() {
    let context = TypeArgs::bind(
        owner(0),
        vec![GenericArg::Ty(Ty::Prim(Prim::Int))],
        TypeArgs::Empty,
    );
    let TypeArgs::Bound(binding) = &context else {
        panic!("expected binding");
    };
    let weak = Rc::downgrade(binding);
    let escaped = context.clone();
    drop(context);
    assert!(weak.upgrade().is_some());
    drop(escaped);
    assert!(weak.upgrade().is_none());
}

#[test]
fn native_functor_arguments_use_declared_bounds() {
    let TypeArgs::Bound(binding) =
        TypeArgs::from_decl(owner(0), &[TypeParameter::Functor(FunctorSetValue::Adj)])
    else {
        panic!("expected functor binding");
    };
    assert_eq!(
        binding.args.as_ref(),
        &[GenericArg::Functor(FunctorSet::Value(FunctorSetValue::Adj))]
    );
    assert_eq!(
        TypeArgs::Bound(binding).resolve_functors(FunctorSet::Param(ParamId::from(0))),
        Ok(FunctorSet::Value(FunctorSetValue::Adj)),
    );
}

#[test]
fn native_type_generic_declaration_stays_unbound() {
    assert_eq!(
        TypeArgs::from_decl(
            owner(0),
            &[TypeParameter::Ty {
                name: "T".into(),
                bounds: ClassConstraints::default(),
            }],
        ),
        TypeArgs::Unbound,
    );
}

#[test]
fn type_resolution_rejects_a_functor_binding() {
    let args = TypeArgs::from_decl(owner(0), &[TypeParameter::Functor(FunctorSetValue::Adj)]);
    assert_eq!(
        args.resolve_ty(&Ty::Param(ParamId::from(0)), &mut TypeBudget::default()),
        Err(TypeArgsError::Kind(ParamId::from(0))),
    );
}

#[test]
fn type_resolution_enforces_the_exact_node_limit() {
    let at_limit = Ty::Tuple(vec![Ty::Prim(Prim::Int); MAX_TYPE_NODES - 1]);
    assert_eq!(
        TypeArgs::Empty.resolve_ty(&at_limit, &mut TypeBudget::default()),
        Ok(at_limit.clone()),
    );
    let mut shared_budget = TypeBudget::default();
    TypeArgs::Empty
        .resolve_ty(&at_limit, &mut shared_budget)
        .expect("the exact limit should resolve");
    assert_eq!(
        TypeArgs::Empty.resolve_ty(&Ty::Prim(Prim::Int), &mut shared_budget),
        Err(TypeArgsError::ExpansionLimit),
    );
    let over_limit = Ty::Tuple(vec![Ty::Prim(Prim::Int); MAX_TYPE_NODES]);
    assert_eq!(
        TypeArgs::Empty.resolve_ty(&over_limit, &mut TypeBudget::default()),
        Err(TypeArgsError::ExpansionLimit),
    );
}

#[test]
fn type_resolution_enforces_the_exact_depth_limit() {
    let mut at_limit = Ty::Prim(Prim::Int);
    for _ in 1..MAX_TYPE_DEPTH {
        at_limit = Ty::Array(Box::new(at_limit));
    }
    assert_eq!(
        TypeArgs::Empty.resolve_ty(&at_limit, &mut TypeBudget::default()),
        Ok(at_limit.clone()),
    );
    assert_eq!(
        TypeArgs::Empty.resolve_ty(&Ty::Array(Box::new(at_limit)), &mut TypeBudget::default()),
        Err(TypeArgsError::ExpansionLimit),
    );
}
