// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Many tests pair a primary assertion with a `check_rewrite` before/after
// snapshot, so the generated Q# pushes function bodies past the line limit.
#![allow(clippy::too_many_lines)]

use crate::defunctionalize::analysis::{LocalState, resolve_captures};

use super::*;
use expect_test::expect;
use qsc_data_structures::index_map::IndexMap;
use qsc_fir::fir::{LocalVarId, Package};
use rustc_hash::FxHashSet;

#[test]
fn analysis_no_callable_params() {
    let source = "operation Main() : Unit { }";
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 0
            call_sites: 0"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
        BEFORE:
        // namespace test
        operation Main() : Unit {}
        // entry
        Main()

        AFTER:
        // namespace test
        operation Main() : Unit {}
        // entry
        Main()
    "#]],
    );
}

#[test]
fn analysis_single_callable_param() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
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
            // namespace test
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
fn analysis_multiple_callable_params() {
    let source = r#"
        operation ApplyTwo(f : Qubit => Unit, g : Qubit => Unit, q : Qubit) : Unit {
            f(q);
            g(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyTwo(H, X, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 2
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
              param: callable_id=<item 3 in package 2>, path=[1], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 2
              site: hof=ApplyTwo<AdjCtl, AdjCtl>, arg=Global(H, Body)
              site: hof=ApplyTwo<AdjCtl, AdjCtl>, arg=Global(X, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
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
            // namespace test
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

#[test]
fn analysis_callable_param_in_tuple() {
    let source = r#"
        operation ApplySecond(q : Qubit, op : Qubit => Unit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplySecond(q, H);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[1], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplySecond<AdjCtl>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplySecond(q : Qubit, op : (Qubit => Unit)) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplySecond_AdjCtl_(q, H);
                __quantum__rt__qubit_release(q);
            }
            operation ApplySecond_AdjCtl_(q : Qubit, op : (Qubit => Unit is Adj + Ctl)) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation ApplySecond(q : Qubit, op : (Qubit => Unit)) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplySecond_AdjCtl__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplySecond_AdjCtl_(q : Qubit, op : (Qubit => Unit is Adj + Ctl)) : Unit {
                op(q);
            }
            operation ApplySecond_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_global_callable_arg() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(X, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(X, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(X, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__X_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
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
fn analysis_closure_callable_arg() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => H(q1), q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 4 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
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
            // namespace test
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
fn analysis_adjoint_callable_arg() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(Adjoint S, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(S, Adj)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
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
            // namespace test
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

#[test]
fn analysis_controlled_callable_arg() {
    let source = r#"
        operation ApplyOp(op : (Qubit[], Qubit) => Unit is Ctl, q : Qubit) : Unit {
            op([], q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(Controlled X, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(((Qubit)[], Qubit) => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(X, Ctl)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplyOp(op : ((Qubit[], Qubit) => Unit), q : Qubit) : Unit {
                op([], q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(Controlled X, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : ((Qubit[], Qubit) => Unit is Adj + Ctl), q : Qubit) : Unit {
                op([], q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation ApplyOp(op : ((Qubit[], Qubit) => Unit), q : Qubit) : Unit {
                op([], q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__Ctl_X_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : ((Qubit[], Qubit) => Unit is Adj + Ctl), q : Qubit) : Unit {
                op([], q);
            }
            operation ApplyOp_AdjCtl__Ctl_X_(q : Qubit) : Unit {
                Controlled X([], q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_multiple_call_sites_same_hof() {
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
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 2
              site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)
              site: hof=ApplyOp<AdjCtl>, arg=Global(X, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
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
            // namespace test
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
fn analysis_single_assignment_local_traced() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let myH = H;
            ApplyOp(myH, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)
            lattice states:
              callable Main:
                2: Single(H:Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
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
            // namespace test
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
fn analysis_dynamic_callable_detected() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            op = X;
            ApplyOp(op, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(X, Body)
            lattice states:
              callable Main:
                2: Single(X:Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                op = X;
                ApplyOp_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                op = X;
                ApplyOp_AdjCtl__X_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
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
fn udt_field_single_level_direct() {
    let source = r#"
        struct Config { Apply : Qubit => Unit }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let config = new Config { Apply = H };
            ApplyOp(config.Apply, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 4 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let config : __UDT_Item_1__Package_2_ = new Config {
                    Apply = H
                };
                ApplyOp_Empty_(config::Apply, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__H_(q);
                __quantum__rt__qubit_release(q);
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
fn udt_field_via_let_binding() {
    let source = r#"
        struct Config { Apply : Qubit => Unit }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let config = new Config { Apply = H };
            let f = config.Apply;
            ApplyOp(f, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 4 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Global(H, Body)
            lattice states:
              callable Main:
                3: Single(H:Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let config : __UDT_Item_1__Package_2_ = new Config {
                    Apply = H
                };
                let f : (Qubit => Unit) = config::Apply;
                ApplyOp_Empty_(f, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__H_(q);
                __quantum__rt__qubit_release(q);
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
fn udt_field_in_hof_body() {
    let source = r#"
        struct Config { Op : Qubit => Unit }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation RunWithConfig(config : Config, q : Qubit) : Unit {
            ApplyOp(config.Op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let config = new Config { Op = H };
            RunWithConfig(config, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 2
              param: callable_id=<item 5 in package 2>, path=[0], ty=(Qubit => Unit)
              param: callable_id=<item 3 in package 2>, path=[0, 0], ty=(Qubit => Unit)
            call_sites: 2
              site: hof=RunWithConfig, arg=Global(H, Body)
              site: hof=ApplyOp<Empty>, arg=Dynamic"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunWithConfig(config : __UDT_Item_1__Package_2_, q : Qubit) : Unit {
                ApplyOp_Empty_(config::Op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let config : __UDT_Item_1__Package_2_ = new Config {
                    Op = H
                };
                RunWithConfig(config, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunWithConfig(config : __UDT_Item_1__Package_2_, q : Qubit) : Unit {
                ApplyOp_Empty_(config::Op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                RunWithConfig_H_((), q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunWithConfig_H_(config : Unit, q : Qubit) : Unit {
                ApplyOp_Empty__H_(q);
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
fn udt_field_in_hof_body_defunctionalizes_end_to_end() {
    let source = r#"
        struct Config { Op : Qubit => Unit }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation RunWithConfig(config : Config, q : Qubit) : Unit {
            ApplyOp(config.Op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let config = new Config { Op = H };
            RunWithConfig(config, q);
        }
        "#;
    check(
        source,
        &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit
            RunWithConfig{H}: input_ty=(Unit, Qubit)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunWithConfig(config : __UDT_Item_1__Package_2_, q : Qubit) : Unit {
                ApplyOp_Empty_(config::Op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let config : __UDT_Item_1__Package_2_ = new Config {
                    Op = H
                };
                RunWithConfig(config, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunWithConfig(config : __UDT_Item_1__Package_2_, q : Qubit) : Unit {
                ApplyOp_Empty_(config::Op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                RunWithConfig_H_((), q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunWithConfig_H_(config : Unit, q : Qubit) : Unit {
                ApplyOp_Empty__H_(q);
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
fn udt_field_in_hof_body_full_pipeline_invariants() {
    let source = r#"
        struct Config { Op : Qubit => Unit }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation RunWithConfig(config : Config, q : Qubit) : Unit {
            ApplyOp(config.Op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let config = new Config { Op = H };
            RunWithConfig(config, q);
        }
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunWithConfig(config : __UDT_Item_1__Package_2_, q : Qubit) : Unit {
                ApplyOp_Empty_(config::Op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let config : __UDT_Item_1__Package_2_ = new Config {
                    Op = H
                };
                RunWithConfig(config, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunWithConfig(config : __UDT_Item_1__Package_2_, q : Qubit) : Unit {
                ApplyOp_Empty_(config::Op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                RunWithConfig_H_((), q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunWithConfig_H_(config : Unit, q : Qubit) : Unit {
                ApplyOp_Empty__H_(q);
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
fn udt_field_nested_two_level() {
    let source = r#"
        struct InnerConfig { Apply : Qubit => Unit }
        struct OuterConfig { Inner : InnerConfig, Label : Int }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let outer = new OuterConfig {
                Inner = new InnerConfig { Apply = H },
                Label = 0,
            };
            ApplyOp(outer.Inner.Apply, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 5 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            newtype InnerConfig = ((Qubit => Unit), );
            newtype OuterConfig = (__UDT_Item_1__Package_2_, Int);
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let outer : __UDT_Item_2__Package_2_ = new OuterConfig {
                    Inner = new InnerConfig {
                        Apply = H
                    },
                    Label = 0
                };
                ApplyOp_Empty_(outer::Inner::Apply, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            newtype InnerConfig = ((Qubit => Unit), );
            newtype OuterConfig = (__UDT_Item_1__Package_2_, Int);
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__H_(q);
                __quantum__rt__qubit_release(q);
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
fn udt_field_nested_two_level_defunctionalizes_end_to_end() {
    let source = r#"
        struct InnerConfig { Apply : Qubit => Unit }
        struct OuterConfig { Inner : InnerConfig, Label : Int }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let outer = new OuterConfig {
                Inner = new InnerConfig { Apply = H },
                Label = 0,
            };
            ApplyOp(outer.Inner.Apply, q);
        }
        "#;
    check(
        source,
        &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            newtype InnerConfig = ((Qubit => Unit), );
            newtype OuterConfig = (__UDT_Item_1__Package_2_, Int);
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let outer : __UDT_Item_2__Package_2_ = new OuterConfig {
                    Inner = new InnerConfig {
                        Apply = H
                    },
                    Label = 0
                };
                ApplyOp_Empty_(outer::Inner::Apply, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            newtype InnerConfig = ((Qubit => Unit), );
            newtype OuterConfig = (__UDT_Item_1__Package_2_, Int);
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__H_(q);
                __quantum__rt__qubit_release(q);
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
fn udt_field_closure_value() {
    let source = r#"
        struct Config { Op : Qubit => Unit }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 1.0;
            let config = new Config { Op = q1 => Rx(angle, q1) };
            ApplyOp(config.Op, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 5 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Closure(target=4, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let angle : Double = 1.;
                let config : __UDT_Item_1__Package_2_ = new Config {
                    Op = / * closure item = 4 captures = [angle] * / _lambda_
                };
                ApplyOp_Empty_(config::Op, q);
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
            // namespace test
            newtype Config = ((Qubit => Unit), );
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
fn udt_field_from_parameter() {
    let source = r#"
        struct Config { Op : Qubit => Unit }
        operation MakeConfig() : Config {
            new Config { Op = H }
        }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let c = MakeConfig();
            ApplyOp(c.Op, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 5 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            newtype Config = ((Qubit => Unit), );
            operation MakeConfig() : __UDT_Item_1__Package_2_ {
                new Config {
                    Op = H
                }

            }
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let c : __UDT_Item_1__Package_2_ = MakeConfig();
                ApplyOp_Empty_(c::Op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            newtype Config = ((Qubit => Unit), );
            operation MakeConfig() : __UDT_Item_1__Package_2_ {
                new Config {
                    Op = H
                }

            }
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__H_(q);
                __quantum__rt__qubit_release(q);
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
fn identity_closure_over_global_callable_collapses() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(a => H(a), q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty_(/ * closure item = 3 captures = [] * / _lambda_, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(a : Qubit, ) : Unit {
                H(a)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(a : Qubit, ) : Unit {
                H(a)
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
fn identity_closure_wrapping_param() {
    let source = r#"
        operation Inner(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Outer(action : Qubit => Unit, q : Qubit) : Unit {
            Inner(a => action(a), q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Outer(H, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation Inner(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Outer(action : (Qubit => Unit), q : Qubit) : Unit {
                Inner_Empty_(/ * closure item = 4 captures = [action] * / _lambda_, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Outer_AdjCtl_(H, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(action : (Qubit => Unit), a : Qubit) : Unit {
                action(a)
            }
            operation Inner_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Outer_AdjCtl_(action : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                Inner_Empty_(/ * closure item = 7 captures = [action] * / _lambda_, q);
            }
            operation _lambda_(action : (Qubit => Unit is Adj + Ctl), a : Qubit) : Unit {
                action(a)
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation Inner(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Outer(action : (Qubit => Unit), q : Qubit) : Unit {
                Inner_Empty_(/ * closure item = 4 captures = [action] * / _lambda_, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Outer_AdjCtl__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(action : (Qubit => Unit), a : Qubit) : Unit {
                action(a)
            }
            operation Inner_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Outer_AdjCtl_(action : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                Inner_Empty_(action, q);
            }
            operation _lambda_(action : (Qubit => Unit is Adj + Ctl), a : Qubit) : Unit {
                action(a)
            }
            operation Outer_AdjCtl__H_(q : Qubit) : Unit {
                Inner_Empty__H_(q);
            }
            operation Inner_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn non_identity_closure_preserved() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(a => { H(a); X(a); }, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 4 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Closure(target=3, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty_(/ * closure item = 3 captures = [] * / _lambda_, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(a : Qubit, ) : Unit {
                {
                    H(a);
                    X(a);
                }

            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__closure_(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(a : Qubit, ) : Unit {
                {
                    H(a);
                    X(a);
                }

            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__closure_(q : Qubit) : Unit {
                _lambda_(q, );
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn identity_closure_tuple_args() {
    let source = r#"
        operation Pair(a : Qubit, b : Qubit) : Unit {
            H(a);
            H(b);
        }
        operation HOF2(op : (Qubit, Qubit) => Unit, q1 : Qubit, q2 : Qubit) : Unit {
            op(q1, q2);
        }
        operation Main() : Unit {
            use q1 = Qubit();
            use q2 = Qubit();
            HOF2((a, b) => Pair(a, b), q1, q2);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation Pair(a : Qubit, b : Qubit) : Unit {
                H(a);
                H(b);
            }
            operation HOF2(op : ((Qubit, Qubit) => Unit), q1 : Qubit, q2 : Qubit) : Unit {
                op(q1, q2);
            }
            operation Main() : Unit {
                let q1 : Qubit = __quantum__rt__qubit_allocate();
                let q2 : Qubit = __quantum__rt__qubit_allocate();
                HOF2_Empty_(/ * closure item = 4 captures = [] * / _lambda_, q1, q2);
                __quantum__rt__qubit_release(q2);
                __quantum__rt__qubit_release(q1);
            }
            operation _lambda_((a : Qubit, b : Qubit), ) : Unit {
                Pair(a, b)
            }
            operation HOF2_Empty_(op : ((Qubit, Qubit) => Unit), q1 : Qubit, q2 : Qubit) : Unit {
                op(q1, q2);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation Pair(a : Qubit, b : Qubit) : Unit {
                H(a);
                H(b);
            }
            operation HOF2(op : ((Qubit, Qubit) => Unit), q1 : Qubit, q2 : Qubit) : Unit {
                op(q1, q2);
            }
            operation Main() : Unit {
                let q1 : Qubit = __quantum__rt__qubit_allocate();
                let q2 : Qubit = __quantum__rt__qubit_allocate();
                HOF2_Empty__Pair_(q1, q2);
                __quantum__rt__qubit_release(q2);
                __quantum__rt__qubit_release(q1);
            }
            operation _lambda_((a : Qubit, b : Qubit), ) : Unit {
                Pair(a, b)
            }
            operation HOF2_Empty_(op : ((Qubit, Qubit) => Unit), q1 : Qubit, q2 : Qubit) : Unit {
                op(q1, q2);
            }
            operation HOF2_Empty__Pair_(q1 : Qubit, q2 : Qubit) : Unit {
                Pair(q1, q2);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn closure_with_captures_not_identity() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 1.0;
            ApplyOp(a => Rx(angle, a), q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 4 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Closure(target=3, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let angle : Double = 1.;
                ApplyOp_Empty_(/ * closure item = 3 captures = [angle] * / _lambda_, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(angle : Double, a : Qubit) : Unit {
                Rx(angle, a)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let angle : Double = 1.;
                ApplyOp_Empty__closure_(q, angle);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(angle : Double, a : Qubit) : Unit {
                Rx(angle, a)
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
fn partial_application_lambda_analysis_shape() {
    let source = r#"
        operation ApplyOp(op : Qubit[] => Unit, register : Qubit[]) : Unit {
            op(register);
        }
        operation Shifted(shift : Int, register : Qubit[]) : Unit {
            ApplyXorInPlace(shift, register);
        }
        operation Main() : Unit {
            use register = Qubit[2];
            ApplyOp(register => Shifted(1, register), register);
        }
        "#;
    check(
        source,
        &expect![
            "<lambda>: input_ty=((Qubit)[],)\nApplyOp<Empty>{closure}: input_ty=(Qubit)[]\nMain: input_ty=Unit\nShifted: input_ty=(Int, (Qubit)[])"
        ],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplyOp(op : (Qubit[] => Unit), register : Qubit[]) : Unit {
                op(register);
            }
            operation Shifted(shift : Int, register : Qubit[]) : Unit {
                ApplyXorInPlace(shift, register);
            }
            operation Main() : Unit {
                let register : Qubit[] = AllocateQubitArray(2);
                ApplyOp_Empty_(/ * closure item = 4 captures = [] * / _lambda_, register);
                ReleaseQubitArray(register);
            }
            operation _lambda_(register : Qubit[], ) : Unit {
                Shifted(1, register)
            }
            operation ApplyOp_Empty_(op : (Qubit[] => Unit), register : Qubit[]) : Unit {
                op(register);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation ApplyOp(op : (Qubit[] => Unit), register : Qubit[]) : Unit {
                op(register);
            }
            operation Shifted(shift : Int, register : Qubit[]) : Unit {
                ApplyXorInPlace(shift, register);
            }
            operation Main() : Unit {
                let register : Qubit[] = AllocateQubitArray(2);
                ApplyOp_Empty__closure_(register);
                ReleaseQubitArray(register);
            }
            operation _lambda_(register : Qubit[], ) : Unit {
                Shifted(1, register)
            }
            operation ApplyOp_Empty_(op : (Qubit[] => Unit), register : Qubit[]) : Unit {
                op(register);
            }
            operation ApplyOp_Empty__closure_(register : Qubit[]) : Unit {
                _lambda_(register, );
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn reaching_def_mutable_single_assign() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            ApplyOp(op, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                ApplyOp_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
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
fn reaching_def_conditional_both_known() {
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
            // namespace test
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
            // namespace test
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
fn reaching_def_mutable_multi_assign() {
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
            // namespace test
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
            // namespace test
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
fn reaching_def_mutable_both_branches() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            if true { set op = X; } else { set op = S; }
            ApplyOp(op, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    op = X;
                } else {
                    op = S;
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
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    op = X;
                } else {
                    op = S;
                }

                if true {
                    ApplyOp_AdjCtl__X_(q)
                } else {
                    ApplyOp_AdjCtl__S_(q)
                };
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
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
fn reaching_def_mutable_in_loop_dynamic() {
    check_errors(
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
        &expect!["callable argument could not be resolved statically"],
    );
}

#[test]
fn analysis_closure_through_multiple_levels() {
    let source = r#"
        operation Inner(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Outer(op : Qubit => Unit, q : Qubit) : Unit { Inner(op, q); }
        operation Main() : Unit {
            use q = Qubit();
            Outer(q1 => H(q1), q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 2
              param: callable_id=<item 5 in package 2>, path=[0], ty=(Qubit => Unit)
              param: callable_id=<item 6 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 2
              site: hof=Outer<Empty>, arg=Global(H, Body)
              site: hof=Inner<Empty>, arg=Dynamic"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation Inner(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Outer(op : (Qubit => Unit), q : Qubit) : Unit {
                Inner_Empty_(op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Outer_Empty_(/ * closure item = 4 captures = [] * / _lambda_, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(q1 : Qubit, ) : Unit {
                H(q1)
            }
            operation Inner_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Outer_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                Inner_Empty_(op, q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation Inner(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Outer(op : (Qubit => Unit), q : Qubit) : Unit {
                Inner_Empty_(op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Outer_Empty__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(q1 : Qubit, ) : Unit {
                H(q1)
            }
            operation Inner_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Outer_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                Inner_Empty_(op, q);
            }
            operation Outer_Empty__H_(q : Qubit) : Unit {
                Inner_Empty__H_(q);
            }
            operation Inner_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_callable_returned_from_function() {
    let source = r#"
        operation GetOp() : Qubit => Unit { H }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            let op = GetOp();
            ApplyOp(op, q);
        }
        "#;
    check_analysis(
        source,
        &expect![
            "callable_params: 1\n  param: callable_id=<item 4 in package 2>, path=[0], ty=(Qubit => Unit)\ncall_sites: 1\n  site: hof=ApplyOp<Empty>, arg=Global(H, Body)\nlattice states:\n  callable Main:\n    2: Single(H:Body)"
        ],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation GetOp() : (Qubit => Unit) {
                H
            }
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let op : (Qubit => Unit) = GetOp();
                ApplyOp_Empty_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation GetOp() : (Qubit => Unit) {
                H
            }
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__H_(q);
                __quantum__rt__qubit_release(q);
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
fn callable_from_function_return_resolves_statically() {
    let source = r#"
        function GetOp() : (Qubit => Unit) { H }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(GetOp(), q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            function GetOp() : (Qubit => Unit) {
                H
            }
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty_(GetOp(), q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            function GetOp() : (Qubit => Unit) {
                H
            }
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__H_(q);
                __quantum__rt__qubit_release(q);
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
fn callable_returning_partial_application_resolves_statically() {
    let source = r#"
        operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
            op(register, target);
        }

        operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
            if bits[0] {
                CNOT(register[0], target);
            }
        }

        operation MakeParity(bits : Bool[]) : (Qubit[], Qubit) => Unit {
            return ApplyParityOperation(bits, _, _);
        }

        operation Main() : Unit {
            use register = Qubit[1];
            use target = Qubit();
            let op = MakeParity([true]);
            ApplyOp(op, register, target);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
                if bits[0] {
                    CNOT(register[0], target);
                }

            }
            operation MakeParity(bits : Bool[]) : ((Qubit[], Qubit) => Unit) {
                return {
                    let arg : Bool[] = bits;
                    / * closure item = 5 captures = [arg] * / _lambda_
                };
            }
            operation Main() : Unit {
                let register : Qubit[] = AllocateQubitArray(1);
                let target : Qubit = __quantum__rt__qubit_allocate();
                let op : ((Qubit[], Qubit) => Unit) = MakeParity([true]);
                ApplyOp_Empty_(op, register, target);
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            operation _lambda_(arg : Bool[], (hole : Qubit[], hole : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole)
            }
            operation ApplyOp_Empty_(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
                if bits[0] {
                    CNOT(register[0], target);
                }

            }
            operation MakeParity(bits : Bool[]) : ((Qubit[], Qubit) => Unit) {
                return {
                    let arg : Bool[] = bits;
                    ()
                };
            }
            operation Main() : Unit {
                let register : Qubit[] = AllocateQubitArray(1);
                let target : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__closure_(register, target, register);
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            operation _lambda_(arg : Bool[], (hole : Qubit[], hole : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole)
            }
            operation ApplyOp_Empty_(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyOp_Empty__closure_(register : Qubit[], target : Qubit, __capture_0 : Bool[]) : Unit {
                _lambda_(__capture_0, (register, target));
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_callable_returning_partial_application_with_explicit_return() {
    let source = r#"
        operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
            op(register, target);
        }

        operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
            if bits[0] {
                CNOT(register[0], target);
            }
        }

        operation MakeParity(bits : Bool[]) : (Qubit[], Qubit) => Unit {
            return ApplyParityOperation(bits, _, _);
        }

        operation Main() : Unit {
            use register = Qubit[1];
            use target = Qubit();
            let op = MakeParity([true]);
            ApplyOp(op, register, target);
        }
        "#;
    check_analysis(
        source,
        &expect![
            "callable_params: 1\n  param: callable_id=<item 6 in package 2>, path=[0], ty=(((Qubit)[], Qubit) => Unit)\ncall_sites: 1\n  site: hof=ApplyOp<Empty>, arg=Closure(target=5, Body)\nlattice states:\n  callable Main:\n    3: Single(Closure(5):Body)"
        ],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
                if bits[0] {
                    CNOT(register[0], target);
                }

            }
            operation MakeParity(bits : Bool[]) : ((Qubit[], Qubit) => Unit) {
                return {
                    let arg : Bool[] = bits;
                    / * closure item = 5 captures = [arg] * / _lambda_
                };
            }
            operation Main() : Unit {
                let register : Qubit[] = AllocateQubitArray(1);
                let target : Qubit = __quantum__rt__qubit_allocate();
                let op : ((Qubit[], Qubit) => Unit) = MakeParity([true]);
                ApplyOp_Empty_(op, register, target);
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            operation _lambda_(arg : Bool[], (hole : Qubit[], hole : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole)
            }
            operation ApplyOp_Empty_(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
                if bits[0] {
                    CNOT(register[0], target);
                }

            }
            operation MakeParity(bits : Bool[]) : ((Qubit[], Qubit) => Unit) {
                return {
                    let arg : Bool[] = bits;
                    ()
                };
            }
            operation Main() : Unit {
                let register : Qubit[] = AllocateQubitArray(1);
                let target : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__closure_(register, target, register);
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            operation _lambda_(arg : Bool[], (hole : Qubit[], hole : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole)
            }
            operation ApplyOp_Empty_(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyOp_Empty__closure_(register : Qubit[], target : Qubit, __capture_0 : Bool[]) : Unit {
                _lambda_(__capture_0, (register, target));
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn callable_returning_partial_application_from_local_arg_preserves_capture_expr() {
    let source = r#"
        operation UseOracle(oracle : ((Qubit[], Qubit) => Unit), n : Int) : Unit {
            use register = Qubit[n];
            use target = Qubit();
            oracle(register, target);
            Reset(target);
            ResetAll(register);
        }

        operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
            if bits[0] {
                CNOT(register[0], target);
            }
        }

        operation Encode(bits : Bool[]) : (Qubit[], Qubit) => Unit {
            ApplyParityOperation(bits, _, _)
        }

        operation Main() : Unit {
            let bits = [true];
            let oracle = Encode(bits);
            UseOracle(oracle, Length(bits));
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation UseOracle(oracle : ((Qubit[], Qubit) => Unit), n : Int) : Unit {
                let register : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                oracle(register, target);
                Reset(target);
                ResetAll(register);
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
                if bits[0] {
                    CNOT(register[0], target);
                }

            }
            operation Encode(bits : Bool[]) : ((Qubit[], Qubit) => Unit) {
                {
                    let arg : Bool[] = bits;
                    / * closure item = 5 captures = [arg] * / _lambda_
                }

            }
            operation Main() : Unit {
                let bits : Bool[] = [true];
                let oracle : ((Qubit[], Qubit) => Unit) = Encode(bits);
                UseOracle_Empty_(oracle, Length(bits));
            }
            operation _lambda_(arg : Bool[], (hole : Qubit[], hole : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole)
            }
            operation UseOracle_Empty_(oracle : ((Qubit[], Qubit) => Unit), n : Int) : Unit {
                let register : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                oracle(register, target);
                Reset(target);
                ResetAll(register);
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation UseOracle(oracle : ((Qubit[], Qubit) => Unit), n : Int) : Unit {
                let register : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                oracle(register, target);
                Reset(target);
                ResetAll(register);
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
                if bits[0] {
                    CNOT(register[0], target);
                }

            }
            operation Encode(bits : Bool[]) : ((Qubit[], Qubit) => Unit) {
                {
                    let arg : Bool[] = bits;
                    ()
                }

            }
            operation Main() : Unit {
                let bits : Bool[] = [true];
                UseOracle_Empty__closure_(Length(bits), bits);
            }
            operation _lambda_(arg : Bool[], (hole : Qubit[], hole : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole)
            }
            operation UseOracle_Empty_(oracle : ((Qubit[], Qubit) => Unit), n : Int) : Unit {
                let register : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                oracle(register, target);
                Reset(target);
                ResetAll(register);
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            operation UseOracle_Empty__closure_(n : Int, __capture_0 : Bool[]) : Unit {
                let register : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                _lambda_(__capture_0, (register, target));
                Reset(target);
                ResetAll(register);
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn callable_from_array_index_resolves_statically() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            let ops = [H, X];
            ApplyOp(ops[0], q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let ops : (Qubit => Unit is Adj + Ctl)[] = [H, X];
                ApplyOp_AdjCtl_(ops[0], q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let ops : (Qubit => Unit is Adj + Ctl)[] = [H, X];
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
fn callable_returning_partial_application_from_function_resolves_statically() {
    let source = r#"
        operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
            op(register, target);
        }

        operation ApplyParityOperation(value : Int, register : Qubit[], target : Qubit) : Unit {
            if value == 1 {
                CNOT(register[0], target);
            }
        }

        function Encode(value : Int) : (Qubit[], Qubit) => Unit {
            return ApplyParityOperation(value, _, _);
        }

        operation Main() : Unit {
            use register = Qubit[1];
            use target = Qubit();
            let value = 1;
            let oracle = Encode(value);
            ApplyOp(oracle, register, target);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyParityOperation(value : Int, register : Qubit[], target : Qubit) : Unit {
                if value == 1 {
                    CNOT(register[0], target);
                }

            }
            function Encode(value : Int) : ((Qubit[], Qubit) => Unit) {
                return {
                    let arg : Int = value;
                    / * closure item = 5 captures = [arg] * / _lambda_
                };
            }
            operation Main() : Unit {
                let register : Qubit[] = AllocateQubitArray(1);
                let target : Qubit = __quantum__rt__qubit_allocate();
                let value : Int = 1;
                let oracle : ((Qubit[], Qubit) => Unit) = Encode(value);
                ApplyOp_Empty_(oracle, register, target);
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            operation _lambda_(arg : Int, (hole : Qubit[], hole : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole)
            }
            operation ApplyOp_Empty_(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyParityOperation(value : Int, register : Qubit[], target : Qubit) : Unit {
                if value == 1 {
                    CNOT(register[0], target);
                }

            }
            function Encode(value : Int) : ((Qubit[], Qubit) => Unit) {
                return {
                    let arg : Int = value;
                    ()
                };
            }
            operation Main() : Unit {
                let register : Qubit[] = AllocateQubitArray(1);
                let target : Qubit = __quantum__rt__qubit_allocate();
                let value : Int = 1;
                ApplyOp_Empty__closure_(register, target, register);
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            operation _lambda_(arg : Int, (hole : Qubit[], hole : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole)
            }
            operation ApplyOp_Empty_(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyOp_Empty__closure_(register : Qubit[], target : Qubit, __capture_0 : Int) : Unit {
                _lambda_(__capture_0, (register, target));
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_callable_from_constant_callable_array_loop() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        operation Main() : Unit {
            use q = Qubit();
            let ops = [H, X];
            for op in ops {
                ApplyOp(op, q);
            }
        }
                "#;
    check_analysis(
        source,
        &expect![
            "callable_params: 1\n  param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)\ncall_sites: 2\n  site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)\n  site: hof=ApplyOp<AdjCtl>, arg=Global(X, Body)\nlattice states:\n  callable Main:\n    7: Multi([H:Body, X:Body])"
        ],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let ops : (Qubit => Unit is Adj + Ctl)[] = [H, X];
                let _generated_ident_77 : Unit = {
                    let _array_id_44 : (Qubit => Unit is Adj + Ctl)[] = ops;
                    let _len_id_48 : Int = Length(_array_id_44);
                    mutable _index_id_53 : Int = 0;
                    while _index_id_53 < _len_id_48 {
                        let op : (Qubit => Unit is Adj + Ctl) = _array_id_44[_index_id_53];
                        ApplyOp_AdjCtl_(op, q);
                        _index_id_53 += 1;
                    }

                };
                __quantum__rt__qubit_release(q);
                _generated_ident_77
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let ops : (Qubit => Unit is Adj + Ctl)[] = [H, X];
                let _generated_ident_77 : Unit = {
                    let _array_id_44 : (Qubit => Unit is Adj + Ctl)[] = ops;
                    let _len_id_48 : Int = Length(_array_id_44);
                    mutable _index_id_53 : Int = 0;
                    while _index_id_53 < _len_id_48 {
                        if _index_id_53 == 0 {
                            ApplyOp_AdjCtl__H_(q)
                        } else {
                            ApplyOp_AdjCtl__X_(q)
                        };
                        _index_id_53 += 1;
                    }

                };
                __quantum__rt__qubit_release(q);
                _generated_ident_77
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
fn analysis_callable_returning_partial_application_from_function_in_loop() {
    let source = r#"
        operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
            op(register, target);
        }

        operation ApplyParityOperation(value : Int, register : Qubit[], target : Qubit) : Unit {
            if value == 1 {
                CNOT(register[0], target);
            }
        }

        function Encode(value : Int) : (Qubit[], Qubit) => Unit {
            return ApplyParityOperation(value, _, _);
        }

        operation Main() : Unit {
            use register = Qubit[1];
            use target = Qubit();
            for value in [1, 2] {
                let oracle = Encode(value);
                ApplyOp(oracle, register, target);
            }
        }
                "#;
    check_analysis(
        source,
        &expect![
            "callable_params: 1\n  param: callable_id=<item 6 in package 2>, path=[0], ty=(((Qubit)[], Qubit) => Unit)\ncall_sites: 1\n  site: hof=ApplyOp<Empty>, arg=Closure(target=5, Body)\nlattice states:\n  callable Main:\n    8: Single(Closure(5):Body)"
        ],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyParityOperation(value : Int, register : Qubit[], target : Qubit) : Unit {
                if value == 1 {
                    CNOT(register[0], target);
                }

            }
            function Encode(value : Int) : ((Qubit[], Qubit) => Unit) {
                return {
                    let arg : Int = value;
                    / * closure item = 5 captures = [arg] * / _lambda_
                };
            }
            operation Main() : Unit {
                let register : Qubit[] = AllocateQubitArray(1);
                let target : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_156 : Unit = {
                    let _array_id_118 : Int[] = [1, 2];
                    let _len_id_122 : Int = Length(_array_id_118);
                    mutable _index_id_127 : Int = 0;
                    while _index_id_127 < _len_id_122 {
                        let value : Int = _array_id_118[_index_id_127];
                        let oracle : ((Qubit[], Qubit) => Unit) = Encode(value);
                        ApplyOp_Empty_(oracle, register, target);
                        _index_id_127 += 1;
                    }

                };
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
                _generated_ident_156
            }
            operation _lambda_(arg : Int, (hole : Qubit[], hole : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole)
            }
            operation ApplyOp_Empty_(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyParityOperation(value : Int, register : Qubit[], target : Qubit) : Unit {
                if value == 1 {
                    CNOT(register[0], target);
                }

            }
            function Encode(value : Int) : ((Qubit[], Qubit) => Unit) {
                return {
                    let arg : Int = value;
                    ()
                };
            }
            operation Main() : Unit {
                let register : Qubit[] = AllocateQubitArray(1);
                let target : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_156 : Unit = {
                    let _array_id_118 : Int[] = [1, 2];
                    let _len_id_122 : Int = Length(_array_id_118);
                    mutable _index_id_127 : Int = 0;
                    while _index_id_127 < _len_id_122 {
                        let value : Int = _array_id_118[_index_id_127];
                        ApplyOp_Empty__closure_(register, target, register);
                        _index_id_127 += 1;
                    }

                };
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
                _generated_ident_156
            }
            operation _lambda_(arg : Int, (hole : Qubit[], hole : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole)
            }
            operation ApplyOp_Empty_(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyOp_Empty__closure_(register : Qubit[], target : Qubit, __capture_0 : Int) : Unit {
                _lambda_(__capture_0, (register, target));
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn reaching_def_mutable_in_while_loop() {
    check_errors(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            mutable n = 3;
            while n > 0 {
                op = X;
                n -= 1;
            }
            ApplyOp(op, q);
        }
        "#,
        &expect!["callable argument could not be resolved statically"],
    );
}

#[test]
fn analysis_nested_callable_in_tuple_param() {
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
    check_analysis(
        source,
        &expect![
            "callable_params: 1\n  param: callable_id=<item 3 in package 2>, path=[0, 0], ty=(Qubit => Unit is Adj + Ctl)\ncall_sites: 1\n  site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)"
        ],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
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
            // namespace test
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
fn analysis_nested_callable_second_element() {
    let source = r#"
        operation Wrapper(pair : (Int, Qubit => Unit), q : Qubit) : Unit {
            let (_, op) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((42, H), q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0, 1], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
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
            // namespace test
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
fn analysis_nested_callable_single_param_supported() {
    let source = r#"
        operation Wrapper(pair : (Qubit => Unit, Int)) : Unit {
            let (op, _) = pair;
            use q = Qubit();
            op(q);
        }
        operation Main() : Unit {
            Wrapper((H, 42));
        }
        "#;
    check_analysis(
        source,
        &expect![
            "callable_params: 1\n  param: callable_id=<item 3 in package 2>, path=[0, 0], ty=(Qubit => Unit is Adj + Ctl)\ncall_sites: 1\n  site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)"
        ],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation Wrapper(pair : ((Qubit => Unit), Int)) : Unit {
                let (op : (Qubit => Unit), _ : Int) = pair;
                let q : Qubit = __quantum__rt__qubit_allocate();
                op(q);
                __quantum__rt__qubit_release(q);
            }
            operation Main() : Unit {
                Wrapper_AdjCtl_(H, 42);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int)) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), _ : Int) = pair;
                let q : Qubit = __quantum__rt__qubit_allocate();
                op(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation Wrapper(pair : ((Qubit => Unit), Int)) : Unit {
                let (op : (Qubit => Unit), _ : Int) = pair;
                let q : Qubit = __quantum__rt__qubit_allocate();
                op(q);
                __quantum__rt__qubit_release(q);
            }
            operation Main() : Unit {
                Wrapper_AdjCtl__H_(42);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int)) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), _ : Int) = pair;
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
fn analysis_branch_split_nested_callable_in_tuple() {
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
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0, 0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 2
              site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)
              site: hof=Wrapper<AdjCtl>, arg=Global(X, Body)
            lattice states:
              callable Main:
                2: Multi([H:Body, X:Body])"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
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
            // namespace test
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
fn analysis_nested_callable_single_param_second_element_supported() {
    let source = r#"
        operation Wrapper(pair : (Int, Qubit => Unit)) : Unit {
            let (_, op) = pair;
            use q = Qubit();
            op(q);
        }
        operation Main() : Unit {
            Wrapper((42, H));
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0, 1], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
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
            // namespace test
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
fn analysis_nested_callable_single_param_recursive_supported() {
    let source = r#"
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
        "#;
    check_analysis(
        source,
        &expect![
            "callable_params: 1\n  param: callable_id=<item 3 in package 2>, path=[0, 0, 0, 0], ty=(Qubit => Unit is Adj + Ctl)\ncall_sites: 1\n  site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)"
        ],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
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
            // namespace test
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
fn identity_closure_adjoint_wrapped_collapses() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => Adjoint S(q1), q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 4 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Global(S, Adj)
            direct_call_sites: 1
              site: callee=S:Adj, default"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty_(/ * closure item = 3 captures = [] * / _lambda_, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(q1 : Qubit, ) : Unit {
                Adjoint S(q1)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__Adj_S_(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_(q1 : Qubit, ) : Unit {
                Adjoint S(q1)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__Adj_S_(q : Qubit) : Unit {
                Adjoint S(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn single_use_immutable_local_promoted() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = H;
            ApplyOp(op, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)
            lattice states:
              callable Main:
                2: Single(H:Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let op : (Qubit => Unit is Adj + Ctl) = H;
                ApplyOp_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
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
fn multi_use_immutable_local_not_promoted() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q1 = Qubit();
            use q2 = Qubit();
            let op = H;
            ApplyOp(op, q1);
            ApplyOp(op, q2);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 2
              site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)
              site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)
            lattice states:
              callable Main:
                3: Single(H:Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q1 : Qubit = __quantum__rt__qubit_allocate();
                let q2 : Qubit = __quantum__rt__qubit_allocate();
                let op : (Qubit => Unit is Adj + Ctl) = H;
                ApplyOp_AdjCtl_(op, q1);
                ApplyOp_AdjCtl_(op, q2);
                __quantum__rt__qubit_release(q2);
                __quantum__rt__qubit_release(q1);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q1 : Qubit = __quantum__rt__qubit_allocate();
                let q2 : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__H_(q1);
                ApplyOp_AdjCtl__H_(q2);
                __quantum__rt__qubit_release(q2);
                __quantum__rt__qubit_release(q1);
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
fn mutable_local_not_promoted() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            op = X;
            ApplyOp(op, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(X, Body)
            lattice states:
              callable Main:
                2: Single(X:Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                op = X;
                ApplyOp_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                op = X;
                ApplyOp_AdjCtl__X_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
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
fn analysis_conditional_callable_binding_produces_multi_lattice() {
    let source = r#"
        operation ApplyConditional(power : Int, target : Qubit) : Unit {
            let u = if power >= 0 { S } else { Adjoint S };
            u(target);
        }

        operation Main() : Unit {
            use q = Qubit();
            ApplyConditional(3, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 0
            call_sites: 0
            direct_call_sites: 2
              site: callee=S:Adj, default
              site: callee=S:Body, condition=ExprId(4)
            lattice states:
              callable ApplyConditional:
                3: Multi([S:Body, S:Adj])"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace test
            operation ApplyConditional(power : Int, target : Qubit) : Unit {
                let u : (Qubit => Unit is Adj + Ctl) = if power >= 0 {
                    S
                } else {
                    Adjoint S
                };
                u(target);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyConditional(3, q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation ApplyConditional(power : Int, target : Qubit) : Unit {
                if power >= 0 {
                    S(target)
                } else {
                    Adjoint S(target)
                };
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyConditional(3, q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_callable_from_tuple_destructured_array_iteration() {
    let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                let arr = [(S, PauliZ), (T, PauliX)];
                for (op, _basis) in arr {
                    use q = Qubit();
                    op(q);
                }
            }
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 0
            call_sites: 0
            direct_call_sites: 2
              site: callee=S:Body, default
              site: callee=T:Body, default
            lattice states:
              callable Main:
                5: Multi([S:Body, T:Body])"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            // namespace Test
            operation Main() : Unit {
                let arr : ((Qubit => Unit is Adj + Ctl), Pauli)[] = [(S, PauliZ), (T, PauliX)];
                {
                    let _array_id_36 : ((Qubit => Unit is Adj + Ctl), Pauli)[] = arr;
                    let _len_id_40 : Int = Length(_array_id_36);
                    mutable _index_id_45 : Int = 0;
                    while _index_id_45 < _len_id_40 {
                        let (op : (Qubit => Unit is Adj + Ctl), _basis : Pauli) = _array_id_36[_index_id_45];
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        op(q);
                        _index_id_45 += 1;
                        __quantum__rt__qubit_release(q);
                    }

                }

            }
            // entry
            Main()

            AFTER:
            // namespace Test
            operation Main() : Unit {
                let arr : ((Qubit => Unit is Adj + Ctl), Pauli)[] = [(S, PauliZ), (T, PauliX)];
                {
                    let _array_id_36 : ((Qubit => Unit is Adj + Ctl), Pauli)[] = arr;
                    let _len_id_40 : Int = Length(_array_id_36);
                    mutable _index_id_45 : Int = 0;
                    while _index_id_45 < _len_id_40 {
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        if _index_id_45 == 0 {
                            S(q)
                        } else {
                            T(q)
                        };
                        _index_id_45 += 1;
                        __quantum__rt__qubit_release(q);
                    }

                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn resolve_captures_missing_binding_returns_none() {
    let package = Package {
        items: IndexMap::new(),
        entry: None,
        entry_exec_graph: qsc_fir::fir::ExecGraph::default(),
        blocks: IndexMap::new(),
        exprs: IndexMap::new(),
        pats: IndexMap::new(),
        stmts: IndexMap::new(),
    };
    let locals = LocalState::default();
    let missing_var = LocalVarId::from(99usize);

    let captures = resolve_captures(&package, &locals, &[missing_var], &FxHashSet::default());

    assert!(
        captures.is_none(),
        "missing capture bindings should degrade analysis instead of panicking"
    );
}
