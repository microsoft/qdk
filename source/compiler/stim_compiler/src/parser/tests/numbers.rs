// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn max_qubit_index() {
    // 2^32 - 1
    check(
        "H 4294967295",
        &expect![[r#"
        Circuit [0-12]:
            items:
                Instruction [0-12]:
                    name: H
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [2-12]:
                            kind: Qubit(4294967295)"#]],
    );
}

#[test]
fn qubit_index_too_large_is_error() {
    // 2^32
    check(
        "H 4294967296",
        &expect![[r#"
            Qdk.Stim.Parser.IntegerTooLarge

              x integer literal is too large to fit in a 32-bit unsigned integer
               ,----
             1 | H 4294967296
               :   ^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn measurement_record_index_too_large_is_error() {
    // 2^32 (the number inside of brackets is - uint, not a normal int)
    check(
        "DETECTOR rec[-4294967296]",
        &expect![[r#"
            Qdk.Stim.Parser.IntegerTooLarge

              x integer literal is too large to fit in a 32-bit unsigned integer
               ,----
             1 | DETECTOR rec[-4294967296]
               :               ^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn non_integer_qubit_index_is_error() {
    check(
        "H 0.5",
        &expect![[r#"
            Qdk.Stim.Parser.Expected

              x expected a target, found double
               ,----
             1 | H 0.5
               :   ^^^
               `----
        "#]],
    );
}

#[test]
fn leading_zeros_in_qubit_index() {
    check(
        "H 007",
        &expect![[r#"
        Circuit [0-5]:
            items:
                Instruction [0-5]:
                    name: H
                    tag: <none>
                    args: <empty>
                    targets:
                        Target [2-5]:
                            kind: Qubit(7)"#]],
    );
}

#[test]
fn sweep_index_too_large_is_error() {
    // 2^32
    check(
        "CX sweep[4294967296] 0",
        &expect![[r#"
            Qdk.Stim.Parser.IntegerTooLarge

              x integer literal is too large to fit in a 32-bit unsigned integer
               ,----
             1 | CX sweep[4294967296] 0
               :          ^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn pauli_value_too_large_is_error() {
    // 2^32
    check(
        "MPP X4294967296",
        &expect![[r#"
            Qdk.Stim.Parser.IntegerTooLarge

              x integer literal is too large to fit in a 32-bit unsigned integer
               ,----
             1 | MPP X4294967296
               :      ^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn loss_value_too_large_is_error() {
    // 2^32
    check(
        "E(0.01) L4294967296",
        &expect![[r#"
            Qdk.Stim.Parser.IntegerTooLarge

              x integer literal is too large to fit in a 32-bit unsigned integer
               ,----
             1 | E(0.01) L4294967296
               :          ^^^^^^^^^^
               `----
        "#]],
    );
}
