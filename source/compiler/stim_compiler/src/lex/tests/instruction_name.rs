// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn single_letter() {
    check("H", &expect!["instruction_name(H) [0-1]"]);
    check("X", &expect!["instruction_name(X) [0-1]"]);
}

#[test]
fn multiple_letters() {
    check("CNOT", &expect!["instruction_name(CNOT) [0-4]"]);
    check("CX", &expect!["instruction_name(CX) [0-2]"]);
}

#[test]
fn letters_followed_by_digits() {
    check("H1", &expect!["instruction_name(H1) [0-2]"]);
    check(
        "DEPOLARIZE2",
        &expect!["instruction_name(DEPOLARIZE2) [0-11]"],
    );
}

#[test]
fn underscores_are_allowed_after_a_leading_letter() {
    check("S_DAG", &expect!["instruction_name(S_DAG) [0-5]"]);
    check("SQRT_X", &expect!["instruction_name(SQRT_X) [0-6]"]);
}

#[test]
fn mixed_case() {
    check("Cnot", &expect!["instruction_name(Cnot) [0-4]"]);
    check("h", &expect!["instruction_name(h) [0-1]"]);
}
