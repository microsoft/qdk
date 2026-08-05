// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn pauli_targets() {
    check(
        "X0 Y12 Z345",
        &expect![[r#"
            pauli_target(X0) [0-2]
            pauli_target(Y12) [3-6]
            pauli_target(Z345) [7-11]"#]],
    );
}

#[test]
fn loss_targets() {
    check(
        "L0 L12",
        &expect![[r#"
            loss_target(L0) [0-2]
            loss_target(L12) [3-6]"#]],
    );
}

#[test]
fn pauli_prefix_does_not_make_every_identifier_a_target() {
    check(
        "X Y Z X_ERROR XYZ X1foo",
        &expect![[r#"
            instruction_name(X) [0-1]
            instruction_name(Y) [2-3]
            instruction_name(Z) [4-5]
            instruction_name(X_ERROR) [6-13]
            instruction_name(XYZ) [14-17]
            instruction_name(X1foo) [18-23]"#]],
    );
}

#[test]
fn loss_prefix_does_not_make_every_identifier_a_target() {
    check(
        "L LOSS_ERROR Loss L1foo",
        &expect![[r#"
            instruction_name(L) [0-1]
            instruction_name(LOSS_ERROR) [2-12]
            instruction_name(Loss) [13-17]
            instruction_name(L1foo) [18-23]"#]],
    );
}

#[test]
fn pauli_product() {
    check(
        "X0*!Y2",
        &expect![[r#"
            pauli_target(X0) [0-2]
            star(*) [2-3]
            bang(!) [3-4]
            pauli_target(Y2) [4-6]"#]],
    );
}

#[test]
fn lowercase_prefixes_are_identifiers() {
    check(
        "x0 y1 z2 l3",
        &expect![[r#"
            instruction_name(x0) [0-2]
            instruction_name(y1) [3-5]
            instruction_name(z2) [6-8]
            instruction_name(l3) [9-11]"#]],
    );
}

#[test]
fn large_indices_are_still_targets() {
    check(
        "X4294967296 L4294967296",
        &expect![[r#"
            pauli_target(X4294967296) [0-11]
            loss_target(L4294967296) [12-23]"#]],
    );
}
