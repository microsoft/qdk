// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::PipelineStage;
use crate::test_utils::compile_and_run_pipeline_to;

// A higher-order operation that applies `Controlled` twice to its callable
// parameter. After defunctionalization the `op` parameter is replaced by the
// concrete `Foo`, so the body's `Controlled Controlled op(...)` callee is
// rebuilt by `alloc_functor_wrapped_expr`/`wrap_in_functors` into a two-layer
// `Ctl(Ctl(Var(Foo)))` wrapper.
const NESTED_CONTROLLED_HOF: &str = r"
    operation Foo(q : Qubit) : Unit is Ctl {
        X(q);
    }
    operation ApplyControlledTwice(op : Qubit => Unit is Ctl, cs1 : Qubit[], cs2 : Qubit[], target : Qubit) : Unit {
        Controlled Controlled op(cs1, (cs2, target));
    }
    operation Main() : Unit {
        use cs1 = Qubit();
        use cs2 = Qubit();
        use target = Qubit();
        ApplyControlledTwice(Foo, [cs1], [cs2], target);
    }
";

// Each `Controlled` wraps a callable's input in one more `(Qubit[], _)`
// register and leaves its output unchanged, so a `Controlled Controlled Foo`
// callee has a distinct arrow type at each of its three levels:
//
//     base (Var Foo)   Qubit => Unit
//     inner Ctl        (Qubit[], Qubit) => Unit
//     outer Ctl        (Qubit[], (Qubit[], Qubit)) => Unit
//
// This test peels the synthesized chain level by level and checks that each
// `Controlled` layer adds exactly one register over the level beneath it. The
// `Defunc` stage is used because that is where the chain is synthesized; later
// passes normalize call-argument types and would mask the defect.
#[test]
fn nested_controlled_callee_adds_one_control_layer_per_wrapper() {
    let (store, pkg_id) = compile_and_run_pipeline_to(NESTED_CONTROLLED_HOF, PipelineStage::Defunc);
    let package = store.get(pkg_id);

    // Locate the synthesized `Ctl(Ctl(Var(Foo)))` callee and peel it level by level.
    let outer_ctl = find_nested_controlled_item_callee(package)
        .expect("defunctionalization should synthesize a `Controlled Controlled Foo` callee");
    let inner_ctl = expect_controlled_wrapper(package, outer_ctl);
    let base = expect_controlled_wrapper(package, inner_ctl);

    // The base is a direct reference to the concrete item, carrying Foo's
    // un-controlled `Qubit => Unit` signature.
    let base_expr = package.get_expr(base);
    assert!(
        matches!(base_expr.kind, ExprKind::Var(Res::Item(_), _)),
        "base callee should be a direct item reference to Foo"
    );
    let Ty::Arrow(base_arrow) = &base_expr.ty else {
        panic!(
            "base callee should be arrow-typed, found {:?}",
            base_expr.ty
        );
    };
    assert_eq!(
        *base_arrow.input,
        Ty::Prim(Prim::Qubit),
        "Foo takes a single Qubit"
    );
    assert_eq!(*base_arrow.output, Ty::UNIT, "Foo returns Unit");

    // Each enclosing `Controlled` adds exactly one `(Qubit[], _)` register.
    let base_ty = base_expr.ty.clone();
    let expected_inner_ty = add_control_layer(&base_ty);
    let expected_outer_ty = add_control_layer(&expected_inner_ty);

    assert_eq!(
        package.get_expr(inner_ctl).ty,
        expected_inner_ty,
        "inner Controlled node must add one control layer over the base"
    );
    assert_eq!(
        package.get_expr(outer_ctl).ty,
        expected_outer_ty,
        "outer Controlled node must add a second control layer"
    );
}

/// Finds the outermost node of a `Ctl(Ctl(Var(Res::Item)))` callee chain — the
/// shape defunctionalization synthesizes for a twice-controlled item call.
fn find_nested_controlled_item_callee(package: &Package) -> Option<ExprId> {
    package.exprs.iter().find_map(|(id, expr)| {
        let inner_ctl = controlled_wrapper_inner(expr)?;
        let base = controlled_wrapper_inner(package.get_expr(inner_ctl))?;
        matches!(package.get_expr(base).kind, ExprKind::Var(Res::Item(_), _)).then_some(id)
    })
}

/// Returns the wrapped inner expr id when `expr` is a `Controlled` functor
/// wrapper, otherwise `None`.
fn controlled_wrapper_inner(expr: &Expr) -> Option<ExprId> {
    match &expr.kind {
        ExprKind::UnOp(UnOp::Functor(Functor::Ctl), inner) => Some(*inner),
        _ => None,
    }
}

/// Asserts the expression at `id` is a `Controlled` functor wrapper and returns
/// its inner expr id.
fn expect_controlled_wrapper(package: &Package, id: ExprId) -> ExprId {
    match controlled_wrapper_inner(package.get_expr(id)) {
        Some(inner) => inner,
        None => panic!("expected a Controlled functor wrapper at {id:?}"),
    }
}

/// Adds one `Controlled` input layer to an arrow type: `I => O` becomes
/// `(Qubit[], I) => O`. This is the forward mirror of the production
/// `strip_controlled_input_layer` helper, written independently so the test
/// does not lean on the code it validates.
fn add_control_layer(ty: &Ty) -> Ty {
    let Ty::Arrow(arrow) = ty else {
        panic!("expected an arrow type, found {ty:?}");
    };
    Ty::Arrow(Box::new(Arrow {
        kind: arrow.kind,
        input: Box::new(Ty::Tuple(vec![
            Ty::Array(Box::new(Ty::Prim(Prim::Qubit))),
            (*arrow.input).clone(),
        ])),
        output: arrow.output.clone(),
        functors: arrow.functors,
    }))
}
