// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Behavioral, lane-coverage, and structural tests for `OpenQASM`
//! `break`/`continue`.
//!
//! The loops in the behavioral tests use classical (compile-time-known) bounds
//! and conditions, so the QIR partial evaluator statically unrolls them.
//! Counting the `__quantum__qis__x__body` *call* sites in the generated QIR
//! therefore reports exactly how many loop iterations executed the `x q;` body,
//! which is what proves `break`/`continue` change runtime control flow as
//! expected.
//!
//! The `cfu_*_post_pass_hir` tests snapshot the HIR after `run_default_passes`
//! (loop unification followed by `control_flow_unification`) to lock in the
//! desugar structure: the `@broke`/`@continued` flags, the
//! `while not @broke and cond` rewrite, the per-iteration `@continued` reset,
//! the `if not flag { suffix }` guard, and the unguarded `for` update statement.

use crate::tests::{compile_qasm_to_post_pass_hir, compile_qasm_to_qir, compile_qasm_to_qsharp};
use expect_test::expect;
use miette::Report;

/// Counts X-gate call sites in the QIR, ignoring the `declare` line.
fn count_x_calls(qir: &str) -> usize {
    qir.lines()
        .filter(|l| l.contains("call") && l.contains("__quantum__qis__x__body"))
        .count()
}

#[test]
fn while_break_exits_loop_early() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        bit r;
        int i = 0;
        while (i < 100) {
            if (i >= 5) {
                break;
            }
            x q;
            i += 1;
        }
        r = measure q;
    "#;

    let qir = compile_qasm_to_qir(source)?;
    assert_eq!(
        count_x_calls(&qir),
        5,
        "while+break should execute the body 5 times, not 100"
    );
    Ok(())
}

#[test]
fn for_continue_skips_but_advances() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        bit r;
        for int i in [0:9] {
            if (i % 2 == 0) {
                continue;
            }
            x q;
        }
        r = measure q;
    "#;

    let qir = compile_qasm_to_qir(source)?;
    assert_eq!(
        count_x_calls(&qir),
        5,
        "for+continue should execute the body on the 5 odd iterations and still terminate"
    );
    Ok(())
}

#[test]
fn nested_inner_break_binds_to_inner_loop() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        bit r;
        for int i in [0:2] {
            for int j in [0:9] {
                if (j >= 2) {
                    break;
                }
                x q;
            }
        }
        r = measure q;
    "#;

    let qir = compile_qasm_to_qir(source)?;
    assert_eq!(
        count_x_calls(&qir),
        6,
        "inner break should bind to the inner loop only: 2 body runs x 3 outer iterations"
    );
    Ok(())
}

#[test]
fn combined_break_and_continue() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        bit r;
        for int i in [0:9] {
            if (i % 2 == 0) {
                continue;
            }
            if (i >= 6) {
                break;
            }
            x q;
        }
        r = measure q;
    "#;

    let qir = compile_qasm_to_qir(source)?;
    assert_eq!(
        count_x_calls(&qir),
        3,
        "continue skips even i; break stops at i=7; body runs on i=1,3,5"
    );
    Ok(())
}

#[test]
fn quantum_op_in_continue_guarded_region_lowers_to_qir() {
    // After CFU the trailing `x q;` is wrapped in `if not @continued { ... }`.
    // ReplaceQubitAllocation runs after CFU and must still lower the allocation
    // of `q` correctly even though the quantum op now sits under that guard.
    let source = r#"
        include "stdgates.inc";
        qubit q;
        bit r;
        for int i in [0:3] {
            if (i == 1) {
                continue;
            }
            x q;
        }
        r = measure q;
    "#;

    assert!(
        compile_qasm_to_qir(source).is_ok(),
        "a quantum op in a continue-guarded region should still lower to valid QIR"
    );
}

#[test]
fn adjoint_of_loop_with_break_is_forbidden() {
    // A gate used with `inv @` is given the `Adj` functor, so spec-gen attempts
    // to generate its adjoint. logic_sep runs before loop-unification (the body
    // is still a `for`), recurses into the loop, and rejects the `break` with the
    // dedicated `LoopControlForbidden` diagnostic.
    let source = r#"
        include "stdgates.inc";
        gate g q {
            for int i in [0:2] {
                break;
            }
            x q;
        }
        qubit q;
        inv @ g q;
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("expected adjoint generation to fail for a loop containing break");
    };
    let combined = errors
        .iter()
        .map(|e| format!("{e:?}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        combined.contains("LoopControlForbidden")
            || combined.contains("cannot take the adjoint of a loop containing break/continue"),
        "expected the dedicated LoopControlForbidden diagnostic, got:\n{combined}"
    );
}

#[test]
fn cfu_while_break_post_pass_hir() -> miette::Result<(), Vec<Report>> {
    // Classical loop body keeps the snapshot focused on the desugar structure
    // (no library-gate item-id references). The `@broke` flag, the
    // `while not @broke and cond` rewrite, and the `if not @broke { suffix }`
    // guard are the regression target.
    let source = r#"
        int i = 0;
        int n = 0;
        while (i < 10) {
            if (i >= 5) {
                break;
            }
            n += 1;
            i += 1;
        }
    "#;

    let hir = compile_qasm_to_post_pass_hir(source)?;
    expect![[r#"
        Package:
            entry expression: Expr 47 [0-0] [Type (Int, Int)]: Call:
                Expr 46 [0-0] [Type (Int, Int)]: Var: Item 1 (Package 2)
                Expr 45 [0-0] [Type Unit]: Unit
            Item 0 [0-0] (Public):
                Namespace (Ident 44 [0-0] "qasm_import"): Item 1
            Item 1 [0-0] (Internal):
                Parent: 0
                EntryPoint
                Callable 0 [0-0] (operation):
                    name: Ident 1 [0-0] "Test"
                    input: Pat 2 [0-0] [Type Unit]: Unit
                    output: (Int, Int)
                    functors: empty set
                    body: SpecDecl 3 [0-0]: Impl:
                        Block 4 [0-0] [Type (Int, Int)]:
                            Stmt 5 [0-0]: Local (Mutable):
                                Pat 6 [0-0] [Type Int]: Bind: Ident 7 [0-0] "i"
                                Expr 8 [0-0] [Type Int]: Lit: Int(0)
                            Stmt 9 [0-0]: Local (Mutable):
                                Pat 10 [0-0] [Type Int]: Bind: Ident 11 [0-0] "n"
                                Expr 12 [0-0] [Type Int]: Lit: Int(0)
                            Stmt 13 [0-0]: Expr: Expr 14 [0-0] [Type Unit]: Expr Block: Block 65 [0-0] [Type Unit]:
                                Stmt 63 [0-0]: Local (Mutable):
                                    Pat 64 [0-0] [Type Bool]: Bind: Ident 48 [0-0] "@broke_48"
                                    Expr 62 [0-0] [Type Bool]: Lit: Bool(false)
                                Stmt 60 [0-0]: Expr: Expr 61 [0-0] [Type Unit]: While:
                                    Expr 57 [0-0] [Type Bool]: BinOp (AndL):
                                        Expr 58 [0-0] [Type Bool]: UnOp (NotL):
                                            Expr 59 [0-0] [Type Bool]: Var: Local 48
                                        Expr 15 [0-0] [Type Bool]: BinOp (Lt):
                                            Expr 16 [0-0] [Type Int]: Var: Local 7
                                            Expr 17 [0-0] [Type Int]: Lit: Int(10)
                                    Block 18 [0-0] [Type Unit]:
                                        Stmt 19 [0-0]: Semi: Expr 20 [0-0] [Type Unit]: If:
                                            Expr 21 [0-0] [Type Bool]: BinOp (Gte):
                                                Expr 22 [0-0] [Type Int]: Var: Local 7
                                                Expr 23 [0-0] [Type Int]: Lit: Int(5)
                                            Expr 24 [0-0] [Type Unit]: Expr Block: Block 25 [0-0] [Type Unit]:
                                                Stmt 26 [0-0]: Semi: Expr 27 [0-0] [Type Unit]: Assign:
                                                    Expr 49 [0-0] [Type Bool]: Var: Local 48
                                                    Expr 50 [0-0] [Type Bool]: Lit: Bool(true)
                                        Stmt 56 [0-0]: Expr: Expr 55 [0-0] [Type Unit]: If:
                                            Expr 51 [0-0] [Type Bool]: UnOp (NotL):
                                                Expr 52 [0-0] [Type Bool]: Var: Local 48
                                            Expr 54 [0-0] [Type Unit]: Expr Block: Block 53 [0-0] [Type Unit]:
                                                Stmt 28 [0-0]: Semi: Expr 29 [0-0] [Type Unit]: Assign:
                                                    Expr 30 [0-0] [Type Int]: Var: Local 11
                                                    Expr 31 [0-0] [Type Int]: BinOp (Add):
                                                        Expr 32 [0-0] [Type Int]: Var: Local 11
                                                        Expr 33 [0-0] [Type Int]: Lit: Int(1)
                                                Stmt 34 [0-0]: Semi: Expr 35 [0-0] [Type Unit]: Assign:
                                                    Expr 36 [0-0] [Type Int]: Var: Local 7
                                                    Expr 37 [0-0] [Type Int]: BinOp (Add):
                                                        Expr 38 [0-0] [Type Int]: Var: Local 7
                                                        Expr 39 [0-0] [Type Int]: Lit: Int(1)
                            Stmt 40 [0-0]: Expr: Expr 41 [0-0] [Type (Int, Int)]: Tuple:
                                Expr 42 [0-0] [Type Int]: Var: Local 7
                                Expr 43 [0-0] [Type Int]: Var: Local 11
                    adj: <none>
                    ctl: <none>
                    ctl-adj: <none>"#]].assert_eq(&hir);
    Ok(())
}

#[allow(clippy::too_many_lines)]
#[test]
fn cfu_for_continue_post_pass_hir() -> miette::Result<(), Vec<Report>> {
    // The `@continued` flag is reset at the top of each iteration, the body
    // suffix is guarded by `if not @continued`, and the `for` update statement
    // (the loop-variable advance from loop unification) stays OUTSIDE the guard
    // so `continue` still advances the loop.
    let source = r#"
        int n = 0;
        for int i in [0:9] {
            if (i % 2 == 0) {
                continue;
            }
            n += 1;
        }
    "#;

    let hir = compile_qasm_to_post_pass_hir(source)?;
    expect![[r#"
        Package:
            entry expression: Expr 39 [0-0] [Type Int]: Call:
                Expr 38 [0-0] [Type Int]: Var: Item 1 (Package 2)
                Expr 37 [0-0] [Type Unit]: Unit
            Item 0 [0-0] (Public):
                Namespace (Ident 36 [0-0] "qasm_import"): Item 1
            Item 1 [0-0] (Internal):
                Parent: 0
                EntryPoint
                Callable 0 [0-0] (operation):
                    name: Ident 1 [0-0] "Test"
                    input: Pat 2 [0-0] [Type Unit]: Unit
                    output: Int
                    functors: empty set
                    body: SpecDecl 3 [0-0]: Impl:
                        Block 4 [0-0] [Type Int]:
                            Stmt 5 [0-0]: Local (Mutable):
                                Pat 6 [0-0] [Type Int]: Bind: Ident 7 [0-0] "n"
                                Expr 8 [0-0] [Type Int]: Lit: Int(0)
                            Stmt 9 [0-0]: Expr: Expr 81 [0-0] [Type Unit]: Expr Block: Block 82 [0-0] [Type Unit]:
                                Stmt 41 [0-0]: Local (Immutable):
                                    Pat 42 [0-0] [Type Range]: Bind: Ident 40 [0-0] "@range_id_40"
                                    Expr 13 [0-0] [Type Range]: Range:
                                        Expr 14 [0-0] [Type Int]: Lit: Int(0)
                                        <no step>
                                        Expr 15 [0-0] [Type Int]: Lit: Int(9)
                                Stmt 46 [0-0]: Local (Mutable):
                                    Pat 47 [0-0] [Type Int]: Bind: Ident 43 [0-0] "@index_id_43"
                                    Expr 44 [0-0] [Type Int]: Field:
                                        Expr 45 [0-0] [Type Range]: Var: Local 40
                                        Prim(Start)
                                Stmt 51 [0-0]: Local (Immutable):
                                    Pat 52 [0-0] [Type Int]: Bind: Ident 48 [0-0] "@step_id_48"
                                    Expr 49 [0-0] [Type Int]: Field:
                                        Expr 50 [0-0] [Type Range]: Var: Local 40
                                        Prim(Step)
                                Stmt 56 [0-0]: Local (Immutable):
                                    Pat 57 [0-0] [Type Int]: Bind: Ident 53 [0-0] "@end_id_53"
                                    Expr 54 [0-0] [Type Int]: Field:
                                        Expr 55 [0-0] [Type Range]: Var: Local 40
                                        Prim(End)
                                Stmt 79 [0-0]: Expr: Expr 80 [0-0] [Type Unit]: While:
                                    Expr 64 [0-0] [Type Bool]: BinOp (OrL):
                                        Expr 65 [0-0] [Type Bool]: BinOp (AndL):
                                            Expr 66 [0-0] [Type Bool]: BinOp (Gt):
                                                Expr 67 [0-0] [Type Int]: Var: Local 48
                                                Expr 68 [0-0] [Type Int]: Lit: Int(0)
                                            Expr 69 [0-0] [Type Bool]: BinOp (Lte):
                                                Expr 70 [0-0] [Type Int]: Var: Local 43
                                                Expr 71 [0-0] [Type Int]: Var: Local 53
                                        Expr 72 [0-0] [Type Bool]: BinOp (AndL):
                                            Expr 73 [0-0] [Type Bool]: BinOp (Lt):
                                                Expr 74 [0-0] [Type Int]: Var: Local 48
                                                Expr 75 [0-0] [Type Int]: Lit: Int(0)
                                            Expr 76 [0-0] [Type Bool]: BinOp (Gte):
                                                Expr 77 [0-0] [Type Int]: Var: Local 43
                                                Expr 78 [0-0] [Type Int]: Var: Local 53
                                    Block 16 [0-0] [Type Unit]:
                                        Stmt 93 [0-0]: Local (Mutable):
                                            Pat 94 [0-0] [Type Bool]: Bind: Ident 83 [0-0] "@continued_83"
                                            Expr 92 [0-0] [Type Bool]: Lit: Bool(false)
                                        Stmt 58 [0-0]: Local (Immutable):
                                            Pat 11 [0-0] [Type Int]: Bind: Ident 12 [0-0] "i"
                                            Expr 59 [0-0] [Type Int]: Var: Local 43
                                        Stmt 17 [0-0]: Semi: Expr 18 [0-0] [Type Unit]: If:
                                            Expr 19 [0-0] [Type Bool]: BinOp (Eq):
                                                Expr 20 [0-0] [Type Int]: BinOp (Mod):
                                                    Expr 21 [0-0] [Type Int]: Var: Local 12
                                                    Expr 22 [0-0] [Type Int]: Lit: Int(2)
                                                Expr 23 [0-0] [Type Int]: Lit: Int(0)
                                            Expr 24 [0-0] [Type Unit]: Expr Block: Block 25 [0-0] [Type Unit]:
                                                Stmt 26 [0-0]: Semi: Expr 27 [0-0] [Type Unit]: Assign:
                                                    Expr 84 [0-0] [Type Bool]: Var: Local 83
                                                    Expr 85 [0-0] [Type Bool]: Lit: Bool(true)
                                        Stmt 91 [0-0]: Expr: Expr 90 [0-0] [Type Unit]: If:
                                            Expr 86 [0-0] [Type Bool]: UnOp (NotL):
                                                Expr 87 [0-0] [Type Bool]: Var: Local 83
                                            Expr 89 [0-0] [Type Unit]: Expr Block: Block 88 [0-0] [Type Unit]:
                                                Stmt 28 [0-0]: Semi: Expr 29 [0-0] [Type Unit]: Assign:
                                                    Expr 30 [0-0] [Type Int]: Var: Local 7
                                                    Expr 31 [0-0] [Type Int]: BinOp (Add):
                                                        Expr 32 [0-0] [Type Int]: Var: Local 7
                                                        Expr 33 [0-0] [Type Int]: Lit: Int(1)
                                        Stmt 61 [0-0]: Semi: Expr 62 [0-0] [Type Unit]: AssignOp (Add):
                                            Expr 63 [0-0] [Type Int]: Var: Local 43
                                            Expr 60 [0-0] [Type Int]: Var: Local 48
                            Stmt 34 [0-0]: Expr: Expr 35 [0-0] [Type Int]: Var: Local 7
                    adj: <none>
                    ctl: <none>
                    ctl-adj: <none>"#]].assert_eq(&hir);
    Ok(())
}
