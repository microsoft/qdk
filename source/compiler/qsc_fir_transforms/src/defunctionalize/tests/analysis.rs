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
            operation Main() : Unit {}
            // entry
            Main()

            AFTER:
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
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 4 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Global(H, Body)
            lattice states:
              callable Main:
                2: Single(H:Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
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
                ApplyOp_Empty__closure_(register, target, [true]);
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
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 6 in package 2>, path=[0], ty=(((Qubit)[], Qubit) => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Closure(target=5, Body)
            lattice states:
              callable Main:
                3: Single(Closure(5):Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
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
                ApplyOp_Empty__closure_(register, target, [true]);
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
                ApplyOp_Empty__closure_(register, target, value);
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
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 2
              site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)
              site: hof=ApplyOp<AdjCtl>, arg=Global(X, Body)
            lattice states:
              callable Main:
                7: Multi([H:Body, X:Body])"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
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
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 6 in package 2>, path=[0], ty=(((Qubit)[], Qubit) => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Closure(target=5, Body)
            lattice states:
              callable Main:
                8: Single(Closure(5):Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
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
                        ApplyOp_Empty__closure_(register, target, value);
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
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0, 0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)"#]],
    );
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
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0, 0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
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
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0, 0, 0, 0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
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
fn callable_from_nested_conditions() {
    let source = r#"
        operation ApplyNested(a : Int, b : Int, target : Qubit) : Unit {
            let u = if a >= 0 { if b >= 0 { S } else { T } } else { Adjoint S };
            u(target);
        }

        operation Main() : Unit {
            use q = Qubit();
            ApplyNested(3, 4, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
        callable_params: 0
        call_sites: 0
        direct_call_sites: 3
          site: callee=S:Adj, default
          site: callee=S:Body, condition=ExprId(4) and ExprId(9)
          site: callee=T:Body, condition=ExprId(4)
        lattice states:
          callable ApplyNested:
            4: Multi([S:Body, T:Body, S:Adj])"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyNested(a : Int, b : Int, target : Qubit) : Unit {
                let u : (Qubit => Unit is Adj + Ctl) = if a >= 0 {
                    if b >= 0 {
                        S
                    } else {
                        T
                    }

                } else {
                    Adjoint S
                };
                u(target);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyNested(3, 4, q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyNested(a : Int, b : Int, target : Qubit) : Unit {
                if a >= 0 {
                    if b >= 0 {
                        S(target)
                    } else {
                        T(target)
                    }
                } else {
                    Adjoint S(target)
                };
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyNested(3, 4, q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn callable_from_triple_nested_conditions() {
    let source = r#"
        operation ApplyTriple(a : Int, b : Int, c : Int, target : Qubit) : Unit {
            let u = if a >= 0 { if b >= 0 { if c >= 0 { S } else { T } } else { X } } else { Y };
            u(target);
        }

        operation Main() : Unit {
            use q = Qubit();
            ApplyTriple(1, 2, 3, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 0
            call_sites: 0
            direct_call_sites: 4
              site: callee=S:Body, condition=ExprId(4) and ExprId(9) and ExprId(14)
              site: callee=T:Body, condition=ExprId(4) and ExprId(9)
              site: callee=X:Body, condition=ExprId(4)
              site: callee=Y:Body, default
            lattice states:
              callable ApplyTriple:
                5: Multi([S:Body, T:Body, X:Body, Y:Body])"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyTriple(a : Int, b : Int, c : Int, target : Qubit) : Unit {
                let u : (Qubit => Unit is Adj + Ctl) = if a >= 0 {
                    if b >= 0 {
                        if c >= 0 {
                            S
                        } else {
                            T
                        }

                    } else {
                        X
                    }

                } else {
                    Y
                };
                u(target);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyTriple(1, 2, 3, q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyTriple(a : Int, b : Int, c : Int, target : Qubit) : Unit {
                if a >= 0 {
                    if b >= 0 {
                        if c >= 0 {
                            S(target)
                        } else {
                            T(target)
                        }
                    } else {
                        X(target)
                    }
                } else {
                    Y(target)
                };
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyTriple(1, 2, 3, q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn callable_from_dual_nested_conditions() {
    let source = r#"
        operation ApplyDual(a : Int, b : Int, c : Int, target : Qubit) : Unit {
            let u = if a >= 0 { if b >= 0 { S } else { T } } else { if c >= 0 { X } else { Y } };
            u(target);
        }

        operation Main() : Unit {
            use q = Qubit();
            ApplyDual(1, 2, 3, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
        callable_params: 0
        call_sites: 0
        direct_call_sites: 4
          site: callee=S:Body, condition=ExprId(4) and ExprId(9)
          site: callee=T:Body, condition=ExprId(4)
          site: callee=X:Body, condition=ExprId(18)
          site: callee=Y:Body, default
        lattice states:
          callable ApplyDual:
            5: Multi([S:Body, T:Body, X:Body, Y:Body])"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyDual(a : Int, b : Int, c : Int, target : Qubit) : Unit {
                let u : (Qubit => Unit is Adj + Ctl) = if a >= 0 {
                    if b >= 0 {
                        S
                    } else {
                        T
                    }

                } else {
                    if c >= 0 {
                        X
                    } else {
                        Y
                    }

                };
                u(target);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyDual(1, 2, 3, q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyDual(a : Int, b : Int, c : Int, target : Qubit) : Unit {
                if a >= 0 {
                    if b >= 0 {
                        S(target)
                    } else {
                        T(target)
                    }
                } else if c >= 0 {
                    X(target)
                } else {
                    Y(target)
                };
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyDual(1, 2, 3, q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn reaching_def_mutable_nested_branches() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            if true { if false { set op = X; } else { set op = T; } } else { set op = S; }
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
                    if false {
                        op = X;
                    } else {
                        op = T;
                    }

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
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    if false {
                        op = X;
                    } else {
                        op = T;
                    }

                } else {
                    op = S;
                }

                if true {
                    if false {
                        ApplyOp_AdjCtl__X_(q)
                    } else {
                        ApplyOp_AdjCtl__T_(q)
                    }
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
            operation ApplyOp_AdjCtl__T_(q : Qubit) : Unit {
                T(q);
            }
            operation ApplyOp_AdjCtl__S_(q : Qubit) : Unit {
                S(q);
            }
            // entry
            Main()
        "#]],
    );
}

/// Documents defunctionalization's behavior when it runs *in isolation* on a
/// side-effecting `if` condition. The `check_rewrite` helper invokes only
/// `defunctionalize`, so the condition `{ Y(q); true }` has not been hoisted
/// and the resulting snapshot still references it twice (once in the original
/// mutable assignment and once in the synthesized branch dispatch).
///
/// In the production pipeline this duplication never reaches codegen: the
/// `cond_normalize` pass runs immediately before `defunctionalize` and rewrites
/// such conditions into a single-evaluation `let __cond = { Y(q); true }; if
/// __cond { .. }` form, so each side effect is emitted exactly once. The
/// end-to-end guarantee is covered by
/// `cond_normalize::tests::full_pipeline_runs_with_side_effecting_condition`
/// and the QIR-level
/// `codegen::tests::defunctionalize_nested_condition_dispatch_evaluates_measurement_once`.
#[test]
fn callable_in_mutable_with_side_effects_in_if_expr() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            if {Y(q); true} { set op = X; }
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
                if {
                    Y(q);
                    true
                }
                {
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
                if {
                    Y(q);
                    true
                }
                {
                    op = X;
                }

                if {
                    Y(q);
                    true
                }
                {
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

/// A HOF (`ApplyChoice`) takes a boolean parameter that selects a callable in a
/// mutable `set` reassignment's `if` condition. The mutable-flow `If` arm now
/// routes its branch condition through `remap_condition_expr` (matching the
/// immutable path) so a substituted condition local resolves to the caller's
/// expression. Here `flag` remains a live parameter of the specialized
/// operation, so it passes through unchanged; the test guards that the mutable
/// nested HOF dispatch preserves both branches (`X` and `Y`) and resolves to
/// `if flag { X(q) } else { Y(q) }` rather than dropping a branch.
#[test]
fn reaching_def_mutable_hof_param_substituted_condition() {
    let source = r#"
        operation ApplyChoice(inner : Qubit => Unit, flag : Bool, q : Qubit) : Unit {
            mutable op = inner;
            if flag { set op = X; } else { set op = Y; }
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyChoice(H, true, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyChoice(inner : (Qubit => Unit), flag : Bool, q : Qubit) : Unit {
                mutable op : (Qubit => Unit) = inner;
                if flag {
                    op = X;
                } else {
                    op = Y;
                }

                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyChoice_AdjCtl_(H, true, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyChoice_AdjCtl_(inner : (Qubit => Unit is Adj + Ctl), flag : Bool, q : Qubit) : Unit {
                mutable op : (Qubit => Unit is Adj + Ctl) = inner;
                if flag {
                    op = X;
                } else {
                    op = Y;
                }

                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyChoice(inner : (Qubit => Unit), flag : Bool, q : Qubit) : Unit {
                mutable op : (Qubit => Unit) = inner;
                if flag {
                    op = X;
                } else {
                    op = Y;
                }

                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyChoice_AdjCtl__H_(true, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyChoice_AdjCtl_(inner : (Qubit => Unit is Adj + Ctl), flag : Bool, q : Qubit) : Unit {
                mutable op : (Qubit => Unit is Adj + Ctl) = inner;
                if flag {
                    op = X;
                } else {
                    op = Y;
                }

                if flag {
                    X(q)
                } else {
                    Y(q)
                };
            }
            operation ApplyChoice_AdjCtl__H_(flag : Bool, q : Qubit) : Unit {
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if flag {
                    op = X;
                } else {
                    op = Y;
                }

                if flag {
                    X(q)
                } else {
                    Y(q)
                };
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

// ---------------------------------------------------------------------------
// Flow-analysis completeness tests.
//
// These cover operand-position and program-point sensitivity in the
// defunctionalize flow analysis (`analyze_expr_flow` /
// `collect_assigned_vars_expr` and program-point-sensitive call recording).
// Each test pairs a top-level `set` with the same reassignment nested in an
// operand-position child; both must specialize calls to the reaching
// definition.

/// A `set f = Bar` in a `BinOp` operand block is observed in evaluation order,
/// so the later `f(5)` specializes to the reaching definition `Bar` (it ran
/// before the call), matching the top-level-`set` case.
#[allow(clippy::too_many_lines)]
#[test]
fn operand_block_set_specializes_to_reaching_definition() {
    // Top-level `set f = Bar;` -> f(5) resolves to Bar.
    check_rewrite(
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            mutable f = Foo;
            f = Bar;
            let z = 1;
            f(5)
        }
        "#,
        &expect![[r#"
            BEFORE:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                f = Bar;
                let z : Int = 1;
                f(5)
            }
            // entry
            Main()

            AFTER:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                f = Bar;
                let z : Int = 1;
                Bar(5)
            }
            // entry
            Main()
        "#]],
    );

    // `set f = Bar` in the left operand block of `+ 1` -> f(5) resolves to Bar.
    check_rewrite(
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            mutable f = Foo;
            let z = { set f = Bar; 0 } + 1;
            f(5)
        }
        "#,
        &expect![[r#"
            BEFORE:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let z : Int = {
                    f = Bar;
                    0
                } + 1;
                f(5)
            }
            // entry
            Main()

            AFTER:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let z : Int = {
                    f = Bar;
                    0
                } + 1;
                Bar(5)
            }
            // entry
            Main()
        "#]],
    );
}

/// An operand-position `set f = Bar` inside a loop body forces `f` to
/// `Dynamic`, so the post-loop `f(5)` is not specialized. A `Dynamic` callable
/// consumed by a direct call leaves an unresolved value, surfacing an
/// actionable `DynamicCallable` error at the call site (the direct-path
/// mirror of the HOF diagnostic), exactly like the top-level-`set`-in-loop
/// case.
#[test]
fn loop_operand_block_set_forces_dynamic() {
    fn assert_forces_dynamic(context: &str, source: &str) {
        let (mut store, package_id) = compile_to_monomorphized_fir(source);
        let mut assigners = crate::package_assigners::PackageAssigners::entry(&store, package_id);
        let errors = defunctionalize(&mut store, package_id, &mut assigners);
        assert_eq!(
            errors.len(),
            1,
            "{context}: expected the loop-reassigned callable to be forced Dynamic \
             (one unresolved direct call), got: {}",
            format_defunctionalization_errors(&errors)
        );
        assert!(
            matches!(errors[0], super::super::Error::DynamicCallable(..)),
            "{context}: expected DynamicCallable error, got {:?}",
            errors[0]
        );
    }

    // Top-level `set` inside the loop body forces `f` Dynamic.
    assert_forces_dynamic(
        "top-level set in loop",
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            mutable f = Foo;
            for i in 0..2 {
                f = Bar;
            }
            f(5)
        }
        "#,
    );

    // `set f = Bar` in an operand block inside the loop also forces `f` Dynamic,
    // producing the same error.
    assert_forces_dynamic(
        "operand-block set in loop",
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            mutable f = Foo;
            for i in 0..2 {
                let z = { set f = Bar; 0 } + 1;
            }
            f(5)
        }
        "#,
    );
}

/// Straight-line call resolution is program-point-sensitive: the `f(1)` that
/// precedes `set f = Bar` specializes to `Foo`, while the later `f(2)`
/// specializes to `Bar`.
#[test]
fn straight_line_reassignment_is_position_sensitive() {
    // No reassignment: both calls resolve to `Foo`.
    check_rewrite(
        r#"
        operation Foo(x : Int) : Unit {}
        operation Bar(x : Int) : Unit {}
        operation Main() : Unit {
            mutable f = Foo;
            f(1);
            f(2);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Foo(x : Int) : Unit {}
            operation Bar(x : Int) : Unit {}
            operation Main() : Unit {
                mutable f : (Int => Unit) = Foo;
                f(1);
                f(2);
            }
            // entry
            Main()

            AFTER:
            operation Foo(x : Int) : Unit {}
            operation Bar(x : Int) : Unit {}
            operation Main() : Unit {
                mutable f : (Int => Unit) = Foo;
                Foo(1);
                Foo(2);
            }
            // entry
            Main()
        "#]],
    );

    // A call before and after a top-level `set f = Bar`.
    check_rewrite(
        r#"
        operation Foo(x : Int) : Unit {}
        operation Bar(x : Int) : Unit {}
        operation Main() : Unit {
            mutable f = Foo;
            f(1);
            f = Bar;
            f(2);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Foo(x : Int) : Unit {}
            operation Bar(x : Int) : Unit {}
            operation Main() : Unit {
                mutable f : (Int => Unit) = Foo;
                f(1);
                f = Bar;
                f(2);
            }
            // entry
            Main()

            AFTER:
            operation Foo(x : Int) : Unit {}
            operation Bar(x : Int) : Unit {}
            operation Main() : Unit {
                mutable f : (Int => Unit) = Foo;
                Foo(1);
                f = Bar;
                Bar(2);
            }
            // entry
            Main()
        "#]],
    );
}

/// Passing a mutable callable local to a higher-order function after an
/// operand-position `set` threads the reaching definition `Bar` into the
/// specialized `Apply` variant, matching the top-level-`set` case.
#[allow(clippy::too_many_lines)]
#[test]
fn hof_operand_block_set_specializes_reaching_definition() {
    // Top-level `set f = Bar;` -> specializes to `Apply_Bar_`.
    check_rewrite(
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        function Apply(g : Int -> Int, x : Int) : Int { g(x) }
        operation Main() : Int {
            mutable f = Foo;
            f = Bar;
            let z = 1;
            Apply(f, 5)
        }
        "#,
        &expect![[r#"
            BEFORE:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            function Apply(g : (Int -> Int), x : Int) : Int {
                g(x)
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                f = Bar;
                let z : Int = 1;
                Apply(f, 5)
            }
            // entry
            Main()

            AFTER:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            function Apply(g : (Int -> Int), x : Int) : Int {
                g(x)
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                f = Bar;
                let z : Int = 1;
                Apply_Bar_(5)
            }
            function Apply_Bar_(x : Int) : Int {
                Bar(x)
            }
            // entry
            Main()
        "#]],
    );

    // `set f = Bar` in an operand block before the HOF call.
    check_rewrite(
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        function Apply(g : Int -> Int, x : Int) : Int { g(x) }
        operation Main() : Int {
            mutable f = Foo;
            let z = { set f = Bar; 0 } + 1;
            Apply(f, 5)
        }
        "#,
        &expect![[r#"
            BEFORE:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            function Apply(g : (Int -> Int), x : Int) : Int {
                g(x)
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let z : Int = {
                    f = Bar;
                    0
                } + 1;
                Apply(f, 5)
            }
            // entry
            Main()

            AFTER:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            function Apply(g : (Int -> Int), x : Int) : Int {
                g(x)
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let z : Int = {
                    f = Bar;
                    0
                } + 1;
                Apply_Bar_(5)
            }
            function Apply_Bar_(x : Int) : Int {
                Bar(x)
            }
            // entry
            Main()
        "#]],
    );
}

/// An operand-position conditional `set` forms the `Multi` lattice so the call
/// emits multi-way dispatch (`if true { X } else { H }`), matching the
/// statement-level case.
#[allow(clippy::too_many_lines)]
#[test]
fn operand_block_conditional_set_preserves_dispatch() {
    // Statement-level conditional `set`: multi-way dispatch.
    check_rewrite(
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

    // The conditional `set` lives in an operand block of `+ 1`.
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            let z = (if true { set op = X; 0 } else { 0 }) + 1;
            ApplyOp(op, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                let z : Int = if true {
                    op = X;
                    0
                } else {
                    0
                } + 1;
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
                let z : Int = if true {
                    op = X;
                    0
                } else {
                    0
                } + 1;
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

/// When a branch contains a statement-level `set op = Y` followed by an
/// operand-position `set op = X`, the dispatch arm for that branch targets the
/// reaching definition `X`, matching the all-statement-level case.
#[allow(clippy::too_many_lines)]
#[test]
fn operand_block_set_in_branch_uses_correct_arm() {
    // Both sets at statement level: true arm dispatches X.
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            if true {
                set op = Y;
                set op = X;
            }
            ApplyOp(op, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    op = Y;
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
                    op = Y;
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

    // The second `set op = X` in an operand block.
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            if true {
                set op = Y;
                let z = ({ set op = X; 0 }) + 0;
            }
            ApplyOp(op, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    op = Y;
                    let z : Int = {
                        op = X;
                        0
                    } + 0;
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
                    op = Y;
                    let z : Int = {
                        op = X;
                        0
                    } + 0;
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

/// An array-of-tuples index dispatch whose tuple-pattern binding
/// (`let (initializer, _basis) = ops[i]`) is nested inside an operand-position
/// block resolves its tuple field path, so the indexed call is rewritten into
/// an `if`/`else` index dispatch.
#[allow(clippy::too_many_lines)]
#[test]
fn operand_block_tuple_pattern_dispatch_resolves_field_path() {
    check_rewrite(
        r#"
        operation Main() : Unit {
            let ops = [(I, PauliZ), (X, PauliZ)];
            for i in 0..1 {
                use q = Qubit();
                let z = { let (initializer, _basis) = ops[i]; initializer(q); 0 } + 1;
            }
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let ops : ((Qubit => Unit is Adj + Ctl), Pauli)[] = [(I, PauliZ), (X, PauliZ)];
                {
                    let _range_id_53 : Range = 0..1;
                    mutable _index_id_56 : Int = _range_id_53::Start;
                    let _step_id_61 : Int = _range_id_53::Step;
                    let _end_id_66 : Int = _range_id_53::End;
                    while _step_id_61 > 0 and _index_id_56 <= _end_id_66 or _step_id_61 < 0 and _index_id_56 >= _end_id_66 {
                        let i : Int = _index_id_56;
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        let z : Int = {
                            let (initializer : (Qubit => Unit is Adj + Ctl), _basis : Pauli) = ops[i];
                            initializer(q);
                            0
                        } + 1;
                        _index_id_56 += _step_id_61;
                        __quantum__rt__qubit_release(q);
                    }

                }

            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let ops : ((Qubit => Unit is Adj + Ctl), Pauli)[] = [(I, PauliZ), (X, PauliZ)];
                {
                    let _range_id_53 : Range = 0..1;
                    mutable _index_id_56 : Int = _range_id_53::Start;
                    let _step_id_61 : Int = _range_id_53::Step;
                    let _end_id_66 : Int = _range_id_53::End;
                    while _step_id_61 > 0 and _index_id_56 <= _end_id_66 or _step_id_61 < 0 and _index_id_56 >= _end_id_66 {
                        let i : Int = _index_id_56;
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        let z : Int = {
                            if i == 0 {
                                I(q)
                            } else {
                                X(q)
                            };
                            0
                        } + 1;
                        _index_id_56 += _step_id_61;
                        __quantum__rt__qubit_release(q);
                    }

                }

            }
            // entry
            Main()
        "#]],
    );
}

/// When the index selecting a callable from an array is a non-trivial
/// expression (here `i + 1`), the synthesized dispatch hoists it into a single
/// shared `let` so it is evaluated once rather than re-evaluated in every
/// `index == k` guard. Contrast `operand_block_tuple_pattern_dispatch_resolves_field_path`,
/// where a bare-variable index keeps referencing the original expression.
#[test]
fn non_trivial_array_index_dispatch_hoists_index_once() {
    let source = r#"
        operation Main() : Unit {
            let ops = [I, X, Y];
            for i in 0..1 {
                use q = Qubit();
                let op = ops[i + 1];
                op(q);
            }
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let ops : (Qubit => Unit is Adj + Ctl)[] = [I, X, Y];
                {
                    let _range_id_40 : Range = 0..1;
                    mutable _index_id_43 : Int = _range_id_40::Start;
                    let _step_id_48 : Int = _range_id_40::Step;
                    let _end_id_53 : Int = _range_id_40::End;
                    while _step_id_48 > 0 and _index_id_43 <= _end_id_53 or _step_id_48 < 0 and _index_id_43 >= _end_id_53 {
                        let i : Int = _index_id_43;
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        let op : (Qubit => Unit is Adj + Ctl) = ops[i + 1];
                        op(q);
                        _index_id_43 += _step_id_48;
                        __quantum__rt__qubit_release(q);
                    }

                }

            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let ops : (Qubit => Unit is Adj + Ctl)[] = [I, X, Y];
                {
                    let _range_id_40 : Range = 0..1;
                    mutable _index_id_43 : Int = _range_id_40::Start;
                    let _step_id_48 : Int = _range_id_40::Step;
                    let _end_id_53 : Int = _range_id_40::End;
                    while _step_id_48 > 0 and _index_id_43 <= _end_id_53 or _step_id_48 < 0 and _index_id_43 >= _end_id_53 {
                        let i : Int = _index_id_43;
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        let index : Int = i + 1;
                        if index == 0 {
                            I(q)
                        } else if index == 1 {
                            X(q)
                        } else {
                            Y(q)
                        };
                        _index_id_43 += _step_id_48;
                        __quantum__rt__qubit_release(q);
                    }

                }

            }
            // entry
            Main()
        "#]],
    );
}

/// A `set f = Bar` inside the short-circuited RHS of `false and { .. }` is not
/// executed at runtime, so the later `f(5)` must keep the reaching definition
/// `Foo`. The fork/join arm applies the RHS conditionally rather than
/// unconditionally overwriting the lattice.
#[test]
fn binop_andl_short_circuit_rhs_set_does_not_reach_call() {
    check_rewrite(
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            mutable f = Foo;
            let b = false and { set f = Bar; true };
            f(5)
        }
        "#,
        &expect![[r#"
            BEFORE:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let b : Bool = false and {
                    f = Bar;
                    true
                };
                f(5)
            }
            // entry
            Main()

            AFTER:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let b : Bool = false and {
                    f = Bar;
                    true
                };
                if false {
                    Bar(5)
                } else {
                    Foo(5)
                }
            }
            // entry
            Main()
        "#]],
    );
}

/// A `set f = Bar` inside the short-circuited RHS of `true or { .. }` is not
/// executed at runtime, so the later `f(5)` must keep the reaching definition
/// `Foo` (same fork/join arm as `and`).
#[test]
fn binop_orl_short_circuit_rhs_set_does_not_reach_call() {
    check_rewrite(
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            mutable f = Foo;
            let b = true or { set f = Bar; false };
            f(5)
        }
        "#,
        &expect![[r#"
            BEFORE:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let b : Bool = true or {
                    f = Bar;
                    false
                };
                f(5)
            }
            // entry
            Main()

            AFTER:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let b : Bool = true or {
                    f = Bar;
                    false
                };
                if true {
                    Foo(5)
                } else {
                    Bar(5)
                }
            }
            // entry
            Main()
        "#]],
    );
}

/// A `set f = Bar` inside the short-circuited RHS of a logical compound-assign
/// `set b and= { .. }` (a distinct `AssignOp` arm) is not executed when the LHS
/// short-circuits, so the later `f(5)` must keep the reaching definition `Foo`.
#[test]
fn assignop_andl_short_circuit_rhs_set_does_not_reach_call() {
    check_rewrite(
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            mutable f = Foo;
            mutable b = false;
            set b and= { set f = Bar; false };
            f(5)
        }
        "#,
        &expect![[r#"
            BEFORE:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                mutable b : Bool = false;
                b and= {
                    f = Bar;
                    false
                };
                f(5)
            }
            // entry
            Main()

            AFTER:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                mutable b : Bool = false;
                b and= {
                    f = Bar;
                    false
                };
                if b {
                    Bar(5)
                } else {
                    Foo(5)
                }
            }
            // entry
            Main()
        "#]],
    );
}

/// In `(new Rec { A = f(5), B = 0 }) w/ B <- { set f = Bar; 0 }`, runtime
/// evaluates the replace operand (`set f = Bar`) before the record operand
/// (which contains `f(5)`), so the call resolves to the reaching definition
/// `Bar`. The reordered `UpdateField` arm recurses replace-then-record.
#[test]
fn update_field_replace_then_record_order_reaches_call() {
    check_rewrite(
        r#"
        struct Rec { A : Int, B : Int }
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            mutable f = Foo;
            let r = (new Rec { A = f(5), B = 0 }) w/ B <- { set f = Bar; 0 };
            r.A
        }
        "#,
        &expect![[r#"
            BEFORE:
            newtype Rec = (Int, Int);
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let r : __UDT_Item_1__Package_2_ = new Rec {
                    A = f(5),
                    B = 0
                }
                    w/::B <- {
                    f = Bar;
                    0
                };
                r::A
            }
            // entry
            Main()

            AFTER:
            newtype Rec = (Int, Int);
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let r : __UDT_Item_1__Package_2_ = new Rec {
                    A = Bar(5),
                    B = 0
                }
                    w/::B <- {
                    f = Bar;
                    0
                };
                r::A
            }
            // entry
            Main()
        "#]],
    );
}

/// In `[f(5), 0] w/ 1 <- { set f = Bar; 0 }`, runtime evaluates the index then
/// the replace operand (`set f = Bar`) before the container operand (which
/// contains `f(5)`), so the call resolves to the reaching definition `Bar`.
/// The reordered `UpdateIndex` arm recurses index-replace-container(last).
#[test]
fn update_index_container_last_order_reaches_call() {
    check_rewrite(
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            mutable f = Foo;
            let arr = [f(5), 0] w/ 1 <- { set f = Bar; 0 };
            arr[0]
        }
        "#,
        &expect![[r#"
            BEFORE:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let arr : Int[] = [f(5), 0] w/ 1 <- {
                    f = Bar;
                    0
                };
                arr[0]
            }
            // entry
            Main()

            AFTER:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let arr : Int[] = [Bar(5), 0] w/ 1 <- {
                    f = Bar;
                    0
                };
                arr[0]
            }
            // entry
            Main()
        "#]],
    );
}

/// Guard: a non-logical compound-assign (`+=`) executes its RHS
/// unconditionally at runtime, so the `set f = Bar` in `set acc += { .. }` does
/// reach the later `f(5)` (resolving to `Bar`). This confirms the `AssignOp`
/// match split did not accidentally route non-logical operators through the
/// conditional fork/join arm.
#[test]
fn assignop_non_logical_rhs_set_reaches_call() {
    check_rewrite(
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            mutable f = Foo;
            mutable acc = 0;
            set acc += { set f = Bar; 1 };
            f(5)
        }
        "#,
        &expect![[r#"
            BEFORE:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                mutable acc : Int = 0;
                acc += {
                    f = Bar;
                    1
                };
                f(5)
            }
            // entry
            Main()

            AFTER:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                mutable acc : Int = 0;
                acc += {
                    f = Bar;
                    1
                };
                Bar(5)
            }
            // entry
            Main()
        "#]],
    );
}

/// With a runtime-dynamic `or` condition, the fork/join produces a
/// condition-tagged `Multi` lattice entry that flows through branch-split
/// dispatch. The `OrL` branches are ordered so dispatch is
/// `if cond { Foo(5) } else { Bar(5) }`: when the condition is true the `or`
/// short-circuits (RHS not run, `f` stays `Foo`); when false the RHS runs and
/// `f = Bar`. Confirms the `OrL` branch ordering end-to-end with no
/// `DynamicCallable`/`FixpointNotReached` regression.
#[test]
fn orl_runtime_dynamic_condition_branch_split_dispatch() {
    check_rewrite_with_capabilities(
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            use q = Qubit();
            mutable f = Foo;
            let cond = MResetZ(q) == One;
            let b = cond or { set f = Bar; false };
            f(5)
        }
        "#,
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
        &expect![[r#"
            BEFORE:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable f : (Int -> Int) = Foo;
                let cond : Bool = MResetZ(q) == One;
                let b : Bool = cond or {
                    f = Bar;
                    false
                };
                let _generated_ident_67 : Int = f(5);
                __quantum__rt__qubit_release(q);
                _generated_ident_67
            }
            // entry
            Main()

            AFTER:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable f : (Int -> Int) = Foo;
                let cond : Bool = MResetZ(q) == One;
                let b : Bool = cond or {
                    f = Bar;
                    false
                };
                let _generated_ident_67 : Int = if cond {
                    Foo(5)
                } else {
                    Bar(5)
                };
                __quantum__rt__qubit_release(q);
                _generated_ident_67
            }
            // entry
            Main()
        "#]],
    );
}
