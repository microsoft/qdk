// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::{check_qasm_to_qsharp, compile_qasm_to_qsharp_operation};
use expect_test::expect;

#[test]
fn indexed_type_has_right_dimensions() {
    let source = "
        array[bool, 2, 3, 4] arr_1;
        array[bool, 3, 4] arr_2 = arr_1[0];
    ";

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable arr_1 = [[[false, false, false, false], [false, false, false, false], [false, false, false, false]], [[false, false, false, false], [false, false, false, false], [false, false, false, false]]];
        mutable arr_2 = arr_1[0];
    "#]],
    );
}

#[test]
fn sliced_type_has_right_dimensions() {
    let source = "
        array[bool, 5, 1, 2] arr_1;
        array[bool, 3, 1, 2] arr_2 = arr_1[1:3];
    ";

    check_qasm_to_qsharp(
        source,
        &expect![[r#"
        import Std.OpenQASM.Intrinsic.*;
        mutable arr_1 = [[[false, false]], [[false, false]], [[false, false]], [[false, false]], [[false, false]]];
        mutable arr_2 = arr_1[1..3];
    "#]],
    );
}

#[test]
fn bigint_output_is_initially_zero_l() {
    let source = "
        input uint[128] input_var;
        output uint[128] output_var;
        output_var = input_var;
    ";

    let expect = expect![[r#"
        operation Test(input_var : BigInt) : BigInt {
            import Std.OpenQASM.Intrinsic.*;
            mutable output_var = 0L;
            set output_var = input_var;
            output_var
        }
    "#]];
    match compile_qasm_to_qsharp_operation(source) {
        Ok(qsharp) => {
            expect.assert_eq(&qsharp);
        }
        Err(errors) => {
            let buffer = errors
                .iter()
                .map(|e| format!("{e:?}"))
                .collect::<Vec<_>>()
                .join("\n");
            expect.assert_eq(&buffer);
        }
    }
}

#[test]
fn complex_input_and_output_is_mapped_correctly() {
    let source = "
        input complex input_var;
        output complex output_var;
        output_var = input_var;
    ";

    let expect = expect![[r#"
        operation Test(input_var : Std.Math.Complex) : Std.Math.Complex {
            import Std.OpenQASM.Intrinsic.*;
            mutable output_var = Std.Math.Complex(0., 0.);
            set output_var = input_var;
            output_var
        }
    "#]];
    match compile_qasm_to_qsharp_operation(source) {
        Ok(qsharp) => {
            expect.assert_eq(&qsharp);
        }
        Err(errors) => {
            let buffer = errors
                .iter()
                .map(|e| format!("{e:?}"))
                .collect::<Vec<_>>()
                .join("\n");
            expect.assert_eq(&buffer);
        }
    }
}
