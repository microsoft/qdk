// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::needless_raw_string_hashes)]

//! Tests for the conditional-guard normalization pass.

use expect_test::{Expect, expect};
use indoc::indoc;

use crate::cond_normalize::normalize_conditions;
use crate::package_assigners::PackageAssigners;
use crate::test_utils::{
    PipelineStage, check_semantic_equivalence, check_semantic_equivalence_with_library,
    compile_and_run_pipeline_to, compile_and_run_pipeline_to_with_library,
    compile_to_monomorphized_fir, find_library_callable,
};

/// Compiles Q# source to monomorphized FIR and snapshots the pretty-printed
/// package before and after [`normalize_conditions`], so the enclosing-block
/// hoist of side-effecting `if` conditions can be reviewed directly.
fn check_normalize(source: &str, expect: &Expect) {
    let (mut store, pkg_id) = compile_to_monomorphized_fir(source);
    let before = crate::pretty::write_package_qsharp_parseable(&store, pkg_id);
    let mut assigners = PackageAssigners::new(&store, pkg_id);
    normalize_conditions(&mut store, pkg_id, &mut assigners);
    let after = crate::pretty::write_package_qsharp_parseable(&store, pkg_id);
    expect.assert_eq(&format!("BEFORE:\n{before}\nAFTER:\n{after}"));
}

/// Asserts that [`normalize_conditions`] leaves the package byte-identical,
/// used for conditions that are pure and must not be hoisted.
fn assert_no_change(source: &str) {
    let (mut store, pkg_id) = compile_to_monomorphized_fir(source);
    let before = crate::pretty::write_package_qsharp_parseable(&store, pkg_id);
    let mut assigners = PackageAssigners::new(&store, pkg_id);
    normalize_conditions(&mut store, pkg_id, &mut assigners);
    let after = crate::pretty::write_package_qsharp_parseable(&store, pkg_id);
    assert_eq!(before, after, "pure condition must not be rewritten");
}

/// A `Var` condition has no side effects, so the `if` is left untouched.
#[test]
fn pure_var_condition_is_not_hoisted() {
    assert_no_change(indoc! {r#"
        operation Main() : Unit {
            use q = Qubit();
            let flag = true;
            if flag {
                X(q);
            }
        }
    "#});
}

/// A comparison of pure values has no side effects, so the `if` is untouched.
#[test]
fn pure_comparison_condition_is_not_hoisted() {
    assert_no_change(indoc! {r#"
        operation Main() : Unit {
            use q = Qubit();
            let n = 3;
            if n >= 0 {
                X(q);
            }
        }
    "#});
}

/// A condition that is a block containing a call is side-effecting, so it is
/// hoisted into a single-evaluation `let` and the `if` tests the temporary.
#[test]
fn side_effecting_block_condition_is_hoisted() {
    check_normalize(
        indoc! {r#"
            operation Main() : Unit {
                use q = Qubit();
                if { Y(q); true } {
                    X(q);
                }
                Z(q);
            }
        "#},
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                if {
                    Y(q);
                    true
                }
                {
                    X(q);
                }

                Z(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let __cond_0 : Bool = {
                    Y(q);
                    true
                };
                if __cond_0 {
                    X(q);
                }

                Z(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

/// Every side-effecting condition in an `if`/`elif`/`else` chain is normalized
/// to a single evaluation. The unconditionally-evaluated outer condition
/// becomes an immutable `let __cond = ...;`, while each `elif` condition becomes
/// a `mutable __cond = false;` accumulator declared in the enclosing block and
/// conditionally assigned in its else scope (`else { __cond = ...; if __cond ..`),
/// so the guard stays in a scope that dominates any later dispatch site while
/// still evaluating its effects only when the preceding guards were false.
#[test]
fn else_cond_side_effecting_block_conditions_are_hoisted() {
    check_normalize(
        indoc! {r#"
            operation Main() : Unit {
                sut(true, true, true);
            }
            operation sut(cond1: Bool, cond2: Bool, cond3: Bool) : Result {
                use q = Qubit();
                if { Y(q); cond1 } {
                    X(q);
                } elif { X(q); cond2 } {
                    Y(q);
                } elif { Y(q); cond3 } {
                    Y(q);
                } else {
                    Z(q);
                }
                MResetZ(q)
            }
        "#},
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                sut(true, true, true);
            }
            operation sut(cond1 : Bool, cond2 : Bool, cond3 : Bool) : Result {
                let q : Qubit = __quantum__rt__qubit_allocate();
                if {
                    Y(q);
                    cond1
                }
                {
                    X(q);
                } else if {
                    X(q);
                    cond2
                }
                {
                    Y(q);
                } else if {
                    Y(q);
                    cond3
                }
                {
                    Y(q);
                } else {
                    Z(q);
                }

                let _generated_ident_92 : Result = MResetZ(q);
                __quantum__rt__qubit_release(q);
                _generated_ident_92
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                sut(true, true, true);
            }
            operation sut(cond1 : Bool, cond2 : Bool, cond3 : Bool) : Result {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let __cond_0 : Bool = {
                    Y(q);
                    cond1
                };
                mutable __cond_1 : Bool = false;
                mutable __cond_2 : Bool = false;
                if __cond_0 {
                    X(q);
                } else {
                    __cond_1 = {
                        X(q);
                        cond2
                    };
                    if __cond_1 {
                        Y(q);
                    } else {
                        __cond_2 = {
                            Y(q);
                            cond3
                        };
                        if __cond_2 {
                            Y(q);
                        } else {
                            Z(q);
                        }

                    }

                }

                let _generated_ident_92 : Result = MResetZ(q);
                __quantum__rt__qubit_release(q);
                _generated_ident_92
            }
            // entry
            Main()
        "#]],
    );
}

/// A condition that calls a measurement is side-effecting and is hoisted, so
/// the measurement runs exactly once even though downstream passes may reuse
/// the guard.
#[test]
fn measurement_condition_is_hoisted() {
    check_normalize(
        indoc! {r#"
            operation Main() : Unit {
                use q = Qubit();
                if MResetZ(q) == One {
                    X(q);
                }
                Z(q);
            }
        "#},
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                if (MResetZ(q) == One) {
                    X(q);
                }

                Z(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let __cond_0 : Bool = (MResetZ(q) == One);
                if __cond_0 {
                    X(q);
                }

                Z(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

/// A side-effecting condition on a *nested* statement-position `if` declares
/// its guard accumulator at the top of the root (specialization) block — which
/// dominates every dispatch site defunctionalization can build — while the
/// side-effecting `set` stays at the original evaluation point in the nested
/// block. This is the scope-aware lift.
#[test]
fn nested_if_condition_accumulator_is_declared_in_root_block() {
    check_normalize(
        indoc! {r#"
            operation Main() : Unit {
                use q = Qubit();
                if MResetZ(q) == One {
                    if { Y(q); true } {
                        X(q);
                    }
                }
                Z(q);
            }
        "#},
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                if (MResetZ(q) == One) {
                    if {
                        Y(q);
                        true
                    }
                    {
                        X(q);
                    }

                }

                Z(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                mutable __cond_1 : Bool = false;
                let q : Qubit = __quantum__rt__qubit_allocate();
                let __cond_0 : Bool = (MResetZ(q) == One);
                if __cond_0 {
                    __cond_1 = {
                        Y(q);
                        true
                    };
                    if __cond_1 {
                        X(q);
                    }

                }

                Z(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

/// A side-effecting `else if` inside a *nested* `if` chain declares its
/// mutable accumulator in the root block, not the nested enclosing block:
/// defunctionalization can lift the reused elif guard to an outer dispatch
/// site that the nested block does not dominate, so the accumulator must sit in
/// the dominating root scope while the conditional `_cond = c;` evaluation stays
/// at the original point. Complements `else_cond_side_effecting_block_conditions_are_hoisted`
/// (top-level elif, accumulator in the enclosing block) and
/// `nested_if_condition_accumulator_is_declared_in_root_block` (nested outer
/// condition, no elif).
#[test]
fn nested_else_if_accumulator_is_declared_in_root_block() {
    check_normalize(
        indoc! {r#"
            operation Main() : Unit {
                use q = Qubit();
                if MResetZ(q) == One {
                    if { Y(q); true } {
                        X(q);
                    } elif { Z(q); false } {
                        Y(q);
                    }
                }
                Z(q);
            }
        "#},
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                if (MResetZ(q) == One) {
                    if {
                        Y(q);
                        true
                    }
                    {
                        X(q);
                    } else if {
                        Z(q);
                        false
                    }
                    {
                        Y(q);
                    }

                }

                Z(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                mutable __cond_1 : Bool = false;
                mutable __cond_2 : Bool = false;
                let q : Qubit = __quantum__rt__qubit_allocate();
                let __cond_0 : Bool = (MResetZ(q) == One);
                if __cond_0 {
                    __cond_1 = {
                        Y(q);
                        true
                    };
                    if __cond_1 {
                        X(q);
                    } else {
                        __cond_2 = {
                            Z(q);
                            false
                        };
                        if __cond_2 {
                            Y(q);
                        }

                    }

                }

                Z(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

/// A `while` guard is re-evaluated each iteration, so it must not be hoisted
/// even when it carries side effects.
#[test]
fn while_condition_is_not_hoisted() {
    assert_no_change(indoc! {r#"
        operation Main() : Unit {
            use q = Qubit();
            mutable count = 0;
            while { Y(q); count < 3 } {
                set count += 1;
            }
        }
    "#});
}

/// A value-position `if` (one that produces a binding's value) is left
/// untouched even when its condition is side-effecting: defunctionalization
/// removes the binding and rebuilds a tree that references each guard once, so
/// this pass deliberately only normalizes statement-position `if`s.
#[test]
fn value_position_if_condition_is_not_hoisted() {
    assert_no_change(indoc! {r#"
        operation Main() : Unit {
            use q = Qubit();
            let r = if { Y(q); true } { 1 } else { 0 };
            if r == 1 {
                X(q);
            }
        }
    "#});
}

/// Normalization preserves observable behavior end to end through the full
/// pipeline for a side-effecting `if` condition.
#[test]
fn side_effecting_condition_preserves_semantics() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                mutable acc = 0;
                if { set acc += 1; acc == 1 } {
                    X(q);
                }
                return MResetZ(q);
            }
        }
    "#});
}

/// Running the pass twice is a no-op after the first application: a hoisted
/// condition is a pure `Var`, so the second pass finds nothing to rewrite.
#[test]
fn normalization_is_idempotent() {
    let (mut store, pkg_id) = compile_to_monomorphized_fir(indoc! {r#"
        operation Main() : Unit {
            use q = Qubit();
            if { Y(q); true } {
                X(q);
            }
            Z(q);
        }
    "#});
    let mut assigners = PackageAssigners::new(&store, pkg_id);
    normalize_conditions(&mut store, pkg_id, &mut assigners);
    let once = crate::pretty::write_package_qsharp_parseable(&store, pkg_id);
    normalize_conditions(&mut store, pkg_id, &mut assigners);
    let twice = crate::pretty::write_package_qsharp_parseable(&store, pkg_id);
    assert_eq!(once, twice, "second normalization pass must be a no-op");
}

/// The full pipeline runs to completion with a side-effecting condition,
/// exercising normalization ahead of defunctionalization.
#[test]
fn full_pipeline_runs_with_side_effecting_condition() {
    let _ = compile_and_run_pipeline_to(
        indoc! {r#"
            namespace Test {
                @EntryPoint()
                operation Main() : Result {
                    use q = Qubit();
                    if { Y(q); true } {
                        X(q);
                    }
                    return MResetZ(q);
                }
            }
        "#},
        PipelineStage::Full,
    );
}

/// A callable held in a mutable is reassigned across an `if`/`elif`/`else`
/// chain whose conditions are side-effecting, then dispatched.
/// Defunctionalization reconstructs the branch decision at the `ApplyOp` site
/// by reusing the chain's guards, so each guard must be a pure read of a
/// binding that dominates that site. The mutable-accumulator normalization of
/// the `elif` condition keeps the guard in the enclosing block while still
/// evaluating its side effects at most once, so the transformed program must
/// match the original in both return value and effect trace.
#[test]
fn mutable_callable_dispatch_across_side_effecting_elif_is_equivalent() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                use a = Qubit();
                mutable op = H;
                if { Z(a); MResetZ(a) == One } {
                    set op = X;
                } elif { X(a); MResetZ(a) == One } {
                    set op = Y;
                } else {
                    set op = T;
                }
                ApplyOp(op, q);
                return MResetZ(q);
            }
        }
    "#});
}

/// Regression: a mutable callable is reassigned inside a *nested*
/// side-effecting `if` (itself the body of an outer branch) and then dispatched
/// at the *outer* level. Defunctionalization lifts the reused inner guard
/// across the branch boundary to the outer dispatch site, so `cond_normalize`
/// must declare the guard accumulator in the specialization's root block (which
/// dominates the dispatch site) rather than the immediately-enclosing block.
/// Before the scope-aware lift this panicked with a `LocalVarId` consistency
/// failure (`references 8, not bound in Main body`).
#[test]
fn nested_reassigning_if_dispatch_is_equivalent() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                use a = Qubit();
                X(a);
                mutable op = H;
                if MResetZ(a) == One {
                    if { X(a); MResetZ(a) == One } {
                        set op = X;
                    } else {
                        set op = Y;
                    }
                }
                ApplyOp(op, q);
                return MResetZ(a);
            }
        }
    "#});
}

/// Regression: a statement-position side-effecting `if` guard nested
/// inside another branch, with no inner `else`, where defunctionalization
/// lifts the guard across the branch boundary to an outer dispatch site. The
/// guard accumulator must be declared in the root block so the reused read is
/// in scope at the outer dispatch; before the scope-aware lift this panicked
/// with a `LocalVarId` consistency failure (`references 8, not bound in Main
/// body`).
#[test]
fn cross_branch_boundary_guard_is_equivalent() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                use a = Qubit();
                X(a);
                mutable op = H;
                if MResetZ(a) == One {
                    if { X(a); MResetZ(a) == One } {
                        set op = X;
                    }
                }
                ApplyOp(op, q);
                return MResetZ(a);
            }
        }
    "#});
}

/// Cross-package: a library operation whose statement-position `if` guards a
/// side-effecting condition is condition-normalized in place, so the
/// measurement is hoisted into a single-evaluation `__cond` binding and
/// evaluates exactly once.
#[test]
fn cross_package_side_effecting_if_condition_hoisted_in_library_body() {
    let lib_source = indoc! {r#"
        namespace TestLib {
            operation CondFlip(q : Qubit) : Unit {
                if MResetZ(q) == One {
                    X(q);
                }
            }
            export CondFlip;
        }
    "#};
    let user_source = indoc! {r#"
        import TestLib.*;
        @EntryPoint()
        operation Main() : Result {
            use q = Qubit();
            X(q);
            CondFlip(q);
            return MResetZ(q);
        }
    "#};

    // Run up to (but not including) cond_normalize, then invoke it directly so
    // the library body can be inspected before defunctionalize consumes the
    // hoisted guard.
    let (mut store, pkg_id) = compile_and_run_pipeline_to_with_library(
        lib_source,
        user_source,
        PipelineStage::ReturnUnify,
    );
    let lib_pkg = find_library_callable(&store, pkg_id, "CondFlip").package;

    let mut assigners = PackageAssigners::new(&store, pkg_id);
    normalize_conditions(&mut store, pkg_id, &mut assigners);

    let rendered = crate::pretty::write_package_qsharp_parseable(&store, lib_pkg);

    // The measurement is hoisted into a single-evaluation `__cond` binding...
    assert!(
        rendered.contains("__cond"),
        "cross-package cond_normalize should hoist the library if-condition into a __cond binding:\n{rendered}"
    );
    // ...and the side-effecting condition therefore evaluates exactly once.
    assert_eq!(
        rendered.matches("MResetZ").count(),
        1,
        "the side-effecting library condition must evaluate exactly once:\n{rendered}"
    );

    // End-to-end behavior is unchanged through the full pipeline.
    check_semantic_equivalence_with_library(lib_source, user_source);
}
