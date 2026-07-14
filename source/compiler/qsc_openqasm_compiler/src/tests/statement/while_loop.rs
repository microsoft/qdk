// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::compile_qasm_to_qsharp;
use expect_test::expect;
use miette::Report;

#[test]
fn can_iterate_over_mutable_var_cmp_expr() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        qubit q;
        bit result;

        int i = 0;
        while (i < 10) {
            h q;
            result = measure q;
            if (result) {
                i += 1;
            }
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        borrow q = Qubit();
        mutable result = Zero;
        mutable i = 0;
        while i < 10 {
            h(q);
            set result = Std.Intrinsic.M(q);
            if Std.OpenQASM.Convert.ResultAsBool(result) {
                set i = i + 1;
            };
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn unbraced_broadcast_gate_call_expands_in_while_body() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        include "stdgates.inc";
        bit keep_going = 1;
        qubit[2] q;
        while (keep_going) x q;
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable keep_going = One;
        borrow q = Qubit[2];
        while Std.OpenQASM.Convert.ResultAsBool(keep_going) {
            x(q[0]);
            x(q[1]);
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn using_cond_that_cannot_implicit_cast_to_bool_fail() {
    let source = r#"
        qubit q;
        while (q) {
            reset q;
        }
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect!["cannot cast expression of type qubit to type bool"].assert_eq(&errors[0].to_string());
}
