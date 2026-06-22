// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Many tests pair a primary assertion with a `check_rewrite` before/after
// snapshot, so the generated Q# pushes function bodies past the line limit.
#![allow(clippy::too_many_lines)]

use crate::package_assigners::PackageAssigners;

use super::*;
use expect_test::expect;

#[test]
fn invariants_single_hof() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(H, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn invariants_closure_with_captures() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 1.0;
            ApplyOp(q1 => Rx(angle, q1), q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let angle : Double = 1.;
                ApplyOp_Empty_(/ * closure item = 3 captures = [angle] * / _lambda_, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(angle : Double, q1 : Qubit) : Unit {
                Rx(angle, q1)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let angle : Double = 1.;
                ApplyOp_Empty__closure_(q, angle);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(angle : Double, q1 : Qubit) : Unit {
                Rx(angle, q1)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__closure_(q : Qubit, __capture_0 : Double) : Unit {
                _lambda_(__capture_0, q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn invariants_functor_composition() {
    let source = r#"
        operation ApplyAdj(op : Qubit => Unit is Adj, q : Qubit) : Unit {
            Adjoint op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyAdj(S, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyAdj(op : (Qubit => Unit), q : Qubit) : Unit {
                Adjoint op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyAdj_AdjCtl_(S, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyAdj_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                Adjoint op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyAdj(op : (Qubit => Unit), q : Qubit) : Unit {
                Adjoint op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyAdj_AdjCtl__S_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyAdj_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                Adjoint op(q);
            }
            operation ApplyAdj_AdjCtl__S_(q : Qubit) : Unit {
                Adjoint S(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn guarded_reassignment_callable_resolves_with_fallthrough_to_initial() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            if true { op = X; }
            ApplyOp(op, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    op = X;
                }

                ApplyOp_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    op = X;
                }

                if true {
                    ApplyOp_AdjCtl__X_(q)
                } else {
                    ApplyOp_AdjCtl__H_(q)
                };
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
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
    let mut assigners = PackageAssigners::entry(&store, package_id);
    let errors = defunctionalize(&mut store, package_id, &mut assigners);
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
    let mut assigners = PackageAssigners::entry(&store, package_id);
    let errors = defunctionalize(&mut store, package_id, &mut assigners);
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
    let source = r#"
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
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyInner(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOuter(op : (Qubit => Unit), q : Qubit) : Unit {
                ApplyInner_Empty_(op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOuter_AdjCtl_(H, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyInner_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOuter_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                ApplyInner_Empty_(op, q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyInner(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOuter(op : (Qubit => Unit), q : Qubit) : Unit {
                ApplyInner_Empty_(op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOuter_AdjCtl__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyInner_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOuter_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                ApplyInner_Empty_(op, q);
            }
            operation ApplyOuter_AdjCtl__H_(q : Qubit) : Unit {
                ApplyInner_Empty__H_(q);
            }
            operation ApplyInner_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hof_inside_for_loop_passes_invariants() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            for _ in 0..3 {
                ApplyOp(H, q);
            }
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_87 : Unit = {
                    let _range_id_39 : Range = 0..3;
                    mutable _index_id_42 : Int = _range_id_39::Start;
                    let _step_id_47 : Int = _range_id_39::Step;
                    let _end_id_52 : Int = _range_id_39::End;
                    while _step_id_47 > 0 and _index_id_42 <= _end_id_52 or _step_id_47 < 0 and _index_id_42 >= _end_id_52 {
                        let _ : Int = _index_id_42;
                        ApplyOp_AdjCtl_(H, q);
                        _index_id_42 += _step_id_47;
                    }

                };
                __quantum__rt__qubit_release(q);
                _generated_ident_87
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_87 : Unit = {
                    let _range_id_39 : Range = 0..3;
                    mutable _index_id_42 : Int = _range_id_39::Start;
                    let _step_id_47 : Int = _range_id_39::Step;
                    let _end_id_52 : Int = _range_id_39::End;
                    while _step_id_47 > 0 and _index_id_42 <= _end_id_52 or _step_id_47 < 0 and _index_id_42 >= _end_id_52 {
                        let _ : Int = _index_id_42;
                        ApplyOp_AdjCtl__H_(q);
                        _index_id_42 += _step_id_47;
                    }

                };
                __quantum__rt__qubit_release(q);
                _generated_ident_87
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn function_callable_argument_defunctionalizes() {
    let source = r#"
        function ApplyFn(f : Int -> Int, x : Int) : Int {
            f(x)
        }
        function Double(x : Int) : Int { x * 2 }
        @EntryPoint()
        operation Main() : Unit {
            let _ = ApplyFn(Double, 5);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            function ApplyFn(f : (Int -> Int), x : Int) : Int {
                f(x)
            }
            function Double(x : Int) : Int {
                x * 2
            }
            operation Main() : Unit {
                let _ : Int = ApplyFn(Double, 5);
            }
            // entry
            Main()

            AFTER:
            function ApplyFn(f : (Int -> Int), x : Int) : Int {
                f(x)
            }
            function Double(x : Int) : Int {
                x * 2
            }
            operation Main() : Unit {
                let _ : Int = ApplyFn_Double_(5);
            }
            function ApplyFn_Double_(x : Int) : Int {
                Double(x)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn explicit_functor_specializations_defunctionalize() {
    let source = r#"
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
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
                controlled (ctls, ...) {
                    Controlled op(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint op(ctls, q);
                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(S, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
                controlled (ctls, ...) {
                    Controlled op(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint op(ctls, q);
                }
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
                controlled (ctls, ...) {
                    Controlled op(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint op(ctls, q);
                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__S_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
                controlled (ctls, ...) {
                    Controlled op(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint op(ctls, q);
                }
            }
            operation ApplyOp_AdjCtl__S_(q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    S(q);
                }
                adjoint ... {
                    Adjoint S(q);
                }
                controlled (ctls, ...) {
                    Controlled S(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint S(ctls, q);
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn full_pipeline_preserves_post_all_invariants() {
    let source = r#"
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
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(H, q);
                ApplyOp_AdjCtl_(X, q);
                let angle : Double = 1.;
                ApplyOp_Empty_(/ * closure item = 3 captures = [angle] * / _lambda_, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(angle : Double, q1 : Qubit) : Unit {
                Rx(angle, q1)
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__H_(q);
                ApplyOp_AdjCtl__X_(q);
                let angle : Double = 1.;
                ApplyOp_Empty__closure_(q, angle);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(angle : Double, q1 : Qubit) : Unit {
                Rx(angle, q1)
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            operation ApplyOp_Empty__closure_(q : Qubit, __capture_0 : Double) : Unit {
                _lambda_(__capture_0, q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn invariant_no_closure_expressions_remain() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => H(q1), q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty_(/ * closure item = 3 captures = [] * / _lambda_, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(q1 : Qubit, ) : Unit {
                H(q1)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(q1 : Qubit, ) : Unit {
                H(q1)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn invariant_no_arrow_params_remain_in_specialized_callables() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
            ApplyOp(X, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(H, q);
                ApplyOp_AdjCtl_(X, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__H_(q);
                ApplyOp_AdjCtl__X_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn five_branch_conditional_callable_resolves_successfully() {
    let source = r#"
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
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Apply(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let n : Int = 2;
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if n == 0 {
                    op = X;
                } else if n == 1 {
                    op = Y;
                } else if n == 2 {
                    op = Z;
                } else if n == 3 {
                    op = S;
                } else {
                    op = T;
                }

                Apply_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation Apply_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation Apply(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let n : Int = 2;
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if n == 0 {
                    op = X;
                } else if n == 1 {
                    op = Y;
                } else if n == 2 {
                    op = Z;
                } else if n == 3 {
                    op = S;
                } else {
                    op = T;
                }

                if n == 0 {
                    Apply_AdjCtl__X_(q)
                } else if n == 1 {
                    Apply_AdjCtl__Y_(q)
                } else if n == 2 {
                    Apply_AdjCtl__Z_(q)
                } else if n == 3 {
                    Apply_AdjCtl__S_(q)
                } else {
                    Apply_AdjCtl__T_(q)
                };
                __quantum__rt__qubit_release(q);
            }
            operation Apply_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation Apply_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            operation Apply_AdjCtl__Y_(q : Qubit) : Unit {
                Y(q);
            }
            operation Apply_AdjCtl__Z_(q : Qubit) : Unit {
                Z(q);
            }
            operation Apply_AdjCtl__S_(q : Qubit) : Unit {
                S(q);
            }
            operation Apply_AdjCtl__T_(q : Qubit) : Unit {
                T(q);
            }
            // entry
            Main()
        "#]],
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

/// Direct-path mirror of `nine_branch_conditional_callable_degrades_to_dynamic`:
/// a direct (non-HOF) call `f(q)` whose callee `f` is forced to `Dynamic` by a
/// loop reassignment now surfaces the actionable `DynamicCallable` diagnostic
/// at the call site, rather than only the less-specific `FixpointNotReached`.
#[test]
fn direct_call_unresolvable_callable_emits_dynamic_callable_diagnostic() {
    check_errors(
        r#"
        operation Foo(q : Qubit) : Unit {}
        operation Bar(q : Qubit) : Unit {}
        operation Main() : Unit {
            use q = Qubit();
            mutable f = Foo;
            for _ in 0..2 {
                f = Bar;
            }
            f(q);
        }
        "#,
        &expect!["callable argument could not be resolved statically"],
    );
}

/// Direct-path mirror of `nine_branch_conditional_callable_degrades_to_dynamic`:
/// a direct (non-HOF) call `op(q)` whose local callee accumulates more than
/// `MULTI_CAP` (8) distinct callables via a nested conditional. The
/// `CalleeLattice::Multi` join saturates to `Dynamic` rather than panicking or
/// silently dropping branches, and the direct call surfaces the actionable
/// `DynamicCallable` diagnostic.
#[test]
fn direct_nine_branch_conditional_callable_degrades_to_dynamic() {
    check_errors(
        r#"
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
            op(q);
        }
        "#,
        &expect!["callable argument could not be resolved statically"],
    );
}

#[test]
fn controlled_functor_count_saturates_without_overflow() {
    let source = r#"
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
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Foo(q : Qubit) : Unit is Ctl {
                body ... {
                    H(q);
                }
                controlled (cs, ...) {
                    Controlled H(cs, q);
                }
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
            operation Main() : Unit {
                let _generated_ident_126 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_128 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_130 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_132 : Qubit = __quantum__rt__qubit_allocate();
                let (q : Qubit, c1 : Qubit, c2 : Qubit, c3 : Qubit) = (_generated_ident_126, _generated_ident_128, _generated_ident_130, _generated_ident_132);
                ApplyCtl1(q, c1);
                ApplyCtl2(q, c1, c2);
                ApplyCtl3(q, c1, c2, c3);
                __quantum__rt__qubit_release(_generated_ident_132);
                __quantum__rt__qubit_release(_generated_ident_130);
                __quantum__rt__qubit_release(_generated_ident_128);
                __quantum__rt__qubit_release(_generated_ident_126);
            }
            // entry
            Main()

            AFTER:
            operation Foo(q : Qubit) : Unit is Ctl {
                body ... {
                    H(q);
                }
                controlled (cs, ...) {
                    Controlled H(cs, q);
                }
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
            operation Main() : Unit {
                let _generated_ident_126 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_128 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_130 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_132 : Qubit = __quantum__rt__qubit_allocate();
                let (q : Qubit, c1 : Qubit, c2 : Qubit, c3 : Qubit) = (_generated_ident_126, _generated_ident_128, _generated_ident_130, _generated_ident_132);
                ApplyCtl1(q, c1);
                ApplyCtl2(q, c1, c2);
                ApplyCtl3(q, c1, c2, c3);
                __quantum__rt__qubit_release(_generated_ident_132);
                __quantum__rt__qubit_release(_generated_ident_130);
                __quantum__rt__qubit_release(_generated_ident_128);
                __quantum__rt__qubit_release(_generated_ident_126);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn newtype_ctor_callable_field_cleanup() {
    // Pins the cleanup behavior for closures inside legacy-`newtype` UDT
    // constructor argument subtrees. The UDT-ctor guard in
    // `cleanup_consumed_closures` lets these closures be replaced after
    // their specialized callable is produced, ensuring convergence.
    //
    // Uses both `Choose(true)` and `Choose(false)` so each conditional
    // branch is specialized at least once; otherwise a literal-conditioned
    // projection leaves the unused branch's closure as dead-code and
    // convergence cannot succeed independently of the UDT-ctor guard.
    let source = r#"
        namespace Test {
          newtype Choice = (F : Int -> Int, Offset : Int);

          function Choose(flag : Bool) : Choice {
            if flag {
              Choice(x -> x + 1, 100)
            } else {
              Choice(x -> x * 2, 7)
            }
          }

          @EntryPoint()
          function Main() : Int {
            let selectedT = Choose(true);
            let selectedF = Choose(false);
            let fT = selectedT::F;
            let fF = selectedF::F;
            fT(10) + fF(10) + selectedT::Offset + selectedF::Offset
          }
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            newtype Choice = ((Int -> Int), Int);
            function Choose(flag : Bool) : __UDT_Item_1__Package_2_ {
                if flag {
                    Choice(/ * closure item = 4 captures = [] * / _lambda_, 100)
                } else {
                    Choice(/ * closure item = 5 captures = [] * / _lambda_, 7)
                }

            }
            function Main() : Int {
                let selectedT : __UDT_Item_1__Package_2_ = Choose(true);
                let selectedF : __UDT_Item_1__Package_2_ = Choose(false);
                let fT : (Int -> Int) = selectedT::F;
                let fF : (Int -> Int) = selectedF::F;
                fT(10) + fF(10) + selectedT::Offset + selectedF::Offset
            }
            function _lambda_(x : Int, ) : Int {
                x + 1
            }
            function _lambda_(x : Int, ) : Int {
                x * 2
            }
            // entry
            Main()

            AFTER:
            newtype Choice = ((Int -> Int), Int);
            function Choose(flag : Bool) : __UDT_Item_1__Package_2_ {
                if flag {
                    Choice((), 100)
                } else {
                    Choice((), 7)
                }

            }
            function Main() : Int {
                let selectedT : __UDT_Item_1__Package_2_ = Choose(true);
                let selectedF : __UDT_Item_1__Package_2_ = Choose(false);
                if true {
                    _lambda_(10)
                } else {
                    _lambda_(10)
                } + if false {
                    _lambda_(10)
                } else {
                    _lambda_(10)
                } + selectedT::Offset + selectedF::Offset
            }
            function _lambda_(x : Int, ) : Int {
                x + 1
            }
            function _lambda_(x : Int, ) : Int {
                x * 2
            }
            // entry
            Main()
        "#]],
    );
}
