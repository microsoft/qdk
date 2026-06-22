// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Many tests pair a primary assertion with a `check_rewrite` before/after
// snapshot, so the generated Q# pushes function bodies past the line limit.
#![allow(clippy::too_many_lines)]

use crate::{
    defunctionalize::specialize::CAPTURE_NAME_PREFIX, package_assigners::PackageAssigners,
};

use super::*;
use expect_test::expect;

#[test]
fn specialize_single_global_callable() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
        }
        "#,
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
fn specialize_two_different_callables() {
    check_rewrite(
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
fn specialize_same_callable_reuse() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
            ApplyOp(H, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(H, q);
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

/// A program with no higher-order functions is a no-op for the pass: the
/// before/after snapshots are identical because there is nothing to specialize.
#[test]
fn specialize_no_hof_unchanged() {
    check_rewrite(
        r#"
        operation Foo(q : Qubit) : Unit {
            H(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Foo(q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Foo(q : Qubit) : Unit {
                H(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Foo(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation Foo(q : Qubit) : Unit {
                H(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Foo(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_closure_no_captures() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => H(q1), q);
        }
        "#,
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
fn specialize_closure_with_captures() {
    check_rewrite(
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
fn specialize_closure_capture_types_preserved() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let n = 3;
            ApplyOp(q1 => { for _ in 0..n { H(q1); } }, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let n : Int = 3;
                ApplyOp_Empty_(/ * closure item = 3 captures = [n] * / _lambda_, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(n : Int, q1 : Qubit) : Unit {
                {
                    {
                        let _range_id_59 : Range = 0..n;
                        mutable _index_id_62 : Int = _range_id_59::Start;
                        let _step_id_67 : Int = _range_id_59::Step;
                        let _end_id_72 : Int = _range_id_59::End;
                        while _step_id_67 > 0 and _index_id_62 <= _end_id_72 or _step_id_67 < 0 and _index_id_62 >= _end_id_72 {
                            let _ : Int = _index_id_62;
                            H(q1);
                            _index_id_62 += _step_id_67;
                        }

                    }

                }

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
                let n : Int = 3;
                ApplyOp_Empty__closure_(q, n);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(n : Int, q1 : Qubit) : Unit {
                {
                    {
                        let _range_id_59 : Range = 0..n;
                        mutable _index_id_62 : Int = _range_id_59::Start;
                        let _step_id_67 : Int = _range_id_59::Step;
                        let _end_id_72 : Int = _range_id_59::End;
                        while _step_id_67 > 0 and _index_id_62 <= _end_id_72 or _step_id_67 < 0 and _index_id_62 >= _end_id_72 {
                            let _ : Int = _index_id_62;
                            H(q1);
                            _index_id_62 += _step_id_67;
                        }

                    }

                }

            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__closure_(q : Qubit, __capture_0 : Int) : Unit {
                _lambda_(__capture_0, q);
            }
            // entry
            Main()
        "#]],
    );
}

/// Adjoint applied only at the *creation site*: `Adjoint S` is passed to a HOF
/// whose body calls `op(q)` plainly, so the specialization bakes in
/// `Adjoint S(q)`. Contrast with `specialize_body_side_adjoint` (adjoint on the
/// body call) and `specialize_double_adjoint_cancels` (both, which cancel).
#[test]
fn specialize_creation_site_adjoint() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(Adjoint S, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(Adjoint S, q);
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
                ApplyOp_AdjCtl__Adj_S_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__Adj_S_(q : Qubit) : Unit {
                Adjoint S(q);
            }
            // entry
            Main()
        "#]],
    );
}

/// Adjoint applied only on the *body call*: plain `S` is passed to a HOF whose
/// body calls `Adjoint op(q)`, so the specialization bakes in `Adjoint S(q)`.
/// Contrast with `specialize_creation_site_adjoint` (adjoint at the argument)
/// and `specialize_double_adjoint_cancels` (both, which cancel).
#[test]
fn specialize_body_side_adjoint() {
    check_rewrite(
        r#"
        operation ApplyAdj(op : Qubit => Unit is Adj, q : Qubit) : Unit {
            Adjoint op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyAdj(S, q);
        }
        "#,
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

/// Adjoint applied at *both* the creation site (`Adjoint S`) and the body call
/// (`Adjoint op(q)`): functor composition cancels the two adjoints, so the
/// specialization bakes in plain `S(q)`. Contrast with the single-adjoint
/// siblings `specialize_creation_site_adjoint` and `specialize_body_side_adjoint`.
#[test]
fn specialize_double_adjoint_cancels() {
    check_rewrite(
        r#"
        operation ApplyAdj(op : Qubit => Unit is Adj, q : Qubit) : Unit {
            Adjoint op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyAdj(Adjoint S, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyAdj(op : (Qubit => Unit), q : Qubit) : Unit {
                Adjoint op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyAdj_AdjCtl_(Adjoint S, q);
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
                ApplyAdj_AdjCtl__Adj_S_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyAdj_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                Adjoint op(q);
            }
            operation ApplyAdj_AdjCtl__Adj_S_(q : Qubit) : Unit {
                S(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_body_side_controlled() {
    check_rewrite(
        r#"
        operation ApplyCtl(op : Qubit => Unit is Ctl, ctl : Qubit, q : Qubit) : Unit {
            Controlled op([ctl], q);
        }
        operation Main() : Unit {
            use (ctl, q) = (Qubit(), Qubit());
            ApplyCtl(X, ctl, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyCtl(op : (Qubit => Unit), ctl : Qubit, q : Qubit) : Unit {
                Controlled op([ctl], q);
            }
            operation Main() : Unit {
                let _generated_ident_44 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_46 : Qubit = __quantum__rt__qubit_allocate();
                let (ctl : Qubit, q : Qubit) = (_generated_ident_44, _generated_ident_46);
                ApplyCtl_AdjCtl_(X, ctl, q);
                __quantum__rt__qubit_release(_generated_ident_46);
                __quantum__rt__qubit_release(_generated_ident_44);
            }
            operation ApplyCtl_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), ctl : Qubit, q : Qubit) : Unit {
                Controlled op([ctl], q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyCtl(op : (Qubit => Unit), ctl : Qubit, q : Qubit) : Unit {
                Controlled op([ctl], q);
            }
            operation Main() : Unit {
                let _generated_ident_44 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_46 : Qubit = __quantum__rt__qubit_allocate();
                let (ctl : Qubit, q : Qubit) = (_generated_ident_44, _generated_ident_46);
                ApplyCtl_AdjCtl__X_(ctl, q);
                __quantum__rt__qubit_release(_generated_ident_46);
                __quantum__rt__qubit_release(_generated_ident_44);
            }
            operation ApplyCtl_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), ctl : Qubit, q : Qubit) : Unit {
                Controlled op([ctl], q);
            }
            operation ApplyCtl_AdjCtl__X_(ctl : Qubit, q : Qubit) : Unit {
                Controlled X([ctl], q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_body_controlled_adjoint_nested() {
    check_rewrite(
        r#"
        operation ApplyCtlAdj(op : Qubit => Unit is Adj + Ctl, ctl : Qubit, q : Qubit) : Unit {
            Controlled Adjoint op([ctl], q);
        }
        operation Main() : Unit {
            use (ctl, q) = (Qubit(), Qubit());
            ApplyCtlAdj(S, ctl, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyCtlAdj(op : (Qubit => Unit), ctl : Qubit, q : Qubit) : Unit {
                Controlled Adjoint op([ctl], q);
            }
            operation Main() : Unit {
                let _generated_ident_45 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_47 : Qubit = __quantum__rt__qubit_allocate();
                let (ctl : Qubit, q : Qubit) = (_generated_ident_45, _generated_ident_47);
                ApplyCtlAdj_AdjCtl_(S, ctl, q);
                __quantum__rt__qubit_release(_generated_ident_47);
                __quantum__rt__qubit_release(_generated_ident_45);
            }
            operation ApplyCtlAdj_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), ctl : Qubit, q : Qubit) : Unit {
                Controlled Adjoint op([ctl], q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyCtlAdj(op : (Qubit => Unit), ctl : Qubit, q : Qubit) : Unit {
                Controlled Adjoint op([ctl], q);
            }
            operation Main() : Unit {
                let _generated_ident_45 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_47 : Qubit = __quantum__rt__qubit_allocate();
                let (ctl : Qubit, q : Qubit) = (_generated_ident_45, _generated_ident_47);
                ApplyCtlAdj_AdjCtl__S_(ctl, q);
                __quantum__rt__qubit_release(_generated_ident_47);
                __quantum__rt__qubit_release(_generated_ident_45);
            }
            operation ApplyCtlAdj_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), ctl : Qubit, q : Qubit) : Unit {
                Controlled Adjoint op([ctl], q);
            }
            operation ApplyCtlAdj_AdjCtl__S_(ctl : Qubit, q : Qubit) : Unit {
                Controlled Adjoint S([ctl], q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_creation_adjoint_body_controlled() {
    check_rewrite(
        r#"
        operation ApplyCtl(op : Qubit => Unit is Adj + Ctl, ctl : Qubit, q : Qubit) : Unit {
            Controlled op([ctl], q);
        }
        operation Main() : Unit {
            use (ctl, q) = (Qubit(), Qubit());
            ApplyCtl(Adjoint S, ctl, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyCtl(op : (Qubit => Unit), ctl : Qubit, q : Qubit) : Unit {
                Controlled op([ctl], q);
            }
            operation Main() : Unit {
                let _generated_ident_45 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_47 : Qubit = __quantum__rt__qubit_allocate();
                let (ctl : Qubit, q : Qubit) = (_generated_ident_45, _generated_ident_47);
                ApplyCtl_AdjCtl_(Adjoint S, ctl, q);
                __quantum__rt__qubit_release(_generated_ident_47);
                __quantum__rt__qubit_release(_generated_ident_45);
            }
            operation ApplyCtl_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), ctl : Qubit, q : Qubit) : Unit {
                Controlled op([ctl], q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyCtl(op : (Qubit => Unit), ctl : Qubit, q : Qubit) : Unit {
                Controlled op([ctl], q);
            }
            operation Main() : Unit {
                let _generated_ident_45 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_47 : Qubit = __quantum__rt__qubit_allocate();
                let (ctl : Qubit, q : Qubit) = (_generated_ident_45, _generated_ident_47);
                ApplyCtl_AdjCtl__Adj_S_(ctl, q);
                __quantum__rt__qubit_release(_generated_ident_47);
                __quantum__rt__qubit_release(_generated_ident_45);
            }
            operation ApplyCtl_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), ctl : Qubit, q : Qubit) : Unit {
                Controlled op([ctl], q);
            }
            operation ApplyCtl_AdjCtl__Adj_S_(ctl : Qubit, q : Qubit) : Unit {
                Controlled Adjoint S([ctl], q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_hof_with_adj_autogen() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
            body ... { op(q); }
            adjoint auto;
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(S, q);
            Adjoint ApplyOp(S, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit is Adj {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(S, q);
                Adjoint ApplyOp_AdjCtl_(S, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit is Adj {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit is Adj {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__S_(q);
                Adjoint ApplyOp_AdjCtl__S_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit is Adj {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
            }
            operation ApplyOp_AdjCtl__S_(q : Qubit) : Unit is Adj {
                body ... {
                    S(q);
                }
                adjoint ... {
                    Adjoint S(q);
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_hof_with_ctl_autogen() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit is Ctl, q : Qubit) : Unit is Ctl {
            body ... { op(q); }
            controlled auto;
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(X, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit is Ctl {
                body ... {
                    op(q);
                }
                controlled (ctls, ...) {
                    Controlled op(ctls, q);
                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(X, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit is Ctl {
                body ... {
                    op(q);
                }
                controlled (ctls, ...) {
                    Controlled op(ctls, q);
                }
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit is Ctl {
                body ... {
                    op(q);
                }
                controlled (ctls, ...) {
                    Controlled op(ctls, q);
                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__X_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit is Ctl {
                body ... {
                    op(q);
                }
                controlled (ctls, ...) {
                    Controlled op(ctls, q);
                }
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit is Ctl {
                body ... {
                    X(q);
                }
                controlled (ctls, ...) {
                    Controlled X(ctls, q);
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_hof_with_adj_ctl_autogen() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit is Adj + Ctl, q : Qubit) : Unit is Adj + Ctl {
            body ... { op(q); }
            adjoint auto;
            controlled auto;
            controlled adjoint auto;
        }
        operation Main() : Unit {
            use (ctl, q) = (Qubit(), Qubit());
            ApplyOp(S, q);
        }
        "#,
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
                let _generated_ident_73 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_75 : Qubit = __quantum__rt__qubit_allocate();
                let (ctl : Qubit, q : Qubit) = (_generated_ident_73, _generated_ident_75);
                ApplyOp_AdjCtl_(S, q);
                __quantum__rt__qubit_release(_generated_ident_75);
                __quantum__rt__qubit_release(_generated_ident_73);
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
                let _generated_ident_73 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_75 : Qubit = __quantum__rt__qubit_allocate();
                let (ctl : Qubit, q : Qubit) = (_generated_ident_73, _generated_ident_75);
                ApplyOp_AdjCtl__S_(q);
                __quantum__rt__qubit_release(_generated_ident_75);
                __quantum__rt__qubit_release(_generated_ident_73);
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
fn specialize_single_assignment_local() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let myH = H;
            ApplyOp(myH, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let myH : (Qubit => Unit is Adj + Ctl) = H;
                ApplyOp_AdjCtl_(myH, q);
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
fn defunctionalized_call_site_drops_callable_argument() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
        }
        "#;
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
    assert_eq!(
        call_arg_tuple_lengths_after_defunc(source, "ApplyOp<AdjCtl>{H}"),
        vec![1],
        "defunctionalized ApplyOp call should pass only the qubit argument"
    );
}

#[test]
fn rewrite_closure_capture_args_inserted() {
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
    assert_eq!(
        call_arg_tuple_lengths_after_defunc(source, "ApplyOp<Empty>{closure}"),
        vec![2],
        "rewritten closure call should pass the qubit and captured angle"
    );
}

#[test]
fn multiple_callable_parameters_specialize_independently() {
    check_rewrite(
        r#"
        operation ApplyTwo(f : Qubit => Unit, g : Qubit => Unit, q : Qubit) : Unit {
            f(q);
            g(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyTwo(H, X, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyTwo(f : (Qubit => Unit), g : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
                g(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyTwo_AdjCtl__AdjCtl_(H, X, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyTwo_AdjCtl__AdjCtl_(f : (Qubit => Unit is Adj + Ctl), g : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                f(q);
                g(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyTwo(f : (Qubit => Unit), g : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
                g(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyTwo_AdjCtl__AdjCtl__H__X_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyTwo_AdjCtl__AdjCtl_(f : (Qubit => Unit is Adj + Ctl), g : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                f(q);
                g(q);
            }
            operation ApplyTwo_AdjCtl__AdjCtl__H_(g : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                H(q);
                g(q);
            }
            operation ApplyTwo_AdjCtl__AdjCtl__X_(g : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                X(q);
                g(q);
            }
            operation ApplyTwo_AdjCtl__AdjCtl__H__X_(q : Qubit) : Unit {
                H(q);
                X(q);
            }
            // entry
            Main()
        "#]],
    );
}

/// Focused coverage for `reindex_sibling_field_access` with more than two
/// callable fields in a single tuple-typed parameter.
///
/// The two-callable-field case only exercises the `Collapse` arm (removing one
/// of two siblings leaves a single element, so the tuple slot collapses). With
/// three callable fields the first removal leaves a two-element tuple, so the
/// later siblings must be *reindexed* (shifted down by one) rather than
/// collapsed — that is the `Reindex` arm. A field-index mix-up here would emit
/// the per-field gates out of order or dispatch the wrong callable, so the
/// snapshot pins `First -> H`, `Second -> X`, `Third -> Y` in that order.
#[test]
fn three_callable_field_tuple_param_reindexes_siblings() {
    check_rewrite(
        r#"
        operation RunOps(ops : (Qubit => Unit, Qubit => Unit, Qubit => Unit), q : Qubit) : Unit {
            let (first, second, third) = ops;
            first(q);
            second(q);
            third(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            RunOps((H, X, Y), q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation RunOps(ops : ((Qubit => Unit), (Qubit => Unit), (Qubit => Unit)), q : Qubit) : Unit {
                let (first : (Qubit => Unit), second : (Qubit => Unit), third : (Qubit => Unit)) = ops;
                first(q);
                second(q);
                third(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                RunOps_AdjCtl__AdjCtl__AdjCtl_((H, X, Y), q);
                __quantum__rt__qubit_release(q);
            }
            operation RunOps_AdjCtl__AdjCtl__AdjCtl_(ops : ((Qubit => Unit is Adj + Ctl), (Qubit => Unit is Adj + Ctl), (Qubit => Unit is Adj + Ctl)), q : Qubit) : Unit {
                let (first : (Qubit => Unit is Adj + Ctl), second : (Qubit => Unit is Adj + Ctl), third : (Qubit => Unit is Adj + Ctl)) = ops;
                first(q);
                second(q);
                third(q);
            }
            // entry
            Main()

            AFTER:
            operation RunOps(ops : ((Qubit => Unit), (Qubit => Unit), (Qubit => Unit)), q : Qubit) : Unit {
                let (first : (Qubit => Unit), second : (Qubit => Unit), third : (Qubit => Unit)) = ops;
                first(q);
                second(q);
                third(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                RunOps_AdjCtl__AdjCtl__AdjCtl__H__X__Y_(q);
                __quantum__rt__qubit_release(q);
            }
            operation RunOps_AdjCtl__AdjCtl__AdjCtl_(ops : ((Qubit => Unit is Adj + Ctl), (Qubit => Unit is Adj + Ctl), (Qubit => Unit is Adj + Ctl)), q : Qubit) : Unit {
                let (first : (Qubit => Unit is Adj + Ctl), second : (Qubit => Unit is Adj + Ctl), third : (Qubit => Unit is Adj + Ctl)) = ops;
                first(q);
                second(q);
                third(q);
            }
            operation RunOps_AdjCtl__AdjCtl__AdjCtl__H_(ops : ((Qubit => Unit is Adj + Ctl), (Qubit => Unit is Adj + Ctl)), q : Qubit) : Unit {
                let (second : (Qubit => Unit is Adj + Ctl), third : (Qubit => Unit is Adj + Ctl)) = ops;
                H(q);
                second(q);
                third(q);
            }
            operation RunOps_AdjCtl__AdjCtl__AdjCtl__X_(ops : ((Qubit => Unit is Adj + Ctl), (Qubit => Unit is Adj + Ctl)), q : Qubit) : Unit {
                let (second : (Qubit => Unit is Adj + Ctl), third : (Qubit => Unit is Adj + Ctl)) = ops;
                X(q);
                second(q);
                third(q);
            }
            operation RunOps_AdjCtl__AdjCtl__AdjCtl__Y_(ops : ((Qubit => Unit is Adj + Ctl), (Qubit => Unit is Adj + Ctl)), q : Qubit) : Unit {
                let (second : (Qubit => Unit is Adj + Ctl), third : (Qubit => Unit is Adj + Ctl)) = ops;
                Y(q);
                second(q);
                third(q);
            }
            operation RunOps_AdjCtl__AdjCtl__AdjCtl__H__X_(ops : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                let third : (Qubit => Unit is Adj + Ctl) = ops;
                H(q);
                X(q);
                third(q);
            }
            operation RunOps_AdjCtl__AdjCtl__AdjCtl__H__Y_(ops : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                let third : (Qubit => Unit is Adj + Ctl) = ops;
                H(q);
                Y(q);
                third(q);
            }
            operation RunOps_AdjCtl__AdjCtl__AdjCtl__H__X__Y_(q : Qubit) : Unit {
                H(q);
                X(q);
                Y(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn capture_local_ids_are_reasonable() {
    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(
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
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners);
    assert_no_defunctionalization_errors("defunctionalization", &errors);
    let package = fir_store.get(fir_pkg_id);

    let mut capture_binding_count = 0;
    for (_, pat) in &package.pats {
        if let fir::PatKind::Bind(ident) = &pat.kind {
            let id: u32 = ident.id.into();
            assert!(
                id < 10_000,
                "LocalVarId {id} is unreasonably large -- capture IDs should be sequential, not u32::MAX-based"
            );
            if ident.name.starts_with(CAPTURE_NAME_PREFIX) {
                capture_binding_count += 1;
            }
        }
    }
    assert_eq!(
        capture_binding_count, 1,
        "the `angle` capture should produce exactly one capture binding, proving the \
         capture-threading path actually ran rather than vacuously passing with no captures"
    );
}

#[test]
fn pipeline_with_captures_no_tuple_decompose_panic() {
    use crate::test_utils::{PipelineStage, compile_and_run_pipeline_to};

    let (_store, _pkg_id) = compile_and_run_pipeline_to(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let pair = (1.0, 2.0);
            let (a, b) = pair;
            ApplyOp(q1 => Rx(a + b, q1), q);
        }
        "#,
        PipelineStage::Full,
    );
}

#[test]
fn multiple_captures_sequential_ids() {
    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let a = 1.0;
            let b = 2.0;
            let c = 3.0;
            ApplyOp(q1 => { Rx(a, q1); Ry(b, q1); Rz(c, q1); }, q);
        }
        "#,
    );
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners);
    assert_no_defunctionalization_errors("defunctionalization", &errors);
    let package = fir_store.get(fir_pkg_id);

    let mut capture_ids: Vec<u32> = Vec::new();
    for (_, pat) in &package.pats {
        if let fir::PatKind::Bind(ident) = &pat.kind
            && ident.name.starts_with(CAPTURE_NAME_PREFIX)
        {
            let id: u32 = ident.id.into();
            capture_ids.push(id);
        }
    }

    assert!(
        capture_ids.len() >= 3,
        "expected at least 3 capture bindings, found {}",
        capture_ids.len()
    );

    for &id in &capture_ids {
        assert!(id < 10_000, "capture LocalVarId {id} is unreasonably large");
    }

    capture_ids.sort_unstable();
    for window in capture_ids.windows(2) {
        assert_eq!(
            window[1] - window[0],
            1,
            "capture IDs should be sequential, got {} and {}",
            window[0],
            window[1]
        );
    }
}

#[test]
fn specialize_closure_capturing_immutable_variable() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 1.0;
            ApplyOp(q1 => Rx(angle, q1), q);
        }
        "#,
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
fn specialize_closure_in_while_loop_body() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            mutable n = 3;
            while n > 0 {
                ApplyOp(q1 => H(q1), q);
                n -= 1;
            }
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable n : Int = 3;
                let _generated_ident_62 : Unit = while n > 0 {
                    ApplyOp_Empty_(/ * closure item = 3 captures = [] * / _lambda_, q);
                    n -= 1;
                };
                __quantum__rt__qubit_release(q);
                _generated_ident_62
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
                mutable n : Int = 3;
                let _generated_ident_62 : Unit = while n > 0 {
                    ApplyOp_Empty__H_(q);
                    n -= 1;
                };
                __quantum__rt__qubit_release(q);
                _generated_ident_62
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
fn specialize_multiple_closures_same_signature() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => H(q1), q);
            ApplyOp(q1 => X(q1), q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty_(/ * closure item = 3 captures = [] * / _lambda_, q);
                ApplyOp_Empty_(/ * closure item = 4 captures = [] * / _lambda_, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(q1 : Qubit, ) : Unit {
                H(q1)
            }
            operation _lambda_(q1 : Qubit, ) : Unit {
                X(q1)
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
                ApplyOp_Empty__X_(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(q1 : Qubit, ) : Unit {
                H(q1)
            }
            operation _lambda_(q1 : Qubit, ) : Unit {
                X(q1)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            operation ApplyOp_Empty__X_(q : Qubit) : Unit {
                X(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn branch_split_two_callees() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let f = if true { H } else { X };
            ApplyOp(f, q);
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
                let f : (Qubit => Unit is Adj + Ctl) = if true {
                    H
                } else {
                    X
                };
                ApplyOp_AdjCtl_(f, q);
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
                if true {
                    ApplyOp_AdjCtl__H_(q)
                } else {
                    ApplyOp_AdjCtl__X_(q)
                };
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
fn branch_split_three_callees() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let f = if true { H } elif false { X } else { S };
            ApplyOp(f, q);
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
                let f : (Qubit => Unit is Adj + Ctl) = if true {
                    H
                } else if false {
                    X
                } else {
                    S
                };
                ApplyOp_AdjCtl_(f, q);
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
                if true {
                    ApplyOp_AdjCtl__H_(q)
                } else if false {
                    ApplyOp_AdjCtl__X_(q)
                } else {
                    ApplyOp_AdjCtl__S_(q)
                };
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
            operation ApplyOp_AdjCtl__S_(q : Qubit) : Unit {
                S(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn branch_split_mutable_conditional() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            if true { set op = X; }
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
fn branch_split_nested_callable_in_tuple() {
    let source = r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let (op, _) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let f = if true { H } else { X };
            Wrapper((f, 42), q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Wrapper(pair : ((Qubit => Unit), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit), _ : Int) = pair;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let f : (Qubit => Unit is Adj + Ctl) = if true {
                    H
                } else {
                    X
                };
                Wrapper_AdjCtl_((f, 42), q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), _ : Int) = pair;
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation Wrapper(pair : ((Qubit => Unit), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit), _ : Int) = pair;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                if true {
                    Wrapper_AdjCtl__H_(42, q)
                } else {
                    Wrapper_AdjCtl__X_(42, q)
                };
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), _ : Int) = pair;
                op(q);
            }
            operation Wrapper_AdjCtl__H_(pair : Int, q : Qubit) : Unit {
                let _ : Int = pair;
                H(q);
            }
            operation Wrapper_AdjCtl__X_(pair : Int, q : Qubit) : Unit {
                let _ : Int = pair;
                X(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn branch_split_nested_callable_in_tuple_args_consistency() {
    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(
        r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let (op, _) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let f = if true { H } else { X };
            Wrapper((f, 42), q);
        }
        "#,
    );
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners);
    assert_no_defunctionalization_errors("defunctionalization", &errors);
    let package = fir_store.get(fir_pkg_id);

    let mut mismatches = Vec::new();
    for (expr_id, expr) in &package.exprs {
        if let fir::ExprKind::Call(_callee_id, args_id) = &expr.kind {
            let args_expr = package.get_expr(*args_id);
            if let fir::ExprKind::Tuple(elements) = &args_expr.kind
                && let qsc_fir::ty::Ty::Tuple(type_elems) = &args_expr.ty
            {
                if elements.len() != type_elems.len() {
                    mismatches.push(format!(
                        "Call expr {expr_id}: args tuple has {} elements but type has {} elements",
                        elements.len(),
                        type_elems.len()
                    ));
                }
                for (i, (&elem_id, ty_elem)) in elements.iter().zip(type_elems.iter()).enumerate() {
                    let elem_expr = package.get_expr(elem_id);
                    let elem_is_tuple = matches!(elem_expr.kind, fir::ExprKind::Tuple(_));
                    let ty_is_tuple = matches!(ty_elem, qsc_fir::ty::Ty::Tuple(_));
                    if elem_is_tuple != ty_is_tuple {
                        mismatches.push(format!(
                            "Call expr {expr_id}: args[{i}] is_tuple={elem_is_tuple} but type is_tuple={ty_is_tuple} (elem_ty={}, type_elem={ty_elem})",
                            elem_expr.ty,
                        ));
                    }
                }
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "Type/value mismatches in branch-split args:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn branch_split_nested_callable_full_pipeline() {
    use crate::test_utils::{PipelineStage, compile_and_run_pipeline_to};

    let (_store, _pkg_id) = compile_and_run_pipeline_to(
        r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let (op, _) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let f = if true { H } else { X };
            Wrapper((f, 42), q);
        }
        "#,
        PipelineStage::Full,
    );
}

#[test]
fn specialize_nested_callable_first_element() {
    check_rewrite(
        r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let (op, _) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((H, 42), q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Wrapper(pair : ((Qubit => Unit), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit), _ : Int) = pair;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl_((H, 42), q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), _ : Int) = pair;
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation Wrapper(pair : ((Qubit => Unit), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit), _ : Int) = pair;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl__H_(42, q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), _ : Int) = pair;
                op(q);
            }
            operation Wrapper_AdjCtl__H_(pair : Int, q : Qubit) : Unit {
                let _ : Int = pair;
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_nested_callable_second_element() {
    check_rewrite(
        r#"
        operation Wrapper(pair : (Int, Qubit => Unit), q : Qubit) : Unit {
            let (_, op) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((42, H), q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Wrapper(pair : (Int, (Qubit => Unit)), q : Qubit) : Unit {
                let (_ : Int, op : (Qubit => Unit)) = pair;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl_((42, H), q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : (Int, (Qubit => Unit is Adj + Ctl)), q : Qubit) : Unit {
                let (_ : Int, op : (Qubit => Unit is Adj + Ctl)) = pair;
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation Wrapper(pair : (Int, (Qubit => Unit)), q : Qubit) : Unit {
                let (_ : Int, op : (Qubit => Unit)) = pair;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl__H_(42, q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : (Int, (Qubit => Unit is Adj + Ctl)), q : Qubit) : Unit {
                let (_ : Int, op : (Qubit => Unit is Adj + Ctl)) = pair;
                op(q);
            }
            operation Wrapper_AdjCtl__H_(pair : Int, q : Qubit) : Unit {
                let _ : Int = pair;
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_nested_callable_both_fields_used() {
    check_rewrite(
        r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let (op, n) = pair;
            op(q);
            let _ = n;
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((H, 42), q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Wrapper(pair : ((Qubit => Unit), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit), n : Int) = pair;
                op(q);
                let _ : Int = n;
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl_((H, 42), q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), n : Int) = pair;
                op(q);
                let _ : Int = n;
            }
            // entry
            Main()

            AFTER:
            operation Wrapper(pair : ((Qubit => Unit), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit), n : Int) = pair;
                op(q);
                let _ : Int = n;
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl__H_(42, q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), n : Int) = pair;
                op(q);
                let _ : Int = n;
            }
            operation Wrapper_AdjCtl__H_(pair : Int, q : Qubit) : Unit {
                let n : Int = pair;
                H(q);
                let _ : Int = n;
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_nested_callable_transitive_alias() {
    check_rewrite(
        r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let (op, _) = pair;
            let f = op;
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((H, 42), q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Wrapper(pair : ((Qubit => Unit), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit), _ : Int) = pair;
                let f : (Qubit => Unit) = op;
                f(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl_((H, 42), q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), _ : Int) = pair;
                let f : (Qubit => Unit is Adj + Ctl) = op;
                f(q);
            }
            // entry
            Main()

            AFTER:
            operation Wrapper(pair : ((Qubit => Unit), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit), _ : Int) = pair;
                let f : (Qubit => Unit) = op;
                f(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl__H_(42, q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), _ : Int) = pair;
                let f : (Qubit => Unit is Adj + Ctl) = op;
                f(q);
            }
            operation Wrapper_AdjCtl__H_(pair : Int, q : Qubit) : Unit {
                let _ : Int = pair;
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_nested_callable_invariants() {
    let source = r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let (op, _) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((H, 42), q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Wrapper(pair : ((Qubit => Unit), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit), _ : Int) = pair;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl_((H, 42), q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), _ : Int) = pair;
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation Wrapper(pair : ((Qubit => Unit), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit), _ : Int) = pair;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl__H_(42, q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), _ : Int) = pair;
                op(q);
            }
            operation Wrapper_AdjCtl__H_(pair : Int, q : Qubit) : Unit {
                let _ : Int = pair;
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_nested_callable_full_pipeline() {
    use crate::test_utils::{PipelineStage, compile_and_run_pipeline_to};

    let (_store, _pkg_id) = compile_and_run_pipeline_to(
        r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let (op, _) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((H, 42), q);
        }
        "#,
        PipelineStage::Full,
    );
}

#[test]
fn branch_split_nested_callable_adj_ctl_args_consistency() {
    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(
        r#"
        operation Op1(q : Qubit) : Unit is Adj + Ctl { H(q); }
        operation Op2(q : Qubit) : Unit is Adj + Ctl { X(q); }
        operation Wrapper(pair : (Qubit => Unit is Adj + Ctl, Int), q : Qubit) : Unit {
            let (op, _) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let b = true;
            let f = if b { Op1 } else { Op2 };
            Wrapper((f, 42), q);
        }
        "#,
    );
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners);
    assert_no_defunctionalization_errors("defunctionalization", &errors);
    let package = fir_store.get(fir_pkg_id);

    let mut mismatches = Vec::new();
    for (expr_id, expr) in &package.exprs {
        if let fir::ExprKind::Call(_callee_id, args_id) = &expr.kind {
            let args_expr = package.get_expr(*args_id);
            if let fir::ExprKind::Tuple(elements) = &args_expr.kind
                && let qsc_fir::ty::Ty::Tuple(type_elems) = &args_expr.ty
                && elements.len() != type_elems.len()
            {
                mismatches.push(format!(
                    "Call expr {expr_id}: args tuple has {} elements but type has {} elements",
                    elements.len(),
                    type_elems.len()
                ));
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "Type/value mismatches in branch-split args:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn closure_with_multiple_captures_threads_all_captures() {
    check_rewrite(
        r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        operation Main() : Unit {
            use q = Qubit();
            let angle1 = 1.0;
            let angle2 = 2.0;
            let myOp = (q) => { Rx(angle1, q); Ry(angle2, q); };
            Apply(myOp, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Apply(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let angle1 : Double = 1.;
                let angle2 : Double = 2.;
                let myOp : (Qubit => Unit) = / * closure item = 3 captures = [angle1, angle2] * / _lambda_;
                Apply_Empty_(myOp, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(angle1 : Double, angle2 : Double, q : Qubit) : Unit {
                {
                    Rx(angle1, q);
                    Ry(angle2, q);
                }

            }
            operation Apply_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
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
                let angle1 : Double = 1.;
                let angle2 : Double = 2.;
                Apply_Empty__closure_(q, angle1, angle2);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(angle1 : Double, angle2 : Double, q : Qubit) : Unit {
                {
                    Rx(angle1, q);
                    Ry(angle2, q);
                }

            }
            operation Apply_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Apply_Empty__closure_(q : Qubit, __capture_0 : Double, __capture_1 : Double) : Unit {
                _lambda_(__capture_0, __capture_1, q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn single_param_tuple_containing_arrow_specializes_end_to_end() {
    check_rewrite(
        r#"
        operation Apply(pair : (Qubit => Unit, Qubit)) : Unit {
            let (op, q) = pair;
            op(q);
        }
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            Apply((H, q));
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Apply(pair : ((Qubit => Unit), Qubit)) : Unit {
                let (op : (Qubit => Unit), q : Qubit) = pair;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Apply_AdjCtl_(H, q);
                __quantum__rt__qubit_release(q);
            }
            operation Apply_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Qubit)) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), q : Qubit) = pair;
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation Apply(pair : ((Qubit => Unit), Qubit)) : Unit {
                let (op : (Qubit => Unit), q : Qubit) = pair;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Apply_AdjCtl__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation Apply_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Qubit)) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), q : Qubit) = pair;
                op(q);
            }
            operation Apply_AdjCtl__H_(pair : Qubit) : Unit {
                let q : Qubit = pair;
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn single_param_tuple_second_element_specializes_end_to_end() {
    check_rewrite(
        r#"
        operation Wrapper(pair : (Int, Qubit => Unit)) : Unit {
            let (_, op) = pair;
            use q = Qubit();
            op(q);
        }
        operation Main() : Unit {
            Wrapper((42, H));
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Wrapper(pair : (Int, (Qubit => Unit))) : Unit {
                let (_ : Int, op : (Qubit => Unit)) = pair;
                let q : Qubit = __quantum__rt__qubit_allocate();
                op(q);
                __quantum__rt__qubit_release(q);
            }
            operation Main() : Unit {
                Wrapper_AdjCtl_(42, H);
            }
            operation Wrapper_AdjCtl_(pair : (Int, (Qubit => Unit is Adj + Ctl))) : Unit {
                let (_ : Int, op : (Qubit => Unit is Adj + Ctl)) = pair;
                let q : Qubit = __quantum__rt__qubit_allocate();
                op(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation Wrapper(pair : (Int, (Qubit => Unit))) : Unit {
                let (_ : Int, op : (Qubit => Unit)) = pair;
                let q : Qubit = __quantum__rt__qubit_allocate();
                op(q);
                __quantum__rt__qubit_release(q);
            }
            operation Main() : Unit {
                Wrapper_AdjCtl__H_(42);
            }
            operation Wrapper_AdjCtl_(pair : (Int, (Qubit => Unit is Adj + Ctl))) : Unit {
                let (_ : Int, op : (Qubit => Unit is Adj + Ctl)) = pair;
                let q : Qubit = __quantum__rt__qubit_allocate();
                op(q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl__H_(pair : Int) : Unit {
                let _ : Int = pair;
                let q : Qubit = __quantum__rt__qubit_allocate();
                H(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn single_param_recursive_tuple_callable_specializes_end_to_end() {
    check_rewrite(
        r#"
        operation Wrapper(bundle : (((Qubit => Unit, Int), Double), Qubit)) : Unit {
            let (((op, n), angle), q) = bundle;
            let _ = n;
            let _ = angle;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((((H, 42), 1.0), q));
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Wrapper(bundle : ((((Qubit => Unit), Int), Double), Qubit)) : Unit {
                let (((op : (Qubit => Unit), n : Int), angle : Double), q : Qubit) = bundle;
                let _ : Int = n;
                let _ : Double = angle;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl_(((H, 42), 1.), q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(bundle : ((((Qubit => Unit is Adj + Ctl), Int), Double), Qubit)) : Unit {
                let (((op : (Qubit => Unit is Adj + Ctl), n : Int), angle : Double), q : Qubit) = bundle;
                let _ : Int = n;
                let _ : Double = angle;
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation Wrapper(bundle : ((((Qubit => Unit), Int), Double), Qubit)) : Unit {
                let (((op : (Qubit => Unit), n : Int), angle : Double), q : Qubit) = bundle;
                let _ : Int = n;
                let _ : Double = angle;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl__H_((42, 1.), q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(bundle : ((((Qubit => Unit is Adj + Ctl), Int), Double), Qubit)) : Unit {
                let (((op : (Qubit => Unit is Adj + Ctl), n : Int), angle : Double), q : Qubit) = bundle;
                let _ : Int = n;
                let _ : Double = angle;
                op(q);
            }
            operation Wrapper_AdjCtl__H_(bundle : ((Int, Double), Qubit)) : Unit {
                let ((n : Int, angle : Double), q : Qubit) = bundle;
                let _ : Int = n;
                let _ : Double = angle;
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn single_param_recursive_tuple_callable_closure_capture_invariants() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Wrapper(bundle : (((Qubit => Unit, Int), Double), Qubit)) : Unit {
            let (((op, n), angle), q) = bundle;
            ApplyOp(
                q1 => {
                    if n == 0 {
                        Rx(angle, q1);
                    }
                    op(q1);
                },
                q
            );
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((((H, 0), 1.0), q));
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
            operation Wrapper(bundle : ((((Qubit => Unit), Int), Double), Qubit)) : Unit {
                let (((op : (Qubit => Unit), n : Int), angle : Double), q : Qubit) = bundle;
                ApplyOp_Empty_(/ * closure item = 4 captures = [op, n, angle] * / _lambda_, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl_(((H, 0), 1.), q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(op : (Qubit => Unit), n : Int, angle : Double, q1 : Qubit) : Unit {
                {
                    if n == 0 {
                        Rx(angle, q1);
                    }

                    op(q1);
                }

            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Wrapper_AdjCtl_(bundle : ((((Qubit => Unit is Adj + Ctl), Int), Double), Qubit)) : Unit {
                let (((op : (Qubit => Unit is Adj + Ctl), n : Int), angle : Double), q : Qubit) = bundle;
                ApplyOp_Empty_(/ * closure item = 7 captures = [op, n, angle] * / _lambda_, q);
            }
            operation _lambda_(op : (Qubit => Unit is Adj + Ctl), n : Int, angle : Double, q1 : Qubit) : Unit {
                {
                    if n == 0 {
                        Rx(angle, q1);
                    }

                    op(q1);
                }

            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Wrapper(bundle : ((((Qubit => Unit), Int), Double), Qubit)) : Unit {
                let (((op : (Qubit => Unit), n : Int), angle : Double), q : Qubit) = bundle;
                ApplyOp_Empty_(/ * closure item = 4 captures = [op, n, angle] * / _lambda_, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl__H_((0, 1.), q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(op : (Qubit => Unit), n : Int, angle : Double, q1 : Qubit) : Unit {
                {
                    if n == 0 {
                        Rx(angle, q1);
                    }

                    op(q1);
                }

            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Wrapper_AdjCtl_(bundle : ((((Qubit => Unit is Adj + Ctl), Int), Double), Qubit)) : Unit {
                let (((op : (Qubit => Unit is Adj + Ctl), n : Int), angle : Double), q : Qubit) = bundle;
                ApplyOp_Empty__closure_(q, op, n, angle);
            }
            operation _lambda_(op : (Qubit => Unit is Adj + Ctl), n : Int, angle : Double, q1 : Qubit) : Unit {
                {
                    if n == 0 {
                        Rx(angle, q1);
                    }

                    op(q1);
                }

            }
            operation Wrapper_AdjCtl__H_(bundle : ((Int, Double), Qubit)) : Unit {
                let ((n : Int, angle : Double), q : Qubit) = bundle;
                ApplyOp_Empty__closure_(q, n, angle);
            }
            operation _lambda_(n : Int, angle : Double, q1 : Qubit) : Unit {
                {
                    if n == 0 {
                        Rx(angle, q1);
                    }

                    H(q1);
                }

            }
            operation ApplyOp_Empty__closure_(q : Qubit, __capture_0 : (Qubit => Unit is Adj + Ctl), __capture_1 : Int, __capture_2 : Double) : Unit {
                _lambda_(__capture_0, __capture_1, __capture_2, q);
            }
            operation ApplyOp_Empty__closure_(q : Qubit, __capture_0 : Int, __capture_1 : Double) : Unit {
                _lambda_(__capture_0, __capture_1, q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn three_branch_conditional_callable_generates_branch_split() {
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
            } else {
                op = Z;
            }
            Apply(op, q);
        }
        "#;
    check_errors(source, &expect!["(no error)"]);
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
                } else {
                    op = Z;
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
                } else {
                    op = Z;
                }

                if n == 0 {
                    Apply_AdjCtl__X_(q)
                } else if n == 1 {
                    Apply_AdjCtl__Y_(q)
                } else {
                    Apply_AdjCtl__Z_(q)
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
            // entry
            Main()
        "#]],
    );
    let targets = callable_call_targets_after_defunc(source, "Main");
    assert!(
        targets.contains(&"Apply<AdjCtl>{X}".to_string())
            && targets.contains(&"Apply<AdjCtl>{Y}".to_string())
            && targets.contains(&"Apply<AdjCtl>{Z}".to_string()),
        "branch split should call X, Y, and Z specializations, got {targets:?}"
    );
}

#[test]
fn identity_closure_peephole_replaces_wrapper() {
    check_rewrite(
        r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        operation Main() : Unit {
            use q = Qubit();
            let wrapper = q => H(q);
            Apply(wrapper, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Apply(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let wrapper : (Qubit => Unit) = / * closure item = 3 captures = [] * / _lambda_;
                Apply_Empty_(wrapper, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(q : Qubit, ) : Unit {
                H(q)
            }
            operation Apply_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
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
                Apply_Empty__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(q : Qubit, ) : Unit {
                H(q)
            }
            operation Apply_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Apply_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn excessive_specializations_warning_emitted() {
    // A HOF called with > 10 different concrete closures triggers the
    // ExcessiveSpecializations warning. Each distinct Rx(angle, _) partial
    // application with a different angle creates a distinct closure, and
    // all closures map to the same functorless Apply<Empty> variant.
    check_errors(
        r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            Apply(q1 => Rx(1.0, q1), q);
            Apply(q1 => Rx(2.0, q1), q);
            Apply(q1 => Rx(3.0, q1), q);
            Apply(q1 => Rx(4.0, q1), q);
            Apply(q1 => Rx(5.0, q1), q);
            Apply(q1 => Rx(6.0, q1), q);
            Apply(q1 => Rx(7.0, q1), q);
            Apply(q1 => Rx(8.0, q1), q);
            Apply(q1 => Rx(9.0, q1), q);
            Apply(q1 => Rx(10.0, q1), q);
            Apply(q1 => Rx(11.0, q1), q);
        }
        "#,
        &expect![[r#"
            higher-order function `Apply<Empty>` generated 11 specializations, exceeding the warning threshold"#]],
    );
}

#[test]
fn below_threshold_no_excessive_specializations_warning() {
    // A HOF with exactly 10 specializations should not trigger the warning.
    let source = r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            Apply(H, q);
            Apply(X, q);
            Apply(Y, q);
            Apply(Z, q);
            Apply(S, q);
            Apply(T, q);
            Apply(I, q);
            Apply(q1 => Rx(1.0, q1), q);
            Apply(q1 => Rx(2.0, q1), q);
            Apply(q1 => Rx(3.0, q1), q);
        }
        "#;
    check_errors(source, &expect!["(no error)"]);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Apply(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Apply_AdjCtl_(H, q);
                Apply_AdjCtl_(X, q);
                Apply_AdjCtl_(Y, q);
                Apply_AdjCtl_(Z, q);
                Apply_AdjCtl_(S, q);
                Apply_AdjCtl_(T, q);
                Apply_AdjCtl_(I, q);
                Apply_Empty_(/ * closure item = 3 captures = [] * / _lambda_, q);
                Apply_Empty_(/ * closure item = 4 captures = [] * / _lambda_, q);
                Apply_Empty_(/ * closure item = 5 captures = [] * / _lambda_, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(q1 : Qubit, ) : Unit {
                Rx(1., q1)
            }
            operation _lambda_(q1 : Qubit, ) : Unit {
                Rx(2., q1)
            }
            operation _lambda_(q1 : Qubit, ) : Unit {
                Rx(3., q1)
            }
            operation Apply_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation Apply_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
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
                Apply_AdjCtl__H_(q);
                Apply_AdjCtl__X_(q);
                Apply_AdjCtl__Y_(q);
                Apply_AdjCtl__Z_(q);
                Apply_AdjCtl__S_(q);
                Apply_AdjCtl__T_(q);
                Apply_AdjCtl__I_(q);
                Apply_Empty__closure_(q);
                Apply_Empty__closure_(q);
                Apply_Empty__closure_(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(q1 : Qubit, ) : Unit {
                Rx(1., q1)
            }
            operation _lambda_(q1 : Qubit, ) : Unit {
                Rx(2., q1)
            }
            operation _lambda_(q1 : Qubit, ) : Unit {
                Rx(3., q1)
            }
            operation Apply_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation Apply_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Apply_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
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
            operation Apply_AdjCtl__I_(q : Qubit) : Unit {
                I(q);
            }
            operation Apply_Empty__closure_(q : Qubit) : Unit {
                _lambda_(q, );
            }
            operation Apply_Empty__closure_(q : Qubit) : Unit {
                _lambda_(q, );
            }
            operation Apply_Empty__closure_(q : Qubit) : Unit {
                _lambda_(q, );
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn excessive_specializations_warning_does_not_block_compilation() {
    // A program that triggers ExcessiveSpecializations should still compile
    // successfully — the warning is non-fatal. We verify by running the
    // full defunctionalization and checking PostDefunc invariants hold.
    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(
        r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            Apply(q1 => Rx(1.0, q1), q);
            Apply(q1 => Rx(2.0, q1), q);
            Apply(q1 => Rx(3.0, q1), q);
            Apply(q1 => Rx(4.0, q1), q);
            Apply(q1 => Rx(5.0, q1), q);
            Apply(q1 => Rx(6.0, q1), q);
            Apply(q1 => Rx(7.0, q1), q);
            Apply(q1 => Rx(8.0, q1), q);
            Apply(q1 => Rx(9.0, q1), q);
            Apply(q1 => Rx(10.0, q1), q);
            Apply(q1 => Rx(11.0, q1), q);
        }
        "#,
    );
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners);

    // Should have exactly one warning, no fatal errors.
    let warnings: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, super::super::Error::ExcessiveSpecializations(..)))
        .collect();
    let fatal: Vec<_> = errors
        .iter()
        .filter(|e| !matches!(e, super::super::Error::ExcessiveSpecializations(..)))
        .collect();
    assert_eq!(warnings.len(), 1, "expected exactly one warning");
    assert!(fatal.is_empty(), "expected no fatal errors, got: {fatal:?}");

    // PostDefunc invariants must still hold.
    fir_invariants::check(&fir_store, fir_pkg_id, InvariantLevel::PostDefunc);
}

#[test]
fn zero_capture_conditional_alias_dispatches_correctly() {
    let source = r#"
        operation ZeroCaptureConditionalAlias(q : Qubit, useAdj : Bool) : Unit {
            let u = if useAdj { Adjoint S } else { S };
            u(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ZeroCaptureConditionalAlias(q, true);
        }
        "#;
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ZeroCaptureConditionalAlias(q : Qubit, useAdj : Bool) : Unit {
                let u : (Qubit => Unit is Adj + Ctl) = if useAdj {
                    Adjoint S
                } else {
                    S
                };
                u(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ZeroCaptureConditionalAlias(q, true);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation ZeroCaptureConditionalAlias(q : Qubit, useAdj : Bool) : Unit {
                if useAdj {
                    Adjoint S(q)
                } else {
                    S(q)
                };
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ZeroCaptureConditionalAlias(q, true);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
    let targets = callable_call_targets_after_defunc(source, "ZeroCaptureConditionalAlias");
    assert!(
        targets.contains(&"Adjoint S".to_string()) && targets.contains(&"S".to_string()),
        "conditional alias should preserve both S and Adjoint S dispatch targets, got {targets:?}"
    );
}

/// When an identity closure `q => H(q)` is eta-reduced and its direct call is
/// rewritten, the surviving direct `Call` expr must carry the original lambda
/// body span (`H(q)`), not the discarded `f(q)` call-site span, so circuit
/// instructions point at the lambda body, matching un-optimized evaluation.
#[test]
fn direct_call_preserves_lambda_body_span() {
    let source = "
        operation Main() : Unit {
            use q = Qubit();
            let f = q => H(q);
            f(q);
        }
    ";
    let (fir_store, fir_pkg_id) = compile_and_defunctionalize(source);
    let package = fir_store.get(fir_pkg_id);
    let decl = callable_decl(package, "Main");

    let mut call_span = None;
    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &decl.implementation,
        &mut |_expr_id, expr| {
            if let fir::ExprKind::Call(callee_id, _) = &expr.kind
                && call_target_name(&fir_store, package, *callee_id).as_deref() == Some("H")
            {
                call_span = Some(expr.span);
            }
        },
    );

    let span = call_span.expect("expected a surviving direct call to H in Main");
    let slice = &source[span.lo as usize..span.hi as usize];
    assert_eq!(
        slice, "H(q)",
        "surviving direct call should carry the lambda body span, got {slice:?}"
    );
}

/// A closure is created by a separate function, `MakeAdder`, that returns it,
/// then bound to a local, `adder`, and passed to a higher-order function,
/// `Apply`. Because the closure's capture, `base`, is defined across the
/// function-return boundary, the HOF call-site rewrite cannot resolve the
/// capture's value from the enclosing block and threads the wrong local.
#[test]
fn cross_function_closure_capture_threads_correct_value() {
    let source = r#"
        import Std.Convert.*;
        operation Apply(f : (Qubit => Unit), q : Qubit) : Unit {
            f(q);
        }

        operation ApplyRotation(base : Int, q : Qubit) : Unit {
            Rx(IntAsDouble(base), q);
        }

        function MakeRotation(base : Int) : (Qubit => Unit) {
            return ApplyRotation(base, _);
        }

        operation Main() : Unit {
            use q = Qubit();
            let amount = 5;
            let rotation = MakeRotation(amount);
            Apply(rotation, q);
        }
        "#;
    check_rewrite_with_capabilities(
        source,
        adaptive_qirgen_capabilities(),
        &expect![[r#"
            BEFORE:
            operation Apply(f : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
            }
            operation ApplyRotation(base : Int, q : Qubit) : Unit {
                Rx(IntAsDouble(base), q);
            }
            function MakeRotation(base : Int) : (Qubit => Unit) {
                return {
                    let arg : Int = base;
                    / * closure item = 5 captures = [arg] * / _lambda_
                };
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let amount : Int = 5;
                let rotation : (Qubit => Unit) = MakeRotation(amount);
                Apply_Empty_(rotation, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(arg : Int, hole : Qubit) : Unit {
                ApplyRotation(arg, hole)
            }
            operation Apply_Empty_(f : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
            }
            // entry
            Main()

            AFTER:
            operation Apply(f : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
            }
            operation ApplyRotation(base : Int, q : Qubit) : Unit {
                Rx(IntAsDouble(base), q);
            }
            function MakeRotation(base : Int) : (Qubit => Unit) {
                return {
                    let arg : Int = base;
                    ()
                };
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let amount : Int = 5;
                Apply_Empty__closure_(q, amount);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(arg : Int, hole : Qubit) : Unit {
                ApplyRotation(arg, hole)
            }
            operation Apply_Empty_(f : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
            }
            operation Apply_Empty__closure_(q : Qubit, __capture_0 : Int) : Unit {
                _lambda_(__capture_0, q);
            }
            // entry
            Main()
        "#]],
    );
}

/// The closure is created inline in the same block as the HOF call.
#[test]
fn inline_closure_capture_threads_correct_value() {
    let source = r#"
        import Std.Convert.*;
        operation Apply(f : (Qubit => Unit), q : Qubit) : Unit {
            f(q);
        }

        operation Main() : Unit {
            use q = Qubit();
            let amount = 5;
            Apply(qubit => Rx(IntAsDouble(amount), qubit), q);
        }
        "#;
    check_rewrite_with_capabilities(
        source,
        adaptive_qirgen_capabilities(),
        &expect![[r#"
            BEFORE:
            operation Apply(f : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let amount : Int = 5;
                Apply_Empty_(/ * closure item = 3 captures = [amount] * / _lambda_, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(amount : Int, qubit : Qubit) : Unit {
                Rx(IntAsDouble(amount), qubit)
            }
            operation Apply_Empty_(f : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
            }
            // entry
            Main()

            AFTER:
            operation Apply(f : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let amount : Int = 5;
                Apply_Empty__closure_(q, amount);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(amount : Int, qubit : Qubit) : Unit {
                Rx(IntAsDouble(amount), qubit)
            }
            operation Apply_Empty_(f : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
            }
            operation Apply_Empty__closure_(q : Qubit, __capture_0 : Int) : Unit {
                _lambda_(__capture_0, q);
            }
            // entry
            Main()
        "#]],
    );
}
