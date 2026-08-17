// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::compile_qasm_to_qsharp;
use expect_test::expect;
use miette::Report;

fn compile_error_buffer(source: &str) -> String {
    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };
    errors
        .iter()
        .map(|e| format!("{e:?}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn break_lowers_in_for_loop() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        for int i in [0:2] {
            break;
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        for i : Int in 0..2 {
            break;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn continue_lowers_in_for_loop() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        for int i in [0:2] {
            continue;
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        for i : Int in 0..2 {
            continue;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn break_lowers_in_while_loop() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        int i = 0;
        while (i < 5) {
            i += 1;
            break;
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable i = 0;
        while i < 5 {
            set i = i + 1;
            break;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn continue_lowers_in_while_loop() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        int i = 0;
        while (i < 5) {
            i += 1;
            continue;
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable i = 0;
        while i < 5 {
            set i = i + 1;
            continue;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn break_lowers_when_guarded_by_if_in_for_loop() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        int sum = 0;
        for int i in [0:10] {
            if (i == 5) {
                break;
            }
            sum += i;
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable sum = 0;
        for i : Int in 0..10 {
            if i == 5 {
                break;
            };
            set sum = sum + i;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn continue_lowers_when_guarded_by_if_in_for_loop() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        int sum = 0;
        for int i in [0:10] {
            if (i == 5) {
                continue;
            }
            sum += i;
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable sum = 0;
        for i : Int in 0..10 {
            if i == 5 {
                continue;
            };
            set sum = sum + i;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn nested_loop_control_targets_nearest_loop() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        for int i in [0:2] {
            while (true) {
                break;
            }
            continue;
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        for i : Int in 0..2 {
            while true {
                break;
            }
        continue;
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn break_in_switch_targets_enclosing_loop() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        OPENQASM 3.1;
        for int i in [0:2] {
            switch (i) {
                case 1 {
                    break;
                }
            }
        }
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        for i : Int in 0..2 {
            if i == 1 {
                break;
            };
        }
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn break_in_switch_without_enclosing_loop_is_rejected() {
    let source = r#"
        OPENQASM 3.1;
        int i = 1;
        switch (i) {
            case 1 {
                break;
            }
        }
    "#;

    let errors = compile_error_buffer(source);
    assert!(errors.contains("Qasm.Lowerer.InvalidScope"));
    assert!(errors.contains("break can only appear in loop scopes"));
}

#[test]
fn break_outside_loop_is_rejected() {
    let source = "break;";

    expect![[r#"
        Qdk.Qasm.Lowerer.InvalidScope

          x break can only appear in loop scopes
           ,-[Test.qasm:1:1]
         1 | break;
           : ^^^^^^
           `----
    "#]]
    .assert_eq(&compile_error_buffer(source));
}

#[test]
fn continue_outside_loop_is_rejected() {
    let source = "continue;";

    expect![[r#"
        Qdk.Qasm.Lowerer.InvalidScope

          x continue can only appear in loop scopes
           ,-[Test.qasm:1:1]
         1 | continue;
           : ^^^^^^^^^
           `----
    "#]]
    .assert_eq(&compile_error_buffer(source));
}

#[test]
fn break_in_def_without_enclosing_loop_is_rejected() {
    let source = r#"
        def f() {
            break;
        }
    "#;

    expect![[r#"
        Qdk.Qasm.Lowerer.InvalidScope

          x break can only appear in loop scopes
           ,-[Test.qasm:3:13]
         2 |         def f() {
         3 |             break;
           :             ^^^^^^
         4 |         }
           `----
    "#]]
    .assert_eq(&compile_error_buffer(source));
}

#[test]
fn continue_in_gate_without_enclosing_loop_is_rejected() {
    let source = r#"
        gate g q {
            continue;
        }
    "#;

    expect![[r#"
        Qdk.Qasm.Lowerer.InvalidScope

          x continue can only appear in loop scopes
           ,-[Test.qasm:3:13]
         2 |         gate g q {
         3 |             continue;
           :             ^^^^^^^^^
         4 |         }
           `----
    "#]]
    .assert_eq(&compile_error_buffer(source));
}
