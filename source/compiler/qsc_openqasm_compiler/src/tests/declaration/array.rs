// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod bit;
mod qubit;

use crate::tests::{compile_fragments, compile_qasm_to_qsharp, fail_on_compilation_errors};
use expect_test::expect;
use miette::Report;

#[test]
fn arrays() -> miette::Result<(), Vec<Report>> {
    let source = "
        int y = 6;
        int z = 7;
        array[uint[16], 1] a;
        array[int[8], 4] b;
        array[float[64], 4, 2] c;
        array[angle[32], 4, 3, 2] d;
        array[complex[float[32]], 4] e;
        array[bool, 3] f;
        array[int[8], 4] g = {1, 2, 3, 4};
        array[int[8], 2] h = {y, y+y};
        array[uint[32], 2, 2] i = {{3, 4}, {2-3, y*5}};
        array[uint[32], 2, 2] j = {z, {2-3, y*5}};
        array[uint[32], 2, 2] k = {z*2, {1, 2}};
    ";

    let unit = compile_fragments(source)?;
    fail_on_compilation_errors(&unit);
    Ok(())
}

#[test]
fn default_simple_arrays() -> miette::Result<(), Vec<Report>> {
    let source = "
        array[bool, 3] a;
        array[int, 3] b;
        array[uint, 3] c;
        array[angle, 3] d;
        array[float, 3] e;
        array[complex, 3] f;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable a = [false, false, false];
        mutable b = [0, 0, 0];
        mutable c = [0, 0, 0];
        mutable d = [new Std.OpenQASM.Angle.Angle {
            Value = 0,
            Size = 53
        }, new Std.OpenQASM.Angle.Angle {
            Value = 0,
            Size = 53
        }, new Std.OpenQASM.Angle.Angle {
            Value = 0,
            Size = 53
        }];
        mutable e = [0., 0., 0.];
        mutable f = [Std.Math.Complex(0., 0.), Std.Math.Complex(0., 0.), Std.Math.Complex(0., 0.)];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn default_multidimensional_arrays() -> miette::Result<(), Vec<Report>> {
    let source = "
        array[bool, 2, 3] a;
        array[int, 2, 3] b;
        array[uint, 2, 3] c;
        array[angle, 2, 3] d;
        array[float, 2, 3] e;
        array[complex, 2, 3] f;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable a = [[false, false, false], [false, false, false]];
        mutable b = [[0, 0, 0], [0, 0, 0]];
        mutable c = [[0, 0, 0], [0, 0, 0]];
        mutable d = [[new Std.OpenQASM.Angle.Angle {
            Value = 0,
            Size = 53
        }, new Std.OpenQASM.Angle.Angle {
            Value = 0,
            Size = 53
        }, new Std.OpenQASM.Angle.Angle {
            Value = 0,
            Size = 53
        }], [new Std.OpenQASM.Angle.Angle {
            Value = 0,
            Size = 53
        }, new Std.OpenQASM.Angle.Angle {
            Value = 0,
            Size = 53
        }, new Std.OpenQASM.Angle.Angle {
            Value = 0,
            Size = 53
        }]];
        mutable e = [[0., 0., 0.], [0., 0., 0.]];
        mutable f = [[Std.Math.Complex(0., 0.), Std.Math.Complex(0., 0.), Std.Math.Complex(0., 0.)], [Std.Math.Complex(0., 0.), Std.Math.Complex(0., 0.), Std.Math.Complex(0., 0.)]];
    "#]].assert_eq(&qsharp);
    Ok(())
}

#[test]
fn initialized_simple_arrays() -> miette::Result<(), Vec<Report>> {
    let source = "
        array[bool, 3] a = { false, true, true };
        array[int, 3] b = { -2, 0, 3 };
        array[uint, 3] c = { 1, 2, 3 };
        array[angle, 3] d = { -1.0, 2.0, 4.0 };
        array[float, 3] e = { -2, 0, 3.0 };
        array[complex, 3] f = { 2, 3.0, 5.0 im };
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable a = [false, true, true];
        mutable b = [-2, 0, 3];
        mutable c = [1, 2, 3];
        mutable d = [Std.OpenQASM.Angle.DoubleAsAngle(-1., 53), Std.OpenQASM.Angle.DoubleAsAngle(2., 53), Std.OpenQASM.Angle.DoubleAsAngle(4., 53)];
        mutable e = [Std.Convert.IntAsDouble(-2), Std.Convert.IntAsDouble(0), 3.];
        mutable f = [Std.Math.Complex(Std.Convert.IntAsDouble(2), 0.), Std.Math.Complex(3., 0.), Std.Math.Complex(0., 5.)];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn initialized_simple_array_with_wrong_size_fails() {
    let source = "
        array[int, 3] b = { -2, 0 };
    ";

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.ArrayDeclarationTypeError

          x expected an array of size 3 but found one of size 2
           ,-[Test.qasm:2:27]
         1 | 
         2 |         array[int, 3] b = { -2, 0 };
           :                           ^^^^^^^^^
         3 |     
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn initialized_multidimensional_arrays() -> miette::Result<(), Vec<Report>> {
    let source = "
        array[bool, 2, 3] a = {{ 0, 1, 1 }, { false, true, true }};
        array[int, 2, 3] b = {{ -2, 0, 0 }, { 1, 2, 2 }};
        array[uint, 2, 3] c = {{ 0, 1, 1 }, { false, true, true }};
        array[angle, 2, 3] d = {{ -1.0, 0.0, 0.0 }, { 1.0, 5.0, 5.0 }};
        array[float, 2, 3] e = {{ -1, 0.0, 0.0 }, { 1.0, 5.0, 5.0 }};
        array[complex, 2, 3] f = {{ -2, 0, 0 }, { 3 im, 1 - 2 im, 0.0 }};
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable a = [[if 0 == 0 {
            false
        } else {
            true
        }, if 1 == 0 {
            false
        } else {
            true
        }, if 1 == 0 {
            false
        } else {
            true
        }], [false, true, true]];
        mutable b = [[-2, 0, 0], [1, 2, 2]];
        mutable c = [[0, 1, 1], [Std.OpenQASM.Convert.BoolAsInt(false), Std.OpenQASM.Convert.BoolAsInt(true), Std.OpenQASM.Convert.BoolAsInt(true)]];
        mutable d = [[Std.OpenQASM.Angle.DoubleAsAngle(-1., 53), Std.OpenQASM.Angle.DoubleAsAngle(0., 53), Std.OpenQASM.Angle.DoubleAsAngle(0., 53)], [Std.OpenQASM.Angle.DoubleAsAngle(1., 53), Std.OpenQASM.Angle.DoubleAsAngle(5., 53), Std.OpenQASM.Angle.DoubleAsAngle(5., 53)]];
        mutable e = [[Std.Convert.IntAsDouble(-1), 0., 0.], [1., 5., 5.]];
        mutable f = [[Std.Math.Complex(Std.Convert.IntAsDouble(-2), 0.), Std.Math.Complex(Std.Convert.IntAsDouble(0), 0.), Std.Math.Complex(Std.Convert.IntAsDouble(0), 0.)], [Std.Math.Complex(0., 3.), Std.Math.MinusC(Std.Math.Complex(1., 0.), Std.Math.Complex(0., 2.)), Std.Math.Complex(0., 0.)]];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn assign_to_simple_arrays() -> miette::Result<(), Vec<Report>> {
    let source = "
        array[bool, 3] a;
        array[int, 3] b;
        array[uint, 3] c;
        array[angle, 3] d;
        array[float, 3] e;
        array[complex, 3] f;

        a[1] = true;
        b[1] = 4;
        c[1] = 4;
        d[1] = 4.0;
        e[1] = 4;
        f[1] = 4;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable a = [false, false, false];
        mutable b = [0, 0, 0];
        mutable c = [0, 0, 0];
        mutable d = [new Std.OpenQASM.Angle.Angle {
            Value = 0,
            Size = 53
        }, new Std.OpenQASM.Angle.Angle {
            Value = 0,
            Size = 53
        }, new Std.OpenQASM.Angle.Angle {
            Value = 0,
            Size = 53
        }];
        mutable e = [0., 0., 0.];
        mutable f = [Std.Math.Complex(0., 0.), Std.Math.Complex(0., 0.), Std.Math.Complex(0., 0.)];
        set a[1] = true;
        set b[1] = 4;
        set c[1] = 4;
        set d[1] = new Std.OpenQASM.Angle.Angle {
            Value = 5734161139222659,
            Size = 53
        };
        set e[1] = 4.;
        set f[1] = Std.Math.Complex(4., 0.);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn assign_to_multidimensional_arrays() -> miette::Result<(), Vec<Report>> {
    let source = "
        array[bool, 3, 2] a;
        array[int, 3, 2] b;
        array[uint, 3, 2] c;
        array[angle, 3, 2] d;
        array[float, 3, 2] e;
        array[complex, 3, 2] f;

        a[2, 1] = true;
        b[2, 1] = 4;
        c[2, 1] = 4;
        d[2, 1] = 4.0;
        e[2, 1] = 4;
        f[2, 1] = 4;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable a = [[false, false], [false, false], [false, false]];
        mutable b = [[0, 0], [0, 0], [0, 0]];
        mutable c = [[0, 0], [0, 0], [0, 0]];
        mutable d = [[new Std.OpenQASM.Angle.Angle {
            Value = 0,
            Size = 53
        }, new Std.OpenQASM.Angle.Angle {
            Value = 0,
            Size = 53
        }], [new Std.OpenQASM.Angle.Angle {
            Value = 0,
            Size = 53
        }, new Std.OpenQASM.Angle.Angle {
            Value = 0,
            Size = 53
        }], [new Std.OpenQASM.Angle.Angle {
            Value = 0,
            Size = 53
        }, new Std.OpenQASM.Angle.Angle {
            Value = 0,
            Size = 53
        }]];
        mutable e = [[0., 0.], [0., 0.], [0., 0.]];
        mutable f = [[Std.Math.Complex(0., 0.), Std.Math.Complex(0., 0.)], [Std.Math.Complex(0., 0.), Std.Math.Complex(0., 0.)], [Std.Math.Complex(0., 0.), Std.Math.Complex(0., 0.)]];
        set a[2][1] = true;
        set b[2][1] = 4;
        set c[2][1] = 4;
        set d[2][1] = new Std.OpenQASM.Angle.Angle {
            Value = 5734161139222659,
            Size = 53
        };
        set e[2][1] = 4.;
        set f[2][1] = Std.Math.Complex(4., 0.);
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn assign_slice() -> miette::Result<(), Vec<Report>> {
    let source = "
        array[int, 3] a;
        array[int, 2] b = {5, 6};
        a[1:2] = b;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable a = [0, 0, 0];
        mutable b = [5, 6];
        set a[1..2] = b;
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn default_simple_array_with_size_zero() -> miette::Result<(), Vec<Report>> {
    let source = "
        array[int, 0] a;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable a = [];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn init_simple_array_with_size_zero() -> miette::Result<(), Vec<Report>> {
    let source = "
        array[int, 0] a = {};
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable a = [];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn default_multidimensional_array_with_size_zero() -> miette::Result<(), Vec<Report>> {
    let source = "
        array[int, 2, 0, 3] a;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable a = [[], []];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn init_multidimensional_array_with_size_zero() -> miette::Result<(), Vec<Report>> {
    let source = "
        array[int, 2, 0] a = {{}, {}};
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable a = [[], []];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn array_with_size_one() -> miette::Result<(), Vec<Report>> {
    let source = "
        array[int, 1] a;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable a = [0];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn indexing_a_simple_array_of_zero_size_fails() {
    let source = "
        array[int, 0] a;
        a[0];
    ";

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.ZeroSizeArrayAccess

          x zero size array access is not allowed
           ,-[Test.qasm:3:9]
         2 |         array[int, 0] a;
         3 |         a[0];
           :         ^^^^
         4 |     
           `----
          help: array size must be a positive integer const expression
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn indexing_a_multidimensional_array_of_size_zero_fails() {
    let source = "
        array[int, 3, 0, 2] a;
        a[1:2];
    ";

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.ZeroSizeArrayAccess

          x zero size array access is not allowed
           ,-[Test.qasm:3:9]
         2 |         array[int, 3, 0, 2] a;
         3 |         a[1:2];
           :         ^^^^^^
         4 |     
           `----
          help: array size must be a positive integer const expression
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn array_declaration_in_non_global_scope_fails() {
    let source = "
        def f() {
            array[int, 2] a;
        }
    ";

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [  x array declarations are only allowed in global scope
           ,-[Test.qasm:3:13]
         2 |         def f() {
         3 |             array[int, 2] a;
           :             ^^^^^^^^^^^^^^^^
         4 |         }
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn arrays_with_7_dimensions_are_supported() -> miette::Result<(), Vec<Report>> {
    let source = "
        array[int, 1, 2, 3, 3, 2, 1, 2] a;
    ";

    let qsharp = compile_qasm_to_qsharp(source)?;
    expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable a = [[[[[[[0, 0]], [[0, 0]]], [[[0, 0]], [[0, 0]]], [[[0, 0]], [[0, 0]]]], [[[[0, 0]], [[0, 0]]], [[[0, 0]], [[0, 0]]], [[[0, 0]], [[0, 0]]]], [[[[0, 0]], [[0, 0]]], [[[0, 0]], [[0, 0]]], [[[0, 0]], [[0, 0]]]]], [[[[[0, 0]], [[0, 0]]], [[[0, 0]], [[0, 0]]], [[[0, 0]], [[0, 0]]]], [[[[0, 0]], [[0, 0]]], [[[0, 0]], [[0, 0]]], [[[0, 0]], [[0, 0]]]], [[[[0, 0]], [[0, 0]]], [[[0, 0]], [[0, 0]]], [[[0, 0]], [[0, 0]]]]]]];
    "#]]
    .assert_eq(&qsharp);
    Ok(())
}

#[test]
fn arrays_with_8_dimensions_are_unsupported() {
    let source = "
        array[int, 1, 2, 3, 4, 5, 6, 7, 8] a;
    ";

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.NotSupported

          x arrays with more than 7 dimensions are not supported
           ,-[Test.qasm:2:9]
         1 | 
         2 |         array[int, 1, 2, 3, 4, 5, 6, 7, 8] a;
           :         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
         3 |     
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}

#[test]
fn casting_arrays_unsupported() {
    let source = "
        array[complex, 1, 2, 3] a;
        array[angle, 1, 2, 3] b = a;
    ";

    let Err(errors) = compile_qasm_to_qsharp(source) else {
        panic!("Expected error");
    };

    expect![[r#"
        [Qasm.Lowerer.CannotCast

          x cannot cast expression of type array[complex[float], 1, 2, 3] to type
          | array[angle, 1, 2, 3]
           ,-[Test.qasm:3:35]
         2 |         array[complex, 1, 2, 3] a;
         3 |         array[angle, 1, 2, 3] b = a;
           :                                   ^
         4 |     
           `----
        ]"#]]
    .assert_eq(&format!("{errors:?}"));
}
