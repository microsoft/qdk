// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::check_qasm_to_qsharp;
use expect_test::expect;

#[test]
fn continue_in_for_loop_generates_qsharp() {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        for int i in [0:9] {
            if (i % 2 == 0) {
                continue;
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
                if i % 2 == 0 {
                    / * continue * /;
                };
                x(q);
            }
        "#]],
    );
}

#[test]
fn continue_in_while_loop_generates_qsharp() {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        int i = 0;
        while (i < 10) {
            i += 1;
            if (i % 2 == 0) {
                continue;
            }
            x q;
        }
    "#;

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            borrow q = Qubit();
            mutable i = 0;
            while i < 10 {
                set i = i + 1;
                if i % 2 == 0 {
                    / * continue * /;
                };
                x(q);
            }
        "#]],
    );
}

#[test]
fn continue_in_nested_loops_binds_to_inner_loop_generates_qsharp() {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        for int i in [0:2] {
            for int j in [0:3] {
                if (j == 1) {
                    continue;
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
                for j : Int in 0..3 {
                    if j == 1 {
                        / * continue * /;
                    };
                    x(q);
                }
            }
        "#]],
    );
}
