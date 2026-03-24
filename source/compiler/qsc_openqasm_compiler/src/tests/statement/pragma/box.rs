// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::compile_qasm_to_qsharp;
use expect_test::expect;
use miette::Report;

#[test]
fn pragma_target_can_be_defined_before_pragma() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        def sample_box_target() {}
        pragma qdk.box.open sample_box_target
        box {}
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        function sample_box_target() : Unit {}
        {
            sample_box_target();
        };
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn pragma_target_can_be_defined_after_pragma() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        pragma qdk.box.open sample_box_target
        def sample_box_target() {}
        box {}
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        function sample_box_target() : Unit {}
        {
            sample_box_target();
        };
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn pragma_target_can_be_used_by_multilple_pragmas() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        pragma qdk.box.open sample_box_target
        pragma qdk.box.close sample_box_target
        def sample_box_target() {}
        box {}
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        function sample_box_target() : Unit {}
        {
            sample_box_target();
            sample_box_target();
        };
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn pragmas_can_have_separate_targets() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        pragma qdk.box.open box_open
        pragma qdk.box.close box_close
        def box_open() {}
        def box_close() {}
        box {}
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        function box_open() : Unit {}
        function box_close() : Unit {}
        {
            box_open();
            box_close();
        };
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn nested_boxes_call_separately() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        pragma qdk.box.open box_open
        pragma qdk.box.close box_close
        def box_open() {}
        def box_close() {}
        box {box {}}
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        function box_open() : Unit {}
        function box_close() : Unit {}
        {
            box_open();
            {
                box_open();
                box_close();
            };
            box_close();
        };
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn last_pragma_overwrites_previous() -> miette::Result<(), Vec<Report>> {
    let source = r#"
        pragma qdk.box.open first
        pragma qdk.box.open second
        pragma qdk.box.close third
        pragma qdk.box.close fourth
        def first() {}
        def second() {}
        def third() {}
        def fourth() {}
        box {box {}}
    "#;

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        function first() : Unit {}
        function second() : Unit {}
        function third() : Unit {}
        function fourth() : Unit {}
        {
            second();
            {
                second();
                fourth();
            };
            fourth();
        };
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn target_with_param_raises_error() {
    let source = r#"
        pragma qdk.box.open sample_box_target
        def sample_box_target(int i) {}
        box {}
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected an error");
    };
    expect![[r#"
        Qasm.Compiler.InvalidBoxPragmaTarget

          x sample_box_target is not defined or is not a valid target for box usage
           ,-[Test.qasm:2:29]
         1 | 
         2 |         pragma qdk.box.open sample_box_target
           :                             ^^^^^^^^^^^^^^^^^
         3 |         def sample_box_target(int i) {}
           `----
    "#]]
    .assert_eq(&format!("{:?}", &errors[0]));
}

#[test]
fn unknown_pragma_raises_error() {
    let source = r#"
        pragma qdk.box.unknown sample_box_target
        def sample_box_target(int i) {}
        box {}
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected an error");
    };
    expect![[r#"
        Qasm.Compiler.NotSupported

          x pragma statement: qdk.box.unknown are not supported
           ,-[Test.qasm:2:9]
         1 | 
         2 |         pragma qdk.box.unknown sample_box_target
           :         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
         3 |         def sample_box_target(int i) {}
           `----
    "#]]
    .assert_eq(&format!("{:?}", &errors[0]));
}

#[test]
fn target_with_return_value_raises_error() {
    let source = r#"
        pragma qdk.box.open sample_box_target
        def sample_box_target() -> int {
            return 42;
        }
        box {}
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected an error");
    };
    expect![[r#"
        Qasm.Compiler.InvalidBoxPragmaTarget

          x sample_box_target is not defined or is not a valid target for box usage
           ,-[Test.qasm:2:29]
         1 | 
         2 |         pragma qdk.box.open sample_box_target
           :                             ^^^^^^^^^^^^^^^^^
         3 |         def sample_box_target() -> int {
           `----
    "#]]
    .assert_eq(&format!("{:?}", &errors[0]));
}

#[test]
fn unknown_target_with_return_value_raises_error() {
    let source = r#"
        pragma qdk.box.open sample_box_target
        box {}
    "#;

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected an error");
    };
    expect![[r#"
        Qasm.Compiler.InvalidBoxPragmaTarget

          x sample_box_target is not defined or is not a valid target for box usage
           ,-[Test.qasm:2:29]
         1 | 
         2 |         pragma qdk.box.open sample_box_target
           :                             ^^^^^^^^^^^^^^^^^
         3 |         box {}
           `----
    "#]]
    .assert_eq(&format!("{:?}", &errors[0]));
}
