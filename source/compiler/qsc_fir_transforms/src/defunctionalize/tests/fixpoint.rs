// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Many tests pair a primary assertion with a `check_rewrite` before/after
// snapshot, so the generated Q# pushes function bodies past the line limit.
#![allow(clippy::too_many_lines)]

use crate::package_assigners::PackageAssigners;

use super::*;
use expect_test::expect;
use std::fmt::Write;

#[test]
fn program_without_hofs_converges_without_changes() {
    let source = r#"
        operation Main() : Unit {
            use q = Qubit();
            H(q);
        }
        "#;
    check(
        source,
        &expect![[r#"
            Main: input_ty=Unit"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                H(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
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
fn fixpoint_no_hof_call_sites_prunes_dead_callable_local_chain() {
    let source = r#"
        operation Main() : Unit {
            let first : Int -> Bool = (value) -> value == 0;
            let second : Int -> Bool = first;
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let first : (Int -> Bool) = / * closure item = 2 captures = [] * / _lambda_2;
                let second : (Int -> Bool) = first;
            }
            function _lambda_2(value : Int, ) : Bool {
                value == 0
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {}
            function _lambda_2(value : Int, ) : Bool {
                value == 0
            }
            // entry
            Main()
        "#]],
    );
}

// Covers both snapshot and invariant verification for the 2-level HOF forwarding chain.
#[test]
fn fixpoint_multi_level_hof() {
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
fn full_pipeline_succeeds_for_simple_hof() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
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
fn nested_hof_convergence() {
    let source = r#"
        operation L1(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation L2(op : Qubit => Unit, q : Qubit) : Unit {
            L1(op, q);
        }
        operation L3(op : Qubit => Unit, q : Qubit) : Unit {
            L2(op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            L3(H, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation L1(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation L2(op : (Qubit => Unit), q : Qubit) : Unit {
                L1_Empty_(op, q);
            }
            operation L3(op : (Qubit => Unit), q : Qubit) : Unit {
                L2_Empty_(op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                L3_AdjCtl_(H, q);
                __quantum__rt__qubit_release(q);
            }
            operation L1_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation L3_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                L2_Empty_(op, q);
            }
            operation L2_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                L1_Empty_(op, q);
            }
            // entry
            Main()

            AFTER:
            operation L1(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation L2(op : (Qubit => Unit), q : Qubit) : Unit {
                L1_Empty_(op, q);
            }
            operation L3(op : (Qubit => Unit), q : Qubit) : Unit {
                L2_Empty_(op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                L3_AdjCtl__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation L1_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation L3_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                L2_Empty_(op, q);
            }
            operation L2_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                L1_Empty_(op, q);
            }
            operation L3_AdjCtl__H_(q : Qubit) : Unit {
                L2_Empty__H_(q);
            }
            operation L2_Empty__H_(q : Qubit) : Unit {
                L1_Empty__H_(q);
            }
            operation L1_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn nested_hof_forwarding_with_adjoint() {
    let source = r#"
        operation Inner(op : Qubit => Unit is Adj, q : Qubit) : Unit {
            op(q);
        }
        operation Outer(op : Qubit => Unit is Adj, q : Qubit) : Unit {
            Inner(Adjoint op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Outer(S, q);
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
            operation Outer(op : (Qubit => Unit), q : Qubit) : Unit {
                Inner_Adj_(Adjoint op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Outer_AdjCtl_(S, q);
                __quantum__rt__qubit_release(q);
            }
            operation Inner_Adj_(op : (Qubit => Unit is Adj), q : Qubit) : Unit {
                op(q);
            }
            operation Outer_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                Inner_Adj_(Adjoint op, q);
            }
            // entry
            Main()

            AFTER:
            operation Inner(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Outer(op : (Qubit => Unit), q : Qubit) : Unit {
                Inner_Adj_(Adjoint op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Outer_AdjCtl__S_(q);
                __quantum__rt__qubit_release(q);
            }
            operation Inner_Adj_(op : (Qubit => Unit is Adj), q : Qubit) : Unit {
                op(q);
            }
            operation Outer_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                Inner_Adj_(Adjoint op, q);
            }
            operation Outer_AdjCtl__S_(q : Qubit) : Unit {
                Inner_Adj__Adj_S_(q);
            }
            operation Inner_Adj__Adj_S_(q : Qubit) : Unit {
                Adjoint S(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn nested_hof_controlled_forwarding() {
    let source = r#"
        operation Inner(op : Qubit => Unit is Ctl, q : Qubit) : Unit {
            op(q);
        }
        operation Outer(op : Qubit => Unit is Ctl, q : Qubit) : Unit {
            Inner(op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Outer(X, q);
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
            operation Outer(op : (Qubit => Unit), q : Qubit) : Unit {
                Inner_Ctl_(op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Outer_AdjCtl_(X, q);
                __quantum__rt__qubit_release(q);
            }
            operation Outer_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                Inner_Ctl_(op, q);
            }
            operation Inner_Ctl_(op : (Qubit => Unit is Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation Inner(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Outer(op : (Qubit => Unit), q : Qubit) : Unit {
                Inner_Ctl_(op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Outer_AdjCtl__X_(q);
                __quantum__rt__qubit_release(q);
            }
            operation Outer_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                Inner_Ctl_(op, q);
            }
            operation Inner_Ctl_(op : (Qubit => Unit is Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation Outer_AdjCtl__X_(q : Qubit) : Unit {
                Inner_Ctl__X_(q);
            }
            operation Inner_Ctl__X_(q : Qubit) : Unit {
                X(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn nested_hof_four_levels() {
    let source = r#"
        operation L1(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation L2(op : Qubit => Unit, q : Qubit) : Unit {
            L1(op, q);
        }
        operation L3(op : Qubit => Unit, q : Qubit) : Unit {
            L2(op, q);
        }
        operation L4(op : Qubit => Unit, q : Qubit) : Unit {
            L3(op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            L4(H, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation L1(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation L2(op : (Qubit => Unit), q : Qubit) : Unit {
                L1_Empty_(op, q);
            }
            operation L3(op : (Qubit => Unit), q : Qubit) : Unit {
                L2_Empty_(op, q);
            }
            operation L4(op : (Qubit => Unit), q : Qubit) : Unit {
                L3_Empty_(op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                L4_AdjCtl_(H, q);
                __quantum__rt__qubit_release(q);
            }
            operation L1_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation L3_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                L2_Empty_(op, q);
            }
            operation L2_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                L1_Empty_(op, q);
            }
            operation L4_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                L3_Empty_(op, q);
            }
            // entry
            Main()

            AFTER:
            operation L1(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation L2(op : (Qubit => Unit), q : Qubit) : Unit {
                L1_Empty_(op, q);
            }
            operation L3(op : (Qubit => Unit), q : Qubit) : Unit {
                L2_Empty_(op, q);
            }
            operation L4(op : (Qubit => Unit), q : Qubit) : Unit {
                L3_Empty_(op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                L4_AdjCtl__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation L1_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation L3_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                L2_Empty_(op, q);
            }
            operation L2_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                L1_Empty_(op, q);
            }
            operation L4_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                L3_Empty_(op, q);
            }
            operation L4_AdjCtl__H_(q : Qubit) : Unit {
                L3_Empty__H_(q);
            }
            operation L3_Empty__H_(q : Qubit) : Unit {
                L2_Empty__H_(q);
            }
            operation L2_Empty__H_(q : Qubit) : Unit {
                L1_Empty__H_(q);
            }
            operation L1_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn nested_hof_two_call_sites_different_args() {
    let source = r#"
        operation Inner(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Outer(op : Qubit => Unit, q : Qubit) : Unit {
            Inner(op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Outer(H, q);
            Outer(X, q);
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
            operation Outer(op : (Qubit => Unit), q : Qubit) : Unit {
                Inner_Empty_(op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Outer_AdjCtl_(H, q);
                Outer_AdjCtl_(X, q);
                __quantum__rt__qubit_release(q);
            }
            operation Inner_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Outer_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
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
                Outer_AdjCtl__H_(q);
                Outer_AdjCtl__X_(q);
                __quantum__rt__qubit_release(q);
            }
            operation Inner_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Outer_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                Inner_Empty_(op, q);
            }
            operation Outer_AdjCtl__H_(q : Qubit) : Unit {
                Inner_Empty__H_(q);
            }
            operation Outer_AdjCtl__X_(q : Qubit) : Unit {
                Inner_Empty__X_(q);
            }
            operation Inner_Empty__X_(q : Qubit) : Unit {
                X(q);
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
fn nested_hof_forwarding_adj_autogen() {
    let source = r#"
        operation Inner(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
            op(q);
        }
        operation Outer(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
            Inner(op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Outer(S, q);
            Adjoint Outer(S, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Inner(op : (Qubit => Unit), q : Qubit) : Unit is Adj {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
            }
            operation Outer(op : (Qubit => Unit), q : Qubit) : Unit is Adj {
                body ... {
                    Inner_Adj_(op, q);
                }
                adjoint ... {
                    Adjoint Inner_Adj_(op, q);
                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Outer_AdjCtl_(S, q);
                Adjoint Outer_AdjCtl_(S, q);
                __quantum__rt__qubit_release(q);
            }
            operation Inner_Adj_(op : (Qubit => Unit is Adj), q : Qubit) : Unit is Adj {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
            }
            operation Outer_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit is Adj {
                body ... {
                    Inner_Adj_(op, q);
                }
                adjoint ... {
                    Adjoint Inner_Adj_(op, q);
                }
            }
            // entry
            Main()

            AFTER:
            operation Inner(op : (Qubit => Unit), q : Qubit) : Unit is Adj {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
            }
            operation Outer(op : (Qubit => Unit), q : Qubit) : Unit is Adj {
                body ... {
                    Inner_Adj_(op, q);
                }
                adjoint ... {
                    Adjoint Inner_Adj_(op, q);
                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Outer_AdjCtl__S_(q);
                Adjoint Outer_AdjCtl__S_(q);
                __quantum__rt__qubit_release(q);
            }
            operation Inner_Adj_(op : (Qubit => Unit is Adj), q : Qubit) : Unit is Adj {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
            }
            operation Outer_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit is Adj {
                body ... {
                    Inner_Adj_(op, q);
                }
                adjoint ... {
                    Adjoint Inner_Adj_(op, q);
                }
            }
            operation Outer_AdjCtl__S_(q : Qubit) : Unit is Adj {
                body ... {
                    Inner_Adj__S_(q);
                }
                adjoint ... {
                    Adjoint Inner_Adj__S_(q);
                }
            }
            operation Inner_Adj__S_(q : Qubit) : Unit is Adj {
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
fn nested_hof_requires_multi_iteration_convergence() {
    let source = r#"
        operation ApplyTwice(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
            op(q);
        }

        operation ApplyAndMeasure(action : (Qubit => Unit, Qubit) => Unit, op : Qubit => Unit, q : Qubit) : Result {
            action(op, q);
            M(q)
        }

        operation Main() : Result {
            use q = Qubit();
            ApplyAndMeasure(ApplyTwice, H, q)
        }
        "#;
    check(
        source,
        &expect![[r#"
            ApplyAndMeasure<Empty, AdjCtl>{ApplyTwice<Empty>}{H}: input_ty=Qubit
            ApplyTwice<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyTwice(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
                op(q);
            }
            operation ApplyAndMeasure(action : (((Qubit => Unit), Qubit) => Unit), op : (Qubit => Unit), q : Qubit) : Result {
                action(op, q);
                M(q)
            }
            operation Main() : Result {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_63 : Result = ApplyAndMeasure_Empty__AdjCtl_(ApplyTwice_Empty_, H, q);
                __quantum__rt__qubit_release(q);
                _generated_ident_63
            }
            operation ApplyAndMeasure_Empty__AdjCtl_(action : (((Qubit => Unit), Qubit) => Unit), op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Result {
                action(op, q);
                M(q)
            }
            operation ApplyTwice_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyTwice(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
                op(q);
            }
            operation ApplyAndMeasure(action : (((Qubit => Unit), Qubit) => Unit), op : (Qubit => Unit), q : Qubit) : Result {
                action(op, q);
                M(q)
            }
            operation Main() : Result {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_63 : Result = ApplyAndMeasure_Empty__AdjCtl__ApplyTwice_Empty___H_(q);
                __quantum__rt__qubit_release(q);
                _generated_ident_63
            }
            operation ApplyAndMeasure_Empty__AdjCtl_(action : (((Qubit => Unit), Qubit) => Unit), op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Result {
                action(op, q);
                M(q)
            }
            operation ApplyTwice_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
                op(q);
            }
            operation ApplyAndMeasure_Empty__AdjCtl__ApplyTwice_Empty___H_(q : Qubit) : Result {
                ApplyTwice_Empty__H_(q);
                M(q)
            }
            operation ApplyTwice_Empty__H_(q : Qubit) : Unit {
                H(q);
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn five_level_hof_chain_converges_at_max_iterations_boundary() {
    let source = r#"
        operation L1(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation L2(op : Qubit => Unit, q : Qubit) : Unit {
            L1(op, q);
        }
        operation L3(op : Qubit => Unit, q : Qubit) : Unit {
            L2(op, q);
        }
        operation L4(op : Qubit => Unit, q : Qubit) : Unit {
            L3(op, q);
        }
        operation L5(op : Qubit => Unit, q : Qubit) : Unit {
            L4(op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            L5(H, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation L1(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation L2(op : (Qubit => Unit), q : Qubit) : Unit {
                L1_Empty_(op, q);
            }
            operation L3(op : (Qubit => Unit), q : Qubit) : Unit {
                L2_Empty_(op, q);
            }
            operation L4(op : (Qubit => Unit), q : Qubit) : Unit {
                L3_Empty_(op, q);
            }
            operation L5(op : (Qubit => Unit), q : Qubit) : Unit {
                L4_Empty_(op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                L5_AdjCtl_(H, q);
                __quantum__rt__qubit_release(q);
            }
            operation L1_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation L3_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                L2_Empty_(op, q);
            }
            operation L2_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                L1_Empty_(op, q);
            }
            operation L4_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                L3_Empty_(op, q);
            }
            operation L5_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                L4_Empty_(op, q);
            }
            // entry
            Main()

            AFTER:
            operation L1(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation L2(op : (Qubit => Unit), q : Qubit) : Unit {
                L1_Empty_(op, q);
            }
            operation L3(op : (Qubit => Unit), q : Qubit) : Unit {
                L2_Empty_(op, q);
            }
            operation L4(op : (Qubit => Unit), q : Qubit) : Unit {
                L3_Empty_(op, q);
            }
            operation L5(op : (Qubit => Unit), q : Qubit) : Unit {
                L4_Empty_(op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                L5_AdjCtl__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation L1_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation L3_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                L2_Empty_(op, q);
            }
            operation L2_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                L1_Empty_(op, q);
            }
            operation L4_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                L3_Empty_(op, q);
            }
            operation L5_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                L4_Empty_(op, q);
            }
            operation L5_AdjCtl__H_(q : Qubit) : Unit {
                L4_Empty__H_(q);
            }
            operation L4_Empty__H_(q : Qubit) : Unit {
                L3_Empty__H_(q);
            }
            operation L3_Empty__H_(q : Qubit) : Unit {
                L2_Empty__H_(q);
            }
            operation L2_Empty__H_(q : Qubit) : Unit {
                L1_Empty__H_(q);
            }
            operation L1_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn transient_dynamic_resolves_after_outer_hof_specialization() {
    let source = r#"
        operation ApplyInner(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        operation ApplyMiddle(op : Qubit => Unit, q : Qubit) : Unit {
            ApplyInner(op, q);
        }

        operation ApplyOuter(action : (Qubit => Unit, Qubit) => Unit, op : Qubit => Unit, q : Qubit) : Unit {
            action(op, q);
        }

        operation Main() : Unit {
            use q = Qubit();
            ApplyOuter(ApplyMiddle, H, q);
        }
        "#;
    check_errors(source, &expect!["(no error)"]);
    check(
        source,
        &expect![[r#"
            ApplyInner<Empty>{H}: input_ty=Qubit
            ApplyMiddle<Empty>{H}: input_ty=Qubit
            ApplyOuter<Empty, AdjCtl>{ApplyMiddle<Empty>}{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyInner(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyMiddle(op : (Qubit => Unit), q : Qubit) : Unit {
                ApplyInner_Empty_(op, q);
            }
            operation ApplyOuter(action : (((Qubit => Unit), Qubit) => Unit), op : (Qubit => Unit), q : Qubit) : Unit {
                action(op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOuter_Empty__AdjCtl_(ApplyMiddle_Empty_, H, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyInner_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOuter_Empty__AdjCtl_(action : (((Qubit => Unit), Qubit) => Unit), op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                action(op, q);
            }
            operation ApplyMiddle_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                ApplyInner_Empty_(op, q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyInner(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyMiddle(op : (Qubit => Unit), q : Qubit) : Unit {
                ApplyInner_Empty_(op, q);
            }
            operation ApplyOuter(action : (((Qubit => Unit), Qubit) => Unit), op : (Qubit => Unit), q : Qubit) : Unit {
                action(op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOuter_Empty__AdjCtl__ApplyMiddle_Empty___H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyInner_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOuter_Empty__AdjCtl_(action : (((Qubit => Unit), Qubit) => Unit), op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                action(op, q);
            }
            operation ApplyMiddle_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                ApplyInner_Empty_(op, q);
            }
            operation ApplyOuter_Empty__AdjCtl__ApplyMiddle_Empty___H_(q : Qubit) : Unit {
                ApplyMiddle_Empty__H_(q);
            }
            operation ApplyMiddle_Empty__H_(q : Qubit) : Unit {
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

/// Two-level cross-HOF regression for callable-array forwarding. An outer HOF
/// receives a closure array as a flat parameter and forwards it to an inner HOF
/// that indexes the array under a loop. The closures capture DISTINCT integer
/// values, so a collapse to a single element would be observable.
///
/// The correct post-fix behavior threads ALL array elements across both HOF
/// levels: the inner HOF specializes into an `if idx == p` dispatch chain with a
/// DISTINCT `__capture_i` per branch, and the outer forwards every capture (not
/// just `__capture_0`). A pre-fix cross-HOF array collapse would have produced a
/// single `__capture_0` and no dispatch chain.
#[test]
fn two_level_cross_hof_closure_array_forwarding_threads_all_captures() {
    let source = r#"
        operation ApplyParityOperation(value : Int, control : Qubit, register : Qubit[]) : Unit {
            if value == 1 {
                Controlled X([control], register[0]);
            }
        }

        operation ApplyInner(
            ops : ((Qubit, Qubit[]) => Unit)[],
            count : Int,
            controls : Qubit[],
            targets : Qubit[]
        ) : Unit {
            for idx in 0..count - 1 {
                ops[idx](controls[idx], targets);
            }
        }

        operation ApplyOuter(
            ops : ((Qubit, Qubit[]) => Unit)[],
            count : Int,
            controls : Qubit[],
            targets : Qubit[]
        ) : Unit {
            ApplyInner(ops, count, controls, targets);
        }

        operation Main() : Unit {
            use qs = Qubit[3];
            let controls = qs[0..1];
            let targets = qs[2...];
            let ops = [ApplyParityOperation(1, _, _), ApplyParityOperation(2, _, _)];
            ApplyOuter(ops, 2, controls, targets);
            ResetAll(qs);
        }
        "#;
    check_errors(source, &expect!["(no error)"]);
    check_analysis(
        source,
        &expect![[r#"
        callable_params: 2
          param: callable_id=<item 8 in package 2>, path=[0], ty=(((Qubit, (Qubit)[]) => Unit))[]
          param: callable_id=<item 7 in package 2>, path=[0], ty=(((Qubit, (Qubit)[]) => Unit))[]
        call_sites: 3
          site: hof=ApplyInner<Empty>, arg=Dynamic
          site: hof=ApplyOuter<Empty>, arg=Closure(target=5, Body)
          site: hof=ApplyOuter<Empty>, arg=Closure(target=6, Body)
        direct_call_sites: 1
          site: callee=X:Ctl, default"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyParityOperation(value : Int, control : Qubit, register : Qubit[]) : Unit {
                if value == 1 {
                    Controlled X([control], register[0]);
                }

            }
            operation ApplyInner(ops : ((Qubit, Qubit[]) => Unit)[], count : Int, controls : Qubit[], targets : Qubit[]) : Unit {
                {
                    let _range_id_183 : Range = 0..count - 1;
                    mutable _index_id_186 : Int = _range_id_183::Start;
                    let _step_id_191 : Int = _range_id_183::Step;
                    let _end_id_196 : Int = _range_id_183::End;
                    while _step_id_191 > 0 and _index_id_186 <= _end_id_196 or _step_id_191 < 0 and _index_id_186 >= _end_id_196 {
                        let idx : Int = _index_id_186;
                        ops[idx](controls[idx], targets);
                        _index_id_186 += _step_id_191;
                    }

                }

            }
            operation ApplyOuter(ops : ((Qubit, Qubit[]) => Unit)[], count : Int, controls : Qubit[], targets : Qubit[]) : Unit {
                ApplyInner_Empty_(ops, count, controls, targets);
            }
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                let controls : Qubit[] = qs[0..1];
                let targets : Qubit[] = qs[2...];
                let ops : ((Qubit, Qubit[]) => Unit)[] = [{
                    let arg : Int = 1;
                    / * closure item = 5 captures = [arg] * / _lambda_5
                }, {
                    let arg_1 : Int = 2;
                    / * closure item = 6 captures = [arg_1] * / _lambda_6
                }];
                ApplyOuter_Empty_(ops, 2, controls, targets);
                ResetAll(qs);
                ReleaseQubitArray(qs);
            }
            operation _lambda_5(arg : Int, (hole : Qubit, hole_1 : Qubit[])) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation _lambda_6(arg : Int, (hole : Qubit, hole_1 : Qubit[])) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation ApplyInner_Empty_(ops : ((Qubit, Qubit[]) => Unit)[], count : Int, controls : Qubit[], targets : Qubit[]) : Unit {
                {
                    let _range_id_183 : Range = 0..count - 1;
                    mutable _index_id_186 : Int = _range_id_183::Start;
                    let _step_id_191 : Int = _range_id_183::Step;
                    let _end_id_196 : Int = _range_id_183::End;
                    while _step_id_191 > 0 and _index_id_186 <= _end_id_196 or _step_id_191 < 0 and _index_id_186 >= _end_id_196 {
                        let idx : Int = _index_id_186;
                        ops[idx](controls[idx], targets);
                        _index_id_186 += _step_id_191;
                    }

                }

            }
            operation ApplyOuter_Empty_(ops : ((Qubit, Qubit[]) => Unit)[], count : Int, controls : Qubit[], targets : Qubit[]) : Unit {
                ApplyInner_Empty_(ops, count, controls, targets);
            }
            // entry
            Main()

            AFTER:
            operation ApplyParityOperation(value : Int, control : Qubit, register : Qubit[]) : Unit {
                if value == 1 {
                    Controlled X([control], register[0]);
                }

            }
            operation ApplyInner(ops : ((Qubit, Qubit[]) => Unit)[], count : Int, controls : Qubit[], targets : Qubit[]) : Unit {
                {
                    let _range_id_183 : Range = 0..count - 1;
                    mutable _index_id_186 : Int = _range_id_183::Start;
                    let _step_id_191 : Int = _range_id_183::Step;
                    let _end_id_196 : Int = _range_id_183::End;
                    while _step_id_191 > 0 and _index_id_186 <= _end_id_196 or _step_id_191 < 0 and _index_id_186 >= _end_id_196 {
                        let idx : Int = _index_id_186;
                        ops[idx](controls[idx], targets);
                        _index_id_186 += _step_id_191;
                    }

                }

            }
            operation ApplyOuter(ops : ((Qubit, Qubit[]) => Unit)[], count : Int, controls : Qubit[], targets : Qubit[]) : Unit {
                ApplyInner_Empty_(ops, count, controls, targets);
            }
            operation Main() : Unit {
                let qs : Qubit[] = AllocateQubitArray(3);
                let controls : Qubit[] = qs[0..1];
                let targets : Qubit[] = qs[2...];
                ApplyOuter_Empty__closure__closure_(2, controls, targets, 1, 2);
                ResetAll(qs);
                ReleaseQubitArray(qs);
            }
            operation _lambda_5(arg : Int, (hole : Qubit, hole_1 : Qubit[])) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation _lambda_6(arg : Int, (hole : Qubit, hole_1 : Qubit[])) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation ApplyInner_Empty_(ops : ((Qubit, Qubit[]) => Unit)[], count : Int, controls : Qubit[], targets : Qubit[]) : Unit {
                {
                    let _range_id_183 : Range = 0..count - 1;
                    mutable _index_id_186 : Int = _range_id_183::Start;
                    let _step_id_191 : Int = _range_id_183::Step;
                    let _end_id_196 : Int = _range_id_183::End;
                    while _step_id_191 > 0 and _index_id_186 <= _end_id_196 or _step_id_191 < 0 and _index_id_186 >= _end_id_196 {
                        let idx : Int = _index_id_186;
                        ops[idx](controls[idx], targets);
                        _index_id_186 += _step_id_191;
                    }

                }

            }
            operation ApplyOuter_Empty_(ops : ((Qubit, Qubit[]) => Unit)[], count : Int, controls : Qubit[], targets : Qubit[]) : Unit {
                ApplyInner_Empty_(ops, count, controls, targets);
            }
            operation ApplyOuter_Empty__closure__closure_(count : Int, controls : Qubit[], targets : Qubit[], __capture_0 : Int, __capture_1 : Int) : Unit {
                ApplyInner_Empty__closure__closure_(count, controls, targets, __capture_0, __capture_1);
            }
            operation ApplyInner_Empty__closure__closure_(count : Int, controls : Qubit[], targets : Qubit[], __capture_0 : Int, __capture_1 : Int) : Unit {
                {
                    let _range_id_183 : Range = 0..count - 1;
                    mutable _index_id_186 : Int = _range_id_183::Start;
                    let _step_id_191 : Int = _range_id_183::Step;
                    let _end_id_196 : Int = _range_id_183::End;
                    while _step_id_191 > 0 and _index_id_186 <= _end_id_196 or _step_id_191 < 0 and _index_id_186 >= _end_id_196 {
                        let idx : Int = _index_id_186;
                        if idx == 0 {
                            _lambda_5(__capture_0, (controls[idx], targets))
                        } else {
                            _lambda_6(__capture_1, (controls[idx], targets))
                        };
                        _index_id_186 += _step_id_191;
                    }

                }

            }
            // entry
            Main()
        "#]],
    );
}

/// A closure callable-array forwarded across two higher-order levels and fully
/// consumed by the innermost indexed dispatch leaves the source-array local
/// dead in the reachable caller. Because closure cleanup blanks each element to
/// unit, the surviving array binding would be an arrow-typed block with a unit
/// tail that trips the `PostDefunc` non-unit block-tail invariant. The dead
/// binding must be removed; this runs the invariant walk and full pipeline over
/// the same shape as
/// `two_level_cross_hof_closure_array_forwarding_threads_all_captures`.
#[test]
fn two_level_cross_hof_closure_array_forwarding_passes_invariants() {
    let source = r#"
        operation ApplyParityOperation(value : Int, control : Qubit, register : Qubit[]) : Unit {
            if value == 1 {
                Controlled X([control], register[0]);
            }
        }

        operation ApplyInner(
            ops : ((Qubit, Qubit[]) => Unit)[],
            count : Int,
            controls : Qubit[],
            targets : Qubit[]
        ) : Unit {
            for idx in 0..count - 1 {
                ops[idx](controls[idx], targets);
            }
        }

        operation ApplyOuter(
            ops : ((Qubit, Qubit[]) => Unit)[],
            count : Int,
            controls : Qubit[],
            targets : Qubit[]
        ) : Unit {
            ApplyInner(ops, count, controls, targets);
        }

        operation Main() : Unit {
            use qs = Qubit[3];
            let controls = qs[0..1];
            let targets = qs[2...];
            let ops = [ApplyParityOperation(1, _, _), ApplyParityOperation(2, _, _)];
            ApplyOuter(ops, 2, controls, targets);
            ResetAll(qs);
        }
        "#;
    check_invariants(source);
    check_pipeline(source);
}

/// Regression test for producer-body closure cleanup: a producer function
/// that returns a partial-application closure causes convergence failure
/// when the closure node survives in the producer body after HOF
/// specialization. The closure cleanup pass must replace consumed closures
/// with Unit so that `remaining_callable_value_info` no longer counts them.
#[test]
fn producer_body_closure_cleanup_converges() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation InnerOp(extra : Bool, q : Qubit) : Unit {
            H(q);
        }
        function MakeOp(extra : Bool) : Qubit => Unit {
            return InnerOp(extra, _);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = MakeOp(true);
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
            operation InnerOp(extra : Bool, q : Qubit) : Unit {
                H(q);
            }
            function MakeOp(extra : Bool) : (Qubit => Unit) {
                return {
                    let arg : Bool = extra;
                    / * closure item = 5 captures = [arg] * / _lambda_5
                };
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let op : (Qubit => Unit) = MakeOp(true);
                ApplyOp_Empty_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_5(arg : Bool, hole : Qubit) : Unit {
                InnerOp(arg, hole)
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
            operation InnerOp(extra : Bool, q : Qubit) : Unit {
                H(q);
            }
            function MakeOp(extra : Bool) : (Qubit => Unit) {
                return {
                    let arg : Bool = extra;
                    ()
                };
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__closure_(q, true);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_5(arg : Bool, hole : Qubit) : Unit {
                InnerOp(arg, hole)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__closure_(q : Qubit, __capture_0 : Bool) : Unit {
                _lambda_5(__capture_0, q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn callable_returning_closure_with_controlled_callable_captures() {
    let source = r#"
        operation PrepareIdentity(qs : Qubit[]) : Unit is Adj + Ctl {}

        operation SelectIdentity(systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {}

        function MakeControlledPrepSelPrepOp(
            prepareOp : Qubit[] => Unit is Adj + Ctl,
            selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
            numSystemQubits : Int,
            numAncillaQubits : Int,
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
            numAncillaQubits : Int,
            power : Int
        ) : Unit {
            use control = Qubit();
            use systems = Qubit[numSystemQubits + numAncillaQubits];
            let op = MakeControlledPrepSelPrepOp(
                prepareOp,
                selectOp,
                numSystemQubits,
                numAncillaQubits,
                power
            );
            op(control, systems);
        }

        operation Main() : Unit {
            MakeControlledPrepSelPrepCircuit(
                PrepareIdentity,
                SelectIdentity,
                1,
                1,
                1
            );
        }
        "#;
    check_invariants(source);
}

/// Two callable arguments passed to a multi-parameter HOF: one partial
/// application closure and one global callable. Both must survive cleanup
/// because they are still live as call arguments.
#[test]
fn closure_in_active_call_arg_survives_cleanup() {
    let source = r#"
        operation Apply2(f : Qubit => Unit, g : Qubit => Unit, q : Qubit) : Unit {
            f(q);
            g(q);
        }
        operation Inner(extra : Bool, q : Qubit) : Unit {
            H(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op1 = Inner(true, _);
            Apply2(op1, X, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Apply2(f : (Qubit => Unit), g : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
                g(q);
            }
            operation Inner(extra : Bool, q : Qubit) : Unit {
                H(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let op1 : (Qubit => Unit) = {
                    let arg : Bool = true;
                    / * closure item = 4 captures = [arg] * / _lambda_4
                };
                Apply2_Empty__AdjCtl_(op1, X, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_4(arg : Bool, hole : Qubit) : Unit {
                Inner(arg, hole)
            }
            operation Apply2_Empty__AdjCtl_(f : (Qubit => Unit), g : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                f(q);
                g(q);
            }
            // entry
            Main()

            AFTER:
            operation Apply2(f : (Qubit => Unit), g : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
                g(q);
            }
            operation Inner(extra : Bool, q : Qubit) : Unit {
                H(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Apply2_Empty__AdjCtl__closure__X_(q, true);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_4(arg : Bool, hole : Qubit) : Unit {
                Inner(arg, hole)
            }
            operation Apply2_Empty__AdjCtl_(f : (Qubit => Unit), g : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                f(q);
                g(q);
            }
            operation Apply2_Empty__AdjCtl__closure__X_(q : Qubit, __capture_0 : Bool) : Unit {
                _lambda_4(__capture_0, q);
                X(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn captured_closure_forwarded_to_nested_hof_converges() {
    let source = r#"
        operation ApplySequential(first : Qubit[] => Unit, second : Qubit[] => Unit, systems : Qubit[]) : Unit {
            first(systems);
            second(systems);
        }

        operation ApplyFirstStep(systems : Qubit[]) : Unit {
            for q in systems {
                H(q);
            }
        }

        operation ApplySecondStep(systems : Qubit[]) : Unit {
            for q in systems {
                X(q);
            }
        }

        operation ApplyThirdStep(systems : Qubit[]) : Unit {
            for q in systems {
                Z(q);
            }
        }

        operation Main() : Unit {
            use systems = Qubit[2];
            let sequential = ApplySequential(ApplyFirstStep, ApplySecondStep, _);
            ApplySequential(sequential, ApplyThirdStep, systems);
        }
        "#;
    check_invariants(source);
    check(
        source,
        &expect![[r#"
            ApplyFirstStep: input_ty=(Qubit)[]
            ApplySecondStep: input_ty=(Qubit)[]
            ApplySequential<Empty, Empty>{ApplyFirstStep}{ApplySecondStep}: input_ty=(Qubit)[]
            ApplySequential<Empty, Empty>{closure}{ApplyThirdStep}: input_ty=(Qubit)[]
            ApplyThirdStep: input_ty=(Qubit)[]
            Main: input_ty=Unit"#]],
    );
}

/// Regression: a partial application of a recursive higher-order function,
/// forwarded as that same recursive HOF's own callable argument, converges.
///
/// `Repeat(Repeat(H, 1, _), n - 1, q)` lowers to a closure that captures the
/// fixed callable `H` (and the literal `1`) and forwards them, as parameters, to
/// a lifted lambda that re-invokes `Repeat`. Before the static closure-capture
/// inlining prepass, the captured `H` could not be resolved statically and this
/// construct errored with `DynamicCallable`. The prepass inlines the callable
/// capture into the lifted body, normalizing the closure into the already
/// converging capture-free explicit-lambda shape.
///
/// The `check_rewrite` snapshot locks the full converged specialization chain so
/// a zero-error miscompile with wrong downstream routing (wrong callable,
/// orphaned/duplicated specialization, or wrong recursion target) fails the test.
/// The routing is `{H}` -> `{closure}` -> lifted lambda -> `{H}`, so `Repeat(H, 2, q)`
/// applies `H` exactly twice.
#[test]
fn partial_app_of_recursive_hof_forwarded_as_its_callable_arg_converges() {
    let source = r#"
        operation Repeat(op : Qubit => Unit, n : Int, q : Qubit) : Unit {
            if n > 0 {
                op(q);
                Repeat(Repeat(H, 1, _), n - 1, q);
            }
        }
        operation Main() : Unit {
            use q = Qubit();
            Repeat(H, 2, q);
        }
        "#;
    check_invariants(source);
    check(
        source,
        &expect![[r#"
            .lambda_3: input_ty=(Int, Qubit)
            .lambda_3: input_ty=(Int, Qubit)
            .lambda_3: input_ty=(Int, Qubit)
            Main: input_ty=Unit
            Repeat<AdjCtl>{H}: input_ty=(Int, Qubit)
            Repeat<AdjCtl>{closure}: input_ty=(Int, Qubit, Int)
            Repeat<AdjCtl>{closure}: input_ty=(Int, Qubit, Int)
            Repeat<AdjCtl>{closure}: input_ty=(Int, Qubit, Int)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Repeat(op : (Qubit => Unit), n : Int, q : Qubit) : Unit {
                if n > 0 {
                    op(q);
                    Repeat_Empty_({
                        let arg : (Qubit => Unit is Adj + Ctl) = H;
                        let arg_1 : Int = 1;
                        / * closure item = 3 captures = [arg, arg_1] * / _lambda_3
                    }, n - 1, q);
                }

            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Repeat_AdjCtl_(H, 2, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(arg : (Qubit => Unit is Adj + Ctl), arg_1 : Int, hole : Qubit) : Unit {
                Repeat_AdjCtl_(arg, arg_1, hole)
            }
            operation Repeat_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), n : Int, q : Qubit) : Unit {
                if n > 0 {
                    op(q);
                    Repeat_AdjCtl_({
                        let arg : (Qubit => Unit is Adj + Ctl) = H;
                        let arg_1 : Int = 1;
                        / * closure item = 5 captures = [arg, arg_1] * / _lambda_3
                    }, n - 1, q);
                }

            }
            operation _lambda_3(arg : (Qubit => Unit is Adj + Ctl), arg_1 : Int, hole : Qubit) : Unit {
                Repeat_AdjCtl_(arg, arg_1, hole)
            }
            operation Repeat_Empty_(op : (Qubit => Unit), n : Int, q : Qubit) : Unit {
                if n > 0 {
                    op(q);
                    Repeat_Empty_({
                        let arg : (Qubit => Unit is Adj + Ctl) = H;
                        let arg_1 : Int = 1;
                        / * closure item = 7 captures = [arg, arg_1] * / _lambda_3
                    }, n - 1, q);
                }

            }
            operation _lambda_3(arg : (Qubit => Unit is Adj + Ctl), arg_1 : Int, hole : Qubit) : Unit {
                Repeat_Empty_(arg, arg_1, hole)
            }
            // entry
            Main()

            AFTER:
            operation Repeat(op : (Qubit => Unit), n : Int, q : Qubit) : Unit {
                if n > 0 {
                    op(q);
                    Repeat_Empty_({
                        let arg : (Qubit => Unit is Adj + Ctl) = H;
                        let arg_1 : Int = 1;
                        / * closure item = 3 captures = [arg, arg_1] * / _lambda_3
                    }, n - 1, q);
                }

            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Repeat_AdjCtl__H_(2, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(arg : (Qubit => Unit is Adj + Ctl), arg_1 : Int, hole : Qubit) : Unit {
                Repeat_AdjCtl_(arg, arg_1, hole)
            }
            operation Repeat_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), n : Int, q : Qubit) : Unit {
                if n > 0 {
                    op(q);
                    Repeat_AdjCtl__closure_(n - 1, q, 1);
                }

            }
            operation _lambda_3(arg : Int, hole : Qubit) : Unit {
                Repeat_AdjCtl__H_(arg, hole)
            }
            operation Repeat_Empty_(op : (Qubit => Unit), n : Int, q : Qubit) : Unit {
                if n > 0 {
                    op(q);
                    Repeat_Empty_({
                        let arg : (Qubit => Unit is Adj + Ctl) = H;
                        let arg_1 : Int = 1;
                        / * closure item = 7 captures = [arg, arg_1] * / _lambda_3
                    }, n - 1, q);
                }

            }
            operation _lambda_3(arg : (Qubit => Unit is Adj + Ctl), arg_1 : Int, hole : Qubit) : Unit {
                Repeat_Empty_(arg, arg_1, hole)
            }
            operation Repeat_AdjCtl__H_(n : Int, q : Qubit) : Unit {
                if n > 0 {
                    H(q);
                    Repeat_AdjCtl__closure_(n - 1, q, 1);
                }

            }
            operation _lambda_3(arg : Int, hole : Qubit) : Unit {
                Repeat_AdjCtl__H_(H, arg, hole)
            }
            operation Repeat_AdjCtl__closure_(n : Int, q : Qubit, __capture_0 : Int) : Unit {
                if n > 0 {
                    _lambda_3(__capture_0, q);
                    Repeat_AdjCtl__closure_(n - 1, q, 1);
                }

            }
            operation _lambda_3(arg : Int, hole : Qubit) : Unit {
                Repeat_AdjCtl__closure_(H, arg, hole)
            }
            operation Repeat_AdjCtl__closure_(n : Int, q : Qubit, __capture_0 : Int) : Unit {
                if n > 0 {
                    _lambda_3(__capture_0, q);
                    Repeat_AdjCtl__closure_(n - 1, q, 1);
                }

            }
            operation Repeat_AdjCtl__closure_(n : Int, q : Qubit, __capture_0 : Int) : Unit {
                if n > 0 {
                    _lambda_3(__capture_0, q);
                    Repeat_AdjCtl__closure_(n - 1, q, 1);
                }

            }
            // entry
            Main()
        "#]],
    );
}

/// Companion to the self-forwarding recursive HOF case: the forwarded partial
/// application targets a *different* recursive HOF (`RepeatB`), proving the
/// static closure-capture inlining prepass is not a self-item special case.
#[test]
fn partial_app_of_recursive_hof_forwarded_to_sibling_recursive_hof_converges() {
    let source = r#"
        operation RepeatB(op : Qubit => Unit, n : Int, q : Qubit) : Unit {
            if n > 0 {
                op(q);
                RepeatB(RepeatB(H, 1, _), n - 1, q);
            }
        }
        operation Main() : Unit {
            use q = Qubit();
            RepeatB(H, 2, q);
        }
        "#;
    check_invariants(source);
    check(
        source,
        &expect![[r#"
            .lambda_3: input_ty=(Int, Qubit)
            .lambda_3: input_ty=(Int, Qubit)
            .lambda_3: input_ty=(Int, Qubit)
            Main: input_ty=Unit
            RepeatB<AdjCtl>{H}: input_ty=(Int, Qubit)
            RepeatB<AdjCtl>{closure}: input_ty=(Int, Qubit, Int)
            RepeatB<AdjCtl>{closure}: input_ty=(Int, Qubit, Int)
            RepeatB<AdjCtl>{closure}: input_ty=(Int, Qubit, Int)"#]],
    );
}

/// Regression: when a callable's entire input is a single closure-valued
/// parameter and the passed closure captures exactly one variable, the
/// specialized callee's input must be flattened to a scalar (the single
/// capture), not wrapped in a 1-tuple. Previously the rewrite side built a
/// 1-tuple arg expression while the specialize side flattened the parameter to
/// a scalar, producing a shape mismatch that later surfaced as a "value
/// doesn't support binop gte" panic during evaluation.
#[test]
fn single_capture_single_closure_param_input_is_scalar() {
    let source = r#"
        function RunOp(op : Int -> Bool) : Bool {
            op(5)
        }
        operation Main() : Bool {
            let threshold = 3;
            RunOp(x -> x >= threshold)
        }
        "#;
    check(
        source,
        &expect![[r#"
            .lambda_3: input_ty=(Int, Int)
            Main: input_ty=Unit
            RunOp{closure}: input_ty=Int"#]],
    );
}

/// Companion before/after snapshot for the single-capture single-closure-param
/// flatten fix. The specialized `RunOp` callee takes the captured `threshold`
/// directly as a scalar `Int` parameter, and the call site passes it as a
/// scalar argument (not a 1-tuple).
#[test]
fn single_capture_single_closure_param_rewrite() {
    let source = r#"
        function RunOp(op : Int -> Bool) : Bool {
            op(5)
        }
        operation Main() : Bool {
            let threshold = 3;
            RunOp(x -> x >= threshold)
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            function RunOp(op : (Int -> Bool)) : Bool {
                op(5)
            }
            operation Main() : Bool {
                let threshold : Int = 3;
                RunOp(/ * closure item = 3 captures = [threshold] * / _lambda_3)
            }
            function _lambda_3(threshold : Int, x : Int) : Bool {
                x >= threshold
            }
            // entry
            Main()

            AFTER:
            function RunOp(op : (Int -> Bool)) : Bool {
                op(5)
            }
            operation Main() : Bool {
                let threshold : Int = 3;
                RunOp_closure_(threshold)
            }
            function _lambda_3(threshold : Int, x : Int) : Bool {
                x >= threshold
            }
            function RunOp_closure_(__capture_0 : Int) : Bool {
                _lambda_3(__capture_0, 5)
            }
            // entry
            Main()
        "#]],
    );
}

/// When a mutable callable variable is reassigned in a loop, the analysis
/// resolves it to `Dynamic` (overdefined). The fixpoint loop detects no
/// progress — remaining callable count is unchanged and no new call sites are
/// discovered — and breaks via stuck detection. The `DynamicCallable` error
/// from the current iteration survives, preventing the post-loop
/// `FixpointNotReached` from firing (which only fires when `errors.is_empty()`).
#[test]
fn stuck_detection_with_unresolvable_callable_emits_dynamic_error() {
    check_errors(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            for _ in 0..3 {
                op = X;
            }
            ApplyOp(op, q);
        }
        "#,
        &expect!["callable argument could not be resolved statically"],
    );
}

/// Multi-level HOF chain where each fixpoint iteration resolves one level.
/// Confirms that the before/after progress tracking does not cause premature
/// exit when each iteration successfully reduces the remaining count.
#[test]
fn progress_tracking_allows_multi_iteration_convergence() {
    let source = r#"
        operation L1(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation L2(inner : (Qubit => Unit, Qubit) => Unit, op : Qubit => Unit, q : Qubit) : Unit {
            inner(op, q);
        }
        operation L3(mid : ((Qubit => Unit, Qubit) => Unit, Qubit => Unit, Qubit) => Unit, inner : (Qubit => Unit, Qubit) => Unit, op : Qubit => Unit, q : Qubit) : Unit {
            mid(inner, op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            L3(L2, L1, H, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation L1(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation L2(inner : (((Qubit => Unit), Qubit) => Unit), op : (Qubit => Unit), q : Qubit) : Unit {
                inner(op, q);
            }
            operation L3(mid : (((((Qubit => Unit), Qubit) => Unit), (Qubit => Unit), Qubit) => Unit), inner : (((Qubit => Unit), Qubit) => Unit), op : (Qubit => Unit), q : Qubit) : Unit {
                mid(inner, op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                L3_Empty__Empty__AdjCtl_(L2_Empty__Empty_, L1_Empty_, H, q);
                __quantum__rt__qubit_release(q);
            }
            operation L3_Empty__Empty__AdjCtl_(mid : (((((Qubit => Unit), Qubit) => Unit), (Qubit => Unit), Qubit) => Unit), inner : (((Qubit => Unit), Qubit) => Unit), op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                mid(inner, op, q);
            }
            operation L2_Empty__Empty_(inner : (((Qubit => Unit), Qubit) => Unit), op : (Qubit => Unit), q : Qubit) : Unit {
                inner(op, q);
            }
            operation L1_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation L1(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation L2(inner : (((Qubit => Unit), Qubit) => Unit), op : (Qubit => Unit), q : Qubit) : Unit {
                inner(op, q);
            }
            operation L3(mid : (((((Qubit => Unit), Qubit) => Unit), (Qubit => Unit), Qubit) => Unit), inner : (((Qubit => Unit), Qubit) => Unit), op : (Qubit => Unit), q : Qubit) : Unit {
                mid(inner, op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                L3_Empty__Empty__AdjCtl__L2_Empty__Empty___L1_Empty___H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation L3_Empty__Empty__AdjCtl_(mid : (((((Qubit => Unit), Qubit) => Unit), (Qubit => Unit), Qubit) => Unit), inner : (((Qubit => Unit), Qubit) => Unit), op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                mid(inner, op, q);
            }
            operation L2_Empty__Empty_(inner : (((Qubit => Unit), Qubit) => Unit), op : (Qubit => Unit), q : Qubit) : Unit {
                inner(op, q);
            }
            operation L1_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation L3_Empty__Empty__AdjCtl__L2_Empty__Empty___L1_Empty___H_(q : Qubit) : Unit {
                L2_Empty__Empty__L1_Empty___H_(q);
            }
            operation L2_Empty__Empty__L1_Empty___H_(q : Qubit) : Unit {
                L1_Empty__H_(q);
            }
            operation L1_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn pipeline_resolves_conditional_callable_binding() {
    let source = r#"
        operation ApplyPower(power : Int, op : Qubit => Unit is Adj, target : Qubit) : Unit is Adj {
            let u = if power >= 0 { op } else { Adjoint op };
            for _ in 1..power {
                u(target);
            }
        }

        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            ApplyPower(3, S, q);
        }
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyPower(power : Int, op : (Qubit => Unit), target : Qubit) : Unit is Adj {
                body ... {
                    let u : (Qubit => Unit) = if power >= 0 {
                        op
                    } else {
                        Adjoint op
                    };
                    {
                        let _range_id_116 : Range = 1..power;
                        mutable _index_id_119 : Int = _range_id_116::Start;
                        let _step_id_124 : Int = _range_id_116::Step;
                        let _end_id_129 : Int = _range_id_116::End;
                        while _step_id_124 > 0 and _index_id_119 <= _end_id_129 or _step_id_124 < 0 and _index_id_119 >= _end_id_129 {
                            let _ : Int = _index_id_119;
                            u(target);
                            _index_id_119 += _step_id_124;
                        }

                    }

                }
                adjoint ... {
                    let u : (Qubit => Unit) = if power >= 0 {
                        op
                    } else {
                        Adjoint op
                    };
                    {
                        let _range : Range = 1..power;
                        {
                            let _range_id_159 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                            mutable _index_id_162 : Int = _range_id_159::Start;
                            let _step_id_167 : Int = _range_id_159::Step;
                            let _end_id_172 : Int = _range_id_159::End;
                            while _step_id_167 > 0 and _index_id_162 <= _end_id_172 or _step_id_167 < 0 and _index_id_162 >= _end_id_172 {
                                let _ : Int = _index_id_162;
                                Adjoint u(target);
                                _index_id_162 += _step_id_167;
                            }

                        }

                    }

                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyPower_AdjCtl_(3, S, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyPower_AdjCtl_(power : Int, op : (Qubit => Unit is Adj + Ctl), target : Qubit) : Unit is Adj {
                body ... {
                    let u : (Qubit => Unit is Adj + Ctl) = if power >= 0 {
                        op
                    } else {
                        Adjoint op
                    };
                    {
                        let _range_id_116 : Range = 1..power;
                        mutable _index_id_119 : Int = _range_id_116::Start;
                        let _step_id_124 : Int = _range_id_116::Step;
                        let _end_id_129 : Int = _range_id_116::End;
                        while _step_id_124 > 0 and _index_id_119 <= _end_id_129 or _step_id_124 < 0 and _index_id_119 >= _end_id_129 {
                            let _ : Int = _index_id_119;
                            u(target);
                            _index_id_119 += _step_id_124;
                        }

                    }

                }
                adjoint ... {
                    let u : (Qubit => Unit is Adj + Ctl) = if power >= 0 {
                        op
                    } else {
                        Adjoint op
                    };
                    {
                        let _range : Range = 1..power;
                        {
                            let _range_id_159 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                            mutable _index_id_162 : Int = _range_id_159::Start;
                            let _step_id_167 : Int = _range_id_159::Step;
                            let _end_id_172 : Int = _range_id_159::End;
                            while _step_id_167 > 0 and _index_id_162 <= _end_id_172 or _step_id_167 < 0 and _index_id_162 >= _end_id_172 {
                                let _ : Int = _index_id_162;
                                Adjoint u(target);
                                _index_id_162 += _step_id_167;
                            }

                        }

                    }

                }
            }
            // entry
            Main()

            AFTER:
            operation ApplyPower(power : Int, op : (Qubit => Unit), target : Qubit) : Unit is Adj {
                body ... {
                    let u : (Qubit => Unit) = if power >= 0 {
                        op
                    } else {
                        Adjoint op
                    };
                    {
                        let _range_id_116 : Range = 1..power;
                        mutable _index_id_119 : Int = _range_id_116::Start;
                        let _step_id_124 : Int = _range_id_116::Step;
                        let _end_id_129 : Int = _range_id_116::End;
                        while _step_id_124 > 0 and _index_id_119 <= _end_id_129 or _step_id_124 < 0 and _index_id_119 >= _end_id_129 {
                            let _ : Int = _index_id_119;
                            u(target);
                            _index_id_119 += _step_id_124;
                        }

                    }

                }
                adjoint ... {
                    let u : (Qubit => Unit) = if power >= 0 {
                        op
                    } else {
                        Adjoint op
                    };
                    {
                        let _range : Range = 1..power;
                        {
                            let _range_id_159 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                            mutable _index_id_162 : Int = _range_id_159::Start;
                            let _step_id_167 : Int = _range_id_159::Step;
                            let _end_id_172 : Int = _range_id_159::End;
                            while _step_id_167 > 0 and _index_id_162 <= _end_id_172 or _step_id_167 < 0 and _index_id_162 >= _end_id_172 {
                                let _ : Int = _index_id_162;
                                Adjoint u(target);
                                _index_id_162 += _step_id_167;
                            }

                        }

                    }

                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyPower_AdjCtl__S_(3, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyPower_AdjCtl_(power : Int, op : (Qubit => Unit is Adj + Ctl), target : Qubit) : Unit is Adj {
                body ... {
                    let u : (Qubit => Unit is Adj + Ctl) = if power >= 0 {
                        op
                    } else {
                        Adjoint op
                    };
                    {
                        let _range_id_116 : Range = 1..power;
                        mutable _index_id_119 : Int = _range_id_116::Start;
                        let _step_id_124 : Int = _range_id_116::Step;
                        let _end_id_129 : Int = _range_id_116::End;
                        while _step_id_124 > 0 and _index_id_119 <= _end_id_129 or _step_id_124 < 0 and _index_id_119 >= _end_id_129 {
                            let _ : Int = _index_id_119;
                            u(target);
                            _index_id_119 += _step_id_124;
                        }

                    }

                }
                adjoint ... {
                    let u : (Qubit => Unit is Adj + Ctl) = if power >= 0 {
                        op
                    } else {
                        Adjoint op
                    };
                    {
                        let _range : Range = 1..power;
                        {
                            let _range_id_159 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                            mutable _index_id_162 : Int = _range_id_159::Start;
                            let _step_id_167 : Int = _range_id_159::Step;
                            let _end_id_172 : Int = _range_id_159::End;
                            while _step_id_167 > 0 and _index_id_162 <= _end_id_172 or _step_id_167 < 0 and _index_id_162 >= _end_id_172 {
                                let _ : Int = _index_id_162;
                                Adjoint u(target);
                                _index_id_162 += _step_id_167;
                            }

                        }

                    }

                }
            }
            operation ApplyPower_AdjCtl__S_(power : Int, target : Qubit) : Unit is Adj {
                body ... {
                    {
                        let _range_id_116 : Range = 1..power;
                        mutable _index_id_119 : Int = _range_id_116::Start;
                        let _step_id_124 : Int = _range_id_116::Step;
                        let _end_id_129 : Int = _range_id_116::End;
                        while _step_id_124 > 0 and _index_id_119 <= _end_id_129 or _step_id_124 < 0 and _index_id_119 >= _end_id_129 {
                            let _ : Int = _index_id_119;
                            if power >= 0 {
                                S(target)
                            } else {
                                Adjoint S(target)
                            };
                            _index_id_119 += _step_id_124;
                        }

                    }

                }
                adjoint ... {
                    {
                        let _range : Range = 1..power;
                        {
                            let _range_id_159 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                            mutable _index_id_162 : Int = _range_id_159::Start;
                            let _step_id_167 : Int = _range_id_159::Step;
                            let _end_id_172 : Int = _range_id_159::End;
                            while _step_id_167 > 0 and _index_id_162 <= _end_id_172 or _step_id_167 < 0 and _index_id_162 >= _end_id_172 {
                                let _ : Int = _index_id_162;
                                if power >= 0 {
                                    Adjoint S(target)
                                } else {
                                    S(target)
                                };
                                _index_id_162 += _step_id_167;
                            }

                        }

                    }

                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn pipeline_callable_from_tuple_destructured_array_iteration() {
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
    check_pipeline(source);
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
fn pipeline_teleportation_pattern_callable_from_array_of_tuples() {
    let source = r#"
        namespace Test {
            operation SetToPlus(q : Qubit) : Unit is Adj + Ctl {
                H(q);
            }
            operation SetToMinus(q : Qubit) : Unit is Adj + Ctl {
                X(q);
                H(q);
            }

            @EntryPoint()
            operation Main() : Unit {
                let ops = [
                    (I, PauliZ),
                    (X, PauliZ),
                    (SetToPlus, PauliX),
                    (SetToMinus, PauliX),
                ];
                for (initializer, _basis) in ops {
                    use q = Qubit();
                    initializer(q);
                }
            }
        }
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation SetToPlus(q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    H(q);
                }
                adjoint ... {
                    Adjoint H(q);
                }
                controlled (ctls, ...) {
                    Controlled H(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint H(ctls, q);
                }
            }
            operation SetToMinus(q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    X(q);
                    H(q);
                }
                adjoint ... {
                    Adjoint H(q);
                    Adjoint X(q);
                }
                controlled (ctls, ...) {
                    Controlled X(ctls, q);
                    Controlled H(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint H(ctls, q);
                    Controlled Adjoint X(ctls, q);
                }
            }
            operation Main() : Unit {
                let ops : ((Qubit => Unit is Adj + Ctl), Pauli)[] = [(I, PauliZ), (X, PauliZ), (SetToPlus, PauliX), (SetToMinus, PauliX)];
                {
                    let _array_id_156 : ((Qubit => Unit is Adj + Ctl), Pauli)[] = ops;
                    let _len_id_160 : Int = Length(_array_id_156);
                    mutable _index_id_165 : Int = 0;
                    while _index_id_165 < _len_id_160 {
                        let (initializer : (Qubit => Unit is Adj + Ctl), _basis : Pauli) = _array_id_156[_index_id_165];
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        initializer(q);
                        _index_id_165 += 1;
                        __quantum__rt__qubit_release(q);
                    }

                }

            }
            // entry
            Main()

            AFTER:
            operation SetToPlus(q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    H(q);
                }
                adjoint ... {
                    Adjoint H(q);
                }
                controlled (ctls, ...) {
                    Controlled H(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint H(ctls, q);
                }
            }
            operation SetToMinus(q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    X(q);
                    H(q);
                }
                adjoint ... {
                    Adjoint H(q);
                    Adjoint X(q);
                }
                controlled (ctls, ...) {
                    Controlled X(ctls, q);
                    Controlled H(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint H(ctls, q);
                    Controlled Adjoint X(ctls, q);
                }
            }
            operation Main() : Unit {
                let ops : ((Qubit => Unit is Adj + Ctl), Pauli)[] = [(I, PauliZ), (X, PauliZ), (SetToPlus, PauliX), (SetToMinus, PauliX)];
                {
                    let _array_id_156 : ((Qubit => Unit is Adj + Ctl), Pauli)[] = ops;
                    let _len_id_160 : Int = Length(_array_id_156);
                    mutable _index_id_165 : Int = 0;
                    while _index_id_165 < _len_id_160 {
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        if _index_id_165 == 0 {
                            I(q)
                        } else if _index_id_165 == 1 {
                            X(q)
                        } else if _index_id_165 == 2 {
                            SetToPlus(q)
                        } else {
                            SetToMinus(q)
                        };
                        _index_id_165 += 1;
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
fn pipeline_callable_at_middle_of_three_tuple_from_array_iteration() {
    let source = r#"
        namespace Test {
            operation SetToPlus(q : Qubit) : Unit is Adj + Ctl {
                H(q);
            }
            operation SetToMinus(q : Qubit) : Unit is Adj + Ctl {
                X(q);
                H(q);
            }

            @EntryPoint()
            operation Main() : Unit {
                let ops = [
                    (PauliZ, I, false),
                    (PauliZ, X, false),
                    (PauliX, SetToPlus, true),
                    (PauliX, SetToMinus, true),
                ];
                for (_basis, initializer, _flag) in ops {
                    use q = Qubit();
                    initializer(q);
                }
            }
        }
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation SetToPlus(q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    H(q);
                }
                adjoint ... {
                    Adjoint H(q);
                }
                controlled (ctls, ...) {
                    Controlled H(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint H(ctls, q);
                }
            }
            operation SetToMinus(q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    X(q);
                    H(q);
                }
                adjoint ... {
                    Adjoint H(q);
                    Adjoint X(q);
                }
                controlled (ctls, ...) {
                    Controlled X(ctls, q);
                    Controlled H(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint H(ctls, q);
                    Controlled Adjoint X(ctls, q);
                }
            }
            operation Main() : Unit {
                let ops : (Pauli, (Qubit => Unit is Adj + Ctl), Bool)[] = [(PauliZ, I, false), (PauliZ, X, false), (PauliX, SetToPlus, true), (PauliX, SetToMinus, true)];
                {
                    let _array_id_162 : (Pauli, (Qubit => Unit is Adj + Ctl), Bool)[] = ops;
                    let _len_id_166 : Int = Length(_array_id_162);
                    mutable _index_id_171 : Int = 0;
                    while _index_id_171 < _len_id_166 {
                        let (_basis : Pauli, initializer : (Qubit => Unit is Adj + Ctl), _flag : Bool) = _array_id_162[_index_id_171];
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        initializer(q);
                        _index_id_171 += 1;
                        __quantum__rt__qubit_release(q);
                    }

                }

            }
            // entry
            Main()

            AFTER:
            operation SetToPlus(q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    H(q);
                }
                adjoint ... {
                    Adjoint H(q);
                }
                controlled (ctls, ...) {
                    Controlled H(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint H(ctls, q);
                }
            }
            operation SetToMinus(q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    X(q);
                    H(q);
                }
                adjoint ... {
                    Adjoint H(q);
                    Adjoint X(q);
                }
                controlled (ctls, ...) {
                    Controlled X(ctls, q);
                    Controlled H(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint H(ctls, q);
                    Controlled Adjoint X(ctls, q);
                }
            }
            operation Main() : Unit {
                let ops : (Pauli, (Qubit => Unit is Adj + Ctl), Bool)[] = [(PauliZ, I, false), (PauliZ, X, false), (PauliX, SetToPlus, true), (PauliX, SetToMinus, true)];
                {
                    let _array_id_162 : (Pauli, (Qubit => Unit is Adj + Ctl), Bool)[] = ops;
                    let _len_id_166 : Int = Length(_array_id_162);
                    mutable _index_id_171 : Int = 0;
                    while _index_id_171 < _len_id_166 {
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        if _index_id_171 == 0 {
                            I(q)
                        } else if _index_id_171 == 1 {
                            X(q)
                        } else if _index_id_171 == 2 {
                            SetToPlus(q)
                        } else {
                            SetToMinus(q)
                        };
                        _index_id_171 += 1;
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
fn pipeline_teleportation_like_callable_from_string_tagged_triple_array() {
    let source = r#"
        namespace Test {
            operation SetToPlus(q : Qubit) : Unit is Adj + Ctl {
                H(q);
            }
            operation SetToMinus(q : Qubit) : Unit is Adj + Ctl {
                X(q);
                H(q);
            }

            @EntryPoint()
            operation Main() : Unit {
                let ops = [
                    (I, PauliZ),
                    (X, PauliZ),
                    (SetToPlus, PauliX),
                    (SetToMinus, PauliX),
                ];
                for (initializer, basis) in ops {
                    use q = Qubit();
                    initializer(q);
                    let _ = Measure([basis], [q]);
                    Reset(q);
                }
            }
        }
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation SetToPlus(q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    H(q);
                }
                adjoint ... {
                    Adjoint H(q);
                }
                controlled (ctls, ...) {
                    Controlled H(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint H(ctls, q);
                }
            }
            operation SetToMinus(q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    X(q);
                    H(q);
                }
                adjoint ... {
                    Adjoint H(q);
                    Adjoint X(q);
                }
                controlled (ctls, ...) {
                    Controlled X(ctls, q);
                    Controlled H(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint H(ctls, q);
                    Controlled Adjoint X(ctls, q);
                }
            }
            operation Main() : Unit {
                let ops : ((Qubit => Unit is Adj + Ctl), Pauli)[] = [(I, PauliZ), (X, PauliZ), (SetToPlus, PauliX), (SetToMinus, PauliX)];
                {
                    let _array_id_169 : ((Qubit => Unit is Adj + Ctl), Pauli)[] = ops;
                    let _len_id_173 : Int = Length(_array_id_169);
                    mutable _index_id_178 : Int = 0;
                    while _index_id_178 < _len_id_173 {
                        let (initializer : (Qubit => Unit is Adj + Ctl), basis : Pauli) = _array_id_169[_index_id_178];
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        initializer(q);
                        let _ : Result = Measure([basis], [q]);
                        Reset(q);
                        _index_id_178 += 1;
                        __quantum__rt__qubit_release(q);
                    }

                }

            }
            // entry
            Main()

            AFTER:
            operation SetToPlus(q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    H(q);
                }
                adjoint ... {
                    Adjoint H(q);
                }
                controlled (ctls, ...) {
                    Controlled H(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint H(ctls, q);
                }
            }
            operation SetToMinus(q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    X(q);
                    H(q);
                }
                adjoint ... {
                    Adjoint H(q);
                    Adjoint X(q);
                }
                controlled (ctls, ...) {
                    Controlled X(ctls, q);
                    Controlled H(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint H(ctls, q);
                    Controlled Adjoint X(ctls, q);
                }
            }
            operation Main() : Unit {
                let ops : ((Qubit => Unit is Adj + Ctl), Pauli)[] = [(I, PauliZ), (X, PauliZ), (SetToPlus, PauliX), (SetToMinus, PauliX)];
                {
                    let _array_id_169 : ((Qubit => Unit is Adj + Ctl), Pauli)[] = ops;
                    let _len_id_173 : Int = Length(_array_id_169);
                    mutable _index_id_178 : Int = 0;
                    while _index_id_178 < _len_id_173 {
                        let (initializer : (Qubit => Unit is Adj + Ctl), basis : Pauli) = _array_id_169[_index_id_178];
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        if _index_id_178 == 0 {
                            I(q)
                        } else if _index_id_178 == 1 {
                            X(q)
                        } else if _index_id_178 == 2 {
                            SetToPlus(q)
                        } else {
                            SetToMinus(q)
                        };
                        let _ : Result = Measure([basis], [q]);
                        Reset(q);
                        _index_id_178 += 1;
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
fn pipeline_callable_array_iteration_exceeding_old_multi_cap() {
    let source = r#"
        namespace Test {
            operation SX(q : Qubit) : Unit is Adj + Ctl {
                Rx(Microsoft.Quantum.Math.PI() / 2.0, q);
            }

            @EntryPoint()
            operation Main() : Unit {
                let gates = [H, X, Y, Z, S, Adjoint S, SX];
                use q = Qubit();
                for gate in gates {
                    gate(q);
                }
            }
        }
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation SX(q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    Rx(PI() / 2., q);
                }
                adjoint ... {
                    Adjoint Rx(PI() / 2., q);
                }
                controlled (ctls, ...) {
                    Controlled Rx(ctls, (PI() / 2., q));
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint Rx(ctls, (PI() / 2., q));
                }
            }
            operation Main() : Unit {
                let gates : (Qubit => Unit is Adj + Ctl)[] = [H, X, Y, Z, S, Adjoint S, SX];
                let q : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_137 : Unit = {
                    let _array_id_104 : (Qubit => Unit is Adj + Ctl)[] = gates;
                    let _len_id_108 : Int = Length(_array_id_104);
                    mutable _index_id_113 : Int = 0;
                    while _index_id_113 < _len_id_108 {
                        let gate : (Qubit => Unit is Adj + Ctl) = _array_id_104[_index_id_113];
                        gate(q);
                        _index_id_113 += 1;
                    }

                };
                __quantum__rt__qubit_release(q);
                _generated_ident_137
            }
            // entry
            Main()

            AFTER:
            operation SX(q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    Rx(PI() / 2., q);
                }
                adjoint ... {
                    Adjoint Rx(PI() / 2., q);
                }
                controlled (ctls, ...) {
                    Controlled Rx(ctls, (PI() / 2., q));
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint Rx(ctls, (PI() / 2., q));
                }
            }
            operation Main() : Unit {
                let gates : (Qubit => Unit is Adj + Ctl)[] = [H, X, Y, Z, S, Adjoint S, SX];
                let q : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_137 : Unit = {
                    let _array_id_104 : (Qubit => Unit is Adj + Ctl)[] = gates;
                    let _len_id_108 : Int = Length(_array_id_104);
                    mutable _index_id_113 : Int = 0;
                    while _index_id_113 < _len_id_108 {
                        if _index_id_113 == 0 {
                            H(q)
                        } else if _index_id_113 == 1 {
                            X(q)
                        } else if _index_id_113 == 2 {
                            Y(q)
                        } else if _index_id_113 == 3 {
                            Z(q)
                        } else if _index_id_113 == 4 {
                            S(q)
                        } else if _index_id_113 == 5 {
                            Adjoint S(q)
                        } else {
                            SX(q)
                        };
                        _index_id_113 += 1;
                    }

                };
                __quantum__rt__qubit_release(q);
                _generated_ident_137
            }
            // entry
            Main()
        "#]],
    );
}

fn nested_hof_source(level_count: usize) -> String {
    assert!(level_count > 0);

    let mut source = String::new();
    source.push_str("operation Level01(op : Qubit => Unit, q : Qubit) : Unit {\n    op(q);\n}\n");

    for level in 2..=level_count {
        write!(
            &mut source,
            "operation Level{level:02}(op : Qubit => Unit, q : Qubit) : Unit {{\n    Level{previous:02}(op, q);\n}}\n",
            previous = level - 1,
        ).expect("failed to write source string");
    }

    write!(
        &mut source,
        "@EntryPoint()\noperation Main() : Unit {{\n    use q = Qubit();\n    Level{level_count:02}(H, q);\n}}\n"
    ).expect("failed to write source string");
    source
}

#[test]
fn defunc_20_level_hof_completes_without_error() {
    // Regression test: 20-level HOF nesting is under the convergence cap.
    let source = nested_hof_source(20);

    let (mut fir_store, fir_pkg_id) = crate::test_utils::compile_to_monomorphized_fir(&source);
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    let errors = super::super::defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners);

    assert!(
        errors.is_empty(),
        "Expected defunctionalization to succeed for 20-level HOF, got: {:?}",
        errors.iter().map(ToString::to_string).collect::<Vec<_>>()
    );
}

#[test]
fn defunc_21_level_hof_returns_static_resolution_error() {
    // Regression test: 21-level HOF nesting exceeds the current static
    // resolution depth, but still reports a defunctionalization diagnostic
    // instead of panicking or lowering invalid FIR.
    let source = nested_hof_source(21);

    let (mut fir_store, fir_pkg_id) = crate::test_utils::compile_to_monomorphized_fir(&source);
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    let errors = super::super::defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners);

    assert!(
        !errors.is_empty(),
        "Expected defunctionalization error for 21-level HOF"
    );

    assert!(
        matches!(errors.as_slice(), [super::super::Error::DynamicCallable(_)]),
        "Expected DynamicCallable error, got: {:?}",
        errors.iter().map(ToString::to_string).collect::<Vec<_>>()
    );
}

#[test]
fn multiple_forwarded_callable_arrays_return_unsupported_error() {
    // Forwarding two or more distinct callable arrays through a single HOF call
    // is a shape the transform does not support. This test pins the diagnostic
    // so a future change cannot silently start generating incorrect code for
    // it.
    //
    // A two-level HOF forwards two distinct arrays of callables through one
    // call. The transform must report exactly one
    // `UnsupportedMultipleCallableArrays` diagnostic. It must not fall through
    // to the per-row path, which would collapse each array to a single member,
    // nor report a spurious `FixpointNotReached`.
    let source = r#"
        operation ApplyTwoArrays(
            firstOps : (Qubit => Unit)[],
            secondOps : (Qubit => Unit)[],
            q : Qubit
        ) : Unit {
            for op in firstOps {
                op(q);
            }
            for op in secondOps {
                op(q);
            }
        }
        operation ForwardTwoArrays(
            firstOps : (Qubit => Unit)[],
            secondOps : (Qubit => Unit)[],
            q : Qubit
        ) : Unit {
            ApplyTwoArrays(firstOps, secondOps, q);
        }
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            ForwardTwoArrays([X, Y], [Z, H], q);
        }
        "#;

    let (mut store, package_id) = compile_to_monomorphized_fir(source);
    let mut assigners = PackageAssigners::new(&store, package_id);
    let errors = defunctionalize(&mut store, package_id, &mut assigners);

    assert!(
        matches!(
            errors.as_slice(),
            [super::super::Error::UnsupportedMultipleCallableArrays(_)]
        ),
        "expected exactly one UnsupportedMultipleCallableArrays error, got: {}",
        format_defunctionalization_errors(&errors)
    );
}

#[test]
fn operation_computed_captured_field_declines_to_dynamic_callable() {
    // A captured struct field whose value is computed by an operation call
    // cannot be specialized. Rebuilding the captured literal in the caller would
    // duplicate and reorder that operation call, which is unsound for a call
    // with quantum side effects because it cannot be run twice or moved. The
    // transform therefore declines the closure to a dynamic call site and
    // reports a recoverable `DynamicCallable` diagnostic. On the base profile
    // this surfaces as a hard error rather than silently incorrect code.
    check_errors(
        r#"
        struct Wrapper { Op : Qubit => Unit }
        operation Choose(flag : Result) : (Qubit => Unit) {
            return flag == One ? X | H;
        }
        operation ApplyWrapped(w : Wrapper, q : Qubit) : Unit {
            w.Op(q);
        }
        operation MakeWrapper(q : Qubit) : Wrapper {
            new Wrapper { Op = Choose(MResetZ(q)) }
        }
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            let w = MakeWrapper(q);
            ApplyWrapped(w, q);
        }
        "#,
        &expect!["callable argument could not be resolved statically"],
    );
}

#[test]
fn operation_call_in_captured_compound_literal_without_locals_declines_to_dynamic_callable() {
    // A closure returned from `MakeOp` captures a `Payload` struct literal whose
    // field is initialized by a runtime operation call (`ReadValue()`), with no
    // intervening local binding to anchor that call. The captured value is not a
    // statically-known callable, so defunctionalization must decline the
    // `ApplyOp(MakeOp(), q)` call site to a dynamic callable — emitting the
    // "callable argument could not be resolved statically" diagnostic — rather than
    // attempt to specialize the unresolved compound-literal capture.
    check_errors(
        r#"
        struct Payload { Value : Int }
        operation ReadValue() : Int {
            return 1;
        }
        operation UsePayload(payload : Payload, q : Qubit) : Unit {
            if payload.Value == 1 {
                H(q);
            }
        }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation MakeOp() : Qubit => Unit {
            let payload = new Payload { Value = ReadValue() };
            return q => UsePayload(payload, q);
        }
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(MakeOp(), q);
        }
        "#,
        &expect!["callable argument could not be resolved statically"],
    );
}
