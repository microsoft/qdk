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
                        0.001
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
                        0.01
                        0.02
                        0.03
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
                        0.001
                    targets:
                        Target [14-15]:
                            kind: Qubit(0)"#]],
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
