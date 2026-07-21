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
                ApplyOp_Empty_(/ * closure item = 3 captures = [angle] * / _lambda_3, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(angle : Double, q1 : Qubit) : Unit {
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
            operation _lambda_3(angle : Double, q1 : Qubit) : Unit {
                Rx(angle, q1)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__closure_(q : Qubit, __capture_0 : Double) : Unit {
                _lambda_3(__capture_0, q);
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
    let mut assigners = PackageAssigners::new(&store, package_id);
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
    let mut assigners = PackageAssigners::new(&store, package_id);
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
                ApplyOp_Empty_(/ * closure item = 3 captures = [angle] * / _lambda_3, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(angle : Double, q1 : Qubit) : Unit {
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
            operation _lambda_3(angle : Double, q1 : Qubit) : Unit {
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
                _lambda_3(__capture_0, q);
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
                ApplyOp_Empty_(/ * closure item = 3 captures = [] * / _lambda_3, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(q1 : Qubit, ) : Unit {
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
            operation _lambda_3(q1 : Qubit, ) : Unit {
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

/// A statically-known callable array with more than `MULTI_CAP` (1000) distinct
/// elements exceeds the per-set candidate bound during indexed-dispatch
/// resolution, so the analysis widens to `Dynamic` (top of the lattice) instead
/// of building a per-index dispatch chain. The higher-order `Apply(op, q)` call
/// over the loop element then surfaces the actionable `DynamicCallable`
/// diagnostic. Arrays at or below the cap resolve to a per-index dispatch
/// instead, so exercising the widen-to-`Dynamic` path requires more than 1000
/// distinct elements. A flat array literal is used (rather than a deeply nested
/// `if`/`elif` chain) to avoid overflowing the parser stack at this width.
#[test]
fn callable_array_exceeding_multi_cap_degrades_to_dynamic() {
    use std::fmt::Write as _;

    // One distinct callable per element; more than `MULTI_CAP` (1000) elements
    // forces indexed-dispatch resolution to widen the candidate set to
    // `Dynamic`.
    const ELEMENTS: usize = 1001;

    let mut defs = String::new();
    let mut elems = String::new();
    for i in 0..ELEMENTS {
        writeln!(defs, "        operation Op{i}(q : Qubit) : Unit {{}}").expect("write succeeds");
        if i > 0 {
            elems.push_str(", ");
        }
        write!(elems, "Op{i}").expect("write succeeds");
    }

    let source = format!(
        r#"
{defs}
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit {{
            op(q);
        }}

        operation Main() : Unit {{
            use q = Qubit();
            let ops = [{elems}];
            for op in ops {{
                Apply(op, q);
            }}
        }}
        "#
    );

    check_errors(
        &source,
        &expect!["callable argument could not be resolved statically"],
    );
}

/// A direct (non-HOF) call `f(q)` whose callee `f` is forced to `Dynamic` by a
/// loop reassignment (loop reassignment is treated as unresolvable regardless
/// of the candidate count) surfaces the actionable `DynamicCallable` diagnostic
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

/// Direct-path (non-HOF) analogue of
/// `callable_array_exceeding_multi_cap_degrades_to_dynamic`: a statically-known
/// callable array with more than `MULTI_CAP` (1000) distinct elements exceeds
/// the per-set candidate bound during indexed-dispatch resolution, so the
/// analysis widens to `Dynamic` instead of building a per-index dispatch chain.
/// The direct call `op(q)` over the loop element then surfaces the actionable
/// `DynamicCallable` diagnostic. A flat array literal is used (rather than a
/// deeply nested `if`/`elif` chain) to avoid overflowing the parser stack at
/// this width.
#[test]
fn direct_callable_array_exceeding_multi_cap_degrades_to_dynamic() {
    use std::fmt::Write as _;

    // One distinct callable per element; more than `MULTI_CAP` (1000) elements
    // forces indexed-dispatch resolution to widen the candidate set to
    // `Dynamic`.
    const ELEMENTS: usize = 1001;

    let mut defs = String::new();
    let mut elems = String::new();
    for i in 0..ELEMENTS {
        writeln!(defs, "        operation Op{i}(q : Qubit) : Unit {{}}").expect("write succeeds");
        if i > 0 {
            elems.push_str(", ");
        }
        write!(elems, "Op{i}").expect("write succeeds");
    }

    let source = format!(
        r#"
{defs}
        operation Main() : Unit {{
            use q = Qubit();
            let ops = [{elems}];
            for op in ops {{
                op(q);
            }}
        }}
        "#
    );

    check_errors(
        &source,
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
                    Choice(/ * closure item = 4 captures = [] * / _lambda_4, 100)
                } else {
                    Choice(/ * closure item = 5 captures = [] * / _lambda_5, 7)
                }

            }
            function Main() : Int {
                let selectedT : __UDT_Item_1__Package_2_ = Choose(true);
                let selectedF : __UDT_Item_1__Package_2_ = Choose(false);
                let fT : (Int -> Int) = selectedT::F;
                let fF : (Int -> Int) = selectedF::F;
                fT(10) + fF(10) + selectedT::Offset + selectedF::Offset
            }
            function _lambda_4(x : Int, ) : Int {
                x + 1
            }
            function _lambda_5(x : Int, ) : Int {
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
                    _lambda_4(10)
                } else {
                    _lambda_5(10)
                } + if false {
                    _lambda_4(10)
                } else {
                    _lambda_5(10)
                } + selectedT::Offset + selectedF::Offset
            }
            function _lambda_4(x : Int, ) : Int {
                x + 1
            }
            function _lambda_5(x : Int, ) : Int {
                x * 2
            }
            // entry
            Main()
        "#]],
    );
}

// A select-style operation whose first parameter is a struct (UDT) is
// partially applied into a closure, then forwarded as `selectOp` through a
// factory that dispatches it via `Controlled selectOp([control], (systems,
// ancilla))`. Specialization must thread the captured struct through the
// controlled-dispatch layer so the rewritten call reads
// `Controlled _lambda_8([control], (__capture_0, (systems, ancilla)))` rather
// than dropping the capture and passing `(systems, ancilla)` directly. Dropping
// the capture would leave the call shape inconsistent with the specialized
// callee's input and trip the post-arg_promote call-shape invariant. This test
// drives the full pipeline (`check_pipeline`) so the shape is validated through
// argument promotion, and pairs it with a rewrite snapshot showing the threaded
// struct capture. It exercises the controlled struct-capture-threading path via
// the source `Main` entry; it does not reproduce the injected-closure entry
// rooting that a compiled-from-Python entry expression would produce.
#[test]
fn struct_capture_select_op_threads_through_controlled_dispatch_pipeline() {
    let source = r#"
        struct PauliSelectParams {
            paulis : Pauli[][],
            qubitIndices : Int[],
            signs : Int[]
        }

        operation ApplySelect(params : PauliSelectParams, systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {
            if Length(params.signs) != 0 {
                X(systems[0]);
            }
        }

        operation ApplyPrepare(systems : Qubit[]) : Unit is Adj + Ctl {}

        function MakeControlledPrepSelPrepOp(
            prepareOp : Qubit[] => Unit is Adj + Ctl,
            selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
            numSystemQubits : Int,
            power : Int
        ) : (Qubit, Qubit[]) => Unit {
            (control, allQubits) => {
                let systems = allQubits[0..numSystemQubits - 1];
                let ancilla = allQubits[numSystemQubits...];
                for _ in 0..power - 1 {
                    Controlled prepareOp([control], systems);
                    Controlled selectOp([control], (systems, ancilla));
                }
            }
        }

        operation MakeControlledPrepSelPrepCircuit(
            prepareOp : Qubit[] => Unit is Adj + Ctl,
            selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
            numSystemQubits : Int,
            power : Int
        ) : Unit {
            use control = Qubit();
            use systems = Qubit[numSystemQubits + 1];
            let op = MakeControlledPrepSelPrepOp(prepareOp, selectOp, numSystemQubits, power);
            op(control, systems);
        }

        operation Main() : Unit {
            let params = new PauliSelectParams {
                paulis = [[PauliX]],
                qubitIndices = [0],
                signs = [1]
            };
            let sel = ApplySelect(params, _, _);
            MakeControlledPrepSelPrepCircuit(ApplyPrepare, sel, 1, 1);
        }
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            newtype PauliSelectParams = (Pauli[][], Int[], Int[]);
            operation ApplySelect(params : __UDT_Item_1__Package_2_, systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {
                body ... {
                    if Length(params::signs) != 0 {
                        X(systems[0]);
                    }

                }
                adjoint ... {
                    if Length(params::signs) != 0 {
                        Adjoint X(systems[0]);
                    }

                }
                controlled (ctls, ...) {
                    if Length(params::signs) != 0 {
                        Controlled X(ctls, systems[0]);
                    }

                }
                controlled adjoint (ctls, ...) {
                    if Length(params::signs) != 0 {
                        Controlled Adjoint X(ctls, systems[0]);
                    }

                }
            }
            operation ApplyPrepare(systems : Qubit[]) : Unit is Adj + Ctl {
                body ... {}
                adjoint ... {}
                controlled (ctls, ...) {}
                controlled adjoint (ctls, ...) {}
            }
            function MakeControlledPrepSelPrepOp(prepareOp : (Qubit[] => Unit), selectOp : ((Qubit[], Qubit[]) => Unit), numSystemQubits : Int, power : Int) : ((Qubit, Qubit[]) => Unit) {
                / * closure item = 7 captures = [prepareOp, selectOp, numSystemQubits, power] * / _lambda_7
            }
            operation MakeControlledPrepSelPrepCircuit(prepareOp : (Qubit[] => Unit), selectOp : ((Qubit[], Qubit[]) => Unit), numSystemQubits : Int, power : Int) : Unit {
                let control : Qubit = __quantum__rt__qubit_allocate();
                let systems : Qubit[] = AllocateQubitArray(numSystemQubits + 1);
                let op : ((Qubit, Qubit[]) => Unit) = MakeControlledPrepSelPrepOp_AdjCtl__AdjCtl_(prepareOp, selectOp, numSystemQubits, power);
                op(control, systems);
                ReleaseQubitArray(systems);
                __quantum__rt__qubit_release(control);
            }
            operation Main() : Unit {
                let params : __UDT_Item_1__Package_2_ = new PauliSelectParams {
                    paulis = [[PauliX]],
                    qubitIndices = [0],
                    signs = [1]
                };
                let sel : ((Qubit[], Qubit[]) => Unit is Adj + Ctl) = {
                    let arg : __UDT_Item_1__Package_2_ = params;
                    / * closure item = 8 captures = [arg] * / _lambda_8
                };
                MakeControlledPrepSelPrepCircuit_AdjCtl__AdjCtl_(ApplyPrepare, sel, 1, 1);
            }
            operation _lambda_7(prepareOp : (Qubit[] => Unit), selectOp : ((Qubit[], Qubit[]) => Unit), numSystemQubits : Int, power : Int, (control : Qubit, allQubits : Qubit[])) : Unit {
                {
                    let systems : Qubit[] = allQubits[0..numSystemQubits - 1];
                    let ancilla : Qubit[] = allQubits[numSystemQubits...];
                    {
                        let _range_id_346 : Range = 0..power - 1;
                        mutable _index_id_349 : Int = _range_id_346::Start;
                        let _step_id_354 : Int = _range_id_346::Step;
                        let _end_id_359 : Int = _range_id_346::End;
                        while _step_id_354 > 0 and _index_id_349 <= _end_id_359 or _step_id_354 < 0 and _index_id_349 >= _end_id_359 {
                            let _ : Int = _index_id_349;
                            Controlled prepareOp([control], systems);
                            Controlled selectOp([control], (systems, ancilla));
                            _index_id_349 += _step_id_354;
                        }

                    }

                }

            }
            operation _lambda_8(arg : __UDT_Item_1__Package_2_, (hole : Qubit[], hole_1 : Qubit[])) : Unit is Adj + Ctl {
                body ... {
                    ApplySelect(arg, hole, hole_1)
                }
                adjoint ... {
                    Adjoint ApplySelect(arg, hole, hole_1)
                }
                controlled (ctls, ...) {
                    Controlled ApplySelect(ctls, (arg, hole, hole_1))
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint ApplySelect(ctls, (arg, hole, hole_1))
                }
            }
            function MakeControlledPrepSelPrepOp_AdjCtl__AdjCtl_(prepareOp : (Qubit[] => Unit is Adj + Ctl), selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl), numSystemQubits : Int, power : Int) : ((Qubit, Qubit[]) => Unit) {
                / * closure item = 10 captures = [prepareOp, selectOp, numSystemQubits, power] * / _lambda_7
            }
            operation _lambda_7(prepareOp : (Qubit[] => Unit is Adj + Ctl), selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl), numSystemQubits : Int, power : Int, (control : Qubit, allQubits : Qubit[])) : Unit {
                {
                    let systems : Qubit[] = allQubits[0..numSystemQubits - 1];
                    let ancilla : Qubit[] = allQubits[numSystemQubits...];
                    {
                        let _range_id_346 : Range = 0..power - 1;
                        mutable _index_id_349 : Int = _range_id_346::Start;
                        let _step_id_354 : Int = _range_id_346::Step;
                        let _end_id_359 : Int = _range_id_346::End;
                        while _step_id_354 > 0 and _index_id_349 <= _end_id_359 or _step_id_354 < 0 and _index_id_349 >= _end_id_359 {
                            let _ : Int = _index_id_349;
                            Controlled prepareOp([control], systems);
                            Controlled selectOp([control], (systems, ancilla));
                            _index_id_349 += _step_id_354;
                        }

                    }

                }

            }
            operation MakeControlledPrepSelPrepCircuit_AdjCtl__AdjCtl_(prepareOp : (Qubit[] => Unit is Adj + Ctl), selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl), numSystemQubits : Int, power : Int) : Unit {
                let control : Qubit = __quantum__rt__qubit_allocate();
                let systems : Qubit[] = AllocateQubitArray(numSystemQubits + 1);
                let op : ((Qubit, Qubit[]) => Unit) = MakeControlledPrepSelPrepOp_AdjCtl__AdjCtl_(prepareOp, selectOp, numSystemQubits, power);
                op(control, systems);
                ReleaseQubitArray(systems);
                __quantum__rt__qubit_release(control);
            }
            // entry
            Main()

            AFTER:
            newtype PauliSelectParams = (Pauli[][], Int[], Int[]);
            operation ApplySelect(params : __UDT_Item_1__Package_2_, systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {
                body ... {
                    if Length(params::signs) != 0 {
                        X(systems[0]);
                    }

                }
                adjoint ... {
                    if Length(params::signs) != 0 {
                        Adjoint X(systems[0]);
                    }

                }
                controlled (ctls, ...) {
                    if Length(params::signs) != 0 {
                        Controlled X(ctls, systems[0]);
                    }

                }
                controlled adjoint (ctls, ...) {
                    if Length(params::signs) != 0 {
                        Controlled Adjoint X(ctls, systems[0]);
                    }

                }
            }
            operation ApplyPrepare(systems : Qubit[]) : Unit is Adj + Ctl {
                body ... {}
                adjoint ... {}
                controlled (ctls, ...) {}
                controlled adjoint (ctls, ...) {}
            }
            function MakeControlledPrepSelPrepOp(prepareOp : (Qubit[] => Unit), selectOp : ((Qubit[], Qubit[]) => Unit), numSystemQubits : Int, power : Int) : ((Qubit, Qubit[]) => Unit) {
                / * closure item = 7 captures = [prepareOp, selectOp, numSystemQubits, power] * / _lambda_7
            }
            operation MakeControlledPrepSelPrepCircuit(prepareOp : (Qubit[] => Unit), selectOp : ((Qubit[], Qubit[]) => Unit), numSystemQubits : Int, power : Int) : Unit {
                let control : Qubit = __quantum__rt__qubit_allocate();
                let systems : Qubit[] = AllocateQubitArray(numSystemQubits + 1);
                let op : ((Qubit, Qubit[]) => Unit) = MakeControlledPrepSelPrepOp_AdjCtl__AdjCtl_(prepareOp, selectOp, numSystemQubits, power);
                op(control, systems);
                ReleaseQubitArray(systems);
                __quantum__rt__qubit_release(control);
            }
            operation Main() : Unit {
                let params : __UDT_Item_1__Package_2_ = new PauliSelectParams {
                    paulis = [[PauliX]],
                    qubitIndices = [0],
                    signs = [1]
                };
                MakeControlledPrepSelPrepCircuit_AdjCtl__AdjCtl__ApplyPrepare__closure_(1, 1, params);
            }
            operation _lambda_7(prepareOp : (Qubit[] => Unit), selectOp : ((Qubit[], Qubit[]) => Unit), numSystemQubits : Int, power : Int, (control : Qubit, allQubits : Qubit[])) : Unit {
                {
                    let systems : Qubit[] = allQubits[0..numSystemQubits - 1];
                    let ancilla : Qubit[] = allQubits[numSystemQubits...];
                    {
                        let _range_id_346 : Range = 0..power - 1;
                        mutable _index_id_349 : Int = _range_id_346::Start;
                        let _step_id_354 : Int = _range_id_346::Step;
                        let _end_id_359 : Int = _range_id_346::End;
                        while _step_id_354 > 0 and _index_id_349 <= _end_id_359 or _step_id_354 < 0 and _index_id_349 >= _end_id_359 {
                            let _ : Int = _index_id_349;
                            Controlled prepareOp([control], systems);
                            Controlled selectOp([control], (systems, ancilla));
                            _index_id_349 += _step_id_354;
                        }

                    }

                }

            }
            operation _lambda_8(arg : __UDT_Item_1__Package_2_, (hole : Qubit[], hole_1 : Qubit[])) : Unit is Adj + Ctl {
                body ... {
                    ApplySelect(arg, hole, hole_1)
                }
                adjoint ... {
                    Adjoint ApplySelect(arg, hole, hole_1)
                }
                controlled (ctls, ...) {
                    Controlled ApplySelect(ctls, (arg, hole, hole_1))
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint ApplySelect(ctls, (arg, hole, hole_1))
                }
            }
            function MakeControlledPrepSelPrepOp_AdjCtl__AdjCtl_(prepareOp : (Qubit[] => Unit is Adj + Ctl), selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl), numSystemQubits : Int, power : Int) : ((Qubit, Qubit[]) => Unit) {
                ()
            }
            operation _lambda_7(prepareOp : (Qubit[] => Unit is Adj + Ctl), selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl), numSystemQubits : Int, power : Int, (control : Qubit, allQubits : Qubit[])) : Unit {
                {
                    let systems : Qubit[] = allQubits[0..numSystemQubits - 1];
                    let ancilla : Qubit[] = allQubits[numSystemQubits...];
                    {
                        let _range_id_346 : Range = 0..power - 1;
                        mutable _index_id_349 : Int = _range_id_346::Start;
                        let _step_id_354 : Int = _range_id_346::Step;
                        let _end_id_359 : Int = _range_id_346::End;
                        while _step_id_354 > 0 and _index_id_349 <= _end_id_359 or _step_id_354 < 0 and _index_id_349 >= _end_id_359 {
                            let _ : Int = _index_id_349;
                            Controlled prepareOp([control], systems);
                            Controlled selectOp([control], (systems, ancilla));
                            _index_id_349 += _step_id_354;
                        }

                    }

                }

            }
            operation MakeControlledPrepSelPrepCircuit_AdjCtl__AdjCtl_(prepareOp : (Qubit[] => Unit is Adj + Ctl), selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl), numSystemQubits : Int, power : Int) : Unit {
                let control : Qubit = __quantum__rt__qubit_allocate();
                let systems : Qubit[] = AllocateQubitArray(numSystemQubits + 1);
                _lambda_7(prepareOp, selectOp, numSystemQubits, power, (control, systems));
                ReleaseQubitArray(systems);
                __quantum__rt__qubit_release(control);
            }
            operation MakeControlledPrepSelPrepCircuit_AdjCtl__AdjCtl__ApplyPrepare__closure_(numSystemQubits : Int, power : Int, __capture_0 : __UDT_Item_1__Package_2_) : Unit {
                let control : Qubit = __quantum__rt__qubit_allocate();
                let systems : Qubit[] = AllocateQubitArray(numSystemQubits + 1);
                _lambda_7_ApplyPrepare__closure_(numSystemQubits, power, (control, systems), __capture_0);
                ReleaseQubitArray(systems);
                __quantum__rt__qubit_release(control);
            }
            function MakeControlledPrepSelPrepOp_AdjCtl__AdjCtl__ApplyPrepare__closure_(numSystemQubits : Int, power : Int, __capture_0 : __UDT_Item_1__Package_2_) : ((Qubit, Qubit[]) => Unit) {
                / * closure item = 14 captures = [numSystemQubits, power] * / _lambda_7
            }
            operation _lambda_7(numSystemQubits : Int, power : Int, (control : Qubit, allQubits : Qubit[])) : Unit {
                {
                    let systems : Qubit[] = allQubits[0..numSystemQubits - 1];
                    let ancilla : Qubit[] = allQubits[numSystemQubits...];
                    {
                        let _range_id_346 : Range = 0..power - 1;
                        mutable _index_id_349 : Int = _range_id_346::Start;
                        let _step_id_354 : Int = _range_id_346::Step;
                        let _end_id_359 : Int = _range_id_346::End;
                        while _step_id_354 > 0 and _index_id_349 <= _end_id_359 or _step_id_354 < 0 and _index_id_349 >= _end_id_359 {
                            let _ : Int = _index_id_349;
                            Controlled ApplyPrepare([control], systems);
                            Controlled _lambda_8([control], (systems, ancilla));
                            _index_id_349 += _step_id_354;
                        }

                    }

                }

            }
            operation _lambda_7_ApplyPrepare__closure_(numSystemQubits : Int, power : Int, (control : Qubit, allQubits : Qubit[]), __capture_0 : __UDT_Item_1__Package_2_) : Unit {
                {
                    let systems : Qubit[] = allQubits[0..numSystemQubits - 1];
                    let ancilla : Qubit[] = allQubits[numSystemQubits...];
                    {
                        let _range_id_346 : Range = 0..power - 1;
                        mutable _index_id_349 : Int = _range_id_346::Start;
                        let _step_id_354 : Int = _range_id_346::Step;
                        let _end_id_359 : Int = _range_id_346::End;
                        while _step_id_354 > 0 and _index_id_349 <= _end_id_359 or _step_id_354 < 0 and _index_id_349 >= _end_id_359 {
                            let _ : Int = _index_id_349;
                            Controlled ApplyPrepare([control], systems);
                            Controlled _lambda_8([control], (__capture_0, (systems, ancilla)));
                            _index_id_349 += _step_id_354;
                        }

                    }

                }

            }
            // entry
            Main()
        "#]],
    );
}

// A direct (non-higher-order) call over a callable local that mixes an
// intrinsic candidate with a partial application dispatches over both
// candidates. The partial application `Rx(0.0, _)` lowers to a closure-tailed
// block bound to `op`; once `op(q)` becomes a branch dispatch, that binding is
// dead. Removing the dead binding keeps closure cleanup from stranding an
// arrow-typed block with no producing tail, which would otherwise violate the
// non-Unit block-tail invariant. This pairs the invariant check with a rewrite
// snapshot showing the dead `op` binding removed and the dispatch inlining the
// specialized callees.
#[test]
fn width2_mixed_direct_dispatch_removes_dead_partial_app_binding() {
    let source = r#"
        operation Main() : Unit {
            use q = Qubit();
            let n = 1;
            mutable op = X;
            if n == 0 {
                op = Rx(0.0, _);
            }
            op(q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let n : Int = 1;
                mutable op : (Qubit => Unit is Adj + Ctl) = X;
                if n == 0 {
                    op = {
                        let arg : Double = 0.;
                        / * closure item = 2 captures = [arg] * / _lambda_2
                    };
                }

                op(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_2(arg : Double, hole : Qubit) : Unit is Adj + Ctl {
                body ... {
                    Rx(arg, hole)
                }
                adjoint ... {
                    Adjoint Rx(arg, hole)
                }
                controlled (ctls, ...) {
                    Controlled Rx(ctls, (arg, hole))
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint Rx(ctls, (arg, hole))
                }
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let n : Int = 1;
                if n == 0 {}

                if n == 0 {
                    _lambda_2(0., q)
                } else {
                    X(q)
                };
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_2(arg : Double, hole : Qubit) : Unit is Adj + Ctl {
                body ... {
                    Rx(arg, hole)
                }
                adjoint ... {
                    Adjoint Rx(arg, hole)
                }
                controlled (ctls, ...) {
                    Controlled Rx(ctls, (arg, hole))
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint Rx(ctls, (arg, hole))
                }
            }
            // entry
            Main()
        "#]],
    );
}

// Indexed-array analogue of the mixed direct dispatch: a callable-array literal
// mixing an intrinsic with a partial application is indexed inside a loop and
// dispatched directly. Once the indexed read is rewritten into a branch
// dispatch, both the indexed local and the now-dead source array (whose element
// holds a closure) are removed, so no arrow-typed block with a blanked tail
// remains.
#[test]
fn indexed_callable_array_mixed_direct_dispatch_passes_invariants() {
    let source = r#"
        operation Main() : Unit {
            use q = Qubit();
            let ops = [X, Rx(0.0, _)];
            for i in 0..1 {
                let op = ops[i];
                op(q);
            }
        }
        "#;
    check_invariants(source);
}

// Pure partial-application direct dispatch: both candidates are partial
// applications, confirming the fix keys on a consumed partial-application
// residual in a reachable block rather than on a mixed candidate set. The
// write-only `op` binding and its reassignment are removed once the dispatch
// consumes them.
#[test]
fn pure_partial_app_direct_dispatch_passes_invariants() {
    let source = r#"
        operation Main() : Unit {
            use q = Qubit();
            let c = true;
            mutable op = Rx(0.0, _);
            if c {
                op = Ry(0.0, _);
            }
            op(q);
        }
        "#;
    check_invariants(source);
}
