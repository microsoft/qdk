// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use expect_test::expect;

#[test]
fn invariants_single_hof() {
    check_invariants(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
        }
        "#,
    );
}

#[test]
fn invariants_closure_with_captures() {
    check_invariants(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 1.0;
            ApplyOp(q1 => Rx(angle, q1), q);
        }
        "#,
    );
}

#[test]
fn invariants_functor_composition() {
    check_invariants(
        r#"
        operation ApplyAdj(op : Qubit => Unit is Adj, q : Qubit) : Unit {
            Adjoint op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyAdj(S, q);
        }
        "#,
    );
}

#[test]
fn error_dynamic_callable() {
    check_invariants(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            if true { set op = X; }
            ApplyOp(op, q);
        }
        "#,
    );
}

#[test]
fn branch_split_resolves_mutable_callable() {
    check_invariants(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable f = X;
            if true { set f = H; } else { set f = S; }
            ApplyOp(f, q);
        }
        "#,
    );
}

#[test]
fn branch_split_resolves_conditional_binding() {
    check_invariants(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let f = if true { H } else { X };
            ApplyOp(f, q);
        }
        "#,
    );
}

#[test]
fn error_returned_not_panicked() {
    let (mut store, package_id) = compile_to_monomorphized_fir(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            for _ in 0..3 { set op = X; }
            ApplyOp(op, q);
        }
        "#,
    );
    let mut assigner = qsc_fir::assigner::Assigner::from_package(store.get(package_id));
    let errors = defunctionalize(&mut store, package_id, &mut assigner);
    assert!(
        !errors.is_empty(),
        "expected errors to be returned, not a panic"
    );
}

#[test]
fn error_multiple_dynamic_sites_collected() {
    let (mut store, package_id) = compile_to_monomorphized_fir(
        r#"
        operation Apply1(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Apply2(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            mutable f = H;
            for _ in 0..3 { set f = X; }
            Apply1(f, q);
            mutable g = X;
            for _ in 0..3 { set g = H; }
            Apply2(g, q);
        }
        "#,
    );
    let mut assigner = qsc_fir::assigner::Assigner::from_package(store.get(package_id));
    let errors = defunctionalize(&mut store, package_id, &mut assigner);
    assert_eq!(
        errors.len(),
        2,
        "expected both dynamic callable sites to be collected"
    );
    for error in &errors {
        assert!(
            matches!(error, super::super::Error::DynamicCallable(_)),
            "expected DynamicCallable error, got {error:?}"
        );
        assert!(
            !error.to_string().is_empty(),
            "each error should have a display message"
        );
    }
}

#[test]
fn nested_hof_call_chain_passes_invariants() {
    check_invariants(
        r#"
        operation ApplyInner(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation ApplyOuter(op : Qubit => Unit, q : Qubit) : Unit {
            ApplyInner(op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOuter(H, q);
        }
        "#,
    );
}

#[test]
fn hof_inside_for_loop_passes_invariants() {
    check_invariants(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            for _ in 0..3 {
                ApplyOp(H, q);
            }
        }
        "#,
    );
}

#[test]
fn function_callable_argument_defunctionalizes() {
    check_invariants(
        r#"
        function ApplyFn(f : Int -> Int, x : Int) : Int {
            f(x)
        }
        function Double(x : Int) : Int { x * 2 }
        @EntryPoint()
        operation Main() : Unit {
            let _ = ApplyFn(Double, 5);
        }
        "#,
    );
}

#[test]
fn explicit_functor_specializations_defunctionalize() {
    check_invariants(
        r#"
        operation ApplyOp(op : Qubit => Unit is Adj + Ctl, q : Qubit) : Unit is Adj + Ctl {
            body ... { op(q); }
            adjoint ... { Adjoint op(q); }
            controlled (ctls, ...) { Controlled op(ctls, q); }
            controlled adjoint (ctls, ...) { Controlled Adjoint op(ctls, q); }
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(S, q);
        }
        "#,
    );
}

#[test]
fn full_pipeline_preserves_post_all_invariants() {
    check_pipeline(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
            ApplyOp(X, q);
            let angle = 1.0;
            ApplyOp(q1 => Rx(angle, q1), q);
        }
        "#,
    );
}

#[test]
fn invariant_no_closures_remain() {
    check_invariants(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => H(q1), q);
        }
        "#,
    );
}

#[test]
fn invariant_no_arrow_params_remain() {
    check_invariants(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
            ApplyOp(X, q);
        }
        "#,
    );
}

#[test]
fn invariant_no_closures_after_full_defunc() {
    check_invariants(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 1.0;
            ApplyOp(q1 => Rx(angle, q1), q);
            ApplyOp(H, q);
        }
        "#,
    );
}

#[test]
fn five_branch_conditional_callable_resolves_successfully() {
    check_invariants(
        r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        operation Main() : Unit {
            use q = Qubit();
            let n = 2;
            mutable op = H;
            if n == 0 {
                op = X;
            } elif n == 1 {
                op = Y;
            } elif n == 2 {
                op = Z;
            } elif n == 3 {
                op = S;
            } else {
                op = T;
            }
            Apply(op, q);
        }
        "#,
    );
}

#[test]
fn nine_branch_conditional_callable_degrades_to_dynamic() {
    check_errors(
        r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        operation Main() : Unit {
            use q = Qubit();
            let n = 2;
            mutable op = H;
            if n == 0 {
                op = X;
            } elif n == 1 {
                op = Y;
            } elif n == 2 {
                op = Z;
            } elif n == 3 {
                op = S;
            } elif n == 4 {
                op = T;
            } elif n == 5 {
                op = Rx(0.0, _);
            } elif n == 6 {
                op = Ry(0.0, _);
            } elif n == 7 {
                op = Rz(0.0, _);
            } else {
                op = SWAP(_, q);
            }
            Apply(op, q);
        }
        "#,
        &expect!["callable argument could not be resolved statically"],
    );
}

#[test]
fn controlled_functor_count_saturates_without_overflow() {
    check_invariants(
        r#"
        operation Foo(q : Qubit) : Unit is Ctl {
            body ... { H(q); }
            controlled (cs, ...) { Controlled H(cs, q); }
        }
        operation ApplyCtl1(q : Qubit, c1 : Qubit) : Unit {
            Controlled Foo([c1], q);
        }
        operation ApplyCtl2(q : Qubit, c1 : Qubit, c2 : Qubit) : Unit {
            Controlled Foo([c1, c2], q);
        }
        operation ApplyCtl3(q : Qubit, c1 : Qubit, c2 : Qubit, c3 : Qubit) : Unit {
            Controlled Foo([c1, c2, c3], q);
        }
        @EntryPoint()
        operation Main() : Unit {
            use (q, c1, c2, c3) = (Qubit(), Qubit(), Qubit(), Qubit());
            ApplyCtl1(q, c1);
            ApplyCtl2(q, c1, c2);
            ApplyCtl3(q, c1, c2, c3);
        }
        "#,
    );
}
