// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn single_arg() {
    check(
        "DEPOLARIZE1(0.001) 0",
        &expect![[r#"
            Circuit [0-20]:
                items:
                    Instruction [0-20]:
                        name: DEPOLARIZE1
                        tag: <none>
                        args:
                            Arg [12-17]: 0.001
                        targets:
                            Target [19-20]:
                                kind: Qubit(0)"#]],
    );
}

#[test]
fn multiple_comma_separated_args() {
    check(
        "PAULI_CHANNEL_1(0.01, 0.02, 0.03) 0",
        &expect![[r#"
            Circuit [0-35]:
                items:
                    Instruction [0-35]:
                        name: PAULI_CHANNEL_1
                        tag: <none>
                        args:
                            Arg [16-20]: 0.01
                            Arg [22-26]: 0.02
                            Arg [28-32]: 0.03
                        targets:
                            Target [34-35]:
                                kind: Qubit(0)"#]],
    );
}

#[test]
fn scientific_notation_arg() {
    check(
        "X_ERROR(1e-3) 0",
        &expect![[r#"
            Circuit [0-15]:
                items:
                    Instruction [0-15]:
                        name: X_ERROR
                        tag: <none>
                        args:
                            Arg [8-12]: 0.001
                        targets:
                            Target [14-15]:
                                kind: Qubit(0)"#]],
    );
}

#[test]
fn radians_args() {
    check(
        "R_X(1rad) 0",
        &expect![[r#"
            Circuit [0-11]:
                items:
                    Instruction [0-11]:
                        name: R_X
                        tag: <none>
                        args:
                            Arg [4-8]: 1 rad
                        targets:
                            Target [10-11]:
                                kind: Qubit(0)"#]],
    );
    check(
        "R_Y(-0.5rad) 0",
        &expect![[r#"
            Circuit [0-14]:
                items:
                    Instruction [0-14]:
                        name: R_Y
                        tag: <none>
                        args:
                            Arg [4-11]: -0.5 rad
                        targets:
                            Target [13-14]:
                                kind: Qubit(0)"#]],
    );
    check(
        "R_Z(+2.5e-3rad) 0",
        &expect![[r#"
            Circuit [0-17]:
                items:
                    Instruction [0-17]:
                        name: R_Z
                        tag: <none>
                        args:
                            Arg [4-14]: 0.0025 rad
                        targets:
                            Target [16-17]:
                                kind: Qubit(0)"#]],
    );
}

#[test]
fn mixed_unit_args() {
    check(
        "U3(0.1, -0.2rad, 3e-1rad) 0",
        &expect![[r#"
            Circuit [0-27]:
                items:
                    Instruction [0-27]:
                        name: U3
                        tag: <none>
                        args:
                            Arg [3-6]: 0.1
                            Arg [8-15]: -0.2 rad
                            Arg [17-24]: 0.3 rad
                        targets:
                            Target [26-27]:
                                kind: Qubit(0)"#]],
    );
}

#[test]
fn unitless_float_too_large_is_error() {
    check(
        "X_ERROR(1e999) 0",
        &expect![[r#"
        Qdk.Stim.Parser.FloatTooLarge

          x floating-point literal is too large to fit in a 64-bit float
           ,----
         1 | X_ERROR(1e999) 0
           :         ^^^^^
           `----
    "#]],
    );
}

#[test]
fn negative_unitless_float_too_large_is_error() {
    check(
        "X_ERROR(-1e999) 0",
        &expect![[r#"
        Qdk.Stim.Parser.FloatTooLarge

          x floating-point literal is too large to fit in a 64-bit float
           ,----
         1 | X_ERROR(-1e999) 0
           :         ^^^^^^
           `----
    "#]],
    );
}

#[test]
fn radians_float_too_large_is_error() {
    check(
        "R_X(1e999rad) 0",
        &expect![[r#"
        Qdk.Stim.Parser.FloatTooLarge

          x floating-point literal is too large to fit in a 64-bit float
           ,----
         1 | R_X(1e999rad) 0
           :     ^^^^^
           `----
    "#]],
    );
}

#[test]
fn negative_radians_float_too_large_is_error() {
    check(
        "R_X(-1e999rad) 0",
        &expect![[r#"
        Qdk.Stim.Parser.FloatTooLarge

          x floating-point literal is too large to fit in a 64-bit float
           ,----
         1 | R_X(-1e999rad) 0
           :     ^^^^^^
           `----
    "#]],
    );
}

#[test]
fn trailing_comma_is_error() {
    check(
        "X_ERROR(0.1,) 0",
        &expect![[r#"
            Qdk.Stim.Parser.Expected

              x expected number, found close(paren)
               ,----
             1 | X_ERROR(0.1,) 0
               :             ^
               `----
        "#]],
    );
}

#[test]
fn missing_comma_between_args_is_error() {
    check(
        "X_ERROR(0.1 0.2) 0",
        &expect![[r#"
            Qdk.Stim.Parser.ExpectedToken

              x expected comma, found double
               ,----
             1 | X_ERROR(0.1 0.2) 0
               :             ^^^
               `----
        "#]],
    );
}

#[test]
fn unclosed_paren_is_error() {
    check(
        "X_ERROR(0.1 \n",
        &expect![[r#"
            Qdk.Stim.Parser.ExpectedToken

              x expected comma, found newline
               ,----
             1 | X_ERROR(0.1 
               :             ^
               `----
        "#]],
    );
}

#[test]
fn non_number_arg_is_error() {
    check(
        "X_ERROR(H) 0",
        &expect![[r#"
            Qdk.Stim.Parser.Expected

              x expected number, found instruction_name
               ,----
             1 | X_ERROR(H) 0
               :         ^
               `----
        "#]],
    );
}
