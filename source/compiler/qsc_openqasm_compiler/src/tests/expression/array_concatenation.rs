// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::check_qasm_to_qsharp as check;
use expect_test::expect;

#[test]
fn array_concatenation_in_alias_fails() {
    let source = "
    array[int, 3] a;
    array[int, 4] b;
    let c = a ++ b;
    ";

    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.InvalidTypeInAlias

              x invalid type in alias expression: array[int, 3]
               ,-[Test.qasm:4:13]
             3 |     array[int, 4] b;
             4 |     let c = a ++ b;
               :             ^
             5 |     
               `----
              help: aliases can only be applied to quantum bits and registers

            Qasm.Lowerer.InvalidTypeInAlias

              x invalid type in alias expression: array[int, 4]
               ,-[Test.qasm:4:18]
             3 |     array[int, 4] b;
             4 |     let c = a ++ b;
               :                  ^
             5 |     
               `----
              help: aliases can only be applied to quantum bits and registers
        "#]],
    );
}

#[test]
fn array_concatenation_has_the_right_type() {
    let source = "
    array[int, 3] a;
    array[int, 4] b;
    array[int, 7] c = a ++ b;
    ";

    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = [0, 0, 0];
            mutable b = [0, 0, 0, 0];
            mutable c = a + b;
        "#]],
    );
}

#[test]
fn array_can_be_concatenated_with_itself() {
    let source = "
    array[int[8], 3] a;
    array[int[8], 6] c = a ++ a;
    ";

    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = [0, 0, 0];
            mutable c = a + a;
        "#]],
    );
}

#[test]
fn array_concatenation_with_different_widths_errors() {
    let source = "
    array[int[8], 3] a;
    array[int[16], 4] b;
    array[int[8], 7] c = a ++ b;
    ";

    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.InconsistentTypesInArrayConcatenation

              x inconsistent types in array concatenation expression: array[int[8], 3],
              | array[int[16], 4]
               ,-[Test.qasm:4:26]
             3 |     array[int[16], 4] b;
             4 |     array[int[8], 7] c = a ++ b;
               :                          ^^^^^^
             5 |     
               `----
        "#]],
    );
}

#[test]
fn array_concatenation_with_different_types_errors() {
    let source = "
    array[int[8], 3] a;
    array[uint[8], 4] b;
    array[int[8], 7] c = a ++ b;
    ";

    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.InconsistentTypesInArrayConcatenation

              x inconsistent types in array concatenation expression: array[int[8], 3],
              | array[uint[8], 4]
               ,-[Test.qasm:4:26]
             3 |     array[uint[8], 4] b;
             4 |     array[int[8], 7] c = a ++ b;
               :                          ^^^^^^
             5 |     
               `----
        "#]],
    );
}

#[test]
fn multidimensional_array_concatenation_has_the_right_type() {
    let source = "
    array[int, 4, 2] a;
    array[int, 5, 2] b;
    array[int, 9, 2] c = a ++ b;
    ";

    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = [[0, 0], [0, 0], [0, 0], [0, 0]];
            mutable b = [[0, 0], [0, 0], [0, 0], [0, 0], [0, 0]];
            mutable c = a + b;
        "#]],
    );
}

#[test]
fn multidimensional_array_can_be_concatenated_with_itself() {
    let source = "
    array[int[8], 4, 2] a;
    array[int[8], 8, 2] c = a ++ a;
    ";

    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = [[0, 0], [0, 0], [0, 0], [0, 0]];
            mutable c = a + a;
        "#]],
    );
}

#[test]
fn multidimensional_array_concatenation_with_different_widths_errors() {
    let source = "
    array[int[8], 4, 2] a;
    array[int[16], 5, 2] b;
    array[int[8], 9, 2] c = a ++ b;
    ";

    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.InconsistentTypesInArrayConcatenation

              x inconsistent types in array concatenation expression: array[int[8], 4, 2],
              | array[int[16], 5, 2]
               ,-[Test.qasm:4:29]
             3 |     array[int[16], 5, 2] b;
             4 |     array[int[8], 9, 2] c = a ++ b;
               :                             ^^^^^^
             5 |     
               `----
        "#]],
    );
}

#[test]
fn multidimensional_array_concatenation_with_different_types_errors() {
    let source = "
    array[int[8], 4, 2] a;
    array[uint[8], 5, 2] b;
    array[int[8], 9, 2] c = a ++ b;
    ";

    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.InconsistentTypesInArrayConcatenation

              x inconsistent types in array concatenation expression: array[int[8], 4, 2],
              | array[uint[8], 5, 2]
               ,-[Test.qasm:4:29]
             3 |     array[uint[8], 5, 2] b;
             4 |     array[int[8], 9, 2] c = a ++ b;
               :                             ^^^^^^
             5 |     
               `----
        "#]],
    );
}
