// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::{compile_qasm_to_qsharp, compile_qasm_to_qsharp_file};
use expect_test::expect;
use miette::Report;

#[test]
fn default_is_optional() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 3.1;
        int i = 15;
        switch (i) {
            case 1 {
                i = 2;
            }
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable i = 15;
        if i == 1 {
            set i = 2;
        };
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn default_as_only_case_causes_parse_error() {
    let source = r#"
        OPENQASM 3.1;
        int i = 15;
        switch (i) {
            default {
                i = 2;
            }
        }
    "#;

    let res = compile_qasm_to_qsharp(source);
    let Err(errors) = res else {
        panic!("Expected an error, got {res:?}");
    };
    assert_eq!(errors.len(), 1);
    expect![["missing switch statement cases"]].assert_eq(&errors[0].to_string());
}

#[test]
fn no_cases_causes_parse_error() {
    let source = r#"
        OPENQASM 3.1;
        int i = 15;
        switch (i) {
        }
    "#;

    let res = compile_qasm_to_qsharp(source);
    let Err(errors) = res else {
        panic!("Expected an error, got {res:?}");
    };
    assert_eq!(errors.len(), 1);
    expect![["missing switch statement cases"]].assert_eq(&errors[0].to_string());
}

#[test]
fn spec_case_1() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 3.1;
        include "stdgates.inc";
        qubit q;

        int i = 15;

        switch (i) {
        case 1, 3, 5 {
            h q;
        }
        case 2, 4, 6 {
            x q;
        }
        case -1 {
            y q;
        }
        default {
            z q;
        }
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        borrow q = Qubit();
        mutable i = 15;
        if i == 1 or i == 3 or i == 5 {
            h(q);
        } elif i == 2 or i == 4 or i == 6 {
            x(q);
        } elif i == -1 {
            y(q);
        } else {
            z(q);
        };
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn spec_case_2() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 3.1;
        include "stdgates.inc";
        qubit q;

        const int A = 0;
        const int B = 1;
        int i = 15;

        switch (i) {
        case A {
            h q;
        }
        case B {
            x q;
        }
        case B+1 {
            y q;
        }
        default {
            z q;
        }
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        borrow q = Qubit();
        let A = 0;
        let B = 1;
        mutable i = 15;
        if i == A {
            h(q);
        } elif i == B {
            x(q);
        } elif i == B + 1 {
            y(q);
        } else {
            z(q);
        };
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn spec_case_3() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 3.1;
        include "stdgates.inc";
        qubit q;
        bit[2] b;
        // int(b) originally, but we don't support that yet
        switch (b) {
        case 0b00 {
            h q;
        }
        case 0b01 {
            x q;
        }
        case 0b10 {
            y q;
        }
        case 0b11 {
            z q;
        }
        }
    "#;

    let qsharp = compile_qasm_to_qsharp_file(source)?;
    expect![[r#"
        namespace qasm_import {
            import Std.OpenQASM.Intrinsic.*;
            @EntryPoint()
            operation Test() : Result[] {
                borrow q = Qubit();
                mutable b = [Zero, Zero];
                if Std.OpenQASM.Convert.ResultArrayAsIntBE(b) == 0 {
                    h(q);
                } elif Std.OpenQASM.Convert.ResultArrayAsIntBE(b) == 1 {
                    x(q);
                } elif Std.OpenQASM.Convert.ResultArrayAsIntBE(b) == 2 {
                    y(q);
                } elif Std.OpenQASM.Convert.ResultArrayAsIntBE(b) == 3 {
                    z(q);
                };
                b
            }
        }"#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn spec_case_4() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 3.1;
        include "stdgates.inc";
        qubit q;
        bit[2] b;
        def foo(int i, qubit[8] d) -> bit {
            return measure d[i];
        }

        int i = 15;

        int j = 1;
        int k = 2;

        bit c1;

        qubit[8] q0;

        switch (i) {
        case 1 {
            j = k + foo(k, q0);
        }
        case 2 {
            float[64] d = j / k;
        }
        case 3 {
        }
        default {
        }
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        borrow q = Qubit();
        mutable b = [Zero, Zero];
        operation foo(i : Int, d : Qubit[]) : Result {
            if Std.Core.Length(d) != 8 {
                fail "Argument `d` is not compatible with its OpenQASM type `qubit[8]`."
            };
            return Std.Intrinsic.M(d[i]);
        }
        mutable i = 15;
        mutable j = 1;
        mutable k = 2;
        mutable c1 = Zero;
        borrow q0 = Qubit[8];
        if i == 1 {
            set j = k + Std.OpenQASM.Convert.ResultAsInt(foo(k, q0));
        } elif i == 2 {
            mutable d = Std.Convert.IntAsDouble(j / k);
        } elif i == 3 {} else {};
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn spec_case_5() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 3.1;
        include "stdgates.inc";


        qubit[8] q;

        int j = 30;
        int i;

        switch (i) {
        case 1, 2, 5, 12 {
        }
        case 3 {
        switch (j) {
        case 10, 15, 20 {
            h q;
        }
        }
        }
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        borrow q = Qubit[8];
        mutable j = 30;
        mutable i = 0;
        if i == 1 or i == 2 or i == 5 or i == 12 {} elif i == 3 {
            if j == 10 or j == 15 or j == 20 {
                h(q[0]);
                h(q[1]);
                h(q[2]);
                h(q[3]);
                h(q[4]);
                h(q[5]);
                h(q[6]);
                h(q[7]);
            };
        };
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}
