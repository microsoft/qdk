// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::check_qasm_to_qsharp;
use expect_test::expect;

#[test]
fn break_in_while_loop_generates_qsharp() {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        int i = 0;
        while (i < 10) {
            if (i >= 5) {
                break;
            }
            x q;
            i += 1;
        }
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            borrow q = Qubit();
            mutable i = 0;
            while i < 10 {
                if i >= 5 {
                    / * break * /;
                };
                x(q);
                set i = i + 1;
            }
        "#]],
    );
}

#[test]
fn break_in_for_loop_generates_qsharp() {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        for int i in [0:9] {
            if (i >= 5) {
                break;
            }
            x q;
        }
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            borrow q = Qubit();
            for i : Int in 0..9 {
                if i >= 5 {
                    / * break * /;
                };
                x(q);
            }
        "#]],
    );
}

#[test]
fn break_in_nested_loops_binds_to_inner_loop_generates_qsharp() {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        for int i in [0:2] {
            for int j in [0:9] {
                if (j >= 2) {
                    break;
                }
                x q;
            }
        }
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            borrow q = Qubit();
            for i : Int in 0..2 {
                for j : Int in 0..9 {
                    if j >= 2 {
                        / * break * /;
                    };
                    x(q);
                }
            }
        "#]],
    );
}
